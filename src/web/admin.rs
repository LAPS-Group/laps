use crate::{
    module_handling::{ModuleError, ModuleInfo},
    types::{BackendError, UserError},
    util,
    web::multipart::MultipartForm,
};
use bollard::{
    container::{
        Config, CreateContainerOptions, HostConfig, ListContainersOptions, RestartContainerOptions,
        StartContainerOptions, StopContainerOptions,
    },
    image::{APIImages, BuildImageOptions, BuildImageResults, ListImagesOptions},
    Docker,
};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use darkredis::{ConnectionPool, Value};
use rand::RngCore;
use rocket::{
    http::{Cookie, Cookies, SameSite, Status},
    request::{Form, State},
    response::NamedFile,
};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use std::io::Write;
use tokio::stream::StreamExt;

mod adminsession;
use super::mime_consts;
use adminsession::AdminSession;

#[cfg(test)]
mod test;

#[get("/admin")]
pub async fn index(_session: AdminSession) -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}

fn has_valid_tiff_header(input: &[u8]) -> bool {
    //Instead of verifying everything in the TIFF file to be valid, just check if the TIFF header is there.
    //If the image is actually invalid this will be detected by GDAL further down the pipeline.
    //Header length is 8 bytes
    if input.len() < 8 {
        false
    } else {
        let buffer = &input[..8];
        //Intel(little endian) or Motorola(big endian) file?
        let little_endian = match &buffer[..2] {
            //Intel byte order
            b"II" => true,
            //Motorola
            b"MM" => false,
            //If neither the file is invalid
            _ => return false,
        };

        let version = if little_endian {
            (&buffer[2..]).read_u16::<LittleEndian>()
        } else {
            (&buffer[2..]).read_u16::<BigEndian>()
        };

        //Version check
        version.map(|v| v == 42).unwrap_or(false)
    }
}

