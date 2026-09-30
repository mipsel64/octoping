use serde_json::Value as Json;

use super::Event;

pub struct PrOpened;

impl Event for PrOpened {
    fn name(&self) -> &'static str {
        "pr_opened"
    }

    fn github_event(&self) -> &'static str {
        "pull_request"
    }

    fn matches(&self, payload: &Json) -> bool {
        payload["action"] == "opened"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_opened_only() {
        assert!(PrOpened.matches(&json!({ "action": "opened" })));
        assert!(!PrOpened.matches(&json!({ "action": "closed" })));
    }
}
