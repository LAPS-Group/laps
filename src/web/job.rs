use crate::{
    module_handling::ModuleInfo,
    types::{BackendError, JobResult, Vector},
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
#[derive(Deserialize)]
pub struct JobSubmission {
    start: Vector,
    end: Vector,
    map_id: i32,
    algorithm: ModuleInfo,
}

#[post("/job/submit", format = "json", data = "<job>")]
pub async fn submit(
    pool: State<'_, darkredis::ConnectionPool>,
    job: Json<JobSubmission>,
) -> Result<Response<'_>, BackendError> {
    let mut conn = pool.get().await;

    //Does this pathfinding module exist?
    let modules = crate::module_handling::get_registered_modules(&mut conn).await?;
    if !modules.contains(&job.algorithm) {
        //No, send a 404
        return Ok(Response::build()
            .status(Status::NotFound)
            .sized_body(Cursor::new("No such module"))
            .await
            .finalize());
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
        stop: job.end,
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
    let span = info_span!("result_redis");
    let _guard = span.enter();

    let redis_conf = &crate::CONFIG.redis;
    let address = std::net::SocketAddr::new(redis_conf.address, redis_conf.port);
    info!("Creating result Redis pool at {}", address);

    let job_conf = &crate::CONFIG.jobs;
    //Use a couple more connections to be able to return 504 when completely congested
    let connection_count = job_conf.max_polling_clients + job_conf.additional_connections;
    let pool = darkredis::ConnectionPool::create(
        address.to_string(),
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

//Wrapper struct to rate-limit the number of polling clients at once

#[get("/job/result/<token>")]
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
            //TODO: Find a different solution which scales properly
            //TODO: Use separate connection pool to make DoS'ing harder
            let job_id = String::from_utf8_lossy(&k).parse::<i32>().unwrap();
            let job_key = util::get_job_key(job_id);
            match conn
                .blpop(&[&job_key], crate::CONFIG.jobs.poll_timeout)
                .await?
            {
                Some(v) => {
                    //Delete the job now that it's been received, and decrement the rate-limiting key.
                    let commands = darkredis::CommandList::new("DEL")
                        .arg(&job_key)
                        .command("DECR")
                        .arg(&rate_limit_key);
                    conn.run_commands(commands)
                        .await
                        .unwrap()
                        .try_collect::<Vec<darkredis::Value>>()
                        .await
                        .unwrap();

                    let value = v.into_iter().nth(1).unwrap();
                    //Cannot fail as it is the same value that gets deserialized in the results receiver
                    let deserialized: JobResult = serde_json::from_slice(&value).unwrap();

                    //Hide the job_id field from the user
                    let json =
                        Cursor::new(serde_json::json!({"points": deserialized.points}).to_string());
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
    use crate::{module_handling::ModuleInfo, util::create_redis_backend_key};
    use rocket::{http::Status, local::Client};

    //Test that submitting and receiving of jobs works
    #[tokio::test]
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

        //Remove all algorithms, add a fake one
        let algorithm_key = create_redis_backend_key("registered_modules");
        conn.del(&algorithm_key).await.unwrap();
        let algorithm = ModuleInfo {
            name: "dummy".to_string(),
            version: "0.0.0".to_string(),
        };
        let json = serde_json::to_vec(&algorithm).unwrap();
        conn.sadd(algorithm_key, json).await.unwrap();
        //Clear ratelimiter
        conn.del(create_redis_backend_key("job_poll_ratelimiter"))
            .await
            .unwrap();

        //Reset the job_id counter to ensure a generated job has ID 1
        conn.del(util::create_redis_backend_key("job_id"))
            .await
            .unwrap();

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
            "end": {
                "x": 1, "y": 2
            },
            "algorithm": fake_algorithm
        });
        let response = client
            .post("/job/submit")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::NotFound);

        //Submit a job with an algorithm that actually exists
        job["algorithm"] = serde_json::json!(algorithm);
        let mut response = client
            .post("/job/submit")
            .header(ContentType::JSON)
            .body(&serde_json::to_vec(&job).unwrap())
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Accepted);
        let token = response.body_string().await.unwrap();

        //Try using a fake token, tokens are never this small so it will never be correct
        let fake_token = "256";
        let uri = format!("/job/result/{}", fake_token);
        let response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::NotFound);

        //Use the real token, but the job times out:
        let uri = format!("/job/result/{}", token);
        let response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::NoContent);

        //Complete the job. Because we cleared the job id counter earlier, the job id is guaranteed to be 1.
        let job_id = 1;
        let info = JobResult {
            job_id,
            points: vec![Vector { x: 0.0, y: 0.0 }, Vector { x: 0.0, y: 0.0 }],
        };
        let key = util::get_job_key(job_id);
        conn.lpush(key, serde_json::to_vec(&info).unwrap())
            .await
            .unwrap();

        //Get the data again using the real token, and this time it should actually exist:
        let uri = format!("/job/result/{}", token);
        let mut response = client.get(&uri).dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        let body = response.body_string().await.unwrap();
        let points: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            points,
            serde_json::json!({
                "points": [
                    { "x": 0.0, "y": 0.0 },
                    { "x": 0.0, "y": 0.0 },
                ]
            })
        );
    }

    #[tokio::test]
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

        //Simulate too many clients connecting at once
        let max_clients = crate::CONFIG.jobs.max_polling_clients;
        let rate_limit_key = create_redis_backend_key("job_poll_ratelimiter");
        conn.set(&rate_limit_key, max_clients.to_string())
            .await
            .unwrap();

        //Verify that it denies us. Token does not matter.
        let response = client.get("/job/result/256").dispatch().await;
        assert_eq!(response.status(), Status::ServiceUnavailable);

        //Make room for another client
        conn.decr(rate_limit_key).await.unwrap();

        //Verify that we are now accepted but that there's no job with this token.
        let response = client.get("/job/result/256").dispatch().await;
        assert_eq!(response.status(), Status::NotFound);
    }
}
