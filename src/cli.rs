use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "lazytime", version, about = "Automatic Sway time tracking")]
pub struct CliArgs {
    #[arg(long, short = 'c')]
    pub config: Option<String>,

    #[arg(long)]
    pub tui: bool,

    /// Set global log level (overrides config/env). Example: --loglevel=DEBUG
    #[arg(long, default_value_t = String::from("INFO"))]
    pub loglevel: String,

    #[arg(long)]
    pub gui: bool,

    #[arg(long)]
    pub daemon: bool,

    #[arg(long)]
    pub summary: bool,

    #[arg(long)]
    pub watch: bool,

    #[arg(long)]
    pub report: bool,

    #[arg(long)]
    pub jira_sync: bool,

    /// Output a single JSON object suitable for waybar state module
    #[arg(long)]
    pub waybar_state: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub start: Option<String>,

    #[arg(long)]
    pub end: Option<String>,
}
