use std::str::FromStr;

use clap::{Args, ValueEnum};
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Args)]
pub struct Config {
    /// Log filter, e.g. `info` or `warn,octoping=debug`.
    #[arg(long, short, env = "RUST_LOG", default_value = "info", value_parser = Targets::from_str)]
    pub log_level: Targets,

    /// Log output format.
    #[arg(long, value_enum, env = "LOG_FORMAT", default_value_t = Format::Compact)]
    pub log_format: Format,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Compact,
    Full,
    Json,
    Pretty,
}

pub fn init(config: &Config) {
    let registry = tracing_subscriber::registry().with(config.log_level.clone());
    let fmt = tracing_subscriber::fmt::layer().with_line_number(true);
    match config.log_format {
        Format::Compact => registry.with(fmt.compact()).init(),
        Format::Full => registry.with(fmt).init(),
        Format::Json => registry.with(fmt.json()).init(),
        Format::Pretty => registry.with(fmt.pretty()).init(),
    }
}
