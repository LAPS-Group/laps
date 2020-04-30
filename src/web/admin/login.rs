use super::AdminSession;
use crate::{types::BackendError, util};
use darkredis::{Command, Connection, ConnectionPool, MSetBuilder, Value};
use futures::stream::StreamExt;
use rand::RngCore;
use rocket::{
    http::{Cookie, Cookies, SameSite, Status},
    request::{Form, State},
    response::{NamedFile, Redirect},
    Response,
};
use std::io::Cursor;

//Index stuff
#[get("/login", rank = 2)]
pub fn login_index() -> Option<NamedFile> {
    NamedFile::open("dist/login.html").ok()
}

//For when the user is logged in, but tries to log in anyway.
//There's no reason to show them the login page again, so don't.
#[get("/login", rank = 1)]
pub fn login_with_session(_session: AdminSession) -> Redirect {
    Redirect::to(uri!(super::index))
}

#[get("/login.js")]
pub fn login_index_js() -> Option<NamedFile> {
    NamedFile::open("dist/login.js").ok()
}

#[derive(FromForm)]
pub struct AdminLogin {
    username: String,
    password: String,
}

//There's no reason to allow a user to log in if they already are logged in.
#[post("/login", data = "<_login>")]
pub fn login_attempt_with_session(_login: Form<AdminLogin>, _session: AdminSession) -> Status {
    Status::Forbidden
}

#[post("/login", data = "<login>", rank = 2)]
pub async fn login(
    pool: State<'_, ConnectionPool>,
    login: Form<AdminLogin>,
    mut cookies: Cookies<'_>,
) -> Result<Status, BackendError> {
    let mut conn = pool.get().await;

    //We should store the administrators in the following way:
    //Key laps.backend.admins.<name.lower()>
    //contains the admin's name, salted hashed password and salt.

    let key = util::get_admin_key(&login.username);
    //TODO Replace with hmget builder in darkredis when that comes along
    let command = Command::new("HMGET").arg(&key).arg(b"hash").arg(b"super");

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
                let mut buffer = vec![0u8; 64];
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

//Return true if there are any admins registered.
async fn has_any_admins(conn: &mut Connection) -> Result<bool, BackendError> {
    //Search the database for admin keys. If we find any, we have registered some administrator from before.
    let pattern = util::get_admin_key("*");
    let admins = conn
        .scan()
        .pattern(&pattern)
        .run()
        .collect::<Vec<_>>()
        .await;
    Ok(!admins.is_empty())
}

//Insert an admin into the database, checking that the password is within the required limits.
async fn insert_admin(
    conn: &mut Connection,
    username: &str,
    password: &str,
    is_super: bool,
) -> Result<Response<'static>, BackendError> {
    //Check that the password is not too long nor too short
    let response = if password.len() < crate::CONFIG.login.minimum_password_length as usize {
        Response::build()
            .status(Status::BadRequest)
            .sized_body(Cursor::new("Password is too short!"))
            .await
            .finalize()
    } else if password.len() > crate::CONFIG.login.maximum_password_length as usize {
        Response::build()
            .status(Status::BadRequest)
            .sized_body(Cursor::new("Password is too long!"))
            .await
            .finalize()
    } else {
        let admin_key = util::get_admin_key(username);
        let config = argon2::Config::default();
        let salt = util::generate_salt();
        let hash = argon2::hash_encoded(password.as_bytes(), &salt, &config).unwrap();
        let builder = MSetBuilder::new()
            .set(b"hash", &hash)
            .set(b"super", if is_super { b"1" } else { b"0" });
        conn.hset_many(&admin_key, builder).await?;
        info!("Registered new admin {}", username);
        Response::build().status(Status::Created).finalize()
    };
    Ok(response)
}

//The route to register an administrator the first time the service starts up.
//Will only be available if there are no administrators configured.
#[post("/register", data = "<login>", rank = 2)]
pub async fn register_super_admin(
    pool: State<'_, ConnectionPool>,
    login: Form<AdminLogin>,
) -> Result<Response<'_>, BackendError> {
    let mut conn = pool.get().await;
    if has_any_admins(&mut conn).await? {
        //This endpoint may only be used by a non-admin during first time setup.
        warn!("Attempt to register a super admin, but we already have one!");
        let response = Response::build().status(Status::Forbidden).finalize();
        Ok(response)
    } else {
        let response = insert_admin(&mut conn, &login.username, &login.password, true).await?;
        Ok(response)
    }
}

#[post("/register", data = "<login>")]
pub async fn register_admin(
    pool: State<'_, ConnectionPool>,
    session: AdminSession,
    login: Form<AdminLogin>,
) -> Result<Response<'_>, BackendError> {
    //This endpoint requires the admin to be a super admin.
    if session.is_super {
        let key = util::get_admin_key(&login.username);
        let mut conn = pool.get().await;
        //If the admin already exists, do not overwrite the existing account
        let response = if conn.exists(&key).await? {
            warn!(
                "Attempt to register admin {} which already exists!",
                session.username
            );
            Response::build()
                .status(Status::Conflict)
                .sized_body(Cursor::new("Admin already exists with that name."))
                .await
                .finalize()
        } else {
            //All is good, create a new admin, but do not make him a super admin.
            info!("Registed new admin {}", login.username);
            insert_admin(&mut conn, &login.username, &login.password, false).await?
        };
        Ok(response)
    } else {
        Ok(Response::build().status(Status::Forbidden).finalize())
    }
}
