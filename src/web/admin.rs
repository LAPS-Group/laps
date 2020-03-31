use crate::{
    types::{BackendError, UserError},
    util,
};
use darkredis::Value;
use rand::RngCore;
use rocket::{
    http::{Cookie, Cookies, SameSite, Status},
    request::{Form, State},
    response::NamedFile,
};
use rocket_contrib::json::Json;
use tokio::{fs::File, io::AsyncWriteExt};

mod adminsession;
mod mapuploadrequest;

use adminsession::AdminSession;
use mapuploadrequest::MapUploadRequest;

#[get("/admin")]
pub async fn index(_session: AdminSession) -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}

#[post("/map", data = "<upload>")]
pub async fn new_map(
    pool: State<'_, darkredis::ConnectionPool>,
    upload: MapUploadRequest,
    session: AdminSession,
) -> Result<Json<u32>, UserError> {
    let mut conn = pool.get().await;
    //If we're in test mode, do not convert. We won't be testing the conversion here, just the endpoint.
    let map_id = if cfg!(test) {
        laps_convert::import_png_as_mapdata_test(&mut conn, upload.data)
            .await
            .expect("importing fake mapdata")
    } else {
        //Generate a temporary file.
        let (file, path) = tempfile::NamedTempFile::new()
            .map_err(|e| UserError::Internal(BackendError::Io(e)))?
            .into_parts();
        let mut file = File::from_std(file);

        file.write_all(&upload.data)
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::util;
    use multipart::client::lazy::Multipart;
    use rocket::{
        http::{ContentType, Status},
        local::Client,
    };
    use std::io::Read;

    #[tokio::test]
    //Will always fail if the login test below fails.
    async fn upload_map() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![new_map, login])
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
        let response_cookies = response.cookies();

        //Create a multipart form in the format which is expected by the add map endpoint.
        let fake_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut multipart = Multipart::new()
            .add_stream::<&str, &[u8], &str>("data", fake_data.as_slice(), None, None)
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
            .cookies(response_cookies);
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_json());
        assert_eq!(
            serde_json::from_slice::<u32>(&response.body_bytes().await.unwrap()).unwrap(),
            2
        );
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
}
