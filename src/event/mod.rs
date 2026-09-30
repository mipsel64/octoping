mod expression;

use serde_json::Value as Json;

pub use self::expression::Expression;

/// A notification-worthy GitHub webhook delivery.
pub trait Event: Send + Sync {
    /// Key under `events` in the config.
    fn name(&self) -> &str;
    /// Value of the `X-GitHub-Event` header this event is derived from.
    fn github_event(&self) -> &str;
    fn matches(&self, payload: &Json) -> bool;
}
