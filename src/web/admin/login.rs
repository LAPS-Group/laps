use super::AdminSession;
use crate::{types::BackendError, util};
use darkredis::Value;
use rand::RngCore;
use rocket::{
    http::{Cookie, Cookies, SameSite, Status},
    request::{Form, State},
};

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
    let hash = String::from_utf8_lossy(&hash);
    let is_super = String::from_utf8_lossy(&iter.next().unwrap().unwrap_string())
        .parse::<isize>()
        .unwrap()
        != 0;

    //Verify that the password matches
    match argon2::verify_encoded(&hash, login.password.as_bytes()) {
        Ok(true) => {
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
        }
        Ok(false) => {
            warn!("Failed authentication attempt for user {}", login.username);
            Ok(Status::Forbidden)
        }
        Err(e) => {
            error!(
                "Failed to check password hash from {}: {}",
                login.username, e
            );
            Ok(Status::InternalServerError)
        }
    }
}
