use crate::{module_handling::ModuleInfo, types::BackendError};
use darkredis::ConnectionPool;
use rocket::State;
use rocket_contrib::json::Json;

//Get a list of available algorithms
#[get("/algorithms")]
pub async fn list(pool: State<'_, ConnectionPool>) -> Result<Json<Vec<ModuleInfo>>, BackendError> {
    let mut conn = pool.get().await;
    let modules = crate::module_handling::get_registered_modules(&mut conn).await?;
    Ok(Json(modules))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::{self, create_redis_backend_key};
    use rocket::{http::Status, local::Client};

    //Test the listing of algorithms
    #[tokio::test]
    async fn list() {
        //Setup rocket instance
        let redis = crate::create_redis_pool().await;
        let rocket = rocket::ignite()
            .mount("/", routes![list])
            .manage(redis.clone());
        let client = Client::new(rocket).unwrap();
        let mut conn = redis.get().await;
        util::clear_redis(&mut conn).await;

        //Macro to make this test easier to read
        macro_rules! check {
            ($result:expr) => {
                let mut response = client.get("/algorithms").dispatch().await;
                assert_eq!(response.status(), Status::Ok);
                assert!(response.content_type().unwrap().is_json());
                let modules: Vec<ModuleInfo> =
                    serde_json::from_slice(&response.body_bytes().await.unwrap()).unwrap();

                //Redis does not specify which order a set value will be in. We have to verify
                //that everything in $result is in modules instead of checking them for equality.
                assert_eq!(modules.len(), $result.len());
                for m in modules {
                    assert!($result.contains(&m));
                }
            };
        }

        //No algorithms around
        check!(Vec::<ModuleInfo>::new());

        // Add a dummy algorithm
        let dummy = ModuleInfo {
            name: "dummy".to_string(),
            version: "0".to_string(),
        };
        let module_key = create_redis_backend_key("registered_modules");
        conn.sadd(&module_key, &serde_json::to_vec(&dummy).unwrap())
            .await
            .unwrap();

        //Only one module
        check!(vec![dummy.clone()]);

        // And another...
        let second_dummy = ModuleInfo {
            name: "dummy".to_string(),
            version: "1".to_string(),
        };
        conn.sadd(&module_key, &serde_json::to_vec(&second_dummy).unwrap())
            .await
            .unwrap();

        check!(vec![dummy.clone(), second_dummy.clone()]);
    }
}
