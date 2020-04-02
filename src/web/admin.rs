use crate::{
    module_handling::{ModuleError, ModuleInfo},
    types::{BackendError, UserError},
    util,
    web::multipart::MultipartForm,
};
use bollard::{
    container::ListContainersOptions,
    image::{APIImages, CreateImageOptions, CreateImageResults, ListImagesOptions},
    Docker,
};
use darkredis::{ConnectionPool, Value};
use rand::RngCore;
use rocket::{
    http::{Cookie, Cookies, SameSite, Status},
    request::{Form, State},
    response::NamedFile,
};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncWriteExt, stream::StreamExt};

mod adminsession;
use super::mime_consts;
use adminsession::AdminSession;

#[cfg(test)]
mod test;

#[get("/admin")]
pub async fn index(_session: AdminSession) -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}

#[post("/map", data = "<upload>")]
pub async fn new_map(
    pool: State<'_, ConnectionPool>,
    mut upload: MultipartForm,
    session: AdminSession,
) -> Result<Json<u32>, UserError> {
    let mut conn = pool.get().await;
    let data = upload
        .get_file(&mime_consts::IMAGE_PNG, "data")
        .ok_or_else(|| UserError::BadForm("Missing `data` field".into()))?;
    //If we're in test mode, do not convert. We won't be testing the conversion here, just the endpoint.
    let map_id = if cfg!(test) {
        laps_convert::import_png_as_mapdata_test(&mut conn, data)
            .await
            .expect("importing fake mapdata")
    } else {
        //Generate a temporary file.
        let (file, path) = tempfile::NamedTempFile::new()
            .map_err(|e| UserError::Internal(BackendError::Io(e)))?
            .into_parts();
        let mut file = File::from_std(file);

        file.write_all(&data)
            .await
            .expect("writing map data to temporary file");

        let image = laps_convert::create_normalized_png(path)?;
        let result = laps_convert::import_png_as_mapdata(&mut conn, image.data)
            .await
            .expect("importing map data");

        info!(
            "Admin {} uploaded a new map with ID {}",
            session.username, result
        );
        result
    };
    Ok(Json(map_id))
}

#[delete("/map/<id>")]
pub async fn delete_map(
    pool: State<'_, ConnectionPool>,
    session: AdminSession,
    id: i32,
) -> Result<Status, BackendError> {
    //We're already authenticated, just get rid of the map in question.
    let mut conn = pool.get().await;
    let mapdata_key = util::create_redis_key("mapdata");
    let id = id.to_string();
    if conn.hdel(mapdata_key, &id).await? {
        info!("Map {} deleted by {}", id, session.username);
        Ok(Status::NoContent)
    } else {
        Ok(Status::NotFound)
    }
}

#[get("/modules/errors")]
pub async fn show_errors(
    pool: State<'_, ConnectionPool>,
    _session: AdminSession,
) -> Result<Json<Vec<ModuleError>>, BackendError> {
    //Grab all recent errors.
    let mut conn = pool.get().await;
    let key = util::create_redis_backend_key("recent-errors");
    let len = conn.llen(&key).await?.unwrap();
    let output: Vec<ModuleError> = conn
        .lrange(key, 0, len)
        .await?
        .into_iter()
        .map(|e| serde_json::from_slice(&e).unwrap())
        .collect();

    Ok(Json(output))
}

#[derive(FromForm)]
pub struct AdminLogin {
    username: String,
    password: String,
}

#[post("/login", data = "<login>")]
pub async fn login(
    pool: State<'_, darkredis::ConnectionPool>,
    login: Form<AdminLogin>,
    mut cookies: Cookies<'_>,
) -> Result<Status, BackendError> {
    let mut conn = pool.get().await;

    //We should store the administrators in the following way:
    //Key laps.backend.admins.<name.lower()>
    //contains the admin's name, salted hashed password and salt.

    let key = util::get_admin_key(&login.username);
    //TODO Replace with hmget builder in darkredis when that comes along
    let command = darkredis::Command::new("HMGET")
        .arg(&key)
        .arg(b"hash")
        .arg(b"salt")
        .arg(b"super");

    //Get the results
    let mut iter = conn.run_command(command).await?.unwrap_array().into_iter();
    let hash = iter.next().unwrap();
    //The values will be Nil if the key doesn't exist
    if let Value::Nil = hash {
        //Do not leak information to the client about which part of the authentication failed.
        warn!(
            "Attempted to authenticate {} but account does not exist",
            login.username
        );
        return Ok(Status::Forbidden);
    }

    //Extract other values, assuming that the data is valid and that all fields are present
    let hash = hash.unwrap_string();
    let salt = iter.next().unwrap().unwrap_string();
    let is_super = String::from_utf8_lossy(&iter.next().unwrap().unwrap_string())
        .parse::<isize>()
        .unwrap()
        != 0;

    //Verify that the password matches
    if hash == util::calculate_password_hash(&login.password, &salt) {
        //yay!
        info!("Successfully authenticated admin {}", login.username);

        //Generate session identifier, rand::thread_rng() is again considered cryptographically secure.
        //ThreadRng does not implement send so make it short-lived
        let token = {
            let mut rng = rand::thread_rng();
            let mut buffer = vec![0u8; 256];
            rng.fill_bytes(&mut buffer);
            base64::encode(buffer)
        };

        //Create the session object
        let session = AdminSession {
            username: login.username.to_lowercase(),
            is_super,
        };

        //Register the session in the database
        let session_key = util::get_session_key(&token);
        conn.set_and_expire_seconds(
            &session_key,
            serde_json::to_vec(&session).unwrap(),
            crate::CONFIG.login.session_timeout,
        )
        .await?;

        //Create and set session cookie
        let cookie = Cookie::build("session-token", token)
            .http_only(true)
            .same_site(SameSite::Strict)
            .finish();
        cookies.add_private(cookie);

        //Done logging in!
        Ok(Status::Ok)
    } else {
        warn!("Failed authentication attempt for user {}", login.username);
        Ok(Status::Forbidden)
    }
}

