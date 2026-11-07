mod discord;

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;

pub use self::discord::Discord;
use crate::{
    config::{Teams, Users},
    error::Result,
    template::Mention,
};

/// A chat or delivery backend that receives rendered messages, mentioning users in its own format.
#[async_trait]
pub trait Upstream: Mention {
    /// Longest message the platform accepts, in UTF-16 code units.
    fn max_len(&self) -> usize;
    async fn send(&self, text: &str) -> Result<()>;
}

/// Cuts `text` to at most `max` UTF-16 code units, ending with `…` when cut.
// UTF-16: Discord counts code units, and it never undercounts code points.
pub fn truncate(text: &str, max: usize) -> Cow<'_, str> {
    if text.encode_utf16().count() <= max {
        return Cow::Borrowed(text);
    }
    if max == 0 {
        return Cow::Borrowed("");
    }
    let mut len = 0;
    let end = text
        .char_indices()
        .find(|(_, c)| {
            len += c.len_utf16();
            len > max - 1
        })
        .map_or(text.len(), |(i, _)| i);
    Cow::Owned(format!("{}…", text[..end].trim_end()))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Config {
    Discord(discord::Config),
}

pub fn build(
    config: Config,
    users: &Users,
    teams: &Teams,
    http: reqwest::Client,
) -> Arc<dyn Upstream> {
    match config {
        Config::Discord(config) => {
            let user_ids = ids(users, config.users_key.as_deref(), "discord");
            let roles = ids(teams, config.teams_key.as_deref(), "discord");
            Arc::new(Discord::new(config, user_ids, roles, http))
        }
    }
}

/// Lowercased GitHub login or team slug -> id under `key`, else under `platform`.
fn ids(
    names: &HashMap<String, HashMap<String, String>>,
    key: Option<&str>,
    platform: &str,
) -> HashMap<String, String> {
    names
        .iter()
        .filter_map(|(name, ids)| {
            let id = key.and_then(|k| ids.get(k)).or_else(|| ids.get(platform))?;
            Some((name.to_lowercase(), id.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::Teams;

    #[test]
    fn ids_prefer_users_key() {
        let users: Users = HashMap::from([
            (
                "Alice".into(),
                HashMap::from([("discord".into(), "1".into()), ("work".into(), "9".into())]),
            ),
            (
                "bob".into(),
                HashMap::from([("discord".into(), "2".into())]),
            ),
            (
                "carol".into(),
                HashMap::from([("slack".into(), "3".into())]),
            ),
        ]);
        let pairs = |key| {
            let mut v: Vec<_> = ids(&users, key, "discord").into_iter().collect();
            v.sort();
            v
        };
        let pair = |l: &str, i: &str| (l.to_owned(), i.to_owned());
        assert_eq!(pairs(None), [pair("alice", "1"), pair("bob", "2")]);
        assert_eq!(pairs(Some("work")), [pair("alice", "9"), pair("bob", "2")]);
    }

    #[test]
    fn build_prefers_teams_key() {
        let teams: Teams = HashMap::from([
            (
                "Reviewers".into(),
                HashMap::from([
                    ("discord".into(), "1".into()),
                    ("discord-ops".into(), "9".into()),
                ]),
            ),
            ("qa".into(), HashMap::from([("discord".into(), "2".into())])),
            (
                "sales".into(),
                HashMap::from([("slack".into(), "3".into())]),
            ),
        ]);
        let config = serde_json::from_value::<Config>(
            json!({ "type": "discord", "url": "", "teams_key": "discord-ops" }),
        )
        .unwrap();
        let upstream = build(config, &Users::new(), &teams, reqwest::Client::new());
        assert_eq!(upstream.mention_team("reviewers").as_deref(), Some("<@&9>"));
        assert_eq!(upstream.mention_team("QA").as_deref(), Some("<@&2>"));
        assert_eq!(upstream.mention_team("sales"), None);
    }

    #[test]
    fn truncates_by_utf16() {
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello world", 7), "hello…");
        assert_eq!(truncate("🆕🆕🆕", 5), "🆕🆕…");
        let long = "🆕 x".repeat(1000);
        assert!(truncate(&long, 2000).encode_utf16().count() <= 2000);
        assert_eq!(truncate("abc", 0), "");
        assert_eq!(truncate("abc", 1), "…");
    }
}
