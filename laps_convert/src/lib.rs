#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

//!Library for handling map data conversions in LAPS.

#[macro_use]
extern crate log;

use gdal::raster::Dataset;
use quick_error::quick_error;
use serde::{Deserialize, Serialize};
use std::fmt;

quick_error! {
    #[derive(Debug)]
    ///Error type for conversion
    pub enum ConvertError {
        ///An error occured while using the GDAL library.
        GDal(err: gdal::errors::Error) {
            from()
            display("Gdal error: {}", err)
        }
        ///More than one raster image in the dataset.
        MoreThanOneBand {
            display("Too many raster bands in dataset!")
        }
        ///No bands of raster data exist in a given dataset.
        NoBands {
            display("No raster bands found")
        }
    }
}

#[derive(Debug)]
///A fully converted image. As all mapdata is stored as PNG in LAPS, this struct stores the image.
pub struct ConvertedImage {
    ///The width of the image.
    pub width: usize,
    ///The height of the image.
    pub height: usize,
    ///Raw, encoded PNG data.
    pub data: Vec<u8>,
}

///Convert `input` from range [min, max] to [new_min, new_max]
fn convert_range(input: f64, max: f64, min: f64, new_min: f64, new_max: f64) -> f64 {
    let old_range = max - min;
    let new_range = new_max - new_min;
    ((input - min) * new_range / old_range) + new_min
}

#[derive(Debug, Deserialize, Serialize)]
///Map metadata. The unit can vary, depending on the input map.
pub struct ImageMetadata {
    ///The width of a pixel
    pub x_res: f64,
    ///The height of a pixel
    pub y_res: f64,
    ///The height of the lowest points on the map.
    pub min_height: f64,
    ///The height of the highest points on the map.
    pub max_height: f64,
    ///The average height for all points.
    pub average_height: f64,
}

impl ImageMetadata {
    pub(crate) fn from_data(
        dataset: &Dataset,
        min_height: f64,
        max_height: f64,
        average_height: f64,
    ) -> Result<Self, ConvertError> {
        let [x, x_res, _, y, _, y_res] = dataset.geo_transform().map_err(ConvertError::GDal)?;
        debug!("X: {}, Y: {}, x_res: {}, y_res: {}", x, y, x_res, y_res);
        debug!(
            "Min height {}, max: {}, avg: {}",
            min_height, max_height, average_height
        );

        Ok(ImageMetadata {
            x_res,
            y_res,
            min_height,
            max_height,
            average_height,
        })
    }
}

impl fmt::Display for ImageMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}m by {}m resolution, lowest point: {}, highest point: {}, avg: {}",
            self.x_res, self.y_res, self.min_height, self.max_height, self.average_height
        )
    }
}

///Convert a GDAL raster format file from `path` into a PNG. The image must have geospecial metadata in it.
pub fn convert_to_png<P>(path: P) -> Result<(ConvertedImage, ImageMetadata), ConvertError>
where
    P: AsRef<std::path::Path>,
{
    let dataset = Dataset::open(path.as_ref()).map_err(ConvertError::GDal)?;
    match dataset.count() {
        0 => Err(ConvertError::NoBands),
        1 => Ok(()),
        //The count will never be less than zero, any value gotten here will be greater than zero.
        //We could match on the negative values too but that requires a nightly feature
        //TODO use exaustive_range_patterns feature when it arrives for correctness
        _ => Err(ConvertError::MoreThanOneBand),
    }?;

    //Our data mostly consists of float32s hopefully, but in case we have other ones
    //just read the data as a double for simplicity. This works with all other data types
    //except the complex ones.
    let (width, height) = dataset.size();
    let data: Vec<f64> = dataset
        .read_full_raster_as(1)
        .map_err(ConvertError::GDal)?
        .data;
    debug!(
        "Decoded raster data of size {}px by {}px with {} points",
        width,
        height,
        data.len()
    );

    //Find the highest and the lowest points on the map
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;

    //Accumulator for calculating the average
    let mut average_acc = 0f64;
    for point in &data {
        if *point < min {
            min = *point;
        } else if *point > max {
            max = *point;
        }
        average_acc += point;
    }
    let average = average_acc / data.len() as f64;

    //pre-allocate buffer for grayscale data for output image.
    let mut out_data = vec![0u8; data.len()];

    //Normalize the data
    let one_part = (max - min) / u8::MAX as f64;
    debug!("One part is: {}, max_min: {}", one_part, max - min);
    for (index, point) in data.into_iter().enumerate() {
        let normalized = convert_range(point, max, min, 0.0, u8::MAX as f64);
        out_data[index] = normalized as u8;
    }

    //Encode data_out as a grayscale png
    let mut data_out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data_out, width as u32, height as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&out_data).unwrap();
    }

    //Finally, get metadata
    let out = ConvertedImage {
        width,
        height,
        data: data_out,
    };
    let metadata = ImageMetadata::from_data(&dataset, min, max, average)?;

    Ok((out, metadata))
}

///Import `data` into the system as mapdata.
///# Panics
///Will panic if it tries to set a map id which already exists, probably from inputting it manually.
pub async fn import_data(
    conn: &mut darkredis::Connection,
    image: ConvertedImage,
    metadata: ImageMetadata,
) -> Result<u32, darkredis::Error> {
    do_import("laps.mapdata", conn, image, metadata).await
}

#[inline]
async fn do_import(
    map_key: &str,
    conn: &mut darkredis::Connection,
    image: ConvertedImage,
    metadata: ImageMetadata,
) -> Result<u32, darkredis::Error> {
    let image_key = format!("{}.image", map_key);
    let meta_key = format!("{}.meta", map_key);
    //Get the biggest unused map id.
    let mut map_ids: Vec<u32> = conn
        .hkeys(&image_key)
        .await?
        .into_iter()
        .map(|s| {
            String::from_utf8_lossy(&s)
                .parse::<u32>()
                .expect("parsing map id as usize")
        })
        .collect();
    map_ids.sort_unstable();

    //Place map data into the system
    let map_id = map_ids.last().unwrap_or(&0) + 1;
    let map_id_string = map_id.to_string();
    if !conn.hsetnx(image_key, &map_id_string, image.data).await? {
        //Map data was already set!
        panic!("Tried to set map field {}, but it already existed!", map_id);
    }

    //Set the metadata
    let serialized = serde_json::to_vec(&metadata).unwrap();
    if !conn.hsetnx(meta_key, &map_id_string, &serialized).await? {
        panic!(
            "Tried to set map metadata field {}, but it already existed!",
            map_id
        );
    }

    info!(
        "Imported map {}: {}px by {}px image with metadata: {}",
        map_id_string, image.width, image.height, metadata
    );

    Ok(map_id)
}

///Import `image` and `metadata` into the system, but place the result in the testing key rather than the actual key.
pub async fn import_data_test(
    conn: &mut darkredis::Connection,
    image: ConvertedImage,
    metadata: ImageMetadata,
) -> Result<u32, darkredis::Error> {
    do_import("laps.testing.mapdata", conn, image, metadata).await
}
