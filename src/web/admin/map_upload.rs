use crate::types::{BackendError, UserError};
use multipart::server::Multipart;
use rocket::{
    data::{Data, FromDataFuture, FromDataSimple, Outcome},
    http::Status,
    Request,
};
use std::io::Read;
use tokio::io::AsyncReadExt;

#[derive(Debug)]
pub struct MapUploadRequest {
    pub data: Vec<u8>,
}

impl FromDataSimple for MapUploadRequest {
    type Error = UserError;

    fn from_data(request: &Request, data: Data) -> FromDataFuture<'static, Self, Self::Error> {
        trace!("Parsing MapUploadRequest");
        //Validate Content-Type header
        let content_type = if let Some(t) = request
            .headers()
            .get_one("Content-Type")
            .map(|t| t.to_string())
        {
            t
        } else {
            trace!("Missing content type");
            return Box::pin(async move {
                Outcome::Failure((
                    Status::BadRequest,
                    UserError::BadForm("Missing Content-Type".to_string()),
                ))
            });
        };
        if !content_type.starts_with("multipart/form-data") {
            trace!("Not multipart");
            return Box::pin(async move {
                Outcome::Failure((
                    Status::BadRequest,
                    UserError::BadType(content_type, "[multipart/form-data]".into()),
                ))
            });
        }

        //Initilaize form struct
        let boundary_string = "boundary=";
        let i = content_type.find(boundary_string);
        if i.is_none() {
            trace!("Missing boundary");
            return Box::pin(async move {
                Outcome::Failure((
                    Status::BadRequest,
                    UserError::BadForm("Missing boundary".into()),
                ))
            });
        }

        Box::pin(async move {
            //Read the request data
            //WARNING: Assumes that there is a form size limit configured on the server!
            let mut stream = data.open();
            let mut request_data = Vec::new();
            match stream.read_to_end(&mut request_data).await {
                Ok(n) => trace!("Read {} bytes from multipart stream", n),
                Err(e) => {
                    error!("Error reading from multipart data stream: {}", e);
                    return Outcome::Failure((
                        Status::InternalServerError,
                        UserError::Internal(BackendError::Io(e)),
                    ));
                }
            };
            let boundary = &content_type[(i.unwrap() + boundary_string.len()..)];
            let mut form = Multipart::with_body(request_data.as_slice(), boundary);

            //Extract the data
            let mut data = None;
            //If any errors occur, put them here
            let mut error = None;
            let form_error = form
                .foreach_entry(|mut entry| match &*entry.headers.name {
                    "data" => {
                        //Already read this data, which is an error
                        if data.is_some() {
                            trace!("Got data twice!");
                            error = Some((
                                Status::BadRequest,
                                UserError::BadForm("Got data filed twice!".into()),
                            ));
                        } else {
                            let mut buffer = Vec::new();
                            match entry.data.read_to_end(&mut buffer) {
                                Ok(i) => {
                                    trace!("Read {} bytes from multipart form", i);
                                    data = Some(buffer);
                                }
                                Err(e) => {
                                    error!("Failed to read from multipart form: {}", e);
                                    error = Some((
                                        Status::InternalServerError,
                                        UserError::Internal(BackendError::Other(format!(
                                            "Reading from multipart form: {}",
                                            e
                                        ))),
                                    ));
                                }
                            }
                        }
                    }

                    _ => {
                        error = Some((
                            Status::BadRequest,
                            UserError::BadForm("Extraneous field".to_string()),
                        ));
                    }
                })
                .map_err(|e| {
                    error!("Error in multipart foreach_entry: {}", e);
                    (
                        Status::BadRequest,
                        UserError::BadForm("Unknown error".to_string()),
                    )
                });

            if let Some(e) = error {
                Outcome::Failure(e)
            } else if let Err(e) = form_error {
                Outcome::Failure(e)
            } else if let Some(data) = data {
                trace!("Successfully parsed MapUploadRequest");
                Outcome::Success(Self { data })
            } else {
                Outcome::Failure((
                    Status::BadRequest,
                    UserError::BadForm("Missing `data` field".to_string()),
                ))
            }
        })
    }
}
