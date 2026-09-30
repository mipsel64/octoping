use std::{collections::HashMap, net::SocketAddr, path::Path};

use config::{Environment, File, FileFormat, Source};
use serde::Deserialize;

use crate::{
    error::{Error, Result},
    upstream,
};

/// GitHub login -> platform (e.g. `discord`) -> user id on that platform.
pub type Users = HashMap<String, HashMap<String, String>>;

#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    pub secret: String,
    #[serde(default)]
    pub users: Users,
    pub upstreams: HashMap<String, upstream::Config>,
    pub events: HashMap<String, Route>,
}

#[derive(Deserialize)]
pub struct Route {
    pub to: Vec<String>,
    pub template: String,
}

impl Config {
    /// Reads YAML from `path`; `OCTOPING__<PATH>` env vars override scalar values, e.g. `OCTOPING__UPSTREAMS__DEV__URL`.
    pub fn load(path: &Path) -> Result<Self> {
        let config = Self::from_source(File::from(path).format(FileFormat::Yaml))?;
        if config.secret.is_empty() {
            return Err(Error::InvalidConfig("secret must not be empty".into()));
        }
        Ok(config)
    }

    fn from_source(file: impl Source + Send + Sync + 'static) -> Result<Self> {
        let env = Environment::with_prefix("OCTOPING")
            .prefix_separator("__")
            .separator("__");
        let config = config::Config::builder()
            .add_source(file)
            .add_source(env)
            .build()?;
        Ok(config.try_deserialize()?)
    }

    #[cfg(test)]
    pub fn from_yaml(text: &str) -> Result<Self> {
        Self::from_source(File::from_str(text, FileFormat::Yaml))
    }
}

fn default_listen() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8080))
}
