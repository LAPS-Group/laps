#![feature(proc_macro_hygiene)]

#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate tracing;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;

use config::Config;
use darkredis::ConnectionPool;
use std::net;
use tracing::Level;

mod module_handling;
mod types;
mod util;
mod web;

//Struct describing the format of the configuration files
#[derive(serde::Deserialize)]
struct Configuration {
    pub redis: RedisConfig,
    pub jobs: JobConfig,
}

#[derive(serde::Deserialize)]
struct RedisConfig {
    address: net::IpAddr,
    port: u16,
    password: Option<String>,
}

#[derive(serde::Deserialize)]
struct JobConfig {
    //Timeouts in seconds for different purposes
    token_timeout: u32,  // the timeout for a token mapping key
    poll_timeout: u32,   // the amount of time a user can poll a running job
    result_timeout: u32, // how long the results of a pathfinding job is kept

    //Number of maximum polling clients at once
    max_polling_clients: u32,
    //Additional connections to use in addition to max_polling clients,
    //in order to quickly deny additional clients
    additional_connections: u32,
}

lazy_static! {
    //Make this a static global to access it easily across the application
    static ref CONFIG: Configuration = {
        let span = warn_span!("config");
        let _guard = span.enter();

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
    let span = span!(Level::INFO, "redis");
    let _guard = span.enter();

    let redis_conf = &CONFIG.redis;
    let address = net::SocketAddr::new(redis_conf.address, redis_conf.port);
    info!("Connecting to Redis at {}", address);

    let pool = ConnectionPool::create(
        address.to_string(),
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

//Initialize tracing
fn setup_tracing() {
    let var = std::env::var("RUST_LOG").unwrap_or_else(|_| "laps=trace,info".into());
    tracing_subscriber::FmtSubscriber::builder()
        .with_target(true)
        .with_ansi(true)
        .with_env_filter(tracing_subscriber::EnvFilter::new(var))
        .init();

    info!("Successfully initialized tracing!");
}

#[tokio::main]
async fn main() {
    setup_tracing();

    info!("Starting up...");
    web::run().await
}
