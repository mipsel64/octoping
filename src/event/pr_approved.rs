use serde_json::Value as Json;

use super::Event;

pub struct PrApproved;

impl Event for PrApproved {
    fn name(&self) -> &'static str {
        "pr_approved"
    }

    fn github_event(&self) -> &'static str {
        "pull_request_review"
    }

    fn matches(&self, payload: &Json) -> bool {
        payload["action"] == "submitted" && payload["review"]["state"] == "approved"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_approved_reviews_only() {
        let review = |state| json!({ "action": "submitted", "review": { "state": state } });
        assert!(PrApproved.matches(&review("approved")));
        assert!(!PrApproved.matches(&review("commented")));
        assert!(
            !PrApproved
                .matches(&json!({ "action": "dismissed", "review": { "state": "approved" } }))
        );
    }
}
