use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::HeaderMap,
    http::StatusCode,
    routing::post,
};
use hmac::{Hmac, KeyInit, Mac};
use minijinja::{Environment, Value, context};
use serde::Deserialize;
use serde_json::Value as Json;
use sha2::Sha256;

const EVENTS: [&str; 3] = ["pr.opened", "pr.approved", "push.default"];

#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_listen")]
    listen: String,
    secret: String,
    /// github login -> platform -> platform user id
    #[serde(default)]
    users: HashMap<String, HashMap<String, String>>,
    upstreams: HashMap<String, Upstream>,
    events: HashMap<String, Route>,
}

fn default_listen() -> String {
    "0.0.0.0:8080".into()
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Upstream {
    Discord { url: String },
}

impl Upstream {
    fn platform(&self) -> &'static str {
        match self {
            Upstream::Discord { .. } => "discord",
        }
    }
}

#[derive(Deserialize)]
struct Route {
    to: Vec<String>,
    template: String,
}

struct App {
    listen: String,
    secret: String,
    upstreams: HashMap<String, Upstream>,
    events: HashMap<String, Vec<String>>,
    env: Environment<'static>,
    http: reqwest::Client,
    discord_ids: Vec<String>,
}

fn expand_env(v: &mut toml::Value) -> Result<(), String> {
    match v {
        toml::Value::String(s) => {
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                let end = rest[start..].find('}').ok_or("unclosed ${")? + start;
                let name = &rest[start + 2..end];
                out += &rest[..start];
                out += &std::env::var(name).map_err(|_| format!("env var {name} is not set"))?;
                rest = &rest[end + 1..];
            }
            *s = out + rest;
        }
        toml::Value::Array(a) => a.iter_mut().try_for_each(expand_env)?,
        toml::Value::Table(t) => t.iter_mut().try_for_each(|(_, v)| expand_env(v))?,
        _ => {}
    }
    Ok(())
}

