use crate::types::{BackendError, UserError};
use rocket::{request::State, response::NamedFile};
use rocket_contrib::json::Json;
use tokio::{fs::File, io::AsyncWriteExt};

mod adminsession;
mod mapuploadrequest;

use mapuploadrequest::MapUploadRequest;

#[get("/admin")]
pub async fn index() -> Option<NamedFile> {
    NamedFile::open("dist/admin.html").ok()
}

#[post("/map", data = "<upload>")]
pub async fn new_map(
    pool: State<'_, darkredis::ConnectionPool>,
    upload: MapUploadRequest,
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
        laps_convert::import_png_as_mapdata(&mut conn, image.data)
            .await
            .expect("importing map data")
    };
    Ok(Json(map_id))
}

#[derive(FromForm)]
struct AdminLogin {
    username: String,
    password: String,
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
    async fn upload_map() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![new_map])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;

        //Remove all mapdata
        let mapdata_key = util::create_redis_key("mapdata");
        dbg!(&mapdata_key);
        conn.del(&mapdata_key).await.unwrap();

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
        let mut request = client.post("/map").header(ContentType::with_params(
            "multipart",
            "form-data",
            ("boundary", boundary.clone()),
        ));
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_json());
        assert_eq!(
            serde_json::from_slice::<u32>(&response.body_bytes().await.unwrap()).unwrap(),
            1
        );

        //And create another to ensure that it gets the correct ID.
        let mut request = client.post("/map").header(ContentType::with_params(
            "multipart",
            "form-data",
            ("boundary", boundary.clone()),
        ));
        request.set_body(form.as_slice());
        let mut response = request.dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_json());
        assert_eq!(
            serde_json::from_slice::<u32>(&response.body_bytes().await.unwrap()).unwrap(),
            2
        );
    }
}
