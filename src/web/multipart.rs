//File to "derive" a multipart form reader.

use crate::types::{BackendError, UserError};
use mime::Mime;
use multipart::server::Multipart;
use rocket::{
    data::{Data, FromDataFuture, FromDataSimple, Outcome},
    http::Status,
    Request,
};
use std::collections::HashMap;
use std::io::Read;
use tokio::io::AsyncReadExt;

pub struct MultipartFile {
    data: Vec<u8>,
    mime: Mime,
}

pub struct MultipartForm {
    files: HashMap<String, MultipartFile>,
    text: HashMap<String, String>,
}

impl MultipartForm {
    pub fn get_file(&mut self, mime: &Mime, field: &str) -> Option<Vec<u8>> {
        if let Some(v) = self.files.remove(field) {
            if &v.mime == mime {
                Some(v.data)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_text(&mut self, field: &str) -> Option<String> {
        self.text.remove(field)
    }
}

impl FromDataSimple for MultipartForm {
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
            let mut files = HashMap::new();
            let mut text = HashMap::new();
            //If any errors occur, put them here

            //Unwrapping here is okay because we are reading directly from memory, and it therefore should never fail.
            while let Some(mut entry) = form.read_entry().expect("reading from memory") {
                let name = entry.headers.name.to_string();
                if files.contains_key(&name) || text.contains_key(&name) {
                    trace!("Received duplicate data");
                    return Outcome::Failure((
                        Status::BadRequest,
                        UserError::BadForm("Form has duplicate fields".into()),
                    ));
                }

                if entry.is_text() {
                    let mut buffer = Vec::new();
                    //unwrapping is still ok
                    entry.data.read_to_end(&mut buffer).unwrap();
                    match String::from_utf8(buffer) {
                        Ok(s) => text.insert(name, s),
                        Err(e) => {
                            trace!("Received invalid UTF-8: {}", e);
                            return Outcome::Failure((
                                Status::BadRequest,
                                UserError::BadForm(format!("Field {} is not valid UTF-8", name)),
                            ));
                        }
                    };
                } else if let Some(content_type) = entry.headers.content_type {
                    let mut data = Vec::new();
                    //unwrapping is still ok
                    entry.data.read_to_end(&mut data).unwrap();
                    let file = MultipartFile {
                        mime: content_type,
                        data,
                    };
                    files.insert(name, file);
                } else {
                    return Outcome::Failure((
                        Status::BadRequest,
                        UserError::BadForm(format!("Missing content-type for field {}", name)),
                    ));
                }
            }

            Outcome::Success(MultipartForm { files, text })
        })
    }
}
