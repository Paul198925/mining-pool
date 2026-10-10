pub mod config;
pub mod db;
pub mod rpc;

use tracing_subscriber::EnvFilter;

/// Log level comes from `RUST_LOG`, defaulting to `info`.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
