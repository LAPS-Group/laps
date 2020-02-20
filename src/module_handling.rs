use crate::util::{create_redis_backend_key, create_redis_key};
use serde::Deserialize;

//Handle any modules unregistrering themselves in a loop, forever.
#[instrument(skip(pool))]
async fn unregister_loop(pool: darkredis::ConnectionPool) {
    let mut conn = pool
        .spawn("unregistration-loop")
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

///Information that a module registers and de-registers itself with.
#[derive(Deserialize)]
pub struct ModuleInfo {
    name: String,
    version: String,
}

//Listen for and handle registration of new modules
#[instrument(skip(pool))]
pub async fn run(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("module-registration").await.unwrap();

    //Run the unregistration loop
    tokio::spawn(unregister_loop(pool.clone()));

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
