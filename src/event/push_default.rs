use serde_json::Value as Json;

use super::Event;

/// A push to the repository's default branch; branch deletions don't count.
pub struct PushDefault;

impl Event for PushDefault {
    fn name(&self) -> &'static str {
        "push_default"
    }

    fn github_event(&self) -> &'static str {
        "push"
    }

    fn matches(&self, payload: &Json) -> bool {
        let branch = payload["ref"]
            .as_str()
            .and_then(|r| r.strip_prefix("refs/heads/"));
        payload["deleted"] != true
            && branch.is_some()
            && branch == payload["repository"]["default_branch"].as_str()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_default_branch_only() {
        let push = |r| json!({ "ref": r, "repository": { "default_branch": "main" } });
        assert!(PushDefault.matches(&push("refs/heads/main")));
        assert!(!PushDefault.matches(&push("refs/heads/feature")));
        assert!(!PushDefault.matches(&push("refs/tags/main")));
        assert!(!PushDefault.matches(&json!({ "ref": "refs/heads/main", "repository": {} })));
        let mut deleted = push("refs/heads/main");
        deleted["deleted"] = json!(true);
        assert!(!PushDefault.matches(&deleted));
    }
}
