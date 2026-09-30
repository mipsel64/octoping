use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::Upstream;
use crate::{error::Result, template::Mention};

const MAX_CONTENT_CHARS: usize = 2000;
const MAX_ALLOWED_USERS: usize = 100;

#[derive(Deserialize)]
pub struct Config {
    pub url: String,
}

pub struct Discord {
    url: String,
    ids: HashMap<String, String>,
    http: reqwest::Client,
}

impl Discord {
    pub fn new(config: Config, ids: HashMap<String, String>, http: reqwest::Client) -> Self {
        Self {
            url: config.url,
            ids,
            http,
        }
    }

    fn body(&self, text: &str) -> serde_json::Value {
        let content: String = text.chars().take(MAX_CONTENT_CHARS).collect();
        // Ping only mapped users the template mentioned; payload text can't ping @everyone, roles or strangers.
        let users: Vec<_> = self
            .ids
            .values()
            .filter(|id| content.contains(&format!("<@{id}>")))
            .take(MAX_ALLOWED_USERS)
            .collect();
        json!({ "content": content, "allowed_mentions": { "parse": [], "users": users } })
    }
}

impl Mention for Discord {
    fn mention(&self, login: &str) -> Option<String> {
        self.ids
            .get(&login.to_lowercase())
            .map(|id| format!("<@{id}>"))
    }
}

#[async_trait]
impl Upstream for Discord {
    async fn send(&self, text: &str) -> Result<()> {
        // without_url: the webhook URL embeds its token, and errors are echoed back to GitHub.
        let response = self
            .http
            .post(&self.url)
            .json(&self.body(text))
            .send()
            .await
            .map_err(reqwest::Error::without_url)?;
        response
            .error_for_status()
            .map_err(reqwest::Error::without_url)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_mentioned_mapped_users() {
        let ids = HashMap::from([
            ("octocat".into(), "42".into()),
            ("other".into(), "7".into()),
        ]);
        let discord = Discord::new(Config { url: String::new() }, ids, reqwest::Client::new());
        assert_eq!(discord.mention("OctoCat").as_deref(), Some("<@42>"));
        assert_eq!(discord.mention("ghost"), None);
        let body = discord.body("<@42> @everyone <@999>");
        assert_eq!(
            body["allowed_mentions"],
            json!({ "parse": [], "users": ["42"] })
        );
        assert_eq!(
            discord.body(&"x".repeat(3000))["content"]
                .as_str()
                .unwrap()
                .len(),
            MAX_CONTENT_CHARS
        );
    }
}
