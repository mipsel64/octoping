use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::Value as Json;

use crate::{
    config::Config,
    error::{Error, Result},
    event::{self, Event},
    template::{MiniJinja, Template},
    upstream::{self, Upstream},
};

/// Below GitHub's 10 s delivery timeout.
const SEND_TIMEOUT: Duration = Duration::from_secs(8);

pub struct App {
    routes: Vec<Route>,
}

struct Route {
    event: Box<dyn Event>,
    template: Box<dyn Template>,
    upstreams: Vec<(String, Arc<dyn Upstream>)>,
}

#[derive(Default)]
pub struct Report {
    pub sent: Vec<&'static str>,
    pub errors: Vec<String>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let Config {
            users,
            upstreams,
            mut events,
            ..
        } = config;
        let http = reqwest::Client::builder().timeout(SEND_TIMEOUT).build()?;
        let upstreams: HashMap<_, _> = upstreams
            .into_iter()
            .map(|(name, config)| (name, upstream::build(config, &users, http.clone())))
            .collect();

        let mut routes = Vec::new();
        for event in event::builtin() {
            let Some(route) = events.remove(event.name()) else {
                continue;
            };
            let targets = route
                .to
                .into_iter()
                .map(|name| match upstreams.get(&name) {
                    Some(upstream) => Ok((name, upstream.clone())),
                    None => Err(Error::InvalidConfig(format!(
                        "event {}: unknown upstream {name:?}",
                        event.name()
                    ))),
                })
                .collect::<Result<_>>()?;
            let template = MiniJinja::new(route.template)
                .map_err(|e| Error::InvalidConfig(format!("event {}: {e}", event.name())))?;
            routes.push(Route {
                event,
                template: Box::new(template),
                upstreams: targets,
            });
        }
        if let Some(name) = events.keys().next() {
            let known: Vec<_> = event::builtin().iter().map(|e| e.name()).collect();
            return Err(Error::InvalidConfig(format!(
                "unknown event {name:?}, expected one of {known:?}"
            )));
        }
        Ok(Self { routes })
    }

    pub async fn dispatch(&self, github_event: &str, payload: &Json) -> Report {
        let mut report = Report::default();
        let routes = self
            .routes
            .iter()
            .filter(|r| r.event.github_event() == github_event);
        for route in routes.filter(|r| r.event.matches(payload)) {
            let name = route.event.name();
            for (upstream_name, upstream) in &route.upstreams {
                let result = match route.template.render(payload, upstream.clone()) {
                    Ok(text) if text.is_empty() => continue,
                    Ok(text) => upstream.send(&text).await,
                    Err(e) => Err(e),
                };
                if let Err(e) = result {
                    eprintln!("{name} -> {upstream_name}: {e}");
                    report.errors.push(format!("{upstream_name}: {e}"));
                }
            }
            report.sent.push(name);
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const EXAMPLE: &str = include_str!("../octoping.example.yaml");

    fn render(app: &App, event: &str, payload: Json) -> String {
        let route = app.routes.iter().find(|r| r.event.name() == event).unwrap();
        route
            .template
            .render(&payload, route.upstreams[0].1.clone())
            .unwrap()
    }

    #[test]
    fn example_config_renders() {
        let app = App::new(Config::from_yaml(EXAMPLE).unwrap()).unwrap();
        let repo = json!({ "full_name": "o/r", "default_branch": "main" });
        let pr = json!({ "number": 1, "title": "T", "html_url": "u", "draft": false, "user": { "login": "octocat" } });
        let commits = json!([{ "id": "abcdef123", "url": "c", "message": "fix\n\nbody" }]);
        let push =
            json!({ "sender": { "login": "octocat" }, "repository": repo, "commits": commits });
        assert_eq!(
            render(&app, "push_default", push.clone()),
            "📦 <@123456789012345678> pushed to `o/r:main`\n- [`abcdef1`](<c>) fix"
        );
        let mut empty = push;
        empty["commits"] = json!([]);
        assert_eq!(render(&app, "push_default", empty), "");

        let opened = json!({ "sender": { "login": "x" }, "pull_request": pr, "repository": repo });
        assert!(render(&app, "pr_opened", opened.clone()).starts_with("🆕 x opened"));
        let mut draft = opened;
        draft["pull_request"]["draft"] = json!(true);
        assert_eq!(render(&app, "pr_opened", draft), "");

        let approved = json!({ "review": { "user": { "login": "x" } }, "pull_request": pr });
        assert!(render(&app, "pr_approved", approved).ends_with("by <@123456789012345678>"));
    }

    #[test]
    fn rejects_bad_config() {
        let app = |yaml: &str| App::new(Config::from_yaml(yaml).unwrap());
        assert!(app(&EXAMPLE.replace("pr_opened:", "pr_merged:")).is_err());
        assert!(app(&EXAMPLE.replacen("to: [dev]", "to: [nope]", 1)).is_err());
        assert!(app(&EXAMPLE.replacen("{% if not pull_request.draft %}", "{% if %}", 1)).is_err());
    }
}
