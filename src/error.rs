pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot load config: {0}")]
    Config(#[from] config::ConfigError),
    #[error("cannot use config: {0}")]
    InvalidConfig(String),
    #[error("cannot render template: {0}")]
    Template(#[from] minijinja::Error),
    #[error("cannot reach upstream: {0}")]
    Http(#[from] reqwest::Error),
    #[error("cannot start server: {0}")]
    Io(#[from] std::io::Error),
}
