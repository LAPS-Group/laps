use crate::{
    types::{BackendError, JobOutcome, JobResult},
    util::{
        create_redis_backend_key, create_redis_key, get_job_key, get_module_log_key,
        get_module_work_key,
    },
    web::job::JobInfo,
};
use chrono::prelude::*;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::fmt;

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
                info!("Module {} shut down", info);

                //Now that the module is shut down, cancel any job it may have queued up.
                let work_key = get_module_work_key(&info);
                let results_key = create_redis_backend_key("path-results");
                let results: Vec<Vec<u8>> = conn
                    .lrange(&work_key, 0, -1)
                    .await
                    .expect("getting module work queue")
                    .into_iter()
                    .map(|s| {
                        let job = serde_json::from_slice::<JobInfo>(&s).unwrap();
                        serde_json::to_vec(&JobResult {
                            job_id: job.job_id,
                            outcome: JobOutcome::Cancelled,
                            points: Vec::new(),
                        })
                        .unwrap()
                    })
                    .collect();
                if !results.is_empty() {
                    conn.rpush_slice(&results_key, &results).await.unwrap();
                }

                info!("Cancelled {} jobs from {}'s job queue", results.len(), info);

                //Also delete the entire job cache for the module, so that every new job submitted to the module will
                //get rejected instead of giving a potentially confusing cancellation message every time.
                let pattern = create_redis_backend_key(&format!("cache.{}.*", info)); //cache key always starts with the module info first.
                let caches = conn
                    .scan()
                    .pattern(&pattern)
                    .run()
                    .collect::<Vec<Vec<u8>>>()
                    .await;
                if !caches.is_empty() {
                    conn.del_slice(&caches)
                        .await
                        .expect("deleting cache entries");
                }
                info!(
                    "Deleted {} cache entries which came from {}",
                    caches.len(),
                    info
                );

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

impl fmt::Display for ModuleInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.name, self.version)
    }
}
//The listener which listens for pathfinding results
async fn result_listener(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("result-listener").await.unwrap();

    //Push every single result to their corresponding job id key and expire it
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
        //TODO: Maybe set the mapping key timeout to match the result timeout
        conn.lpush(&key, &value).await.unwrap();
        conn.expire_seconds(&key, crate::CONFIG.jobs.result_timeout)
            .await
            .unwrap();
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
struct ModuleLog {
    pub module: ModuleInfo,
    pub message: String,
    pub level: String,
    pub instant: i64,
}

//Listen and report module logs.
pub async fn log_listener(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("log-listener").await.unwrap();

    let listen_key = create_redis_key("moduleLogs"); // the key to listen for module logs

    loop {
        //Ok to use expect and unwrap as something would probably have gone very wrong.
        let (_, value) = conn
            .blpop(&[&listen_key], 0)
            .await
            .expect("listening for module logs")
            .unwrap();
        let entry: ModuleLog = serde_json::from_slice(&value).expect("deserializing module log");

        //We have deserialized the log entry, now store it.
        let log_key = get_module_log_key(&entry.module);
        let time = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(entry.instant, 0), Utc);
        //Store the log entry as a simple string.
        let stored_entry = format!(
            "[{} {}] {}",
            time.to_rfc3339_opts(SecondsFormat::Secs, true),
            entry.level,
            entry.message
        );
        conn.rpush(log_key, stored_entry)
            .await
            .expect("pushing module logs");

        let log_message = format!("Module {}: {}", entry.module, entry.message);

        //Print out the message into the server logs
        match entry.level.as_str() {
            "info" => info!("{}", log_message),
            "error" => error!("{}", log_message),
            "warn" => warn!("{}", log_message),
            "debug" => debug!("{}", log_message),
            _ => {
                warn!("Unknown module log level {}", entry.level);
                info!("{}", log_message)
            }
        }
    }
}

