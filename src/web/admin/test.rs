use super::*;
use crate::{module_handling::ModuleInfo, util};
use bollard::{image::RemoveImageOptions, Docker};
use modules::{module_exists, module_is_running};
use multipart::client::lazy::Multipart;
use rocket::{
    http::{ContentType, Cookie, Status},
    local::{Client, LocalResponse},
};
use serial_test::serial;
use std::io::Read;

//Create a test account using the initial setup handler. Will only work with that.
async fn create_test_account(username: &str, password: &str, client: &Client) {
    let form = format!("username={}&password={}", username, password);
    let response = client
        .post("/register")
        .header(ContentType::Form)
        .body(&form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Created);
}

//Create an account and sign in for use in these tests
async fn create_test_account_and_login(client: &Client) -> Vec<Cookie<'static>> {
    //Register a test super admin
    let username = "test-admin";
    let password = "password";
    create_test_account(username, password, client).await;

    //Sign in
    //Create form
    let payload = format!("username={}&password={}", username, password);
    let response = client
        .post("/login")
        .header(ContentType::Form)
        .body(&payload)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    //Keep track of the cookies as they're used to verify that we're logged in
    response
        .cookies()
        .into_iter()
        .map(|s| s.into_owned())
        .collect()
}

#[tokio::test]
#[serial]
//Will always fail if the login test below fails.
async fn map_manipulation() {
    //Setup rocket instance
    let redis = crate::create_redis_pool().await;
    let rocket = rocket::ignite()
        .mount(
            "/",
            routes![new_map, login, delete_map, register_super_admin],
        )
        .manage(redis.clone());
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    //Keep track of the cookies as they're used to verify that we're logged in
    let response_cookies = create_test_account_and_login(&client).await;

    //Send invalid map data
    let fake_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let mut multipart = Multipart::new()
        .add_stream::<&str, &[u8], &str>(
            "data",
            fake_data.as_slice(),
            None,
            Some(mime_consts::IMAGE_TIFF.clone()),
        )
        .prepare()
        .unwrap();
    let mut form = Vec::new();
    let boundary = multipart.boundary().to_string();
    multipart.read_to_end(&mut form).unwrap();
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
    assert_eq!(response.status(), Status::BadRequest);
    assert!(response
        .body_string()
        .await
        .unwrap()
        .contains("Invalid Tiff header"));

    //Send a valid TIFF this time.
    let mut multipart = Multipart::new()
        .add_stream::<&str, &[u8], &str>(
            "data",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/test_data/height_data/dtm1.tif"
            )),
            None,
            Some(mime_consts::IMAGE_TIFF.clone()),
        )
        .prepare()
        .unwrap();
    let mut form = Vec::new();
    let boundary = multipart.boundary().to_string();
    multipart.read_to_end(&mut form).unwrap();
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
#[serial]
async fn registration() {
    let redis = crate::create_redis_pool().await;
    let rocket = rocket::ignite()
        .mount("/", routes![login, register_super_admin, register_admin])
        .manage(redis.clone());
    let client = Client::untracked(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    //Test that registering accounts work. First up, that the new instance setup part is working:
    let cookies = create_test_account_and_login(&client).await;
    //Verify that the created admin is a super admin
    let key = util::get_admin_key("test-admin");
    assert_eq!(conn.hget(&key, "super").await.unwrap(), Some(b"1".to_vec()));

    //Try to register a new account without being signed in, which should fail:
    let username = "second-admin";
    let password = "password";
    let new_account_form = format!("username={}&password={}", username, password);
    let response = client
        .post("/register")
        .body(&new_account_form)
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Forbidden);

    //Now try to register an admin with a session, this should succeed:
    let response = client
        .post("/register")
        .body(&new_account_form)
        .cookies(cookies.clone())
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Created);
    //Verify that the created admin is NOT a super admin
    let key = util::get_admin_key(username);
    assert_eq!(conn.hget(&key, "super").await.unwrap(), Some(b"0".to_vec()));

    //Create another, which should fail:
    let response = client
        .post("/register")
        .body(&new_account_form)
        .cookies(cookies.clone())
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Conflict);

    //Try to create an administrator whose password is too short:
    let too_short = format!("username=another-admin&password=1");
    let mut response = client
        .post("/register")
        .body(too_short)
        .header(ContentType::Form)
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::BadRequest);
    assert!(response.body_string().await.unwrap().contains("too short"));

    //Try to create an administrator whose password is too long:
    let too_long = format!("username=another-admin&password=1234567890");
    let mut response = client
        .post("/register")
        .body(too_long)
        .header(ContentType::Form)
        .cookies(cookies)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::BadRequest);
    assert!(response.body_string().await.unwrap().contains("too long"));

    //Log in with the second admin we created earlier and try to add an administrator.
    let response = client
        .post("/login")
        .body(new_account_form)
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    let cookies = response.cookies();
    let response = client
        .post("/register")
        .cookies(cookies)
        .header(ContentType::Form)
        .body(format!("username=thid-admin&password=password"))
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Forbidden);
}

