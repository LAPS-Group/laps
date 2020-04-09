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
    let data = upload
        .get_file(&mime_consts::IMAGE_TIFF, "data")
        .ok_or_else(|| UserError::BadForm("Missing `data` field".into()))?;

    //Do a quick and dirty check that the file has the TIF image header
    if !has_valid_tiff_header(&data) {
        return Err(UserError::ModuleImport("Invalid Tiff header".into()));
    }

    //If we're in test mode, do not convert. We won't be testing the conversion here, just the endpoint.
    let map_id = if cfg!(test) {
        laps_convert::import_png_as_mapdata_test(&mut conn, data)
            .await
            .expect("importing fake mapdata")
    } else {
        //Put the map into a temporary file. Tokio::fs::File is stupidly slow and resource intensive, so
        //using the normal std::fs::File is much better.
        let image = tokio::task::spawn_blocking(move || {
            match tempfile::NamedTempFile::new()
                .map_err(|e| UserError::Internal(BackendError::Io(e)))
            {
                Ok(o) => {
                    let (mut file, path) = o.into_parts();
                    file.write_all(data.as_slice())
                        .expect("writing map data to temporary file");

                    laps_convert::create_normalized_png(path).map_err(UserError::MapConvert)
                }
                Err(e) => Err(e),
            }
        })
        .await
        .expect("spawn_blocking");

        let result = laps_convert::import_png_as_mapdata(&mut conn, image?.data)
            .await
            .expect("importing map data");

        info!(
            "Admin {} uploaded a new map with ID {}",
            session.username, result
        );
        result
    };
    Ok(Json(map_id))
}

#[delete("/map/<id>")]
pub async fn delete_map(
    pool: State<'_, ConnectionPool>,
    session: AdminSession,
    id: i32,
) -> Result<Status, BackendError> {
    //We're already authenticated, just get rid of the map in question.
    let mut conn = pool.get().await;
    let mapdata_key = util::create_redis_key("mapdata");
    let id = id.to_string();
    if conn.hdel(mapdata_key, &id).await? {
        info!("Map {} deleted by {}", id, session.username);
        Ok(Status::NoContent)
    } else {
        Ok(Status::NotFound)
    }
}