#[post("/map", data = "<upload>")]
pub async fn new_map(
    pool: State<'_, ConnectionPool>,
    mut upload: MultipartForm,
    session: AdminSession,
) -> Result<Json<u32>, UserError> {
    let mut conn = pool.get().await;
    let data = upload
        .get_file(&mime_consts::IMAGE_TIFF, "data")
        .ok_or_else(|| UserError::BadForm("Missing `data` field".into()))?;

    //Do a quick and dirty check that the file has the TIF image header
    if !has_valid_tiff_header(&data) {
        return Err(UserError::ModuleImport("Invalid Tiff header".into()));
    }

    //Put the map into a temporary file. This is needed because GDAL does not allow us to give it a buffer, it has
    //to be put into some sort of file.
    //Tokio::fs::File is stupidly slow and resource intensive, so using the normal std::fs::File is much better.
    let data = tokio::task::spawn_blocking(move || {
        match tempfile::NamedTempFile::new().map_err(|e| UserError::Internal(BackendError::Io(e))) {
            Ok(o) => {
                let (mut file, path) = o.into_parts();
                file.write_all(data.as_slice())
                    .expect("writing map data to temporary file");

                laps_convert::convert_to_png(path).map_err(UserError::MapConvert)
            }
            Err(e) => Err(e),
        }
    })
    .await
    .expect("spawn_blocking");

    //Actually import the data
    let (image, metadata) = data?;
    //Use the test version in test mode.
    let map_id = if cfg!(test) {
        laps_convert::import_data_test(&mut conn, image, metadata)
            .await
            .expect("importing map data")
    } else {
        laps_convert::import_data(&mut conn, image, metadata)
            .await
            .expect("importing map data")
    };

    info!(
        "Admin {} uploaded a new map with ID {}",
        session.username, map_id
    );
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
    let image_key = util::create_redis_key("mapdata.image");
    let meta_key = util::create_redis_key("mapdata.meta");
    let id = id.to_string();
    if conn.hdel(image_key, &id).await? {
        //Don't really care what the result of this is
        let _ = conn.hdel(meta_key, &id).await?;
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
        Ok(Status::NoContent)
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

//Get a list of the running modules
async fn running_modules(docker: &Docker) -> Result<Vec<ModuleInfo>, BackendError> {
    Ok(docker
        .list_containers(None::<ListContainersOptions<String>>)
        .await?
        .into_iter()
        .map(|s| extract_module_info_from_tag(&s.image).unwrap())
        .collect())
}

//Check if a module exists.
async fn module_exists(docker: &Docker, module: &ModuleInfo) -> Result<bool, BackendError> {
    //Get a list of all modules
    let images: Vec<APIImages> = docker
        .list_images(None::<ListImagesOptions<String>>)
        .await
        .map_err(BackendError::Docker)?;
    //Figure out if module with name `name` and version `version` is in that list.
    Ok(images.into_iter().any(|i| {
        //We are are guaranteed to have a version if repo_tags is Some.
        if let Some(m) = i.repo_tags.map(|s| {
            s.last()
                .map(|l| extract_module_info_from_tag(l).unwrap())
                .unwrap()
        }) {
            module == &m
        } else {
            false
        }
    }))
}

//Check if a module is running
async fn module_is_running(docker: &Docker, module: &ModuleInfo) -> Result<bool, BackendError> {
    let running_modules = running_modules(&docker).await?;
    Ok(running_modules.iter().any(|m| m == module))
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

    let running_containers = running_modules(&docker).await?;

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
    session: AdminSession,
) -> Result<Status, UserError> {
    //Include the module runner dependencies into the executable to make managing them easier.
    const MODULE_DOCKERFILE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/laps_module_runner/Dockerfile"
    ));
    const MODULE_LAPS_PY: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/laps_module_runner/laps.py"
    ));

    //Get the required fields out of the form.
    let name = form
        .get_text("name")
        .ok_or_else(|| UserError::BadForm("Missing name".into()))?
        .trim()
        .to_string();
    let version = form
        .get_text("version")
        .ok_or_else(|| UserError::BadForm("Missing version".into()))?
        .trim()
        .to_string();

    //Accept only .tar
    let module = form
        .get_file(&mime_consts::X_TAR, "module")
        .ok_or_else(|| UserError::BadForm("Expected module with type application/x-tar".into()))?;

    //Validation
    //Check the name
    if name.chars().any(|c| c == ':') {
        return Err(UserError::BadForm("Name cannot have ':' in it".into()));
    }

    //Check that there's no image with the same name and version currently
    let info = ModuleInfo { name, version };
    if module_exists(&docker, &info).await? {
        return Err(UserError::ModuleImport("Module already exists".into()));
    }

    //Time to create the image, pack it all into a tar:
    let mut tarball = Vec::new();
    {
        //use an inner scope to drop `builder` when we're done.
        let mut builder = tar::Builder::new(&mut tarball);
        //Insert LAPS files. Insertion of data cannot fail because we are writing directly to memory.
        let mut header = tar::Header::new_gnu();
        header.set_size(MODULE_DOCKERFILE.len() as u64);
        builder
            .append_data(&mut header, "Dockerfile", MODULE_DOCKERFILE)
            .unwrap();
        header.set_size(MODULE_LAPS_PY.len() as u64);
        builder
            .append_data(&mut header, "laps.py", MODULE_LAPS_PY)
            .unwrap();

        //Finally append the user data to the archive
        header.set_size(module.len() as u64);
        builder
            .append_data(&mut header, "contents.tar", module.as_slice())
            .unwrap();

        builder.finish().expect("writing image tarball");
    }

    //Build the image
    let options = BuildImageOptions {
        t: format!("{}:{}", info.name, info.version),
        rm: true,
        forcerm: true,
        ..Default::default()
    };
    let mut stream = docker.build_image(options, None, Some(tarball.into()));
    while let Some(update) = stream.next().await {
        let update = match update {
            Ok(u) => Ok(u),
            Err(e) => {
                use bollard::errors::ErrorKind;
                match e.kind() {
                    ErrorKind::JsonDeserializeError { .. } => {
                        warn!(
                            "Failed to deserialize Docker response: {:?}. Trying to keep going...",
                            e.kind()
                        );
                        continue;
                    }
                    ErrorKind::JsonDataError { .. } => {
                        warn!(
                            "Failed to deserialize Docker response: {} Trying to keep going...",
                            e.kind(),
                        );
                        continue;
                    }
                    _ => {
                        error!("Other Docker error: {}", e);
                        Err(e)
                    }
                }
            }
        }
        .map_err(|e| UserError::ModuleImport(e.to_string()))?;
        debug!("Importing {}: {:?}", info, update);
        if let BuildImageResults::BuildImageError {
            error,
            error_detail,
        } = update
        {
            return Err(UserError::ModuleImport(format!(
                "Module import error: {}\nDetails: {:?}",
                error, error_detail
            )));
        }
    }

    info!("{} imported module {}", session.username, info);
    Ok(Status::Created)
}

