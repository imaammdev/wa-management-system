//! Meta's inbound webhook. One URL serves every hosted number; the number is
//! identified by `phone_number_id` inside the payload.

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::Router;
use mms_core::crypto;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/meta", get(verify).post(receive))
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

/// Meta's subscription handshake: echo the challenge if the token matches.
async fn verify(
    State(state): State<AppState>,
    Query(q): Query<VerifyQuery>,
) -> (StatusCode, String) {
    let ok = q.mode.as_deref() == Some("subscribe")
        && q.verify_token
            .as_deref()
            .map(|t| crypto::constant_time_eq(t, &state.cfg.meta_webhook_verify_token))
            .unwrap_or(false);

    if ok {
        (StatusCode::OK, q.challenge.unwrap_or_default())
    } else {
        (StatusCode::FORBIDDEN, "verification failed".into())
    }
}

/// Always answers 200 once the payload is stored.
///
/// Meta retries aggressively on any non-2xx and will disable a flapping
/// endpoint, so we persist the raw body first, then process. A processing
/// failure is our problem to retry, not a reason to make Meta resend.
async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, &'static str) {
    // Verify X-Hub-Signature-256 when an app secret is configured.
    let signature_ok = match &state.cfg.meta_app_secret {
        Some(secret) => headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .map(|sig| crypto::verify_signature(secret, &body, sig))
            .unwrap_or(false),
        // No secret set (dev / mock): accept, but record that it was unverified.
        None => false,
    };

    if state.cfg.meta_app_secret.is_some() && !signature_ok {
        tracing::warn!("rejected inbound webhook with a bad signature");
        return (StatusCode::UNAUTHORIZED, "bad signature");
    }

    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "inbound webhook was not valid json");
            // Nothing to gain from a retry of unparseable bytes.
            return (StatusCode::OK, "ignored");
        }
    };

    // Dedup on a hash of the whole body: Meta re-delivers the same payload.
    let dedup_key = hex::encode(Sha256::digest(&body));
    let inserted: Result<Option<(uuid::Uuid,)>, sqlx::Error> = sqlx::query_as(
        r#"INSERT INTO inbound_events (provider, dedup_key, payload, signature_ok)
           VALUES ('meta_cloud', $1, $2, $3)
           ON CONFLICT (dedup_key) DO NOTHING
           RETURNING id"#,
    )
    .bind(&dedup_key)
    .bind(&parsed)
    .bind(signature_ok)
    .fetch_optional(&state.pool)
    .await;

    let event_id = match inserted {
        Ok(Some((id,))) => id,
        // Already seen this exact payload.
        Ok(None) => return (StatusCode::OK, "duplicate"),
        Err(e) => {
            tracing::error!(error = %e, "could not persist inbound webhook");
            // Ask Meta to resend — we genuinely lost it.
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage error");
        }
    };

    let batch = mms_provider::meta_cloud::parse_webhook(&parsed);
    match crate::service::inbound::process_batch(&state, &batch).await {
        Ok((stored, applied)) => {
            let _ = sqlx::query("UPDATE inbound_events SET processed_at = now() WHERE id = $1")
                .bind(event_id)
                .execute(&state.pool)
                .await;
            tracing::info!(stored, applied, "processed inbound webhook");
        }
        Err(e) => {
            // Kept for replay from the dashboard rather than lost.
            let _ = sqlx::query("UPDATE inbound_events SET process_error = $2 WHERE id = $1")
                .bind(event_id)
                .bind(e.to_string())
                .execute(&state.pool)
                .await;
            tracing::error!(error = %e, "failed to process inbound webhook");
        }
    }

    (StatusCode::OK, "ok")
}
