use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::app::App;

/// GitHub's maximum webhook payload size.
const MAX_BODY_BYTES: usize = 25 << 20;

struct Shared {
    app: App,
    secret: String,
}

pub fn router(app: App, secret: String) -> Router {
    Router::new()
        .route("/webhook", post(webhook))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(Arc::new(Shared { app, secret }))
}

async fn webhook(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    if !verify(&shared.secret, &body, header("x-hub-signature-256")) {
        return (StatusCode::UNAUTHORIZED, "bad signature".into());
    }
    let github_event = header("x-github-event").unwrap_or_default();
    if github_event == "ping" {
        return (StatusCode::OK, "pong".into());
    }
    let Ok(payload) = serde_json::from_slice(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid json".into());
    };

    let report = shared.app.dispatch(github_event, &payload).await;
    if !report.errors.is_empty() {
        return (StatusCode::BAD_GATEWAY, report.errors.join("\n"));
    }
    match report.sent.is_empty() {
        true => (StatusCode::OK, format!("ignored {github_event}")),
        false => (StatusCode::OK, format!("sent {}", report.sent.join(", "))),
    }
}

fn verify(secret: &str, body: &[u8], signature: Option<&str>) -> bool {
    let signature = signature
        .and_then(|s| s.strip_prefix("sha256="))
        .and_then(|s| hex::decode(s).ok());
    let Some(signature) = signature else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_signature() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"s3cret").unwrap();
        mac.update(b"{}");
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify("s3cret", b"{}", Some(&signature)));
        assert!(!verify("other", b"{}", Some(&signature)));
        assert!(!verify("s3cret", b"{}", None));
        assert!(!verify("s3cret", b"{}", Some("sha1=00")));
    }
}
