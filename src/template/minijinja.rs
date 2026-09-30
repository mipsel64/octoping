use std::sync::Arc;

use minijinja::{Environment, Value, context};
use serde_json::Value as Json;

use super::{Mention, Template};
use crate::error::Result;

const NAME: &str = "message";

pub struct MiniJinja {
    env: Environment<'static>,
}

impl MiniJinja {
    pub fn new(source: String) -> Result<Self> {
        let mut env = Environment::new();
        env.add_filter("coauthors", coauthors);
        env.add_template_owned(NAME, source)?;
        Ok(Self { env })
    }
}

impl Template for MiniJinja {
    fn render(&self, payload: &Json, mention: Arc<dyn Mention>) -> Result<String> {
        // Option: a null or missing login renders as nothing instead of failing the delivery forever.
        let mention = Value::from_function(move |login: Option<&str>| match login {
            Some(login) => mention.mention(login).unwrap_or_else(|| login.to_owned()),
            None => String::new(),
        });
        let ctx = context! { mention, ..Value::from_serialize(payload) };
        let text = self.env.get_template(NAME)?.render(ctx)?;
        Ok(text.trim().to_owned())
    }
}

/// `Co-authored-by: Name <email>` trailers as `{name, login}`; the login is only known for
/// GitHub no-reply emails (`12345+login@users.noreply.github.com`).
fn coauthors(message: &str) -> Vec<Value> {
    message
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, rest) = line.split_once(':')?;
            if !key.eq_ignore_ascii_case("co-authored-by") {
                return None;
            }
            let (name, email) = rest.split_once('<')?;
            let login = email
                .trim_end()
                .strip_suffix('>')?
                .strip_suffix("@users.noreply.github.com")
                .map(|local| local.rsplit('+').next().unwrap_or(local));
            Some(context! { name => name.trim(), login })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct Test;

    impl Mention for Test {
        fn mention(&self, login: &str) -> Option<String> {
            (login == "octocat").then(|| "@octo".into())
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
    fn lists_coauthors() {
        let template = MiniJinja::new(
            "{% for a in m | coauthors %}{{ mention(a.login) or a.name }};{% endfor %}".into(),
        )
        .unwrap();
        let m = "Fix\n\nbody\nCo-authored-by: Octo Cat <1+octocat@users.noreply.github.com>\n\
                 co-authored-by: Old <ghost@users.noreply.github.com>\n\
                 Co-authored-by: Jane Doe <jane@example.com>\nCo-authored-by: broken";
        assert_eq!(
            template.render(&json!({ "m": m }), Arc::new(Test)).unwrap(),
            "@octo;ghost;Jane Doe;"
        );
    }

    #[test]
    fn rejects_bad_syntax() {
        assert!(MiniJinja::new("{% if %}".into()).is_err());
    }
}
