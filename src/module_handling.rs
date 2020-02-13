use crate::{
    util::{create_redis_backend_key, create_redis_key},
    REDIS_POOL,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
struct ModuleInfo {
    name: String,
    version: String,
}

#[derive(Serialize, Debug)]
struct Vector {
    x: f32,
    y: f32,
}

#[derive(Serialize, Debug)]
struct JobInfo {
    job_id: i32,
    start: Vector,
    stop: Vector,
    map_id: i32,
}

#[instrument(skip(connection))]
async fn run_module(metadata: ModuleInfo, mut connection: darkredis::Connection) {
    let key = create_redis_key(&format!(
        "runner.{}:{}.work",
        metadata.name, metadata.version
    ));
    let info = JobInfo {
        job_id: 20,
        start: Vector { x: 200.0, y: 90.0 },
        stop: Vector { x: 10.0, y: 10.0 },
        map_id: 2,
    };
    debug!("Sending job: {:?}", info);
    connection
        .rpush(&key, serde_json::to_string(&info).unwrap())
        .await
        .unwrap();

    let result = connection
        .blpop(&[&create_redis_backend_key("path-results")], 0)
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .nth(1)
        .unwrap();

    debug!("Got response {}", String::from_utf8_lossy(&result));
}

#[instrument]
pub async fn run() {
    let pool = REDIS_POOL.clone();
    let mut conn = pool.spawn(None).await.unwrap();
    loop {
        let metadata: ModuleInfo = serde_json::from_slice(
            &conn
                .blpop(&[create_redis_backend_key("register-module")], 0)
                .await
                .unwrap()
                .unwrap()
                .into_iter()
                .nth(1)
                .unwrap(),
        )
        .unwrap();

        info!(
            "Registered module {} version {}",
            metadata.name, metadata.version
        );

        tokio::spawn(run_module(metadata, pool.spawn(None).await.unwrap()));
    }
}
