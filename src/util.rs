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
