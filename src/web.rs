use crate::{
    types::{self, JobError, WebError},
    util::create_redis_key,
};
use serde::Deserialize;
use std::convert::Infallible;
use warp::{
    http::{header::HeaderValue, Response, StatusCode},
    reply::Json,
    Filter, Rejection, Reply,
};

//Convenience enum for returning different types as a reply
enum WebReply {
    PNG(Vec<u8>),
}

impl warp::Reply for WebReply {
    fn into_response(self) -> warp::reply::Response {
        match self {
            Self::PNG(data) => {
                //Wish I could use the builder pattern here..
                let mut res = Response::new(data.into());
                res.headers_mut()
                    .insert("Content-Type", HeaderValue::from_static("image/png"));
                *res.status_mut() = StatusCode::OK;

                res
            }
        }
    }
}

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
        .map_err(warp::reject::custom)
}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let reply;
    if err.is_not_found() {
        reply = "not found".to_string();
        code = StatusCode::NOT_FOUND;
    } else if let Some(e) = err.find::<JobError>() {
        match e {
            JobError::Redis(e) => {
                error!("Redis error executing job: {}", e);
                reply = "server error".to_string();
                code = StatusCode::INTERNAL_SERVER_ERROR;
            }
            JobError::InvalidModule(_, _) | JobError::InvalidInput(_) => {
                reply = format!("{}", e);
                code = StatusCode::BAD_REQUEST;
            }
        }
    } else if let Some(e) = err.find::<WebError>() {
        match e {
            WebError::Redis(e) => {
                error!("Redis error in web: {}", e);
                reply = "server error".to_string();
                code = StatusCode::INTERNAL_SERVER_ERROR;
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
        reply = "server error".to_string();
        code = StatusCode::INTERNAL_SERVER_ERROR;
    }

    Ok(warp::reply::with_status(reply, code))
}

//Endpoint for getting map data
#[instrument]
async fn get_map(id: i32) -> Result<impl Reply, Rejection> {
    let mut conn = crate::REDIS_POOL.get().await;
    match conn
        .hget(&create_redis_key("mapdata"), &id.to_string())
        .await
        .map_err(|e| warp::reject::custom(WebError::Redis(e)))?
    {
        Some(data) => {
            trace!("Found map");
            Ok(WebReply::PNG(data))
        }
        None => {
            trace!("No map found");
            Err(warp::reject::not_found())
        }
    }
}

//Endpoint for listning available maps.
async fn get_maps() -> Result<Json, Rejection> {
    trace!("Listing maps");
    let mut conn = crate::REDIS_POOL.get().await;
    //Return an empty list if none are available
    let keys = conn
        .hkeys(&create_redis_key("mapdata"))
        .await
        .map_err(|e| warp::reject::custom(WebError::Redis(e)))?;

    //Convert each key to UTF-8, lossy in order to ignore errors
    let converted: Vec<std::borrow::Cow<'_, str>> =
        keys.iter().map(|s| String::from_utf8_lossy(&s)).collect();

    Ok(warp::reply::json(&serde_json::json!({ "maps": converted })))
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

    let get_map = warp::get().and(warp::path!("maps" / i32)).and_then(get_map);

    let get_maps = warp::get()
        .and(warp::path!("maps" / "available"))
        .and_then(get_maps);

    let routes = index
        .or(job_submission)
        .or(get_map)
        .or(get_maps)
        .recover(handle_rejection);

    let config = &crate::CONFIG.web;
    let address = std::net::SocketAddr::new(config.address, config.port);
    warp::serve(routes).run(address).await;
}
