mod discord;

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;

pub use self::discord::Discord;
use crate::{config::Users, error::Result, template::Mention};

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

#[cfg(test)]
mod tests {
    use super::*;

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
