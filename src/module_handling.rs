use crate::{
    types::{BackendError, JobResult},
    util::{create_redis_backend_key, get_job_key},
};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

//Handle any modules unregistrering themselves in a loop, forever.
async fn unregister_loop(pool: darkredis::ConnectionPool) {
    let mut conn = pool
        .spawn("unregistration-loop")
        .await
        .expect("Spawning Redis connection");

    let key = create_redis_backend_key("module-shutdown");
    loop {
        let (_, data) = conn
            .blpop(&[&key], 0)
            .await
            .expect("popping from shutdown queue")
            .unwrap();
        let shutdown: Result<ModuleInfo, BackendError> =
            serde_json::from_slice(&data).map_err(BackendError::JsonError);

        match shutdown {
            Ok(info) => {
                info!("Module {} v{} shut down", info.name, info.version);

                //Remove from the registered_modules set.
                //Rely on modules sending the exact same shutdown data as they sent registration data.
                if !conn
                    .srem(create_redis_backend_key("registered_modules"), &data)
                    .await
                    .expect("Removing from registered-modules set")
                {
                    error!("Module {} {} wasn't registered!", info.name, info.version);
                    trace!("Raw module info: {}", String::from_utf8_lossy(&data));
                }
            }
            Err(e) => error!("Couldn't parse shutdown message: {}", e),
        }
    }
}

//Information that a module registers and de-registers itself with.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub version: String,
}

//The listener which listens for pathfinding results
async fn result_listener(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("result-listener").await.unwrap();

    //Push every single result to their corresponding job id key and expire it
    let mut buffer = Vec::new();
    loop {
        //Cannot use BRPOPLPUSH here because we have to parse the value
        let (_, value) = conn
            .blpop(&[create_redis_backend_key("path-results")], 0)
            .await
            .expect("popping path results")
            .unwrap();

        let deserialized: JobResult = match serde_json::from_slice(&value) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "Ignoring job result '{}': {}",
                    String::from_utf8_lossy(&value),
                    e
                );
                continue;
            }
        };
        let key = get_job_key(deserialized.job_id);

        //Expire after a given period if the result has not been retrieved by the user
        let timeout = crate::CONFIG.jobs.result_timeout.to_string();
        let command = darkredis::CommandList::new("LPUSH")
            .arg(&key)
            .arg(&value)
            .command("EXPIRE")
            .arg(&key)
            .arg(&timeout);

        //TODO: Maybe set the mapping key timeout to match the result timeout

        let results = conn
            .run_commands_with_buffer(command, &mut buffer)
            .await
            .unwrap();

        results
            .try_collect::<Vec<darkredis::Value>>()
            .await
            .unwrap();
    }
}

//Listen for and handle registration of new modules
pub async fn run(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("module-registration").await.unwrap();

    //Run the unregistration loop
    tokio::spawn(unregister_loop(pool.clone()));
    //Run the results listener
    tokio::spawn(result_listener(pool.clone()));

    loop {
        let (_, data) = &conn
            .blpop(&[create_redis_backend_key("register-module")], 0)
            .await
            .unwrap()
            .unwrap();

        let metadata: ModuleInfo = serde_json::from_slice(&data).unwrap();

        //Register the module for use later using a set
        conn.sadd(create_redis_backend_key("registered_modules"), data)
            .await
            .expect("registering existing module");

        info!(
            "Registered module {} version {}",
            metadata.name, metadata.version
        );
    }
}

//Get a list of every single pathfinding module which has been registered thus far.
//Will log and ignore invalid entries. Takes a mutable reference to be able to use it from a MutexGuard..
pub async fn get_registered_modules(
    conn: &mut darkredis::Connection,
) -> Result<Vec<ModuleInfo>, BackendError> {
    let mut output = Vec::new();

    let key = create_redis_backend_key("registered_modules");
    let modules = conn.smembers(key).await?;
    for module in modules {
        match serde_json::from_slice(&module) {
            Ok(m) => output.push(m),
            Err(e) => {
                //Log and ignore the erroneous entry.
                error!("Failed to parse registered module: {}", e);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::create_redis_backend_key;
    use std::time::Duration;
    use tokio::time;

    #[tokio::test]
    async fn module_registration() {
        //setup
        crate::setup_logging();
        let pool = crate::create_redis_pool().await;
        tokio::spawn(super::run(pool.clone()));
        let mut conn = pool.get().await;

        //Delete all modules
        let module_key = create_redis_backend_key("registered_modules");
        conn.del(&module_key).await.unwrap();

        //Register a fake module
        let module_info = br#"{"name": "test_module", "version": "1.0.0"}"#.to_vec();
        conn.rpush(create_redis_backend_key("register-module"), &module_info)
            .await
            .unwrap();

        //Check that we were actually registered
        time::delay_for(Duration::from_millis(100)).await; // Might take some time to handle the request so wait for a sec
        assert!(conn.sismember(&module_key, &module_info).await.unwrap());

        //Deregister ourselves
        conn.rpush(create_redis_backend_key("module-shutdown"), &module_info)
            .await
            .unwrap();
        time::delay_for(Duration::from_millis(100)).await; // Might take some time to handle the request so wait for a sec
        assert!(!conn.sismember(&module_key, &module_info).await.unwrap());
    }
}
