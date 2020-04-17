use crate::{
    module_handling::ModuleInfo,
    types::{BackendError, Vector},
    util,
};
use futures::TryStreamExt;
use rand::RngCore;
use rocket::{
    http::{ContentType, Status},
    Response, State,
};
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

//The job message which gets sent to a pathfinding module.
#[derive(Serialize, Debug)]
struct JobInfo {
    job_id: i32,
    start: Vector,
    stop: Vector,
    map_id: i32,
}

//A job request from the frontend.
#[derive(Deserialize, Serialize)]
pub struct JobSubmission {
    start: Vector,
    stop: Vector,
    map_id: i32,
    algorithm: ModuleInfo,
}

impl JobSubmission {
    //Check if `self` is a valid job. Returns (isvalid, errormessage).
    pub async fn validity_check(
        &self,
        redis: &mut darkredis::Connection,
    ) -> Result<(bool, &'static str), BackendError> {
        //Check that the start and end points are not the same
        if self.start == self.stop {
            return Ok((false, "Start and end points are equal"));
        }

        //Check that the algorithm requested actually exists
        let modules = crate::module_handling::get_registered_modules(redis).await?;
        if !modules.contains(&self.algorithm) {
            return Ok((false, "Module does not exist"));
        }

        let mapdata_key = util::create_redis_key("mapdata");
        //Check that the requested map actually exists.
        if let Some(data) = redis.hget(mapdata_key, self.map_id.to_string()).await? {
            //Verify that the job is within the bounds of the map
            let decoder = png::Decoder::new(data.as_slice());

            let (info, _) = decoder
                .read_info()
                .map_err(|s| BackendError::Other(format!("PNG error: {}", s)))?;
            //No need to check if they're negative as the type only allows for u32.
            //Only check the biggest one
            let max_x = self.start.x.max(self.stop.x);
            let max_y = self.start.y.max(self.stop.y);
            let out = info.width > max_x && info.height > max_y;
            if out {
                Ok((true, ""))
            } else {
                Ok((false, "Points are out of bounds"))
            }
        } else {
            Ok((false, "Invalid map id"))
        }
    }
}

#[post("/job", format = "json", data = "<job>")]
pub async fn submit(
    pool: State<'_, darkredis::ConnectionPool>,
    job: Json<JobSubmission>,
) -> Result<Response<'_>, BackendError> {
    let mut conn = pool.get().await;

    //Try to find the job in the cache. If it is in the cache, we can assume that the job submission has been validated already.
    let cache_key = util::get_job_cache_key(&job);
    if let Some(v) = conn.get(&cache_key).await? {
        //Already cached, just return the job token we have stored instead of performing the job again.

        //Reset the time to live of the job mapping
        let job_timeout = crate::CONFIG.jobs.result_timeout.to_string();
        let job_mapping_key = util::get_job_mapping_key(&*String::from_utf8_lossy(&v));
        let mut commands = darkredis::CommandList::new("EXPIRE")
            .arg(&cache_key)
            .arg(&job_timeout)
            .command("EXPIRE")
            .arg(&job_mapping_key)
            .arg(&job_timeout);

        //Reset the time to live for the job key as well.
        //Bind job_key here to resolve a lifetime issue
        let job_key;
        if let Some(k) = conn.get(&job_mapping_key).await? {
            job_key = util::get_job_key(String::from_utf8_lossy(&k).parse().unwrap());
            commands = commands.command("EXPIRE").arg(&job_key).arg(&job_timeout);
        }

        conn.run_commands(commands)
            .await?
            .try_collect::<Vec<darkredis::Value>>()
            .await?;

        return Ok(Response::build()
            .status(Status::Accepted)
            .header(ContentType::Plain)
            .sized_body(Cursor::new(v))
            .await
            .finalize());
    }

    //Before we do anything, verify that the request is actually valid.
    match job.validity_check(&mut conn).await {
        Ok((true, _)) => (),
        Ok((false, msg)) => {
            return Ok(Response::build()
                .status(Status::BadRequest)
                .sized_body(std::io::Cursor::new(msg))
                .await
                .finalize())
        }
        Err(e) => {
            error!("Failed to check job validity {}", &e);
            return Err(e);
        }
    }
    //TODO Find a random job id
    let job_id = conn
        .incr(util::create_redis_backend_key("job_id"))
        .await
        .expect("getting job id");

    let key = util::get_module_key(&job.algorithm);

    let info = JobInfo {
        job_id: job_id as i32,
        start: job.start,
        stop: job.stop,
        map_id: job.map_id,
    };
    debug!("Sending job: {:?}", info);
    conn.rpush(&key, serde_json::to_string(&info).unwrap())
        .await?;

    //Job submitted, now generate a token the user can use to get the result
    let mut buffer = vec![0u8; 256];
    rand::thread_rng().fill_bytes(&mut buffer);
    let token = base64::encode_config(&buffer, base64::URL_SAFE_NO_PAD);

    //Create a mapping from user token to a job id
    let map_key = util::get_job_mapping_key(&token);
    conn.set_and_expire_seconds(
        map_key,
        job_id.to_string(),
        crate::CONFIG.jobs.token_timeout,
    )
    .await
    .unwrap();

    //Create a cache element such that the job is already in the cache.
    let token_clone = token.clone();
    conn.set_and_expire_seconds(cache_key, token_clone, crate::CONFIG.jobs.token_timeout)
        .await?;

    //All is good, do things
    let response = Response::build()
        .status(Status::Accepted)
        .header(ContentType::Plain)
        .sized_body(Cursor::new(token))
        .await
        .finalize();
    Ok(response)
}

