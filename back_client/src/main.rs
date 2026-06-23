mod acquisition;
mod config;
mod error;
mod model;
mod storage;
mod web;

use std::time::Duration;

use config::Config;
use error::Result;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("labori=info,tower_http=info")),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("LABORI_CONFIG").ok())
        .unwrap_or_else(|| "config.toml".to_string());
    let config = Config::from_file(config_path)?;

    let (pool, storage) = storage::open(
        &config.database_path,
        config.storage_queue_capacity,
        config.storage_batch_size,
        Duration::from_millis(config.storage_flush_millis),
    )
    .await?;
    let (live, _) = broadcast::channel(16_384);
    let controller = acquisition::spawn(config.clone(), storage.clone(), live.clone());

    let web_result = web::serve(
        &config.listen_addr,
        &config.web_root,
        controller.clone(),
        pool,
        live,
    )
    .await;

    if controller.status().await.running {
        let _ = controller.stop().await;
        while controller.status().await.running {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    storage.shutdown().await?;
    web_result
}
