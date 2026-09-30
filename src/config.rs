use std::{collections::HashMap, net::SocketAddr, path::Path};

use config::{Environment, File, FileFormat};
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
    /// `X-GitHub-Event` header value, e.g. `pull_request`.
    pub event: String,
    /// MiniJinja expression over the payload; the event fires when it is truthy.
    pub when: Option<String>,
    pub to: Vec<String>,
    pub template: String,
}

impl Config {
    /// Reads YAML from `path`, expanding `$VAR`/`${VAR}` (unset vars are left as is);
    /// `OCTOPING__<PATH>` env vars then override scalar values, e.g. `OCTOPING__UPSTREAMS__DEV__URL`.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::ReadConfig {
            path: path.into(),
            source,
        })?;
        let config = Self::from_yaml(&text)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // An unset `${VAR}` stays literal, which would make a guessable secret.
        if self.secret.is_empty() || self.secret.contains('$') {
            return Err(Error::InvalidConfig(
                "secret must be set and not contain `$`".into(),
            ));
        }
        Ok(())
    }

    pub fn from_yaml(text: &str) -> Result<Self> {
        let text = shellexpand::env_with_context_no_errors(text, |var| std::env::var(var).ok());
        let env = Environment::with_prefix("OCTOPING")
            .prefix_separator("__")
            .separator("__");
        let config = config::Config::builder()
            .add_source(File::from_str(&text, FileFormat::Yaml))
            .add_source(env)
            .build()?;
        Ok(config.try_deserialize()?)
    }
}

fn default_listen() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8080))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_env_vars() {
        let yaml = "secret: ${HOME}-${OCTOPING_TEST_UNSET}\nupstreams: {}\nevents: {}\n";
        let config = Config::from_yaml(yaml).unwrap_or_else(|e| panic!("{e}"));
        let home = std::env::var("HOME").unwrap();
        assert_eq!(config.secret, format!("{home}-${{OCTOPING_TEST_UNSET}}"));
        assert!(config.validate().is_err());
    }
}
