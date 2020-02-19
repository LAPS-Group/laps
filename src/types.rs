use serde::{Deserialize, Serialize};
//General types to be used throughout the application

///General vector type to be used internally
#[derive(Serialize, Deserialize, Debug)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

///Struct for storing the ouptut of a pathfinding job
#[derive(Serialize, Deserialize, Debug)]
pub struct JobResult {
    pub job_id: i32,
    pub points: Vec<Vector>,
}

quick_error::quick_error! {
    ///Error type for working with jobs
    #[derive(Debug)]
    pub enum JobError {
        Redis(err: darkredis::Error) {
            from()
            display("Redis error: {}", err)
        }
        InvalidModule(name: String, version: String) {
            display("Module {} v{} is invalid", name, version)
        }
        InvalidInput(message: String) {
            display("Malformed request: {}", message)
        }
    }
}

impl warp::reject::Reject for JobError {}

quick_error::quick_error! {
    ///General web error type
    #[derive(Debug)]
    pub enum WebError {
        Redis(err: darkredis::Error) {
            from()
            display("Redis error: {}", err)
        }
    }

}

impl warp::reject::Reject for WebError {}
