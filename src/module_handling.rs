use crate::{
    types::{JobError, JobResult, Vector},
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
struct JobInfo {
    job_id: i32,
    start: Vector,
    stop: Vector,
    map_id: i32,
}

//TODO: Selection of module
#[instrument]
pub async fn execute_job(start: Vector, end: Vector) -> Result<JobResult, JobError> {
    let mut connection = REDIS_POOL.get().await;
    let key = connection
        .get(create_redis_backend_key("current_module"))
        .await
        .unwrap()
        .unwrap();

    let info = JobInfo {
        job_id: 20,
        start: Vector { x: 200.0, y: 90.0 },
        stop: Vector { x: 10.0, y: 10.0 },
        map_id: 2,
    };
    debug!("Sending job: {:?}", info);
    connection
        .rpush(&key, serde_json::to_string(&info).unwrap())
        .await?;

    let result = connection
        .blpop(&[&create_redis_backend_key("path-results")], 0)
        .await?
        .unwrap()
        .into_iter()
        .nth(1)
        .unwrap();

    debug!("Got response {}", String::from_utf8_lossy(&result));

    let deserialized: JobResult = serde_json::from_slice(&result).map_err(|e| {
        warn!("Failed to parse job result: {}", &e);
        JobError::InvalidInput(format!("couldn't deserialize result: {}", e))
    })?;

    Ok(deserialized)
}

//Handle any modules unregistrering themselves in a loop, forever.
#[instrument]
async fn unregister_loop() {
    let mut conn = REDIS_POOL
        .spawn("Unregistration-loop")
        .await
        .expect("Spawning Redis connection");

    let key = create_redis_backend_key("module-shutdown");
    loop {
        let shutdown: ModuleInfo = serde_json::from_slice(
            &conn
                .blpop(&[&key], 0)
                .await
                .expect("popping from shutdown queue")
                .unwrap()
                .into_iter()
                .nth(1)
                .unwrap(),
        )
        .expect("parsing shutdown message");

        info!("Module {} v{} shut down", shutdown.name, shutdown.version);
    }
}

//Listen for and handle registration of new modules
#[instrument]
pub async fn run() {
    let pool = REDIS_POOL.clone();
    let mut conn = pool.spawn(None).await.unwrap();

    //Run the unregistration loop
    tokio::spawn(unregister_loop());

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

        //TEMP: Change when adding support for multiple modules
        let key = create_redis_key(&format!(
            "runner.{}:{}.work",
            metadata.name, metadata.version
        ));
        conn.set(create_redis_backend_key("current_module"), key)
            .await
            .unwrap();
    }
}
