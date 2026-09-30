mod minijinja;

use std::sync::Arc;

use serde_json::Value as Json;

pub use self::minijinja::MiniJinja;
use crate::{error::Result, upstream::Upstream};

/// A message template, rendered once per upstream so mentions use that upstream's format.
pub trait Template: Send + Sync {
    /// An empty result means "don't send".
    fn render(&self, payload: &Json, upstream: Arc<dyn Upstream>) -> Result<String>;
}
