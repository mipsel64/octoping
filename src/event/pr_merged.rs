use serde_json::Value as Json;

use super::Event;

pub struct PrMerged;

impl Event for PrMerged {
    fn name(&self) -> &'static str {
        "pr_merged"
    }

    fn github_event(&self) -> &'static str {
        "pull_request"
    }

    fn matches(&self, payload: &Json) -> bool {
        payload["action"] == "closed" && payload["pull_request"]["merged"] == true
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_merged_close_only() {
        let closed = |merged| json!({ "action": "closed", "pull_request": { "merged": merged } });
        assert!(PrMerged.matches(&closed(true)));
        assert!(!PrMerged.matches(&closed(false)));
        assert!(
            !PrMerged.matches(&json!({ "action": "opened", "pull_request": { "merged": true } }))
        );
    }
}
