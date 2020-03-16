#![feature(exclusive_range_pattern)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

//!Library for handling map data conversions in LAPS.

#[macro_use]
extern crate log;

use gdal::raster::Dataset;
use quick_error::quick_error;

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

///Return a vector of normalized pixels
pub fn create_normalized_png<P>(path: P) -> Result<ConvertedImage, ConvertError>
where
    P: AsRef<std::path::Path>,
{
    let dataset = Dataset::open(path.as_ref()).map_err(ConvertError::GDal)?;
    match dataset.count() {
        0 => Err(ConvertError::NoBands),
        1 => Ok(()),
        2..std::isize::MAX => Err(ConvertError::MoreThanOneBand),
        _ => unreachable!(),
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
        "Decoded raster data of size {}pxX{}px with {} points",
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

    debug!(
        "Min height: {:.2}MASL, max height: {:.2}MASL, average: {:.2}MASL",
        min, max, average
    );

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

    let out = ConvertedImage {
        width,
        height,
        data: data_out,
    };

    Ok(out)
}

///Import `data` into the system as mapdata.
///# Panics
///Will panic if it tries to set a map id which already exists, probably from inputting it manually.
pub async fn import_png_as_mapdata(
    conn: &mut darkredis::Connection,
    data: Vec<u8>,
) -> Result<(), darkredis::Error> {
    let map_index = conn.incr("laps.mapdata.id_counter").await?.to_string();
    if !conn.hsetnx("laps.mapdata", &map_index, data).await? {
        //Map data was already set!
        panic!(
            "Tried to set map data field {}, but it already existed!",
            map_index
        );
    }

    Ok(())
}
