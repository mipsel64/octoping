use std::sync::Arc;

use minijinja::{Environment, Value, context};
use serde_json::Value as Json;

use super::Template;
use crate::{error::Result, upstream::Upstream};

const NAME: &str = "message";

pub struct MiniJinja {
    env: Environment<'static>,
}

impl MiniJinja {
    pub fn new(source: String) -> Result<Self> {
        let mut env = Environment::new();
        env.add_template_owned(NAME, source)?;
        Ok(Self { env })
    }
}

impl Template for MiniJinja {
    fn render(&self, payload: &Json, upstream: Arc<dyn Upstream>) -> Result<String> {
        // Option: a null or missing login renders as nothing instead of failing the delivery forever.
        let mention = Value::from_function(move |login: Option<&str>| match login {
            Some(login) => upstream.mention(login).unwrap_or_else(|| login.to_owned()),
            None => String::new(),
        });
        let ctx = context! { mention, ..Value::from_serialize(payload) };
        let text = self.env.get_template(NAME)?.render(ctx)?;
        Ok(text.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;

    struct Test;

    #[async_trait]
    impl Upstream for Test {
        fn mention(&self, login: &str) -> Option<String> {
            (login == "octocat").then(|| "@octo".into())
        }

        async fn send(&self, text: &str) -> Result<()> {
            let _ = text;
            Ok(())
        }
    }

    #[test]
    fn renders_mentions_and_payload() {
        let template = MiniJinja::new(
            "{{ mention(a) }} {{ mention(b) }}{{ mention(missing) }} #{{ n }}\n".into(),
        )
        .unwrap();
        let payload = json!({ "a": "octocat", "b": "ghost", "n": 7 });
        assert_eq!(
            template.render(&payload, Arc::new(Test)).unwrap(),
            "@octo ghost #7"
        );
    }

    #[test]
    fn rejects_bad_syntax() {
        assert!(MiniJinja::new("{% if %}".into()).is_err());
    }
}
