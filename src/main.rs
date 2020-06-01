//src/main.rs: Backend Entry Point
//Author: Håkon Jordet
//Distributed under the zlib licence, see LICENCE.

#![feature(proc_macro_hygiene)]

#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;

use bollard::Docker;
use config::Config;
use darkredis::ConnectionPool;
use rocket::config::{Environment, LoggingLevel};

mod module_handling;
mod types;
mod util;
mod web;

#[cfg(test)]
mod test;

//Struct describing the format of the configuration files
#[derive(serde::Deserialize)]
struct Configuration {
    pub redis: RedisConfig,
    pub jobs: JobConfig,
    pub login: LoginConfig,
    pub module: ModuleConfig,
}

#[derive(serde::Deserialize)]
struct RedisConfig {
    address: String,
    password: Option<String>,
}

#[derive(serde::Deserialize)]
struct JobConfig {
    //Timeouts in seconds for different purposes
    token_timeout: u32,  // the timeout for a token mapping key
    poll_timeout: u32,   // the amount of time a user can poll a running job
    result_timeout: u32, // how long the results of a pathfinding job is kept

    //Maximum number of clients who can poll for jobs at once. Creates this many Redis connections.
    max_polling_clients: u32,
}

#[derive(serde::Deserialize)]
struct LoginConfig {
    //Timeout in seconds for sessions
    session_timeout: u32,
    //Minimum password length
    minimum_password_length: u8,
    //Maximum password length
    maximum_password_length: u8,
}

#[derive(serde::Deserialize)]
struct ModuleConfig {
    //Images to ignore in the admin panel list.
    ignore: Vec<String>,
}

lazy_static! {
    //Make this a static global to access it easily across the application
    static ref CONFIG: Configuration = {
        //Main config file
        let mut s = Config::new();
        info!("Loading default configuration...");
        if let Err(e) = s.merge(config::File::with_name("config/default.toml")) {
            error!("Failed to load default configuration: {}", e);
            std::process::exit(2);
        }

        //This is where any local configuration is done
        info!("Loading local configuration...");
        if let Err(e) = s.merge(config::File::with_name("config/local.toml").required(false)) {
            warn!("Failed to load local configuration: {}", e);
        }

        //Load configuration for testing mode
        if cfg!(test) {
            //ok to unwrap as this is only used in tests
            s.merge(config::File::with_name("config/test.toml").required(false)).unwrap();
        }

        match s.try_into() {
            Ok(conf) => {
                info!("Successfully loaded configuration!");
                conf
            }
            Err(e) => {
                error!("Invalid configuration: {}", e);
                std::process::exit(2);
            }
        }
    };
}

//Create the Redis pool which is used in the application
async fn create_redis_pool() -> ConnectionPool {
    let redis_conf = &CONFIG.redis;
    info!("Connecting to Redis at {}", redis_conf.address);

    let pool = ConnectionPool::create(
        redis_conf.address.clone(),
        redis_conf.password.as_deref(),
        num_cpus::get() * 2,
    )
    .await;
    match pool {
        Ok(p) => {
            info!("Successfully connected to Redis!");
            p
        }
        Err(e) => {
            error!("Failed to connect to Redis: {:?}", e);
            std::process::exit(1);
        }
    }
}

//There's not much reason to use a connection pool for the Docker client because there will never be
//that many administrators connecting at once. There's also no pre-made solution for Bollard so it's
//best to not bother.
async fn connect_to_docker() -> bollard::Docker {
    info!("Connecting to Docker...");
    match Docker::connect_with_local_defaults() {
        Ok(d) => {
            info!("Succesfully connected to Docker!");
            d
        }
        Err(e) => {
            error!("Failed to connect to Docker: {:?}", e);
            std::process::exit(1)
        }
    }
}

fn setup_logging() {
    //Set the log level of things.
    //We want to always have info active for LAPS, but not necesarrily for Rocket.

    //Map the Rocket environment into different log levels.
    let env = Environment::active().expect("getting rocket environment");
    let laps_level = match &env {
        Environment::Development => "trace",
        Environment::Staging => "debug",
        Environment::Production => "info",
    };

    //Set the log levels for Rocket as described in the Rocket documentation
    let rocket_config = rocket::config::Config::active().expect("getting rocket config");
    let other_level = match rocket_config.log_level {
        LoggingLevel::Critical => Some("warn"),
        LoggingLevel::Normal => Some("info"),
        LoggingLevel::Debug => Some("trace"),
        LoggingLevel::Off => None,
    };

    //Set the environment variable correctly
    let mut log_value = format!("{}={}", env!("CARGO_PKG_NAME"), laps_level);
    //Set the same log level for laps_convert
    log_value += &format!(",laps_convert={}", laps_level);

    if let Some(level) = other_level {
        log_value += &format!(",{}", level);
    }
    std::env::set_var("RUST_LOG", &log_value);

    env_logger::Builder::from_default_env()
        .format_timestamp_secs()
        .init();

    info!("Successfully initialized logging!");
}

#[tokio::main]
async fn main() {
    setup_logging();

    info!("Starting up...");
    web::run().await
}
