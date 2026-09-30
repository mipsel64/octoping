mod pr_approved;
mod pr_merged;
mod pr_opened;
mod push_default;

use serde_json::Value as Json;

pub use self::{
    pr_approved::PrApproved, pr_merged::PrMerged, pr_opened::PrOpened, push_default::PushDefault,
};

/// A notification-worthy GitHub webhook delivery.
pub trait Event: Send + Sync {
    /// Name used as the key under `events` in the config.
    fn name(&self) -> &'static str;
    /// Value of the `X-GitHub-Event` header this event is derived from.
    fn github_event(&self) -> &'static str;
    fn matches(&self, payload: &Json) -> bool;
}

pub fn builtin() -> Vec<Box<dyn Event>> {
    vec![
        Box::new(PrOpened),
        Box::new(PrApproved),
        Box::new(PrMerged),
        Box::new(PushDefault),
    ]
}
