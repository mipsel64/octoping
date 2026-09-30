mod minijinja;

use std::sync::Arc;

use serde_json::Value as Json;

pub use self::minijinja::MiniJinja;
use crate::error::Result;

/// Formats a GitHub login as a mention on some platform.
pub trait Mention: Send + Sync {
    /// `None` when the login isn't mapped on this platform.
    fn mention(&self, login: &str) -> Option<String>;
}

/// A message template, rendered once per upstream so mentions use that upstream's format.
pub trait Template: Send + Sync {
    /// An empty result means "don't send".
    fn render(&self, payload: &Json, mention: Arc<dyn Mention>) -> Result<String>;
}
