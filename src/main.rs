mod app;
mod config;
mod error;
mod event;
mod logger;
mod server;
mod template;
mod upstream;

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;

use crate::{app::App, config::Config, error::Result};

/// Forward GitHub webhooks to chat upstreams with templated messages and user mentions.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the YAML config file.
    #[arg(short, long, env = "OCTOPING_CONFIG", default_value = "octoping.yaml")]
    config: PathBuf,

    #[command(flatten)]
    log: logger::Config,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logger::init(&cli.log);
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Not `error!`: a log filter must not hide why startup failed.
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let config = Config::load(&cli.config)?;
    let listen = config.listen;
    let secret = config.secret.clone();
    let app = App::new(config)?;
    let listener = TcpListener::bind(listen).await?;
    info!(%listen, "octoping listening");
    axum::serve(listener, server::router(app, secret)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn parses_cli() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from([
            "octoping",
            "-l",
            "warn,octoping=debug",
            "--log-format",
            "json",
        ]);
        assert!(cli.is_ok());
        assert!(Cli::try_parse_from(["octoping", "-l", "octoping=loud"]).is_err());
        assert!(Cli::try_parse_from(["octoping", "--log-format", "xml"]).is_err());
    }
}
