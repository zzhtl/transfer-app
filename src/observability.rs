use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub fn init(log_filter: &str) {
    let filter = EnvFilter::try_new(log_filter)
        .unwrap_or_else(|_| EnvFilter::new("info,transfer_app=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_target(true)
                .with_thread_ids(false)
                .with_file(false),
        )
        .init();
}
