pub mod cli;
pub mod config;
pub mod daemon;
pub mod db;
pub mod ipc;
pub mod jira;
pub mod jira_sync;
pub mod popup;
pub mod platform;
pub mod rules;
pub mod time;
pub mod tui;
pub mod gui;

pub fn init_logging() {
    // Default to INFO, but allow override via RUST_LOG or provided level from CLI
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
