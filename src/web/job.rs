use crate::{
    types::{BackendError, JobResult, Vector},
    util,
};
use rocket::State;
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};

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
}

#[post("/job/submit", format = "json", data = "<job>")]
pub async fn submit(
    pool: State<'_, darkredis::ConnectionPool>,
    job: Json<JobSubmission>,
) -> Result<Json<JobResult>, BackendError> {
    let mut conn = pool.get().await;

    let key = conn
        .get(util::create_redis_backend_key("current_module"))
        .await?
        .unwrap();

    let info = JobInfo {
        job_id: 20,
        start: job.start,
        stop: job.end,
        map_id: 2,
    };
    debug!("Sending job: {:?}", info);
    conn.rpush(&key, serde_json::to_string(&info).unwrap())
        .await?;

    let result = conn
        .blpop(&[util::create_redis_backend_key("path-results")], 0)
        .await?
        .unwrap()
        .into_iter()
        .nth(1)
        .unwrap();

    debug!("Got response {}", String::from_utf8_lossy(&result));

    let deserialized: JobResult = serde_json::from_slice(&result).map_err(|e| {
        error!("Failed to parse job result: {}", &e);
        BackendError::InvalidResponse
    })?;

    Ok(Json(deserialized))
}
