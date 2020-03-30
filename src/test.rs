//Test utility functions and such

use crate::util;
use png::Decoder;
use tokio::{fs::File, io::AsyncReadExt};

//Insert some test mapdata to use in the tests. Will always place it at map ID 1. Returns the width and height of the image.
pub async fn insert_test_mapdata(conn: &mut darkredis::Connection) -> (u32, u32) {
    let mut file = File::open("test_data/dom1.png").await.unwrap();
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await.unwrap();

    conn.hset(util::create_redis_key("mapdata"), b"1", &contents)
        .await
        .unwrap();
    let decoder = Decoder::new(contents.as_slice());
    let (info, _) = decoder.read_info().unwrap();

    (info.width, info.height)
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
