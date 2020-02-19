use crate::types::{self, JobError};
use serde::Deserialize;
use std::convert::Infallible;
use warp::{http::StatusCode, reply::Json, Filter, Rejection, Reply};

//Sent from Frontend
#[derive(Deserialize)]
struct PathfindingJob {
    //TODO Add name and version fields when adding pathfinding module selection
    // name: String,
    // version: String,
    start: types::Vector,
    end: types::Vector,
}

async fn submit_job(job: PathfindingJob) -> Result<Json, Rejection> {
    crate::module_handling::execute_job(job.start, job.end)
        .await
        .map(|s| warp::reply::json(&serde_json::json!({ "points": s.points })))
        .map_err(|e| warp::reject::custom(e))
}

#[instrument(ignore(err))]
async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let reply;
    if err.is_not_found() {
        reply = format!("not found");
        code = StatusCode::NOT_FOUND;
    } else if let Some(e) = err.find::<JobError>() {
        match e {
            JobError::Redis(e) => {
                error!("Redis error executing job: {}", e);
                reply = format!("server error");
                code = StatusCode::INTERNAL_SERVER_ERROR;
            }
            JobError::InvalidModule(_, _) | JobError::InvalidInput(_) => {
                reply = format!("{}", e);
                code = StatusCode::BAD_REQUEST;
            }
        }
    } else if let Some(e) = err.find::<warp::filters::body::BodyDeserializeError>() {
        reply = format!("bad request: {}", e);
        code = StatusCode::BAD_REQUEST;
    } else if let Some(e) = err.find::<warp::reject::MethodNotAllowed>() {
        reply = format!("method not allowed: {}", e);
        debug!("Method not allowed: {:?}", e);
        code = StatusCode::METHOD_NOT_ALLOWED;
    } else {
        error!("Unknown rejection: {:?}", err);
        reply = format!("server error");
        code = StatusCode::INTERNAL_SERVER_ERROR;
    }

    Ok(warp::reply::with_status(reply, code))
}

pub async fn run() {
    //TODO: Return 404 when not found

    let index = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./dist/index.html"))
        .or(warp::get().and(warp::path("main.js").and(warp::fs::file("./dist/main.js"))));

    let job_submission = warp::post()
        .and(warp::path!("job" / "submit"))
        .and(warp::body::content_length_limit(128))
        .and(warp::body::json())
        .and_then(submit_job);

    let routes = index.or(job_submission).recover(handle_rejection);

    let config = &crate::CONFIG.web;
    let address = std::net::SocketAddr::new(config.address, config.port);
    warp::serve(routes).run(address).await;
}