//Typed connection pool for use with getting job results.
pub struct ResultConnectionPool(darkredis::ConnectionPool);

impl std::ops::Deref for ResultConnectionPool {
    type Target = darkredis::ConnectionPool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ResultConnectionPool {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

//Create Redis pool for use with the result polling
pub async fn create_result_redis_pool() -> ResultConnectionPool {
    let redis_conf = &crate::CONFIG.redis;
    info!("Creating result Redis pool at {}", redis_conf.address);

    let job_conf = &crate::CONFIG.jobs;
    //Use a couple more connections to be able to return 504 when completely congested
    let connection_count = job_conf.max_polling_clients + job_conf.additional_connections;
    let pool = darkredis::ConnectionPool::create(
        redis_conf.address.clone(),
        redis_conf.password.as_deref(),
        connection_count as usize,
    )
    .await;
    match pool {
        Ok(p) => {
            info!("Successfully connected to Redis!");
            ResultConnectionPool(p)
        }
        Err(e) => {
            error!("Failed to connect to Redis: {:?}", e);
            std::process::exit(1);
        }
    }
}

//Get the result of a pathfinding job
#[get("/job/<token>")]
pub async fn result(
    pool: State<'_, ResultConnectionPool>,
    token: String,
) -> Result<Response<'_>, BackendError> {
    let mut conn = pool.get().await;

    //Rate limit the number of clients
    //Is the number of clients too big?
    let rate_limit_key = util::create_redis_backend_key("job_poll_ratelimiter");
    //Is the number of polling clients too big?
    if conn.incr(&rate_limit_key).await.unwrap() > crate::CONFIG.jobs.max_polling_clients as isize {
        //Yes, send a 504
        conn.decr(rate_limit_key).await.unwrap();
        return Ok(Response::build()
            .status(Status::ServiceUnavailable)
            .finalize());
    }

    let key = util::get_job_mapping_key(&token);
    match conn.get(key).await {
        Ok(Some(k)) => {
            //Poll for a result on this job
            let job_id = String::from_utf8_lossy(&k).parse::<i32>().unwrap();

            match util::try_poll_job_result(&mut conn, job_id).await {
                Some(result) => {
                    //Decrement the rate-limiting key.
                    conn.decr(&rate_limit_key).await?;

                    //Cannot fail as it is the same value that gets deserialized in the results receiver
                    //Hide the job_id field from the user
                    let json =
                        Cursor::new(serde_json::json!({"points": result.points}).to_string());
                    let response = Response::build()
                        .status(Status::Ok)
                        .header(ContentType::JSON)
                        .sized_body(json)
                        .await
                        .finalize();

                    Ok(response)
                }
                //Not ready yet
                None => {
                    conn.decr(rate_limit_key).await.unwrap();
                    Ok(Response::build().status(Status::NoContent).finalize())
                }
            }
        }
        Ok(None) => {
            conn.decr(rate_limit_key).await.unwrap();
            Ok(Response::build().status(Status::NotFound).finalize())
        }
        Err(e) => {
            conn.decr(rate_limit_key).await.unwrap();
            Err(BackendError::Redis(e))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{module_handling::ModuleInfo, types::JobResult, util::create_redis_backend_key};
    use rocket::{http::Status, local::Client};
    use serial_test::serial;

    //Test that submitting and receiving of jobs works
    #[tokio::test]
    #[serial]
    async fn submission() {
        //setup
        //Need both the result_redis pool and the normal one for this test.
        let redis_result_pool = create_result_redis_pool().await;
        let redis_pool = crate::create_redis_pool().await;
        let mut conn = redis_pool.get().await;
        let rocket = rocket::ignite()
            .mount("/", routes![submit, result])
            .manage(redis_result_pool)
            .manage(redis_pool.clone());
        let client = Client::new(rocket).unwrap();
        crate::test::clear_redis(&mut conn).await;
        crate::test::insert_test_mapdata(&mut conn).await;

        //Add a fake algorithm
        let algorithm_key = create_redis_backend_key("registered_modules");
        let algorithm = ModuleInfo {
            name: "dummy".to_string(),
            version: "0.0.0".to_string(),
        };
        let json = serde_json::to_vec(&algorithm).unwrap();
        conn.sadd(algorithm_key, json).await.unwrap();

        //Submit a dummy job with an algorithm that doesn't exist
        let fake_algorithm = ModuleInfo {
            name: "does-not-exist".to_string(),
            version: "0.0.0".to_string(),
        };
        let mut job = serde_json::json!({
            "map_id": 1,
            "start": {
                "x": 1, "y": 2
            },
            "stop": {
                "x": 2, "y": 1
            },
            "algorithm": fake_algorithm
        });
        let response = client
            .post("/job")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::BadRequest);

        //Submit a job with an algorithm that actually exists
        job["algorithm"] = serde_json::json!(algorithm);
        let mut response = client
            .post("/job")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Accepted);
        let token = response.body_string().await.unwrap();

        //Try using a fake token, tokens are never this small so it will never be correct
        let fake_token = "256";
        let uri = format!("/job/{}", fake_token);
        let response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::NotFound);

        //Use the real token, but the job times out:
        let uri = format!("/job/{}", token);
        let response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::NoContent);