//Return value for the module structs, with an additional field to determine if a module is currently running.
#[derive(Serialize, Deserialize, PartialEq)]
pub struct PathModule {
    running: bool,
    #[serde(flatten)]
    module: ModuleInfo,
}

fn extract_module_info_from_tag(tag: &str) -> Option<ModuleInfo> {
    //A valid tag will always have the format "a:b"
    tag.find(':').map(|s| ModuleInfo {
        name: tag[..s].to_string(),
        version: tag[s + 1..].to_string(),
    })
}

#[get("/module/all")]
pub async fn get_all_modules(
    docker: State<'_, Docker>,
    _session: AdminSession,
) -> Result<Json<Vec<PathModule>>, BackendError> {
    //Mostly just list available docker images to create
    let images: Vec<APIImages> = docker
        .list_images(None::<ListImagesOptions<String>>)
        .await?;

    let running_containers: Vec<ModuleInfo> = docker
        .list_containers(None::<ListContainersOptions<String>>)
        .await?
        .into_iter()
        .map(|s| extract_module_info_from_tag(&s.image).unwrap())
        .collect();

    let mut out = Vec::new();
    for image in images {
        //The repo_tags field will always have at least one element in it if it's `Some`.
        if let Some(tag) = image.repo_tags.map(|mut t| t.pop().unwrap()) {
            //A valid tag created by the backend will always have a version.
            let module = extract_module_info_from_tag(&tag).unwrap();

            //Check if the container we just found is actually running
            let running = running_containers.iter().any(|s| s == &module);
            out.push(PathModule { running, module });
        }
    }
    Ok(Json(out))
}

#[post("/module", data = "<form>")]
pub async fn upload_module(
    mut form: MultipartForm,
    docker: State<'_, Docker>,
    _session: AdminSession,
) -> Result<(), UserError> {
    //Get the required fields out of the form.
    let name = form
        .get_text("name")
        .ok_or_else(|| UserError::BadForm("Missing name".into()))?;
    let name = name.trim();
    let version = form
        .get_text("version")
        .ok_or_else(|| UserError::BadForm("Missing version".into()))?;
    let version = version.trim();

    //Accept both .tar and .gz for the Docker image.
    let module = form
        .get_file(&mime_consts::X_TAR, "module")
        .or_else(|| form.get_file(&mime_consts::X_TAR_GZ, "module"))
        .ok_or_else(|| UserError::BadForm("Expected module with type application/x-tar".into()))?;

    //Validation
    //Check the name
    if name.chars().any(|c| c == ':') {
        dbg!(&name);
        return Err(UserError::BadForm("Name cannot have ':' in it".into()));
    }

    //Check that there's no image with the same name and version currently
    let images: Vec<APIImages> = docker
        .list_images(None::<ListImagesOptions<String>>)
        .await
        .map_err(BackendError::Docker)?;
    let already_exists = images.into_iter().any(|i| {
        //We are are guaranteed to have a version if repo_tags is Some.
        if let Some(s) = i.repo_tags.map(|s| {
            s.last()
                .map(|l| extract_module_info_from_tag(l).unwrap())
                .unwrap()
        }) {
            s.name == name && s.version == version
        } else {
            false
        }
    });
    if already_exists {
        return Err(UserError::ModuleImport("Module already exists".into()));
    }

    //Create the image
    let options = CreateImageOptions {
        tag: version,
        from_src: "-",
        repo: name,
        ..Default::default()
    };
    let mut stream = docker.create_image(Some(options), Some(module.into()), None);
    while let Some(update) = stream.next().await {
        let update = update.map_err(|e| UserError::ModuleImport(e.to_string()))?;
        println!("Importing {}:{}: {:?}", name, version, update);
        if let CreateImageResults::CreateImageError {
            error,
            error_detail,
        } = update
        {
            error!("Failed to import image {:?}", error_detail);
            return Err(UserError::ModuleImport(error));
        }
    }
    Ok(())
}
