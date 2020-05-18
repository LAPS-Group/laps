//src/util.rs: Different utility functions, mostly Redis key getters.
//Author: Håkon Jordet
//Copyright (c) 2020 LAPS Group
//Distributed under the zlib licence, see LICENCE.

use crate::{module_handling::ModuleInfo, web::job::JobSubmission};
use rand::{thread_rng, RngCore};

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

//Get the job queue key for `module`.
pub fn get_module_work_key(module: &ModuleInfo) -> String {
    let prefix = create_redis_key("runner");
    format!("{}.{}:{}.work", prefix, module.name, module.version)
}

pub fn get_module_log_key(module: &ModuleInfo) -> String {
    let prefix = create_redis_backend_key("moduleLogs");
    format!("{}.{}:{}", prefix, module.name, module.version)
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

//Get the administrator entry key
pub fn get_admin_key(username: &str) -> String {
    let prefix = create_redis_backend_key("admin");
    format!("{}.admins.{}", prefix, username.to_lowercase())
}

//Generate a cryptographically secure salt for password hashing
pub fn generate_salt() -> Vec<u8> {
    //according to the rand documentation, ThreadRng is supposed to be cryptographically secure.
    //All we want to do when salting the hash is to give equal passwords different hashes, so generating
    //8 bytes is plenty.
    let mut rng = thread_rng();
    let mut out = vec![0u8; 8];
    rng.fill_bytes(&mut out);
    out
}

//Get the session key associated with the session token `token`.
pub fn get_session_key(token: &str) -> String {
    let prefix = create_redis_backend_key("sessions");
    format!("{}.{}", prefix, token)
}
//Get a job cache key
pub fn get_job_cache_key(job: &JobSubmission) -> String {
    let prefix = create_redis_backend_key("cache");
    //We want the key to have the same format every time
    format!("{}.{}", prefix, job.cache_key())
}

//Get the key where we store the number of workers we can create of this module type.
pub fn get_module_workers_key(module: &ModuleInfo) -> String {
    let prefix = create_redis_backend_key("module-workers");
    format!("{}.{}", prefix, module)
}

//Get the key where we keep the counter to how many workers are actually running for `module`.
pub fn get_registered_module_workers_key(module: &ModuleInfo) -> String {
    let prefix = get_module_workers_key(module);
    format!("{}.active", prefix)
}
