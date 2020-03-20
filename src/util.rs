use crate::module_handling::ModuleInfo;
use blake2::{Blake2b, Digest};
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

//Get the administrator entry key
pub fn get_admin_key(username: &str) -> String {
    let prefix = create_redis_backend_key("admin");
    format!("{}.admins.{}", prefix, username.to_lowercase())
}

//Calculate a password hash from a password and salt.
pub fn calculate_password_hash(password: &str, salt: &[u8]) -> Vec<u8> {
    let mut hasher = Blake2b::new();
    hasher.input(password);
    hasher.input(salt);
    hasher.result().to_vec()
}

//Generate a cryptographically secure salt for password hashing
pub fn generate_salt() -> Vec<u8> {
    //according to the rand documentation, ThreadRng is supposed to be cryptographically secure.
    let mut rng = thread_rng();
    let mut out = vec![0u8; 256];
    rng.fill_bytes(&mut out);
    out
}

//Get the session key associated with the session token `token`.
pub fn get_session_key(token: &str) -> String {
    let prefix = create_redis_backend_key("sessions");
    format!("{}.{}", prefix, token)
}
