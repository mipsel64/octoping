use std::collections::{BTreeSet, HashMap};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::Upstream;
use crate::{error::Result, template::Mention};

const MAX_CONTENT_LEN: usize = 2000;
// Discord caps both allowed_mentions.users and .roles at 100.
const MAX_ALLOWED_MENTIONS: usize = 100;

#[derive(Deserialize)]
pub struct Config {
    pub url: String,
    /// `users` key holding this server's ids, for people with a second Discord account; logins
    /// without it fall back to `discord`.
    pub users_key: Option<String>,
    /// `teams` key holding this server's role ids (role ids are per server); teams without it
    /// fall back to `discord`.
    pub teams_key: Option<String>,
}

pub struct Discord {
    url: String,
    ids: HashMap<String, String>,
    roles: HashMap<String, String>,
    http: reqwest::Client,
}

impl Discord {
    pub fn new(
        config: Config,
        ids: HashMap<String, String>,
        roles: HashMap<String, String>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            url: config.url,
            ids,
            roles,
            http,
        }
    }

    fn body(&self, text: &str) -> serde_json::Value {
        // Ping only mapped users and roles the template mentioned; payload text can't ping @everyone or strangers.
        let users = mentioned(&self.ids, text, "<@");
        let roles = mentioned(&self.roles, text, "<@&");
        json!({ "content": text, "allowed_mentions": { "parse": [], "users": users, "roles": roles } })
    }
}

/// Distinct mapped ids that `text` mentions as `{prefix}{id}>`. Logins or teams can share an id,
/// so dedupe before Discord's cap.
fn mentioned<'a>(ids: &'a HashMap<String, String>, text: &str, prefix: &str) -> Vec<&'a String> {
    let found: BTreeSet<_> = ids
        .values()
        .filter(|id| text.contains(&format!("{prefix}{id}>")))
        .collect();
    found.into_iter().take(MAX_ALLOWED_MENTIONS).collect()
}

impl Mention for Discord {
    fn mention(&self, login: &str) -> Option<String> {
        self.ids
            .get(&login.to_lowercase())
            .map(|id| format!("<@{id}>"))
    }

    fn mention_team(&self, slug: &str) -> Option<String> {
        self.roles
            .get(&slug.to_lowercase())
            .map(|id| format!("<@&{id}>"))
    }
}

#[async_trait]
impl Upstream for Discord {
    fn max_len(&self) -> usize {
        MAX_CONTENT_LEN
    }

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
    fn allows_only_mentioned_mapped_users_and_roles() {
        let ids = HashMap::from([
            ("octocat".into(), "42".into()),
            ("other".into(), "7".into()),
        ]);
        let roles = HashMap::from([("reviewers".into(), "5".into()), ("ops".into(), "8".into())]);
        let discord = Discord::new(
            Config {
                url: String::new(),
                users_key: None,
                teams_key: None,
            },
            ids,
            roles,
            reqwest::Client::new(),
        );
        assert_eq!(discord.mention("OctoCat").as_deref(), Some("<@42>"));
        assert_eq!(discord.mention("ghost"), None);
        assert_eq!(discord.mention_team("Reviewers").as_deref(), Some("<@&5>"));
        assert_eq!(discord.mention_team("ghosts"), None);
        let body = discord.body("<@42> @everyone <@999> <@&5> <@&999>");
        assert_eq!(
            body["allowed_mentions"],
            json!({ "parse": [], "users": ["42"], "roles": ["5"] })
        );
        // `<@5>` is user syntax: it must not allow role 5.
        assert_eq!(
            discord.body("<@5>")["allowed_mentions"],
            json!({ "parse": [], "users": [], "roles": [] })
        );
    }

    #[test]
    fn allows_each_shared_id_once() {
        let discord = Discord::new(
            Config {
                url: String::new(),
                users_key: None,
                teams_key: None,
            },
            HashMap::from([
                ("alice".into(), "42".into()),
                ("alice-work".into(), "42".into()),
            ]),
            HashMap::from([("backend".into(), "5".into()), ("leads".into(), "5".into())]),
            reqwest::Client::new(),
        );
        assert_eq!(
            discord.body("<@42> <@&5>")["allowed_mentions"],
            json!({ "parse": [], "users": ["42"], "roles": ["5"] })
        );
    }
}