#[tokio::test]
#[serial]
async fn login() {
    //Setup rocket instance
    let redis = crate::create_redis_pool().await;
    let rocket = rocket::ignite()
        .mount("/", routes![login, register_super_admin, get_me])
        .manage(redis.clone());
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    //A function to test the /admin/me endpoint
    async fn get_me<'a>(client: &'a Client, cookies: Vec<Cookie<'a>>) -> LocalResponse<'a> {
        client.get("/admin/me").cookies(cookies).dispatch().await
    }

    //Register a test super admin
    let username = "test-admin";
    let password = "password";
    create_test_account(username, password, &client).await;

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
    //Verify that we can get the admin we logged in as.
    assert_eq!(
        get_me(&client, response.cookies()).await.status(),
        Status::NotFound
    );

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
    assert_eq!(
        get_me(&client, response.cookies()).await.status(),
        Status::NotFound
    );

    //Login with the correct password
    let form = format!("username={}&password={}", username, password);
    let response = client
        .post("/login")
        .body(&form)
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    assert_eq!(response.cookies().len(), 1);
    //Check that we can get ourselves from the session.
    let mut me = get_me(&client, response.cookies()).await;
    assert_eq!(me.status(), Status::Ok);
    assert_eq!(
        serde_json::from_slice::<AdminSession>(&me.body_bytes().await.unwrap())
            .unwrap()
            .username,
        username
    );
    //Login again, but this time using all uppercase letters
    let form = format!("username={}&password={}", username.to_uppercase(), password);
    let response = client
        .post("/login")
        .body(&form)
        .header(ContentType::Form)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    assert_eq!(response.cookies().len(), 1);
    //Check that we can get ourselves from the session.
    let mut me = get_me(&client, response.cookies()).await;
    assert_eq!(me.status(), Status::Ok);
    assert_eq!(
        serde_json::from_slice::<AdminSession>(&me.body_bytes().await.unwrap())
            .unwrap()
            .username,
        username
    );
}

