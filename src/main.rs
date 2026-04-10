use transfer_app::config::AppConfig;
use transfer_app::observability;
use transfer_app::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()?;

    observability::init(&config.log_filter);

    server::run(config).await
}
