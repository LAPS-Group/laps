#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate tracing;
#[macro_use]
extern crate lazy_static;

use config::Config;
use darkredis::ConnectionPool;
use std::net;
use tracing::Level;

mod module_handling;
mod types;
mod util;
mod web;

#[derive(serde::Deserialize)]
struct Configuration {
    pub redis: RedisConfig,
    pub web: WebConfig,
}

#[derive(serde::Deserialize)]
struct RedisConfig {
    address: net::IpAddr,
    port: u16,
    password: Option<String>,
}

#[derive(serde::Deserialize)]
struct WebConfig {
    pub address: net::IpAddr,
    pub port: u16,
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
    //Same thing for the Redis pool, mainly because warp does not have a good way to manage state
    static ref REDIS_POOL: ConnectionPool = {
        let span = span!(Level::INFO, "redis");
        let _guard = span.enter();
        let pool = futures::executor::block_on(async {
            let redis_conf = &CONFIG.redis;
            let address = net::SocketAddr::new(redis_conf.address, redis_conf.port);
            info!("Connecting to Redis at {}", address);

            ConnectionPool::create(
                address.to_string(),
                redis_conf.password.as_ref().map(|s| s.as_str()),
                num_cpus::get() * 2,
            )
            .await
        });
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
    };
}

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

    // Launch module handling logic
    tokio::spawn(module_handling::run());

    info!("Running web server...");
    web::run().await
}
