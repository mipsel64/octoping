use minijinja::Environment;
use serde_json::Value as Json;
use tracing::warn;

use super::Event;
use crate::error::Result;

const NAME: &str = "when";

/// Matches payloads for which a MiniJinja expression over the payload is truthy.
pub struct Expression {
    name: String,
    github_event: String,
    env: Environment<'static>,
}

impl Expression {
    pub fn new(name: String, github_event: String, when: Option<&str>) -> Result<Self> {
        let mut env = Environment::new();
        let when = when.unwrap_or("true");
        env.add_template_owned(NAME, format!("{{% if {when} %}}1{{% endif %}}"))?;
        Ok(Self {
            name,
            github_event,
            env,
        })
    }
}

impl Event for Expression {
    fn name(&self) -> &str {
        &self.name
    }

    fn github_event(&self) -> &str {
        &self.github_event
    }

    fn matches(&self, payload: &Json) -> bool {
        match self.env.get_template(NAME).and_then(|t| t.render(payload)) {
            Ok(out) => out == "1",
            Err(e) => {
                warn!(event = self.name, "cannot evaluate when: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_expression() {
        let event = |when| Expression::new("e".into(), "push".into(), when).unwrap();
        let payload = json!({ "action": "closed", "pull_request": { "merged": true } });
        assert!(event(None).matches(&payload));
        assert!(event(Some(r#"action == "closed" and pull_request.merged"#)).matches(&payload));
        assert!(!event(Some(r#"action == "opened""#)).matches(&payload));
        assert!(!event(Some("missing.field")).matches(&payload));
        assert!(Expression::new("e".into(), "push".into(), Some("a ==")).is_err());
    }
}