#[post("/module/<name>/<version>/restart")]
pub async fn restart_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
) -> Result<Status, BackendError> {
    //First, verify that the requested module actually exists:
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        return Ok(Status::NotFound);
    }

    //If the module is already running, use the restart_container
    let container_name = module.to_string().replace(":", "-");
    if module_is_running(&docker, &module).await? {
        //Give the module 30s to shut down
        let options = RestartContainerOptions { t: 30 };
        docker
            .restart_container(&container_name, Some(options))
            .await?;
        info!("{} restarted module {}", session.username, &module);
        Ok(Status::NoContent)
    } else {
        //If not, start it up for the first time
        let redis = &crate::CONFIG.redis.address;
        //For Redis to succeed in connecting the format of the address field must be <host>:<port>
        let split = redis.find(':').unwrap();
        let redis_host = &redis[..split];
        let redis_port = &redis[split + 1..];

        //Run it with a default set of commands
        let mut command = vec![
            "python3",
            "main.py",
            &module.name,
            &module.version,
            "--redis_host",
            redis_host,
            "--port",
            redis_port,
        ];
        //Use test keys in laps.py if running in test mode
        if cfg!(test) {
            command.push("--test");
        }

        //Setup the settings
        let module_name = module.to_string();
        let host_config = HostConfig {
            network_mode: Some("host"),
            ..Default::default()
        };
        let config = Config {
            image: Some(module_name.as_str()),
            cmd: Some(command),
            host_config: Some(host_config),
            stop_signal: Some("SIGINT"),
            ..Default::default()
        };
        let options = CreateContainerOptions {
            name: &container_name,
        };
        //Print any warnings
        let result = docker.create_container(Some(options), config).await?;
        debug!(
            "Successfully created container with name {}:{}",
            container_name, result.id
        );
        let id = &result.id;
        if let Some(w) = result.warnings {
            w.into_iter().for_each(|w| warn!("Container {}: {}", id, w));
        }

        //Fire this sucker up~
        docker
            .start_container(
                &module.to_string().replace(":", "-"),
                None::<StartContainerOptions<String>>,
            )
            .await?;

        info!(
            "{} successfully started module {}",
            session.username, module
        );
        Ok(Status::Created)
    }
}

#[post("/module/<name>/<version>/stop")]
pub async fn stop_module(
    session: AdminSession,
    name: String,
    version: String,
    docker: State<'_, Docker>,
) -> Result<Status, BackendError> {
    //If the module doesn't exist, 404
    let module = ModuleInfo { name, version };
    if !module_exists(&docker, &module).await? {
        warn!("Couln't find module {}", module);
        Ok(Status::NotFound)
    } else {
        //If the module isn't running, don't bother stopping it
        if !module_is_running(&docker, &module).await? {
            Ok(Status::NotModified)
        } else {
            let options = StopContainerOptions { t: 60 };
            let container = module.to_string().replace(":", "-");
            match docker.stop_container(&container, Some(options)).await {
                Ok(_) => {
                    info!("Module {} stopped by {}", container, session.username);
                    Ok(Status::NoContent)
                }
                Err(e) => {
                    error!(
                        "Failed attempt to stop {} by {}: {:?}",
                        container, session.username, e
                    );
                    Err(BackendError::Docker(e))
                }
            }
        }
    }
}
