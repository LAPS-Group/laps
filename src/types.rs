use crate::web::multipart::FormError;
use rocket::{
    http::Status,
    request::Request,
    response::{self, Responder},
    Response,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

//General vector type to be used internally
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct Vector {
    pub x: u32,
    pub y: u32,
}

//The outcome of a Job.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum JobOutcome {
    Success,
    Failure,
    Cancelled,
}
//Struct for storing the ouptut of a pathfinding job.
#[derive(Serialize, Deserialize, Debug)]
pub struct JobResult {
    //The ID of the job.
    pub job_id: i32,
    //The outcome of this job
    pub outcome: JobOutcome,
    //The list of points containing the path of the job.
    #[serde(default)]
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
        Docker(err: bollard::errors::Error) {
            from()
            display("Docker error: {}", err)
        }
        //A pathfinding module gave an incorrect response
        InvalidResponse {}
        //An IO error happened
        Io(err: std::io::Error) {
            from()
        }
        //Something wrong happened that can't be handled
        Other(msg: String) {
            display("Other error: {}", msg)
        }
    }
}

#[rocket::async_trait]
#[allow(clippy::needless_lifetimes)]
impl<'r> Responder<'r> for BackendError {
    async fn respond_to(self, _: &'r Request<'_>) -> response::Result<'r> {
        let error_message = Cursor::new("internal server error");
        error!("An internal error occurred: {}", self);
        Ok(Response::build()
            .status(Status::InternalServerError)
            .sized_body(error_message)
            .await
            .finalize())
    }
}

quick_error::quick_error! {
    ///Error type for errors we want to show the user.
    #[derive(Debug)]
    pub enum UserError {
        Internal(err: BackendError) {
            from()
            display("Internal server error")
        }
        BadType(got: String, allowed: String) {
            display("Invalid type \"{}\", expected one of {}", got, allowed)
        }
        BadForm(err: FormError) {
            from()
            display("Invalid form data: {}", err)
        }
        MapConvert(err: laps_convert::ConvertError) {
            from()
            display("Conversion error: {}", err)
        }
        ModuleImport(err: String) {
            display("Importing module image: {}", err)
        }
    }
}

#[rocket::async_trait]
#[allow(clippy::needless_lifetimes)]
impl<'r> Responder<'r> for UserError {
    async fn respond_to(self, request: &'r Request<'_>) -> response::Result<'r> {
        let message = std::io::Cursor::new(format!("{}", &self));
        let status_code = match self {
            UserError::Internal(e) => {
                return e.respond_to(request).await;
            }
            UserError::MapConvert(_) => Status::UnprocessableEntity,
            UserError::BadType(_, _) | UserError::BadForm(_) => Status::BadRequest,
            UserError::ModuleImport(_) => Status::BadRequest,
        };

        Ok(Response::build()
            .status(status_code)
            .sized_body(message)
            .await
            .finalize())
    }
}
