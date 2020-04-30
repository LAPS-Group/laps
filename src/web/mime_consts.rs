//Define a bunch of mime types as the version of mime used by multipart does not export such constants itself.
lazy_static::lazy_static! {
    pub static ref X_TAR: mime::Mime = "application/x-tar".parse().unwrap();
    pub static ref X_TAR_GZ: mime::Mime = "application/x-tar+gz".parse().unwrap();
    pub static ref IMAGE_PNG: mime::Mime = "image/png".parse().unwrap();
    pub static ref IMAGE_TIFF: mime::Mime = "image/tiff".parse().unwrap();
}
