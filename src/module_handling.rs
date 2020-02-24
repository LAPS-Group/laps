use crate::{
    types::{BackendError, JobResult},
    util::{create_redis_backend_key, get_job_key},
};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

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
        let value = conn
            .blpop(&[create_redis_backend_key("path-results")], 0)
            .await
            .expect("popping path results")
            .unwrap()
            .into_iter()
            .nth(1)
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
#[instrument(skip(pool))]
pub async fn run(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("module-registration").await.unwrap();

    //Run the unregistration loop
    tokio::spawn(unregister_loop(pool.clone()));
    //Run the results listener
    tokio::spawn(result_listener(pool.clone()));

    loop {
        let data = &conn
            .blpop(&[create_redis_backend_key("register-module")], 0)
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .nth(1)
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
                error!(
                    module = %String::from_utf8_lossy(&module),
                    "Failed to parse registered module: {}", e
                );
            }
        }
    }
    Ok(output)
}
