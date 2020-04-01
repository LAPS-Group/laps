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