//Listen for and handle registration of new modules
pub async fn run(pool: darkredis::ConnectionPool) {
    let mut conn = pool.spawn("module-registration").await.unwrap();

    //Run the unregistration loop
    tokio::spawn(unregister_loop(pool.clone()));
    //Run the results listener
    tokio::spawn(result_listener(pool.clone()));
    //run the log listener
    tokio::spawn(log_listener(pool.clone()));

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
    use super::ModuleInfo;
    use crate::{
        types::{JobOutcome, JobResult, Vector},
        util::{create_redis_backend_key, get_job_cache_key, get_module_work_key},
        web::job::{JobInfo, JobSubmission},
    };
    use futures::StreamExt;
    use serial_test::serial;
    use std::time::Duration;
    use tokio::time;

    #[tokio::test]
    #[serial]
    async fn module_registration() {
        //setup
        let pool = crate::create_redis_pool().await;
        tokio::spawn(super::run(pool.clone()));
        let mut conn = pool.get().await;
        crate::test::clear_redis(&mut conn).await;

        let module_key = create_redis_backend_key("registered_modules");

        //Register a fake module
        let module_info = br#"{"name": "test_module", "version": "1.0.0"}"#.to_vec();
        conn.rpush(create_redis_backend_key("register-module"), &module_info)
            .await
            .unwrap();

        //Check that we were actually registered
        time::delay_for(Duration::from_millis(100)).await; //We have to yield to let the registration code run.
        assert!(conn.sismember(&module_key, &module_info).await.unwrap());

        //Deregister ourselves
        conn.rpush(create_redis_backend_key("module-shutdown"), &module_info)
            .await
            .unwrap();
        time::delay_for(Duration::from_millis(100)).await; //We have to yield to let the registration code run.
        assert!(!conn.sismember(&module_key, &module_info).await.unwrap());
    }

    //Test that a module's queue is cancelled when it shuts down.
    #[tokio::test]
    #[serial]
    async fn queue_cancellation() {
        //setup
        let pool = crate::create_redis_pool().await;
        let mut conn = pool.get().await;
        crate::test::clear_redis(&mut conn).await;
        crate::test::insert_test_mapdata(&mut conn).await;
        tokio::spawn(super::unregister_loop(pool.clone())); //only run deregistration loop

        //Make some fake module info. We only need to unregister it.
        let module_info = ModuleInfo {
            name: "mod".into(),
            version: "ver".into(),
        };

        //How many jobs to submit for the test.
        const JOB_COUNT: i32 = 5;

        //Submit a bunch of jobs. It doesn't matter that they are all the same because the
        //Input validation is done in the web code.
        let work_key = get_module_work_key(&module_info);
        let mut job = JobInfo {
            start: Vector { x: 1, y: 1 },
            map_id: 1,
            job_id: 1,
            stop: Vector { x: 2, y: 2 },
        };
        let mut jobs = Vec::new();
        for i in 0..JOB_COUNT {
            job.job_id = i;
            jobs.push(serde_json::to_vec(&job).unwrap());
        }
        conn.lpush_slice(&work_key, &jobs).await.unwrap();

        //Add a couple of dummy cache entries. It doesn't matter whether or not these are correct as long
        //as the module is right, so just fill them with blank space.
        for i in 0..JOB_COUNT {
            job.job_id = i;
            let submission = JobSubmission {
                map_id: 1,
                start: Vector { x: 1, y: 1 },
                stop: Vector { x: 2, y: 2 },
                algorithm: module_info.clone(),
            };
            let cache_key = get_job_cache_key(&submission);
            conn.set(&cache_key, b"").await.unwrap();
        }

        //Now "shut down" the module.
        conn.rpush(
            create_redis_backend_key("module-shutdown"),
            &serde_json::to_vec(&module_info).unwrap(),
        )
        .await
        .unwrap();

        //Yield for a bit so that the shutdown can be processed
        tokio::time::delay_for(std::time::Duration::from_millis(100)).await;

        //Check that each job was cancelled by verifying that the path results backlog is big enough.
        let path_results_key = create_redis_backend_key("path-results");
        assert_eq!(
            conn.llen(&path_results_key).await.unwrap().unwrap(),
            JOB_COUNT as isize
        );
        assert_eq!(
            serde_json::from_slice::<JobResult>(
                &conn.rpop(&path_results_key).await.unwrap().unwrap()
            )
            .unwrap()
            .outcome,
            JobOutcome::Cancelled
        );

        //Verify that there are no cache entries.
        let pattern = create_redis_backend_key(&format!("cache.{}.*", module_info)); //cache key always starts with the module info first.
        let caches = conn
            .scan()
            .pattern(&pattern)
            .run()
            .collect::<Vec<Vec<u8>>>()
            .await;

        assert!(caches.is_empty());
    }
}
