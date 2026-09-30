mod discord;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;

pub use self::discord::Discord;
use crate::{config::Users, error::Result, template::Mention};

/// A chat or delivery backend that receives rendered messages, mentioning users in its own format.
#[async_trait]
pub trait Upstream: Mention {
    async fn send(&self, text: &str) -> Result<()>;
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Config {
    Discord(discord::Config),
}

pub fn build(config: Config, users: &Users, http: reqwest::Client) -> Arc<dyn Upstream> {
    match config {
        Config::Discord(config) => Arc::new(Discord::new(config, ids(users, "discord"), http)),
    }
}

/// Lowercased GitHub login -> user id on `platform`.
fn ids(users: &Users, platform: &str) -> HashMap<String, String> {
    users
        .iter()
        .filter_map(|(login, ids)| Some((login.to_lowercase(), ids.get(platform)?.clone())))
        .collect()
}
