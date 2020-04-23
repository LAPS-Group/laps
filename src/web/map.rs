use crate::util::create_redis_key;
use rocket::{http::ContentType, Response, State};
use rocket_contrib::{json, json::JsonValue};

//Endpoint for getting map data
#[get("/map/<id>")]
pub async fn get_map(pool: State<'_, darkredis::ConnectionPool>, id: i32) -> Option<Response<'_>> {
    let mut conn = pool.get().await;
    match conn
        .hget(&create_redis_key("mapdata"), &id.to_string())
        .await
        .unwrap()
    {
        Some(data) => {
            trace!("Found map");
            let response = Response::build()
                .header(ContentType::from_extension("png").unwrap())
                .sized_body(std::io::Cursor::new(data))
                .await
                .finalize();

            Some(response)
        }
        None => {
            trace!("No map found");
            None
        }
    }
}

//Endpoint for listning available maps.
#[get("/maps")]
pub async fn get_maps(pool: State<'_, darkredis::ConnectionPool>) -> JsonValue {
    let mut conn = pool.get().await;
    trace!("Listing maps");
    //Return an empty list if none are available
    let keys = conn.hkeys(&create_redis_key("mapdata")).await.unwrap();

    //Convert each key to UTF-8, lossy in order to ignore errors
    let converted: Vec<std::borrow::Cow<'_, str>> =
        keys.iter().map(|s| String::from_utf8_lossy(&s)).collect();

    json!({ "maps": converted })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util;
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

        //Set dummy map data
        conn.hset(create_redis_key("mapdata"), "1", "FOO")
            .await
            .unwrap();

        //Verify that the new map is now there
        let mut response = client.get("/maps").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        //Verify that the number of maps is zero.
        let expected = r#"{"maps":["1"]}"#.to_string();
        assert_eq!(response.body_string().await, Some(expected));

        //Finally, ensure that we can get the map back
        let mut response = client.get("/map/1").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert!(response.content_type().unwrap().is_png());
        assert_eq!(response.body_string().await, Some("FOO".into()));
    }
}
