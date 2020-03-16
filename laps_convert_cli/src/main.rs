#[macro_use]
extern crate log;

use laps_convert::{ConvertError, ConvertedImage};
use std::{io::Write, path::PathBuf};
use structopt::StructOpt;
use tokio::io::AsyncWriteExt;

#[derive(StructOpt, Debug)]
struct Options {
    ///Import every image into a running LAPS system instead of outputting the converted
    ///PNGs to files.
    #[structopt(short, long)]
    import: bool,

    ///The directory to output the converted PNG files to. Ignored when importing data into the system.
    #[structopt(short, long, default_value = ".", parse(from_os_str))]
    output_dir: PathBuf,

    ///Redis instance to connect to when importing.
    #[structopt(short = "-H", long, default_value = "localhost:6379")]
    redis_host: String,

    ///Password to use when connecting to Redis.
    #[structopt(short = "-p", long)]
    redis_password: Option<String>,

    ///Database to use when imporpting mapdata.
    #[structopt(short = "-d", long)]
    redis_db: Option<u8>,

    ///GDAL compatible raster files to import.
    #[structopt(name = "INPUT", required = true, min_values = 1, parse(from_os_str))]
    files: Vec<PathBuf>,
}

fn convert_files(files: &[PathBuf]) -> Vec<Result<ConvertedImage, ConvertError>> {
    let mut out = Vec::new();
    for f in files {
        out.push(laps_convert::create_normalized_png(f))
    }
    out
}

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();
    let options = Options::from_args();

    if options.import {
        //Connect to Redis, optionally select the correct database
        debug!("Connecting to Redis..");
        let mut conn = if let Some(ref p) = options.redis_password {
            darkredis::Connection::connect_and_auth(&options.redis_host, p).await
        } else {
            darkredis::Connection::connect(&options.redis_host).await
        }
        .map_err(|e| format!("Failed to connect to Redis: {}", e))?;
        if let Some(db) = options.redis_db {
            let db = db.to_string();
            let command = darkredis::Command::new("SELECT").arg(&db);
            conn.run_command(command)
                .await
                .map_err(|e| format!("Failed to select database: {}", e))?;
        }

        //Perform the conversion and store the result
        let converted = convert_files(&options.files);
        for (index, image) in converted.into_iter().enumerate() {
            laps_convert::import_png_as_mapdata(
                &mut conn,
                image
                    .map_err(|e| {
                        format!(
                            "Failed to convert {}: {}",
                            options.files[index].as_os_str().to_string_lossy(),
                            e
                        )
                    })?
                    .data,
            )
            .await
            .unwrap();
        }
    } else {
        if options.output_dir.is_file() {
            return Err("output-dir must be a directory!".to_string());
        }
        //Create list of output file names
        let output_files: Vec<PathBuf> = options
            .files
            .clone()
            .into_iter()
            .map(|p| {
                //Convert a path like /path/to/file/file.tif into <output_dir>/file.png
                let stem = p.file_stem().unwrap();
                let mut buf = PathBuf::new();
                buf.push(&options.output_dir);
                buf.push(stem);
                buf.set_extension("png");
                buf
            })
            .collect();

        //Do the conversion and write the files to disk
        let converted = convert_files(&options.files);
        for (index, image) in converted.into_iter().enumerate() {
            let image = image.map_err(|e| {
                format!(
                    "Failed to convert file {}: {}",
                    options.files[index].as_os_str().to_string_lossy(),
                    e
                )
            })?;

            let mut file = tokio::fs::File::create(&output_files[index])
                .await
                .map_err(|e| format!("Failed to create file: {}", e))?;
            file.write_all(&image.data)
                .await
                .map_err(|e| format!("Couldn't write to file: {}", e))?;
        }
    }

    Ok(())
}