        //Complete the job. Because we cleared the job id counter earlier, the job id is guaranteed to be 1.
        let job_id = 1;
        let info = JobResult {
            job_id,
            points: vec![Vector { x: 0, y: 0 }, Vector { x: 0, y: 0 }],
        };
        let key = util::get_job_key(job_id);
        conn.set(key, serde_json::to_vec(&info).unwrap())
            .await
            .unwrap();

        //Get the data again using the real token, and this time it should actually exist:
        let uri = format!("/job/{}", token);
        let mut response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        let body = response.body_string().await.unwrap();
        let points: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            points,
            serde_json::json!({
                "points": [
                    { "x": 0, "y": 0 },
                    { "x": 0, "y": 0 },
                ]
            })
        );
    }

    #[tokio::test]
    #[serial]
    async fn rate_limiting() {
        //setup
        let redis_pool = crate::create_redis_pool().await;
        let redis_result_pool = create_result_redis_pool().await;
        let mut conn = redis_pool.get().await;
        let rocket = rocket::ignite()
            .mount("/", routes![result])
            .manage(redis_result_pool)
            .manage(redis_pool.clone());
        let client = Client::new(rocket).unwrap();
        crate::test::clear_redis(&mut conn).await;

        //Simulate too many clients connecting at once
        let max_clients = crate::CONFIG.jobs.max_polling_clients;
        let rate_limit_key = create_redis_backend_key("job_poll_ratelimiter");
        conn.set(&rate_limit_key, max_clients.to_string())
            .await
            .unwrap();

        //Verify that it denies us. Token does not matter.
        let response = client.get("/job/256").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);

        //Make room for another client
        conn.decr(rate_limit_key).await.unwrap();

        //Verify that we are now accepted but that there's no job with this token.
        let response = client.get("/job/256").dispatch().await;
        assert_eq!(response.status(), Status::NotFound);
    }

    //Test that we avoid unnecesarry calculations of the same job.
    #[tokio::test]
    #[serial]
    async fn job_cache() {
        //setup
        let redis_pool = crate::create_redis_pool().await;
        let mut conn = redis_pool.get().await;
        let rocket = rocket::ignite()
            .mount("/", routes![submit])
            .manage(redis_pool.clone());
        let client = Client::new(rocket).unwrap();
        crate::test::clear_redis(&mut conn).await;
        crate::test::insert_test_mapdata(&mut conn).await;

        //Register a fake module
        let algorithm_key = create_redis_backend_key("registered_modules");
        let algorithm = ModuleInfo {
            name: "dummy".to_string(),
            version: "0.0.0".to_string(),
        };
        let json = serde_json::to_vec(&algorithm).unwrap();
        conn.sadd(algorithm_key, json).await.unwrap();

        //Submit a job
        let job = serde_json::json!({
          "map_id": 1,
          "start": {
              "x": 1, "y": 2
          },
          "stop": {
              "x": 2, "y": 1
          },
          "algorithm": algorithm
        });
        let mut response = client
            .post("/job")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Accepted);
        let first_token = response.body_bytes().await.unwrap();

        //Submit the job again and verify that it maps to the same token
        let mut response = client
            .post("/job")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Accepted);
        assert_eq!(response.body_bytes().await.unwrap(), first_token);

        //Submit a new job and verify that it actually sends it.
        let job = serde_json::json!({
            "map_id": 1,
            "start": {
                "x": 2, "y": 1
            },
            "stop": {
                "x": 1, "y": 2
            },
            "algorithm": algorithm
        });
        let mut response = client
            .post("/job")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Accepted);
        assert_ne!(response.body_bytes().await.unwrap(), first_token);
    }

    #[tokio::test]
    #[serial]
    async fn job_validation() {
        //Setup
        let redis_pool = crate::create_redis_pool().await;
        let mut redis = redis_pool.get().await;
        crate::test::clear_redis(&mut redis).await;

        //Insert test mapdata
        let (width, height) = crate::test::insert_test_mapdata(&mut redis).await;

        //Insert a module
        let algorithm_key = create_redis_backend_key("registered_modules");
        let algorithm = ModuleInfo {
            name: "dummy".to_string(),
            version: "0.0.0".to_string(),
        };
        let json = serde_json::to_vec(&algorithm).unwrap();
        redis.sadd(algorithm_key, json).await.unwrap();

        let mut job_submission = JobSubmission {
            start: Vector { x: 0, y: 100 },
            stop: Vector { x: 0, y: 100 },
            map_id: 1,
            algorithm,
        };

        macro_rules! check_valid {
            () => {
                assert!(job_submission.validity_check(&mut redis).await.unwrap().0);
            };
        }
        macro_rules! check_invalid {
            () => {
                assert!(!job_submission.validity_check(&mut redis).await.unwrap().0);
            };
        }

        //Equal start and stop points
        check_invalid!();
        job_submission.stop.y = 50;

        //Map Id is valid
        check_valid!();

        //Invalid module
        job_submission.algorithm.version = "0.1.0".to_string();
        check_invalid!();

        //Invalid Map ID
        job_submission.map_id = 2;
        job_submission.algorithm.version = "0.0.0".to_string();
        check_invalid!();

        //Out of bounds
        job_submission.map_id = 1;
        check_valid!(); //Check that it's ok again
        job_submission.start.x = width + 200;
        check_invalid!();
        job_submission.start.x = 0;
        check_valid!(); //Check that it's ok again
        job_submission.start.y = height + 300;
        check_invalid!();
        job_submission.start.y = 0;
        check_valid!(); //Check that it's ok again

        //Out of bounds, but this time for the stop point
        job_submission.stop.x = width + 200;
        check_invalid!();
        job_submission.stop.x = 0;
        check_valid!(); //Check that it's ok again
        job_submission.stop.y = height + 300;
        check_invalid!();
    }
}
