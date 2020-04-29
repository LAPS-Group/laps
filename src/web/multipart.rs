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

quick_error::quick_error! {
    #[derive(Debug)]
    pub enum FormError {
        //Would borrow here but quick_error doesn't support generic types
        //The mime type for the field was incorrect.
        BadMime(field: String, got: String, expected: Mime) {
            display("Invalid MIME type for field '{}', expected '{}', got '{}'", field, expected, got)
        }
        //A file field was missing
        MissingFileField(field: String, mime: Mime) {
            display("Expected field '{}' with MIME type '{}'", field, mime)
        }
        //A text field was missing
        MissingText(field: String) {
            display("Missing text field '{}'", field)
        }
        //The Content-Type header of the request is not set properly
        MissingContentType {
            display("Invalid content type")
        }
        //The multipart form is missing a boundary
        MissingBoundary {
            display("Missing boundary")
        }
        //A field was given more than once
        DuplicateFields(field: String) {
            display("Duplicate field '{}'", field)
        }
        //A text field was not UTF-8
        InvalidUtf8(field: String) {
            display("Field '{}' is not valid UTF-8", field)
        }
    }
}

impl MultipartForm {
    pub fn get_file(&mut self, mime: &Mime, field: &str) -> Result<Vec<u8>, FormError> {
        if let Some(v) = self.files.get(field) {
            if &v.mime == mime {
                Ok(self.files.remove(field).unwrap().data)
            } else {
                Err(FormError::BadMime(
                    field.to_owned(),
                    v.mime.to_string(),
                    mime.clone(),
                ))
            }
        } else {
            Err(FormError::MissingFileField(field.to_owned(), mime.clone()))
        }
    }

    pub fn get_text(&mut self, field: &str) -> Result<String, FormError> {
        self.text
            .remove(field)
            .ok_or_else(|| FormError::MissingText(field.to_owned()))
    }
}

impl FromDataSimple for MultipartForm {
    type Error = UserError;

    fn from_data(request: &Request, data: Data) -> FromDataFuture<'static, Self, Self::Error> {
        trace!("Parsing multipart form");
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
                    UserError::BadForm(FormError::MissingContentType),
                ))
            });
        };
        if !content_type.starts_with("multipart/form-data") {
            trace!("Invalid content-type");
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
                    UserError::BadForm(FormError::MissingBoundary),
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

            //Unwrapping here is okay because we are reading directly from memory, and it therefore should never fail.
            while let Some(mut entry) = form.read_entry().expect("reading from memory") {
                let name = entry.headers.name.to_string();
                if files.contains_key(&name) || text.contains_key(&name) {
                    trace!("Received duplicate data");
                    return Outcome::Failure((
                        Status::BadRequest,
                        UserError::BadForm(FormError::DuplicateFields(name)),
                    ));
                }

                if entry.is_text() {
                    let mut buffer = Vec::new();
                    //unwrapping is still ok
                    entry.data.read_to_end(&mut buffer).unwrap();
                    match String::from_utf8(buffer) {
                        Ok(s) => {
                            trace!("Got text field {}={}", name, s);
                            text.insert(name, s)
                        }
                        Err(e) => {
                            trace!("Received invalid UTF-8: {}", e);
                            return Outcome::Failure((
                                Status::BadRequest,
                                UserError::BadForm(FormError::InvalidUtf8(name)),
                            ));
                        }
                    };
                } else if let Some(content_type) = entry.headers.content_type {
                    trace!("Got file field {}", name);
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
                        UserError::BadForm(FormError::MissingContentType),
                    ));
                }
            }

            Outcome::Success(MultipartForm { files, text })
        })
    }
}
