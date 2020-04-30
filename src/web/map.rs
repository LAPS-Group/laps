use crate::{types::BackendError, util::create_redis_key};
use rocket::{http::ContentType, Response, State};
use rocket_contrib::{json, json::JsonValue};
use std::io::Cursor;

//Endpoint for getting map data
#[get("/map/<id>")]
pub async fn get_map(
    pool: State<'_, darkredis::ConnectionPool>,
    id: i32,
) -> Result<Option<Response<'_>>, BackendError> {
    let mut conn = pool.get().await;
    match conn
        .hget(&create_redis_key("mapdata.image"), &id.to_string())
        .await?
    {
        Some(data) => {
            trace!("Found map");
            let response = Response::build()
                .header(ContentType::from_extension("png").unwrap())
                .sized_body(Cursor::new(data))
                .await
                .finalize();

            Ok(Some(response))
        }
        None => {
            trace!("No map found");
            Ok(None)
        }
    }
}

//Endpoint for listning available maps.
#[get("/maps")]
pub async fn get_maps(pool: State<'_, darkredis::ConnectionPool>) -> JsonValue {
    let mut conn = pool.get().await;
    trace!("Listing maps");
    //Return an empty list if none are available
    let keys = conn
        .hkeys(&create_redis_key("mapdata.image"))
        .await
        .unwrap();

    //Convert each key to UTF-8, lossy in order to ignore errors
    let converted: Vec<std::borrow::Cow<'_, str>> =
        keys.iter().map(|s| String::from_utf8_lossy(&s)).collect();

    json!({ "maps": converted })
}

#[get("/map/<id>/meta")]
pub async fn get_map_metadata(
    pool: State<'_, darkredis::ConnectionPool>,
    id: String,
) -> Result<Option<Response<'_>>, BackendError> {
    let mut conn = pool.get().await;
    let key = create_redis_key("mapdata.meta");
    match conn.hget(&key, id).await? {
        Some(s) => Ok(Some(
            Response::build()
                .header(ContentType::JSON)
                .sized_body(Cursor::new(s))
                .await
                .finalize(),
        )),
        None => Ok(None),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use laps_convert::ImageMetadata;
    use rocket::{http::Status, local::Client};
    use serial_test::serial;

    //Test the listing of available maps and getting of map data
    #[tokio::test]
    #[serial]
    async fn get_maps() {
        // Test setup
        let redis = crate::create_redis_pool().await;
        let mut conn = redis.get().await;
        let rocket = rocket::ignite()
            .mount("/", routes![get_map, get_maps])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        crate::test::clear_redis(&mut conn).await;

        //Verify that there is no registered map data at this time.
        let mut response = client.get("/maps").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        let expected = r#"{"maps":[]}"#.to_string();
        assert_eq!(response.body_string().await, Some(expected));

        //Insert testing mapdata
        crate::test::insert_test_mapdata(&mut conn).await;

        //Verify that the new map is now there
        let mut response = client.get("/maps").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        //Verify that the number of maps is one.
        let expected = r#"{"maps":["1"]}"#.to_string();
        assert_eq!(response.body_string().await, Some(expected));

        //Finally, ensure that we can get the map back
        let mut response = client.get("/map/1").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_png());
        //Check for PNG header to verify that we didn't accidentally receive text
        assert_eq!(
            &response.body_bytes().await.unwrap()[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );

        //Insert a new one and ensure that it's there
        crate::test::insert_test_mapdata(&mut conn).await;
        let mut response = client.get("/maps").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        //Verify that each element is there. We can't just compare the expected output as a string with the result
        //because Redis can return the keys in any order. So instead just check that each of the expected maps
        //are actually in the array.
        let expected = vec!["1", "2"];
        let value: serde_json::Value =
            serde_json::from_slice(&response.body_bytes().await.unwrap()).unwrap();
        assert_eq!(expected.len(), value["maps"].as_array().unwrap().len());
        for map in value["maps"].as_array().unwrap() {
            assert!(expected.contains(&map.as_str().unwrap()));
        }

        let mut response = client.get("/map/2").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_png());
        //Check for PNG header to verify that we didn't accidentally receive text
        assert_eq!(
            &response.body_bytes().await.unwrap()[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }

    #[tokio::test]
    #[serial]
    async fn get_map_metadata() {
        // Test setup
        let redis = crate::create_redis_pool().await;
        let mut conn = redis.get().await;
        let rocket = rocket::ignite()
            .mount("/", routes![get_map_metadata])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        crate::test::clear_redis(&mut conn).await;

        //Insert the test data
        crate::test::insert_test_mapdata(&mut conn).await;

        let mut response = client.get("/map/1/meta").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.content_type(), Some(ContentType::JSON));
        let metadata: ImageMetadata =
            serde_json::from_slice(&response.body_bytes().await.unwrap()).unwrap();
        //Map data has an x_res of 1.
        approx::assert_relative_eq!(metadata.x_res, 1.0);
    }
}
