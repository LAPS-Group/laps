use crate::module_handling::ModuleInfo;
use crate::types::JobResult;
use crate::web::job::JobSubmission;

///Create a general Redis key to be used in the system.
#[cfg(not(test))]
pub fn create_redis_key(name: &str) -> String {
    format!("laps.{}", name)
}

//Testing versions of same keys
#[cfg(test)]
pub fn create_redis_key(name: &str) -> String {
    format!("laps.testing.{}", name)
}

#[cfg(not(test))]
///Create a Redis key for something specific to the backend.
pub fn create_redis_backend_key(name: &str) -> String {
    format!("laps.backend.{}", name)
}

#[cfg(test)]
pub fn create_redis_backend_key(name: &str) -> String {
    format!("laps.testing.backend.{}", name)
}

//A nice function for resetting the entire testing database.
#[cfg(test)]
pub async fn clear_redis(conn: &mut darkredis::Connection) {
    use futures::StreamExt;

    let keys: Vec<Vec<u8>> = conn.scan().pattern(b"laps.testing.*").run().collect().await;
    for k in keys {
        conn.del(&k).await.unwrap();
    }
}

//Get the job queue key for `module`.
pub fn get_module_key(module: &ModuleInfo) -> String {
    let prefix = create_redis_key("runner");
    format!("{}.{}:{}.work", prefix, module.name, module.version)
}

//Get the job token to job id map token key using `token`.
pub fn get_job_mapping_key(token: &str) -> String {
    let prefix = create_redis_backend_key("job_mapping");
    format!("{}.{}", prefix, token)
}

//Get the key where the result of a job with job_id is or will be.
pub fn get_job_key(job_id: i32) -> String {
    let prefix = create_redis_backend_key("job_result");
    format!("{}.{}", prefix, job_id)
}

//Get a job cache key
pub fn get_job_cache_key(job: &JobSubmission) -> String {
    let prefix = create_redis_backend_key("cache");
    //We want the order of a submission's fields to not matter, so re-serialize the job submisson. This ensures that
    //the fields show up in the same order every time. This operation cannot fail, and is unlikely to cost much in terms
    //of performance, though this has not been tested vs converting it to a string in other ways.
    let submission_data = serde_json::to_string(&job).unwrap();
    format!("{}.{}", prefix, submission_data)
}

//Repeatadely try to get a job result using the system configuration.
pub async fn try_poll_job_result(
    redis: &mut darkredis::Connection,
    job_id: i32,
) -> Option<JobResult> {
    let times = crate::CONFIG.jobs.poll_times;
    let poll_interval =
        std::time::Duration::from_secs((crate::CONFIG.jobs.poll_timeout / times) as u64);
    let key = get_job_key(job_id);
    for _ in 0..times {
        tokio::time::delay_for(poll_interval).await;

        let result = redis
            .get(&key)
            .await
            .map(|s| s.map(|s| serde_json::from_slice(&s).unwrap()))
            .expect("getting job result");
        if result.is_some() {
            return result.unwrap();
        }
    }
    None
}
