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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{module_handling::ModuleInfo, util};
    use bollard::image::{BuildImageOptions, BuildImageResults};
    use futures::StreamExt;
    use multipart::client::lazy::Multipart;
    use rocket::{
        http::{ContentType, Status},
        local::Client,
    };
    use std::io::Read;
    use tokio::{fs::File, io::AsyncReadExt};

    //Create an account and sign in for use in these tests
    async fn create_account_and_login(
        conn: &mut darkredis::Connection,
        client: &Client,
    ) -> Vec<Cookie<'static>> {
        //Register a test super admin
        let username = "test-admin";
        let admin_key = util::get_admin_key("test-admin");
        let salt = util::generate_salt();
        let password = "password";
        let hash = util::calculate_password_hash(&password, &salt);
        let builder = darkredis::MSetBuilder::new()
            .set(b"hash", &hash)
            .set(b"salt", &salt)
            .set(b"super", b"1");
        conn.hset_many(&admin_key, builder).await.unwrap();

        //Sign in
        //Create form
        let payload = format!("username={}&password={}", username, password);
        let response = client
            .post("/login")
            .header(ContentType::Form)
            .body(&payload)
            .dispatch()
            .await;
        //Keep track of the cookies as they're used to verify that we're logged in
        response
            .cookies()
            .into_iter()
            .map(|s| s.into_owned())
            .collect()
    }

    #[tokio::test]
    //Will always fail if the login test below fails.
    async fn map_manipulation() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![new_map, login, delete_map])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;
        crate::test::clear_redis(&mut conn).await;

        //Keep track of the cookies as they're used to verify that we're logged in
        let response_cookies = create_account_and_login(&mut conn, &client).await;

        //Create a multipart form in the format which is expected by the add map endpoint.
        let fake_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut multipart = Multipart::new()
            .add_stream::<&str, &[u8], &str>(
                "data",
                fake_data.as_slice(),
                None,
                Some(mime_consts::IMAGE_PNG.clone()),
            )
            .prepare()
            .unwrap();
        let mut form = Vec::new();
        let boundary = multipart.boundary().to_string();
        multipart.read_to_end(&mut form).unwrap();

        //Insert some map data
        let mut request = client
            .post("/map")
            .header(ContentType::with_params(
                "multipart",
                "form-data",
                ("boundary", boundary.clone()),
            ))
            .cookies(response_cookies.clone());
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_json());
        assert_eq!(
            serde_json::from_slice::<u32>(&response.body_bytes().await.unwrap()).unwrap(),
            1
        );

        //And create another to ensure that it gets the correct ID.
        let mut request = client
            .post("/map")
            .header(ContentType::with_params(
                "multipart",
                "form-data",
                ("boundary", boundary.clone()),
            ))
            .cookies(response_cookies.clone());
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_json());
        assert_eq!(
            serde_json::from_slice::<u32>(&response.body_bytes().await.unwrap()).unwrap(),
            2
        );

        //Test that deletion works.
        let request = client.delete("/map/2").cookies(response_cookies.clone());
        let response = request.dispatch().await;
        assert_eq!(response.status(), Status::NoContent);

        //Try to delete it again and fail.
        let request = client.delete("/map/2").cookies(response_cookies);
        let response = request.dispatch().await;
        assert_eq!(response.status(), Status::NotFound);
    }

    #[tokio::test]
    async fn login() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![login])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;
        crate::test::clear_redis(&mut conn).await;

        //Register a test super admin
        let username = "test-admin";
        let admin_key = util::get_admin_key("test-admin");
        let salt = util::generate_salt();
        let password = "password";
        let hash = util::calculate_password_hash(&password, &salt);
        let builder = darkredis::MSetBuilder::new()
            .set(b"hash", &hash)
            .set(b"salt", &salt)
            .set(b"super", b"1");
        conn.hset_many(&admin_key, builder).await.unwrap();

        //Try to login with a fake account
        let form = format!("username={}&password={}", "does-not-exist", "password");
        let response = client
            .post("/login")
            .body(&form)
            .header(ContentType::Form)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Forbidden);
        assert!(response.cookies().is_empty());

        //Try to login with the wrong password
        let form = format!("username={}&password={}", username, "incorrect-password");
        let response = client
            .post("/login")
            .body(&form)
            .header(ContentType::Form)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Forbidden);
        assert!(response.cookies().is_empty());

        //Login with the correct password
        let form = format!("username={}&password={}", username, password);
        let response = client
            .post("/login")
            .body(&form)
            .header(ContentType::Form)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.cookies().len(), 1);

        //Login again, but this time using all uppercase letters
        let form = format!("username={}&password={}", username.to_uppercase(), password);
        let response = client
            .post("/login")
            .body(&form)
            .header(ContentType::Form)
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.cookies().len(), 1);
    }

    #[tokio::test]
    //Fails if login test fails
    async fn list_errors() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![login, show_errors])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;
        crate::test::clear_redis(&mut conn).await;

        let cookies = create_account_and_login(&mut conn, &client).await;

        //Quick macro to check the response
        macro_rules! check_response {
            ($input:expr) => {
                let mut response = client
                    .get("/modules/errors")
                    .cookies(cookies.clone())
                    .dispatch()
                    .await;
                assert_eq!(response.status(), Status::Ok);
                assert_eq!(response.content_type(), Some(ContentType::JSON));
                let result: Vec<ModuleError> =
                    serde_json::from_slice(&response.body_bytes().await.unwrap()).unwrap();
                assert_eq!(result, $input);
            };
        }

        //Verify that no errors are present
        check_response!(vec![]);

        //add an error
        let module = ModuleInfo {
            name: "fake-module".into(),
            version: "0".into(),
        };
        let fake_error = ModuleError {
            instant: 100,
            module,
            message: "hello".into(),
        };
        let error_key = util::create_redis_backend_key("recent-errors");
        conn.rpush(&error_key, serde_json::to_vec(&fake_error).unwrap())
            .await
            .unwrap();

        //Verify that there's one
        check_response!(vec![fake_error.clone()]);

        //Add another one to check that the order is correct
        let mut other_error = fake_error.clone();
        other_error.message = "goodbye".to_string();
        conn.rpush(&error_key, serde_json::to_vec(&other_error).unwrap())
            .await
            .unwrap();

        check_response!(vec![fake_error, other_error]);
    }

    #[tokio::test]
    //Also fails if login fails
    async fn get_modules() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let docker = crate::connect_to_docker().await;
        let rocket = rocket::ignite()
            .mount("/", routes![login, get_all_modules, upload_module])
            .manage(redis.clone())
            .manage(crate::connect_to_docker().await);
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;
        crate::test::clear_redis(&mut conn).await;

        let cookies = create_account_and_login(&mut conn, &client).await;

        //Remove the test image if it exists
        let module_name = "laps-test";
        let module_version = "0.1.0";
        //The bollard Error type is unhelpful, assume that if this fails, it's because the image does
        //not exist.
        let _ = docker
            .remove_image(
                &format!("{}:{}", module_name, module_version),
                None::<bollard::image::RemoveImageOptions>,
                None,
            )
            .await;

        //Build the test image
        let mut file = File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test_data/test_image.tar"
        ))
        .await
        .unwrap();
        let mut tarball = Vec::new();
        file.read_to_end(&mut tarball).await.unwrap();
        let options = BuildImageOptions {
            t: "laps-test-image",
            nocache: true,
            ..Default::default()
        };
        let results: Vec<BuildImageResults> = docker
            .build_image(options, None, Some(tarball.into()))
            .map(|r| r.unwrap())
            .collect()
            .await;
        for r in results {
            println!("{:?}", r);
            if let BuildImageResults::BuildImageError { .. } = r {
                panic!("Failed to build image");
            }
        }
        let mut tarball = Vec::new();
        let mut stream = docker.export_image("laps-test-image");
        while let Some(s) = stream.next().await {
            tarball.extend_from_slice(&s.unwrap());
        }

        //Upload the test image using the endpoint
        let mut multipart = Multipart::new()
            .add_stream::<&str, &[u8], &str>(
                "module",
                tarball.as_slice(),
                None,
                Some("application/x-tar".parse().unwrap()),
            )
            .add_text("version", module_version)
            .add_text("name", module_name)
            .prepare()
            .unwrap();
        let mut form = Vec::new();
        let boundary = multipart.boundary().to_string();
        multipart.read_to_end(&mut form).unwrap();

        let mut request = client
            .post("/module")
            .header(ContentType::with_params(
                "multipart",
                "form-data",
                ("boundary", boundary.clone()),
            ))
            .cookies(cookies.clone());
        request.set_body(form.as_slice());
        let response = request.dispatch().await;
        // let body = response.body_string().await.unwrap();
        // dbg!(&body);
        assert_eq!(response.status(), Status::Ok);

        // let module_name = "laps-test";
        // let module_version = "0.1.0";

        //Check that the test module is returned by /module/all.
        let mut response = client
            .get("/module/all")
            .cookies(cookies.clone())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.content_type().unwrap(), ContentType::JSON);
        let images: Vec<ModuleInfo> =
            serde_json::from_slice(&response.body_bytes().await.unwrap()).unwrap();
        images
            .into_iter()
            .find(|m| m.name == module_name && m.version == module_version)
            .unwrap();

        //Try to add the module again, should fail as we already have a module with the same name and version.
        let mut request = client
            .post("/module")
            .header(ContentType::with_params(
                "multipart",
                "form-data",
                ("boundary", boundary),
            ))
            .cookies(cookies.clone());
        request.set_body(form.as_slice());
        let response = request.dispatch().await;
        assert_eq!(response.status(), Status::BadRequest);

        //Upload an invalid module.
        let mut multipart = Multipart::new()
            .add_stream::<&str, &[u8], &str>(
                "module",
                &[0],
                None,
                Some("application/x-tar+gz".parse().unwrap()),
            )
            .add_text("version", module_version)
            .add_text("name", "cool-test")
            .prepare()
            .unwrap();
        form.clear();
        let boundary = multipart.boundary().to_string();
        multipart.read_to_end(&mut form).unwrap();

        let mut request = client
            .post("/module")
            .header(ContentType::with_params(
                "multipart",
                "form-data",
                ("boundary", boundary),
            ))
            .cookies(cookies);
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::BadRequest);
        let body = response.body_string().await.unwrap();
        assert!(body.contains("tar file"));
    }
}
