#![feature(proc_macro_hygiene)]

#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;

use config::Config;
use darkredis::ConnectionPool;
use rocket::config::{Environment, LoggingLevel};

mod module_handling;
mod types;
mod util;
mod web;

#[derive(serde::Deserialize)]
struct Configuration {
    pub redis: RedisConfig,
}

#[derive(serde::Deserialize)]
struct RedisConfig {
    address: String,
    password: Option<String>,
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
