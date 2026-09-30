use std::sync::{Arc, RwLock};

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, Request, StatusCode},
    routing::post,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span, info_span};

use crate::app::App;

/// GitHub's maximum webhook payload size.
const MAX_BODY_BYTES: usize = 25 << 20;

/// The running app; swapped in place when the config is reloaded.
pub type Shared = RwLock<Arc<App>>;

pub fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/webhook", post(webhook))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(shared)
}

fn request_span<B>(request: &Request<B>) -> Span {
    let header = |name| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    info_span!(
        "request",
        method = %request.method(),
        path = request.uri().path(),
        event = header("x-github-event"),
        delivery = header("x-github-delivery"),
    )
}

async fn webhook(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let app = shared.read().unwrap_or_else(|e| e.into_inner()).clone();
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    if !verify(&app.secret, &body, header("x-hub-signature-256")) {
        return (StatusCode::UNAUTHORIZED, "bad signature".into());
    }
    let github_event = header("x-github-event").unwrap_or_default();
    if github_event == "ping" {
        return (StatusCode::OK, "pong".into());
    }
    let Ok(payload) = serde_json::from_slice(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid json".into());
    };

    let report = app.dispatch(github_event, &payload).await;
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
