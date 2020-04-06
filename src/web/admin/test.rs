use super::*;
use crate::{module_handling::ModuleInfo, util};
use bollard::image::RemoveImageOptions;
use multipart::client::lazy::Multipart;
use rocket::{
    http::{ContentType, Status},
    local::Client,
};
use std::io::Read;

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
    assert_eq!(response.status(), Status::Ok);
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

//Read the test container from disk.
async fn get_test_container() -> Vec<u8> {
    //Use blocking IO for this because async files are extremely slow...
    let mut file = std::fs::File::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test_data/test_module.tar"
    ))
    .unwrap();
    let mut tarball = Vec::new();
    file.read_to_end(&mut tarball).unwrap();
    tarball
}

//Cleanup test containers and test images
async fn clean_docker(docker: &Docker) {
    let options = RemoveImageOptions {
        force: true,
        ..Default::default()
    };
    //We have to delete both the test image and the imported test image.
    for image in &["laps-test-image:latest", "laps-test:0.1.0"] {
        match docker.remove_image(image, Some(options), None).await {
            Ok(_) => println!("Found and deleted old test image {}", image),
            Err(e) => println!("Did not remove old test image: {}", e),
        }
    }

    //Delete all containers
    let options = bollard::container::RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    for container in &["laps-test-0.1.0"] {
        match docker.remove_container(container, Some(options)).await {
            Ok(_) => println!("Found and deleted old test container {}", container),
            Err(e) => println!("Did not remove old test container: {}", e),
        }
    }
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
    clean_docker(&docker).await;

    //Ensure the test image is built
    let tarball = get_test_container().await;

    //Upload the test image using the endpoint
    let module = ModuleInfo {
        name: "laps-test".into(),
        version: "0.1.0".into(),
    };
    let mut multipart = Multipart::new()
        .add_stream::<&str, &[u8], &str>(
            "module",
            tarball.as_slice(),
            None,
            Some("application/x-tar".parse().unwrap()),
        )
        .add_text("version", &module.version)
        .add_text("name", &module.name)
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
    assert_eq!(response.status(), Status::Created);

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
    images.into_iter().find(|m| m == &module).unwrap();

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
        .add_text("version", &module.version)
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
    assert!(body.contains("Expected module with type"));
}

#[tokio::test]
async fn start_stop_module() {
    //Setup rocket instance
    let redis = crate::create_redis_pool().await;
    let docker = crate::connect_to_docker().await;
    let rocket = rocket::ignite()
        .mount(
            "/",
            routes![
                get_all_modules,
                login,
                restart_module,
                stop_module,
                upload_module,
            ],
        )
        .manage(redis.clone())
        .manage(crate::connect_to_docker().await);
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    let cookies = create_account_and_login(&mut conn, &client).await;

    //Remove any old images if they exist and the container
    clean_docker(&docker).await;

    //Upload the test image
    let module = ModuleInfo {
        name: "laps-test".into(),
        version: "0.1.0".into(),
    };
    let tarball = get_test_container().await;
    let mut multipart = Multipart::new()
        .add_stream::<&str, &[u8], &str>(
            "module",
            tarball.as_slice(),
            None,
            Some("application/x-tar".parse().unwrap()),
        )
        .add_text("version", &module.version)
        .add_text("name", &module.name)
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
    assert_eq!(response.status(), Status::Created);

    //Interresting part: Start the module and check that it's running
    let response = client
        .post(format!(
            "/module/{}/{}/restart",
            module.name, module.version
        ))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Created);
    assert!(module_is_running(&docker, &module).await.unwrap());

    //Restart the module, verify that it was restarted and not started.
    let response = client
        .post(format!(
            "/module/{}/{}/restart",
            module.name, module.version
        ))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    assert!(module_is_running(&docker, &module).await.unwrap());

    //Now kill it
    let response = client
        .post(format!("/module/{}/{}/stop", module.name, module.version))
        .cookies(cookies)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    assert!(!module_is_running(&docker, &module).await.unwrap());
}