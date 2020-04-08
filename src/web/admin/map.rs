use super::mime_consts;
use super::AdminSession;
use crate::{
    types::{BackendError, UserError},
    util,
    web::multipart::MultipartForm,
};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use darkredis::ConnectionPool;
use rocket::{http::Status, request::State};
use rocket_contrib::json::Json;
use std::io::Write;

fn has_valid_tiff_header(input: &[u8]) -> bool {
    //Instead of verifying everything in the TIFF file to be valid, just check if the TIFF header is there.
    //If the image is actually invalid this will be detected by GDAL further down the pipeline.
    //Header length is 8 bytes
    if input.len() < 8 {
        false
    } else {
        let buffer = &input[..8];
        //Intel(little endian) or Motorola(big endian) file?
        let little_endian = match &buffer[..2] {
            //Intel byte order
            b"II" => true,
            //Motorola
            b"MM" => false,
            //If neither the file is invalid
            _ => return false,
        };

        let version = if little_endian {
            (&buffer[2..]).read_u16::<LittleEndian>()
        } else {
            (&buffer[2..]).read_u16::<BigEndian>()
        };

        //Version check
        version.map(|v| v == 42).unwrap_or(false)
    }
}

#[post("/map", data = "<upload>")]
pub async fn new_map(
    pool: State<'_, ConnectionPool>,
    mut upload: MultipartForm,
    session: AdminSession,
) -> Result<Json<u32>, UserError> {
    let mut conn = pool.get().await;
    let data = upload.get_file(&mime_consts::IMAGE_TIFF, "data")?;

    //Do a quick and dirty check that the file has the TIF image header
    if !has_valid_tiff_header(&data) {
        return Err(UserError::ModuleImport("Invalid Tiff header".into()));
    }

    //Put the map into a temporary file. This is needed because GDAL does not allow us to give it a buffer, it has
    //to be put into some sort of file, which is reflected in the laps_convert API.
    //Using blocking IO for this because it is a literally a thousand times faster than tokio::fs::File.
    let (image, metadata) =
        tokio::task::spawn_blocking(move || {
            match tempfile::NamedTempFile::new()
                .map_err(|e| UserError::Internal(BackendError::Io(e)))
            {
                Ok(o) => {
                    let (mut file, path) = o.into_parts();
                    file.write_all(data.as_slice())
                        .expect("writing map data to temporary file");

                    laps_convert::convert_to_png(path).map_err(UserError::MapConvert)
                }
                Err(e) => Err(e),
            }
        })
        .await
        .expect("spawn_blocking")?;

    //Use the proper testing keys in test mode
    let result = if cfg!(test) {
        laps_convert::import_data_test(&mut conn, image, metadata)
            .await
            .expect("importing map data")
    } else {
        laps_convert::import_data(&mut conn, image, metadata)
            .await
            .expect("importing map data")
    };

    info!(
        "Admin {} uploaded a new map with ID {}",
        session.username, result
    );

    Ok(Json(result))
}

#[delete("/map/<id>")]
pub async fn delete_map(
    pool: State<'_, ConnectionPool>,
    session: AdminSession,
    id: i32,
) -> Result<Status, BackendError> {
    //We're already authenticated, just get rid of the map in question.
    let mut conn = pool.get().await;
    let image_key = util::create_redis_key("mapdata.image");
    let meta_key = util::create_redis_key("mapdata.meta");
    let id = id.to_string();
    if conn.hdel(image_key, &id).await? {
        //Don't really care what the result of this is
        let _ = conn.hdel(meta_key, &id).await?;
        info!("Map {} deleted by {}", id, session.username);
        Ok(Status::NoContent)
    } else {
        Ok(Status::NotFound)
    }
}
