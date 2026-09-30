mod app;
mod config;
mod error;
mod event;
mod logger;
mod reload;
mod server;
mod template;
mod upstream;

use std::{path::PathBuf, process::ExitCode, time::Duration};

use clap::Parser;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
};
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

/// Kubelet refreshes ConfigMap volumes about once a minute, so polling faster adds nothing.
const RELOAD_EVERY: Duration = Duration::from_secs(10);

async fn run(cli: Cli) -> Result<()> {
    let text = Config::read(&cli.config)?;
    let app = App::new(Config::parse(&text)?)?;
    let listen = app.listen;
    let shared = reload::shared(app);
    tokio::spawn(reload::watch(
        cli.config,
        text,
        shared.clone(),
        RELOAD_EVERY,
    ));
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let listener = TcpListener::bind(listen).await?;
    info!(%listen, "octoping listening");
    // Graceful: SIGTERM (k8s pod stop) or Ctrl-C lets in-flight deliveries finish.
    let shutdown = async move {
        tokio::select! {
            _ = int.recv() => {}
            _ = term.recv() => {}
        }
        info!("shutting down");
    };
    axum::serve(listener, server::router(shared))
        .with_graceful_shutdown(shutdown)
        .await?;
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
