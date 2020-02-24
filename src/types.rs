use rocket::{
    http::Status,
    request::Request,
    response::{self, Responder},
    Response,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
//General types to be used throughout the application

///General vector type to be used internally
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

///Struct for storing the ouptut of a pathfinding job.
#[derive(Serialize, Deserialize, Debug)]
pub struct JobResult {
    pub job_id: i32,
    pub points: Vec<Vector>,
}

quick_error::quick_error! {
    ///General backend error type. Should not be shown to the user
    #[derive(Debug)]
    pub enum BackendError {
        Redis(err: darkredis::Error) {
            from()
            display("Redis error: {}", err)
        }
        JsonError(err: serde_json::Error) {
            from()
            display("JSON error: {}", err)
        }
        //A pathfinding module gave an incorrect response
        InvalidResponse {}
        //Something wrong happened that can't be handled
        Other(msg: String) {
            display("Other error: {}", msg)
        }
    }
}

#[rocket::async_trait]
impl<'r> Responder<'r> for BackendError {
    async fn respond_to(self, _: &'r Request<'_>) -> response::Result<'r> {
        let error_message = Cursor::new("internal server error");
        Ok(Response::build()
            .status(Status::InternalServerError)
            .sized_body(error_message)
            .await
            .finalize())
    }
}