#[tokio::test]
#[serial]
//Fails if login test fails
async fn module_logs() {
    //Setup rocket instance
    let redis = crate::create_redis_pool().await;
    let rocket = rocket::ignite()
        .mount(
            "/",
            routes![
                upload_module,
                login,
                get_module_logs,
                register_super_admin,
                restart_module
            ],
        )
        .manage(redis.clone())
        .manage(crate::connect_to_docker().await);
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;
    clean_docker(&crate::connect_to_docker().await).await;
    tokio::spawn(crate::module_handling::run(redis.clone()));

    let cookies = create_test_account_and_login(&client).await;

    //Check that this module does not exist.
    let name = "laps-test";
    let version = "0.1.0";
    let response = client
        .get(format!("/module/{}/{}/logs", name, version))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NotFound);

    //Upload a test module
    let tarball = get_test_container().await;
    let response = upload_test_image(&client, &cookies, &tarball, name, version).await;
    assert_eq!(response.status(), Status::Created);

    //Get the module logs again, this time it should exist but be empty:
    let response = client
        .get(format!("/module/{}/{}/logs", name, version))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);

    //Start up the test module
    let response = client
        .post(format!("/module/{}/{}/restart", name, version))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Created);

    //Sleep for a bit to let the module start up, 500ms should be more than plenty.
    //This line does mean this test is kind of flaky but by picking a large enough
    //number it should be okay.
    tokio::time::delay_for(std::time::Duration::from_millis(500)).await;

    //Try to get the module logs, this time it should have the startup message.
    let mut response = client
        .get(format!("/module/{}/{}/logs", name, version))
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);
    let body = response.body_string().await.unwrap();
    assert!(body.contains("Registered as"));
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

//Upload a testing image from `tarball` with name `name` and version `version`.
async fn upload_test_image<'a>(
    client: &'a Client,
    cookies: &'a Vec<Cookie<'a>>,
    tarball: &'a [u8],
    name: &'a str,
    version: &'a str,
) -> LocalResponse<'a> {
    let mut multipart = Multipart::new()
        .add_stream::<&str, &[u8], &str>(
            "module",
            tarball,
            None,
            Some("application/x-tar".parse().unwrap()),
        )
        .add_text("version", version)
        .add_text("name", name)
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
    request.dispatch().await
}

#[tokio::test]
#[serial]
//Also fails if login fails
async fn get_modules() {
    //Setup rocket instance
    let redis = crate::create_redis_pool().await;
    let docker = crate::connect_to_docker().await;
    let rocket = rocket::ignite()
        .mount(
            "/",
            routes![login, get_all_modules, upload_module, register_super_admin],
        )
        .manage(redis.clone())
        .manage(crate::connect_to_docker().await);
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    let cookies = create_test_account_and_login(&client).await;

    //Remove the test image if it exists
    clean_docker(&docker).await;

    //Ensure the test image is built
    let tarball = get_test_container().await;

    //Upload the test image using the endpoint
    let module = ModuleInfo {
        name: "laps-test".into(),
        version: "0.1.0".into(),
    };
    let response =
        upload_test_image(&client, &cookies, &tarball, &module.name, &module.version).await;
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
    let response =
        upload_test_image(&client, &cookies, &tarball, &module.name, &module.version).await;
    assert_eq!(response.status(), Status::BadRequest);

    //Upload an invalid module.
    let response = upload_test_image(
        &client,
        &cookies,
        &[0u8],
        "some-unique-name",
        &module.version,
    )
    .await;
    assert_eq!(response.status(), Status::BadRequest);
}

#[tokio::test]
#[serial]
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
                register_super_admin,
            ],
        )
        .manage(redis.clone())
        .manage(crate::connect_to_docker().await);
    let client = Client::new(rocket).unwrap();
    let mut conn = redis.get().await;
    crate::test::clear_redis(&mut conn).await;

    let cookies = create_test_account_and_login(&client).await;

    //Remove any old images if they exist and the container
    clean_docker(&docker).await;

    //Check that the module doesn't exist from before
    let module = ModuleInfo {
        name: "laps-test".into(),
        version: "0.1.0".into(),
    };
    assert!(!module_exists(&docker, &module).await.unwrap());
    assert!(!module_is_running(&docker, &module).await.unwrap());

    //Upload the test image
    let tarball = get_test_container().await;
    let response =
        upload_test_image(&client, &cookies, &tarball, &module.name, &module.version).await;
    assert_eq!(response.status(), Status::Created);
    assert!(module_exists(&docker, &module).await.unwrap());
    assert!(!module_is_running(&docker, &module).await.unwrap());

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
        .cookies(cookies.clone())
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NoContent);
    assert!(!module_is_running(&docker, &module).await.unwrap());
}
