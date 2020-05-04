//Test utility functions and such
use bollard::{image::RemoveImageOptions, Docker};
use multipart::client::lazy::Multipart;
use rocket::{
    http::{ContentType, Cookie},
    local::{Client, LocalResponse},
};
use std::io::Read;

//Insert some test mapdata to use in the tests. Will always place it at map ID 1. Returns the width and height of the image.
pub async fn insert_test_mapdata(conn: &mut darkredis::Connection) -> (u32, u32) {
    let path = "test_data/height_data/dtm1.tif";
    let (image, metadata) = laps_convert::convert_to_png(path).unwrap();

    let (width, height) = (image.width as u32, image.height as u32);
    laps_convert::import_data_test(conn, image, metadata)
        .await
        .unwrap();

    (width, height)
}

//A nice function for resetting only the test part of the database.
pub async fn clear_redis(conn: &mut darkredis::Connection) {
    use futures::StreamExt;

    let keys: Vec<Vec<u8>> = conn.scan().pattern(b"laps.testing.*").run().collect().await;
    for k in keys {
        conn.del(&k).await.unwrap();
    }
}

//Cleanup test containers and test images
pub async fn clean_docker(docker: &Docker) {
    let options = RemoveImageOptions {
        force: true,
        ..Default::default()
    };
    //We have to delete both the test image and the imported test image.
    for image in &[
        "laps-test-image:latest",
        "laps-test:0.1.0",
        "laps-failing-test:0.1.0",
        "laps-test-ignore:0.1.0",
        "laps-foo:0.1.0",
    ] {
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
    for container in &[
        "laps-test-0.1.0-0",
        "laps-test-0.1.0-1",
        "laps-failing-test-0.1.0-0",
    ] {
        match docker.remove_container(container, Some(options)).await {
            Ok(_) => println!("Found and deleted old test container {}", container),
            Err(e) => println!("Did not remove old test container: {}", e),
        }
    }
}

//Upload a testing image from `tarball` with name `name` and version `version`.
pub async fn upload_test_image<'a>(
    client: &'a Client,
    cookies: &'a Vec<Cookie<'a>>,
    tarball: &'a [u8],
    name: &'a str,
    version: &'a str,
    workers: Option<u8>,
) -> LocalResponse<'a> {
    //Create the multipart form with the optional worker field.
    let mut multipart = Multipart::new();
    multipart
        .add_stream::<&str, &[u8], &str>(
            "module",
            tarball,
            None,
            Some("application/x-tar".parse().unwrap()),
        )
        .add_text("version", version)
        .add_text("name", name);
    if let Some(w) = workers {
        multipart.add_text("workers", w.to_string());
    }

    //Finalise the form and send it
    let mut multipart = multipart.prepare().unwrap();
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

macro_rules! include_test_module {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test_data/test_modules/",
            $name
        ))
    };
}

//The standard test container, which works and will return extremely simple paths which are valid.
pub const TEST_CONTAINER: &[u8] = include_test_module!("simple.tar");
//The test container which immediately fails no matter what.
pub const INSTANTLY_FAILING_TEST_CONTAINER: &[u8] = include_test_module!("instant_fail.tar");
//The test container which will only return failing jobs.
pub const FAILING_TEST_CONTAINER: &[u8] = include_test_module!("failing.tar");
