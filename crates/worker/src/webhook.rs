//! Signed webhook delivery with retry.
//!
//! Each POST carries `X-Mms-Signature-256`, an HMAC-SHA256 of the exact body
//! using the endpoint's secret — the same scheme Meta uses, so customers can
//! reuse any Meta-compatible verifier.

use mms_core::repo::webhooks;
use mms_core::{crypto, PgPool, Result};
use std::time::Instant;
use uuid::Uuid;

pub async fn deliver(pool: &PgPool, http: &reqwest::Client, delivery_id: Uuid) -> Result<()> {
    let delivery = webhooks::get_delivery(pool, delivery_id).await?;

    // Replay or a double-queued job: nothing to do.
    if delivery.status == "delivered" {
        return Ok(());
    }

    let endpoint = webhooks::get_endpoint(pool, delivery.endpoint_id).await?;
    if !endpoint.active {
        webhooks::mark_attempt_failed(pool, delivery.id, None, "endpoint disabled", true).await?;
        return Ok(());
    }

    let body = serde_json::to_vec(&delivery.payload).unwrap_or_default();
    let signature = crypto::sign_payload(&endpoint.secret, &body);
    let started = Instant::now();

    let result = http
        .post(&endpoint.url)
        .header("content-type", "application/json")
        .header("x-mms-signature-256", &signature)
        .header("x-mms-event-id", delivery.event_id.to_string())
        .header("x-mms-event-type", &delivery.event_type)
        .header("x-mms-delivery-attempt", (delivery.attempt + 1).to_string())
        .body(body)
        .send()
        .await;

    let elapsed = started.elapsed().as_millis() as i32;
    // One more attempt than are left means this was the last one.
    let exhausted = delivery.attempt + 1 >= delivery.max_attempts;

    match result {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                webhooks::mark_delivered(pool, delivery.id, status.as_u16() as i32, &text, elapsed)
                    .await?;
                tracing::info!(
                    delivery = %delivery.id, status = status.as_u16(), elapsed_ms = elapsed,
                    "webhook delivered"
                );
            } else {
                // 4xx other than 408/429 won't succeed on retry either, but we
                // still retry: customers commonly deploy a fix and expect the
                // event to land.
                webhooks::mark_attempt_failed(
                    pool,
                    delivery.id,
                    Some(status.as_u16() as i32),
                    &format!(
                        "HTTP {status}: {}",
                        text.chars().take(300).collect::<String>()
                    ),
                    exhausted,
                )
                .await?;
                tracing::warn!(
                    delivery = %delivery.id, status = status.as_u16(), exhausted,
                    "webhook rejected by endpoint"
                );
            }
        }
        Err(e) => {
            webhooks::mark_attempt_failed(pool, delivery.id, None, &e.to_string(), exhausted)
                .await?;
            tracing::warn!(delivery = %delivery.id, error = %e, exhausted, "webhook request failed");
        }
    }

    Ok(())
}
