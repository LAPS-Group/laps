#[macro_use]
extern crate tracing;

#[macro_use]
extern crate lazy_static;

use darkredis::ConnectionPool;
use tracing::Level;

mod module_handling;
mod util;
mod web;

lazy_static! {
    static ref REDIS_POOL: ConnectionPool = {
        let span = span!(Level::INFO, "redis");
        let _guard = span.enter();
        info!("Starting connection pool...");
        let pool = futures::executor::block_on(async {
            ConnectionPool::create("127.0.0.1:6379".into(), None, 2).await
        });
        match pool {
            Ok(p) => {
                info!("Successfully connected to Redis");
                p
            }
            Err(e) => {
                error!("Failed to connect to Redis: {:?}", e);
                std::process::exit(1);
            }
        }
    };
}

#[instrument]
fn setup_tracing() {
    tracing_subscriber::fmt::init();

    info!("Successfully initialized tracing");
}

#[tokio::main]
async fn main() {
    setup_tracing();

    // Launch module handling logic
    tokio::spawn(module_handling::run());

    info!("Running web server...");
    web::run().await
}
