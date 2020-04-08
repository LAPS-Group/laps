//Test utility functions and such

//Insert some test mapdata to use in the tests. Will always place it at map ID 1. Returns the width and height of the image.
pub async fn insert_test_mapdata(conn: &mut darkredis::Connection) -> (u32, u32) {
    let path = "test_data/height_data/dtm1.tif";
    let (image, metadata) = laps_convert::convert_to_png(path).unwrap();

    let (width, height) = (image.width as u32, image.height as u32);
    laps_convert::import_data_test(conn, image, metadata)
        .await
        .unwrap();

    (width, height)
}

//A nice function for resetting only the test part of the database.
#[cfg(test)]
pub async fn clear_redis(conn: &mut darkredis::Connection) {
    use futures::StreamExt;

    let keys: Vec<Vec<u8>> = conn.scan().pattern(b"laps.testing.*").run().collect().await;
    for k in keys {
        conn.del(&k).await.unwrap();
    }
}
