use crate::{
    types::{BackendError, JobOutcome, JobResult},
    util::{
        create_redis_backend_key, create_redis_key, get_job_key, get_module_log_key,
        get_module_work_key, get_module_workers_key, get_registered_module_workers_key,
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
                //Only remove a module from the active module set if *all* the workers are shut down.
                let remaining_workers = conn
                    .decr(get_registered_module_workers_key(&info))
                    .await
                    .unwrap();
                if remaining_workers > 0 {
                    info!(
                        "Worker for module {} shut down, {} workers remaining!",
                        info, remaining_workers
                    );
                    continue;
                } else if remaining_workers < 0 {
                    warn!("Remaining {} workers is < 0! {}", info, remaining_workers);
                }

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

                info!("Canceled {} jobs from {}'s job queue", results.len(), info);

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

//A log message received from a module worker.
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
struct ModuleLog {
    //The module the message is from.
    pub module: ModuleInfo,
    //The message itself.
    pub message: String,
    //The log level of the message.
    pub level: String,
    //UNIX timestamp when the message was emitted.
    pub instant: i64,
    //The worker number the message came from.
    pub worker: u8,
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
            "[{} {} worker:{}] {}",
            time.to_rfc3339_opts(SecondsFormat::Secs, true),
            entry.level,
            entry.worker,
            entry.message
        );
        conn.rpush(log_key, stored_entry)
            .await
            .expect("pushing module logs");

        let log_message = format!(
            "Module {}[{}]: {}",
            entry.module, entry.worker, entry.message
        );

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

        //Increment the registered module counter.
        let workers = conn
            .incr(get_registered_module_workers_key(&metadata))
            .await
            .expect("updating module worker count");

        //Only bother adding the module to the registered set it the module was registered for the first time.
        if workers > 1 {
            //Get the number of expected workers to print a nice log message.
            let total_workers = conn
                .get(get_module_workers_key(&metadata))
                .await
                .expect("getting desired worker count")
                .map(|s| String::from_utf8_lossy(&s).parse::<u8>().unwrap())
                .unwrap();
            info!(
                "Registered {}/{} workers for module {}",
                workers, total_workers, metadata
            )
        } else {
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
        util::{
            create_redis_backend_key, get_job_cache_key, get_module_work_key,
            get_module_workers_key, get_registered_module_workers_key,
        },
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

    #[tokio::test]
    #[serial]
    //Test that concurrent modules are handled properly.
    async fn concurrent_modules() {
        let pool = crate::create_redis_pool().await;
        let mut conn = pool.get().await;
        crate::test::clear_redis(&mut conn).await;
        tokio::task::spawn(super::run(pool.clone()));

        let workers = 2isize; //How many workers to simulate in the test. Only 2 or higher makes sense here.
        let worker_module = ModuleInfo {
            name: "laps-test".into(),
            version: "0.1.0".into(),
        };
        let module_key = create_redis_backend_key("registered_modules");
        let registration_key = create_redis_backend_key("register-module");
        let shutdown_key = create_redis_backend_key("module-shutdown");

        //Set the number of expected workers.
        conn.set(get_module_workers_key(&worker_module), workers.to_string())
            .await
            .unwrap();

        //Simulate the startup of the workers.
        let message = serde_json::to_string_pretty(&worker_module).unwrap();
        for _ in 0..workers {
            conn.rpush(&registration_key, &message).await.unwrap();
        }

        //Yield to let the handler code run.
        tokio::time::delay_for(Duration::from_millis(100)).await;

        //Check that the register count is correct and that the module is registered.
        assert!(conn.sismember(&module_key, &message).await.unwrap());
        assert_eq!(
            conn.get(&get_registered_module_workers_key(&worker_module))
                .await
                .unwrap(),
            Some(workers.to_string().into_bytes())
        ); //count check

        //Shut down one of the workers.
        conn.rpush(&shutdown_key, &message).await.unwrap();
        tokio::time::delay_for(Duration::from_millis(300)).await;

        //Check that one worker less is running, but that the module is still running.
        assert!(conn.sismember(&module_key, &message).await.unwrap());
        assert_eq!(
            conn.get(&get_registered_module_workers_key(&worker_module))
                .await
                .unwrap(),
            Some((workers - 1).to_string().into_bytes())
        ); //count check

        //Kill the last workers
        for _ in 1..workers {
            conn.rpush(&shutdown_key, &message).await.unwrap();
        }

        //Now that all workers are down, ensure that the module is down too.
        tokio::time::delay_for(Duration::from_millis(300)).await;
        assert!(!conn.sismember(&module_key, &message).await.unwrap());
        assert_eq!(
            conn.get(&get_registered_module_workers_key(&worker_module))
                .await
                .unwrap(),
            Some("0".into())
        ); //count check
    }
}
