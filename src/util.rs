use crate::module_handling::ModuleInfo;

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
pub fn get_module_key(module: &ModuleInfo) -> String {
    let prefix = create_redis_key("runner");
    format!("{}.{}:{}.work", prefix, module.name, module.version)
}

//Get the job token to job id map token key using `token`.
pub fn get_job_mapping_key(token: &str) -> String {
    let prefix = create_redis_backend_key("job_mapping");
    format!("{}.{}", prefix, token)
}

pub fn get_job_key(jobid: i32) -> String {
    let prefix = create_redis_backend_key("job_result");
    format!("{}.{}", prefix, jobid)
}
