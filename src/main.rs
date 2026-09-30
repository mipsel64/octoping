mod app;
mod config;
mod error;
mod event;
mod server;
mod template;
mod upstream;

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use tokio::net::TcpListener;

use crate::{app::App, config::Config, error::Result};

/// Forward GitHub webhooks to chat upstreams with templated messages and user mentions.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the YAML config file.
    #[arg(short, long, env = "OCTOPING_CONFIG", default_value = "octoping.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
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
    println!("octoping listening on {listen}");
    axum::serve(listener, server::router(app, secret)).await?;
    Ok(())
}