fn load(text: &str) -> Result<App, String> {
    let mut raw: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    expand_env(&mut raw)?;
    let cfg: Config = raw.try_into().map_err(|e: toml::de::Error| e.to_string())?;
    if cfg.secret.is_empty() {
        return Err("secret must not be empty".into());
    }
    let users: HashMap<String, HashMap<String, String>> = cfg
        .users
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();
    let discord_ids = users
        .values()
        .filter_map(|ids| ids.get("discord").cloned())
        .collect();

    let mut env = Environment::new();
    env.add_function(
        "mention",
        move |state: &minijinja::State, login: Option<&str>| {
            let Some(login) = login else {
                return String::new();
            };
            let platform = state
                .lookup("upstream")
                .map(|v| v.to_string())
                .unwrap_or_default();
            match users
                .get(&login.to_lowercase())
                .and_then(|ids| ids.get(&platform))
            {
                Some(id) if platform == "discord" => format!("<@{id}>"),
                _ => login.to_string(),
            }
        },
    );

    let mut events = HashMap::new();
    for (name, route) in cfg.events {
        if !EVENTS.contains(&name.as_str()) {
            return Err(format!(
                "unknown event {name:?}, expected one of {EVENTS:?}"
            ));
        }
        if let Some(bad) = route.to.iter().find(|u| !cfg.upstreams.contains_key(*u)) {
            return Err(format!("event {name}: unknown upstream {bad:?}"));
        }
        env.add_template_owned(name.clone(), route.template)
            .map_err(|e| format!("event {name}: {e}"))?;
        events.insert(name, route.to);
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(App {
        listen: cfg.listen,
        secret: cfg.secret,
        upstreams: cfg.upstreams,
        events,
        env,
        http,
        discord_ids,
    })
}

fn verify(secret: &str, body: &[u8], signature: Option<&str>) -> bool {
    let Some(sig) = signature
        .and_then(|s| s.strip_prefix("sha256="))
        .and_then(|s| hex::decode(s).ok())
    else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key size");
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

fn classify(event: &str, p: &Json) -> Option<&'static str> {
    match event {
        "pull_request" if p["action"] == "opened" => Some("pr.opened"),
        "pull_request_review"
            if p["action"] == "submitted" && p["review"]["state"] == "approved" =>
        {
            Some("pr.approved")
        }
        "push"
            if p["deleted"] != true
                && p["ref"]
                    .as_str()
                    .zip(p["repository"]["default_branch"].as_str())
                    .is_some_and(|(r, b)| r.strip_prefix("refs/heads/") == Some(b)) =>
        {
            Some("push.default")
        }
        _ => None,
    }
}

fn render(
    env: &Environment,
    event: &str,
    platform: &str,
    payload: &Json,
) -> Result<String, minijinja::Error> {
    let ctx = context! { upstream => platform, ..Value::from_serialize(payload) };
    Ok(env.get_template(event)?.render(ctx)?.trim().to_string())
}

async fn send(app: &App, upstream: &Upstream, text: &str) -> Result<(), String> {
    match upstream {
        Upstream::Discord { url } => {
            let content: String = text.chars().take(2000).collect();
            // Ping only configured users that the template mentioned; payload text can't ping @everyone, roles or strangers.
            let users: Vec<_> = app
                .discord_ids
                .iter()
                .filter(|id| content.contains(&format!("<@{id}>")))
                .take(100)
                .collect();
            let body = serde_json::json!({ "content": content, "allowed_mentions": { "parse": [], "users": users } });
            // without_url: the Discord webhook URL embeds its token and errors are echoed to GitHub.
            let res = app
                .http
                .post(url)
                .json(&body)
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?;
            res.error_for_status()
                .map(|_| ())
                .map_err(|e| e.without_url().to_string())
        }
    }
}

async fn hook(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    if !verify(&app.secret, &body, header("x-hub-signature-256")) {
        return (StatusCode::UNAUTHORIZED, "bad signature".into());
    }
    let Ok(payload) = serde_json::from_slice::<Json>(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid json".into());
    };
    let gh_event = header("x-github-event").unwrap_or_default();
    let Some(event) = classify(gh_event, &payload).filter(|e| app.events.contains_key(*e)) else {
        return (StatusCode::OK, format!("ignored {gh_event}"));
    };

    let mut errors = vec![];
    for name in &app.events[event] {
        let upstream = &app.upstreams[name];
        let result = match render(&app.env, event, upstream.platform(), &payload) {
            Ok(text) if text.is_empty() => continue,
            Ok(text) => send(&app, upstream, &text).await,
            Err(e) => Err(format!("template: {e}")),
        };
        if let Err(e) = result {
            eprintln!("{event} -> {name}: {e}");
            errors.push(format!("{name}: {e}"));
        }
    }
    match errors.is_empty() {
        true => (StatusCode::OK, format!("sent {event}")),
        false => (StatusCode::BAD_GATEWAY, errors.join("\n")),
    }
}

#[tokio::main]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "octoping.toml".into());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let app = load(&text).unwrap_or_else(|e| panic!("config {path}: {e}"));
    let listen = app.listen.clone();

    let router = Router::new()
        .route("/webhook", post(hook))
        .layer(DefaultBodyLimit::max(25 << 20))
        .with_state(Arc::new(app));
    let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind");
    println!("octoping listening on {listen}");
    axum::serve(listener, router).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CFG: &str = r#"
secret = "s3cret"
[users]
Octocat = { discord = "42" }
[upstreams.dev]
type = "discord"
url = "http://localhost/unused"
[events."pr.opened"]
to = ["dev"]
template = "{{ mention(sender.login) }} {{ mention(ghost) }}{{ mention(missing) }} opened #{{ pull_request.number }}"
"#;

    #[test]
    fn signature() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"s3cret").unwrap();
        mac.update(b"{}");
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify("s3cret", b"{}", Some(&sig)));
        assert!(!verify("other", b"{}", Some(&sig)));
        assert!(!verify("s3cret", b"{}", None));
    }

    #[test]
    fn classifies() {
        let push = |r: &str| json!({ "ref": r, "repository": { "default_branch": "main" } });
        assert_eq!(
            classify("push", &push("refs/heads/main")),
            Some("push.default")
        );
        assert_eq!(classify("push", &push("refs/heads/feature")), None);
        assert_eq!(classify("push", &push("refs/tags/main")), None);
        let mut deleted = push("refs/heads/main");
        deleted["deleted"] = json!(true);
        assert_eq!(classify("push", &deleted), None);
        assert_eq!(
            classify("pull_request", &json!({ "action": "opened" })),
            Some("pr.opened")
        );
        assert_eq!(
            classify("pull_request", &json!({ "action": "closed" })),
            None
        );
        let review = |s: &str| json!({ "action": "submitted", "review": { "state": s } });
        assert_eq!(
            classify("pull_request_review", &review("approved")),
            Some("pr.approved")
        );
        assert_eq!(classify("pull_request_review", &review("commented")), None);
    }

    #[test]
    fn renders_mentions() {
        let app = load(CFG).unwrap();
        let p = json!({ "sender": { "login": "octocat" }, "ghost": "nobody", "pull_request": { "number": 7 } });
        assert_eq!(
            render(&app.env, "pr.opened", "discord", &p).unwrap(),
            "<@42> nobody opened #7"
        );
    }

    #[test]
    fn example_config_renders() {
        let text = include_str!("../octoping.example.toml")
            .replace("${GITHUB_WEBHOOK_SECRET}", "x")
            .replace("${DISCORD_WEBHOOK_URL}", "http://x");
        let app = load(&text).unwrap();
        let repo = json!({ "full_name": "o/r", "default_branch": "main" });
        let pr = json!({ "number": 1, "title": "T", "html_url": "u", "draft": false, "user": { "login": "octocat" } });
        let push = json!({ "sender": { "login": "octocat" }, "repository": repo,
            "commits": [{ "id": "abcdef123", "url": "c", "message": "fix\n\nbody" }] });
        assert_eq!(
            render(&app.env, "push.default", "discord", &push).unwrap(),
            "📦 <@123456789012345678> pushed to `o/r:main`\n- [`abcdef1`](<c>) fix"
        );
        let mut empty = push.clone();
        empty["commits"] = json!([]);
        assert_eq!(
            render(&app.env, "push.default", "discord", &empty).unwrap(),
            ""
        );
        let opened = json!({ "sender": { "login": "x" }, "pull_request": pr, "repository": repo });
        assert!(
            render(&app.env, "pr.opened", "discord", &opened)
                .unwrap()
                .starts_with("🆕 x opened")
        );
        let mut draft = opened.clone();
        draft["pull_request"]["draft"] = json!(true);
        assert_eq!(
            render(&app.env, "pr.opened", "discord", &draft).unwrap(),
            ""
        );
        let approved = json!({ "review": { "user": { "login": "x" } }, "pull_request": pr });
        assert!(
            render(&app.env, "pr.approved", "discord", &approved)
                .unwrap()
                .ends_with("by <@123456789012345678>")
        );
    }

    #[test]
    fn rejects_bad_config() {
        assert!(load(&CFG.replace("\"pr.opened\"", "\"pr.merged\"")).is_err());
        assert!(load(&CFG.replace("to = [\"dev\"]", "to = [\"nope\"]")).is_err());
        assert!(load(&CFG.replace("s3cret", "${OCTOPING_TEST_UNSET}")).is_err());
    }
}
