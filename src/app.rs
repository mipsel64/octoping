use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use serde_json::Value as Json;
use tracing::warn;

use crate::{
    config::{Config, HotReload},
    error::{Error, Result},
    event::{Event, Expression},
    template::{MiniJinja, Template},
    upstream::{self, Upstream},
};

/// Below GitHub's 10 s delivery timeout.
const SEND_TIMEOUT: Duration = Duration::from_secs(8);

pub struct App {
    pub listen: SocketAddr,
    pub secret: String,
    pub hot_reload: HotReload,
    routes: Vec<Route>,
}

struct Route {
    event: Box<dyn Event>,
    template: Box<dyn Template>,
    upstreams: Vec<(String, Arc<dyn Upstream>)>,
}

#[derive(Default)]
pub struct Report {
    pub sent: Vec<String>,
    pub errors: Vec<String>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let Config {
            listen,
            secret,
            hot_reload,
            users,
            teams,
            upstreams,
            events,
            ..
        } = config;
        let http = reqwest::Client::builder().timeout(SEND_TIMEOUT).build()?;
        let upstreams: HashMap<_, _> = upstreams
            .into_iter()
            .map(|(name, config)| (name, upstream::build(config, &users, &teams, http.clone())))
            .collect();

        let mut routes = Vec::new();
        for (name, route) in events {
            let invalid =
                |e: &dyn std::fmt::Display| Error::InvalidConfig(format!("event {name}: {e}"));
            let targets = route
                .to
                .into_iter()
                .map(|upstream| match upstreams.get(&upstream) {
                    Some(target) => Ok((upstream, target.clone())),
                    None => Err(invalid(&format!("unknown upstream {upstream:?}"))),
                })
                .collect::<Result<_>>()?;
            let template = MiniJinja::new(route.template).map_err(|e| invalid(&e))?;
            let event = Expression::new(name.clone(), route.event, route.when.as_deref())
                .map_err(|e| invalid(&e))?;
            routes.push(Route {
                event: Box::new(event),
                template: Box::new(template),
                upstreams: targets,
            });
        }
        Ok(Self {
            listen,
            secret,
            hot_reload,
            routes,
        })
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
                    Ok(text) => {
                        upstream
                            .send(&upstream::truncate(&text, upstream.max_len()))
                            .await
                    }
                    Err(e) => Err(e),
                };
                if let Err(e) = result {
                    warn!(event = name, upstream = upstream_name, "{e}");
                    report.errors.push(format!("{upstream_name}: {e}"));
                }
            }
            report.sent.push(name.to_owned());
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
        let commits = json!([{
            "id": "abcdef123", "url": "c", "author": { "name": "Octo", "username": "octocat" },
            "message": "fix\n\nbody\nCo-authored-by: Jane <jane@example.com>",
        }]);
        let push =
            json!({ "sender": { "login": "octocat" }, "repository": repo, "commits": commits });
        assert_eq!(
            render(&app, "push_default", push.clone()),
            "📦 <@123456789012345678> pushed to `o/r:main`\n\
             - [`abcdef1`](<c>) fix · <@123456789012345678>, Jane"
        );
        let mut empty = push;
        empty["commits"] = json!([]);
        assert_eq!(render(&app, "push_default", empty), "");

        let opened = json!({ "sender": { "login": "x" }, "pull_request": pr, "repository": repo });
        assert!(render(&app, "pr_opened", opened.clone()).starts_with("🆕 x opened"));
        let mut long = opened.clone();
        long["pull_request"]["body"] = json!("b".repeat(5000));
        assert!(render(&app, "pr_opened", long).ends_with(&format!(">>> {}…", "b".repeat(300))));
        let mut ready = opened.clone();
        ready["action"] = json!("ready_for_review");
        assert!(render(&app, "pr_opened", ready).starts_with("🆕 x marked ready for review"));
        let mut draft = opened;
        draft["pull_request"]["draft"] = json!(true);
        assert_eq!(render(&app, "pr_opened", draft), "");

        let approved = json!({ "review": { "user": { "login": "x" } }, "pull_request": pr });
        assert!(render(&app, "pr_approved", approved).ends_with("by <@123456789012345678>"));

        let mut merged_pr = pr.clone();
        merged_pr["merged_by"] = json!({ "login": "x" });
        merged_pr["base"] = json!({ "ref": "main" });
        let mut merged = json!({ "pull_request": merged_pr, "repository": repo });
        assert_eq!(
            render(&app, "pr_merged", merged.clone()),
            "🔀 x merged [#1 T](<u>) into `o/r:main` by <@123456789012345678>"
        );
        merged["pull_request"]["merged_by"]["login"] = json!("octocat");
        assert_eq!(
            render(&app, "pr_merged", merged),
            "🔀 <@123456789012345678> merged [#1 T](<u>) into `o/r:main`"
        );

        let request = |extra: Json| {
            let mut p = json!({ "action": "review_requested", "sender": { "login": "x" }, "pull_request": pr, "repository": repo });
            p.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            p
        };
        assert_eq!(
            render(
                &app,
                "pr_review_requested",
                request(json!({ "requested_reviewer": { "login": "octocat" } }))
            ),
            "👀 x requested a review from <@123456789012345678> on [#1 T](<u>) in `o/r`"
        );
        assert_eq!(
            render(
                &app,
                "pr_review_requested",
                request(json!({ "requested_team": { "slug": "reviewers" } }))
            ),
            "👀 x requested a review from <@&234567890123456789> on [#1 T](<u>) in `o/r`"
        );
        assert_eq!(
            render(
                &app,
                "pr_review_requested",
                request(json!({ "requested_team": { "slug": "ops" } }))
            ),
            "👀 x requested a review from ops on [#1 T](<u>) in `o/r`"
        );
    }

    fn matched<'a>(app: &'a App, github_event: &str, payload: Json) -> Vec<&'a str> {
        let routes = app
            .routes
            .iter()
            .filter(|r| r.event.github_event() == github_event);
        let mut names: Vec<_> = routes
            .filter(|r| r.event.matches(&payload))
            .map(|r| r.event.name())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn example_config_matches() {
        let app = App::new(Config::from_yaml(EXAMPLE).unwrap()).unwrap();
        let pr = |action, merged| json!({ "action": action, "pull_request": { "merged": merged } });
        assert_eq!(
            matched(&app, "pull_request", pr("opened", false)),
            ["pr_opened"]
        );
        assert_eq!(
            matched(&app, "pull_request", pr("closed", true)),
            ["pr_merged"]
        );
        assert_eq!(
            matched(&app, "pull_request", pr("ready_for_review", false)),
            ["pr_opened"]
        );
        assert_eq!(
            matched(&app, "pull_request", pr("review_requested", false)),
            ["pr_review_requested"]
        );
        assert!(matched(&app, "pull_request", pr("review_request_removed", false)).is_empty());
        assert!(matched(&app, "pull_request", pr("closed", false)).is_empty());
        assert!(matched(&app, "push", pr("opened", false)).is_empty());

        let review = |action, state| json!({ "action": action, "review": { "state": state } });
        assert_eq!(
            matched(&app, "pull_request_review", review("submitted", "approved")),
            ["pr_approved"]
        );
        assert!(
            matched(
                &app,
                "pull_request_review",
                review("submitted", "commented")
            )
            .is_empty()
        );
        assert!(matched(&app, "pull_request_review", review("dismissed", "approved")).is_empty());

        let push = |r: &str| json!({ "ref": r, "repository": { "default_branch": "main" } });
        assert_eq!(
            matched(&app, "push", push("refs/heads/main")),
            ["push_default"]
        );
        assert!(matched(&app, "push", push("refs/heads/feature")).is_empty());
        assert!(matched(&app, "push", push("refs/tags/main")).is_empty());
        let mut deleted = push("refs/heads/main");
        deleted["deleted"] = json!(true);
        assert!(matched(&app, "push", deleted).is_empty());
    }

    #[test]
    fn rejects_bad_config() {
        let app = |yaml: &str| App::new(Config::from_yaml(yaml).unwrap());
        assert!(app(&EXAMPLE.replacen("when: ", "when: == ", 1)).is_err());
        assert!(app(&EXAMPLE.replacen("to: [dev]", "to: [nope]", 1)).is_err());
        assert!(app(&EXAMPLE.replacen("{% if not pull_request.draft %}", "{% if %}", 1)).is_err());
    }
}
