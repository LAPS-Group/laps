///Create a general Redis key to be used in the system.
pub fn create_redis_key(name: &str) -> String {
    format!("laps.{}", name)
}

///Create a Redis key for something specific to the backend.
pub fn create_redis_backend_key(name: &str) -> String {
    format!("laps.backend.{}", name)
}
