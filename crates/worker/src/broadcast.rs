//! Broadcast execution.
//!
//! Sends in throttled batches and re-queues itself, so a 50k campaign is a
//! sequence of short jobs rather than one long-running task. A paused or
//! cancelled campaign stops at the next batch boundary.

use chrono::{Duration, Utc};
use mms_core::jobs::{self, Enqueue};
use mms_core::repo::{inbox, wa};
use mms_core::{quota, PgPool, Result};
use mms_provider::{MessageContent, OutboundMessage, ProviderCreds, WaProvider};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// How many recipients one job handles before re-queueing.
const BATCH: i64 = 100;

pub struct Providers {
    pub meta: Arc<dyn WaProvider>,
    pub mock: Arc<dyn WaProvider>,
}

impl Providers {
    fn for_name(&self, name: &str) -> Arc<dyn WaProvider> {
        match name {
            "meta_cloud" => self.meta.clone(),
            _ => self.mock.clone(),
        }
    }
}

pub async fn run(
    pool: &PgPool,
    providers: &Providers,
    enc_key: &[u8; 32],
    broadcast_id: Uuid,
) -> Result<()> {
    let row: Option<(Uuid, Uuid, Option<Uuid>, String, serde_json::Value, i32)> = sqlx::query_as(
        r#"SELECT org_id, wa_account_id, template_id, status, variables, throttle_per_minute
               FROM broadcasts WHERE id = $1"#,
    )
    .bind(broadcast_id)
    .fetch_optional(pool)
    .await?;

    let Some((org_id, wa_account_id, template_id, status, variables, throttle)) = row else {
        tracing::warn!(%broadcast_id, "broadcast vanished, dropping job");
        return Ok(());
    };

    if status != "running" {
        tracing::info!(%broadcast_id, status, "broadcast not running, stopping");
        return Ok(());
    }

    let Some(template_id) = template_id else {
        finish(pool, broadcast_id, "failed").await?;
        return Ok(());
    };

    let tpl: Option<(String, String)> =
        sqlx::query_as("SELECT name, language FROM message_templates WHERE id = $1")
            .bind(template_id)
            .fetch_optional(pool)
            .await?;
    let Some((tpl_name, tpl_lang)) = tpl else {
        finish(pool, broadcast_id, "failed").await?;
        return Ok(());
    };

    let acc = wa::get(pool, org_id, wa_account_id).await?;
    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .unwrap_or_else(|| format!("mock-{}", acc.id.simple())),
        waba_id: acc.waba_id.clone(),
        access_token: wa::access_token(enc_key, &acc).unwrap_or_default(),
    };
    let provider = providers.for_name(&acc.provider);

    // Claim a batch so concurrent workers can't double-send to a contact.
    let batch: Vec<(Uuid, Uuid, String, Option<String>)> = sqlx::query_as(
        r#"
        WITH picked AS (
            SELECT br.id FROM broadcast_recipients br
            WHERE br.broadcast_id = $1 AND br.status = 'pending'
            ORDER BY br.id
            FOR UPDATE SKIP LOCKED
            LIMIT $2
        )
        UPDATE broadcast_recipients br
        SET status = 'sent', processed_at = now()
        FROM picked, contacts c
        WHERE br.id = picked.id AND c.id = br.contact_id
        RETURNING br.id, c.id, c.wa_id, COALESCE(c.name, c.profile_name)
        "#,
    )
    .bind(broadcast_id)
    .bind(BATCH)
    .fetch_all(pool)
    .await?;

    if batch.is_empty() {
        finish(pool, broadcast_id, "completed").await?;
        tracing::info!(%broadcast_id, "broadcast completed");
        return Ok(());
    }

    let mut sent = 0i32;
    let mut failed = 0i32;

    for (recipient_id, contact_id, wa_id, contact_name) in batch {
        // Stop early if the org ran out of monthly quota mid-campaign.
        if quota::check_message_quota(pool, org_id).await.is_err() {
            sqlx::query(
                "UPDATE broadcast_recipients SET status='pending', processed_at=NULL WHERE id=$1",
            )
            .bind(recipient_id)
            .execute(pool)
            .await?;
            finish(pool, broadcast_id, "paused").await?;
            tracing::warn!(%broadcast_id, "paused: monthly message quota exhausted");
            return Ok(());
        }

        let content = MessageContent::Template {
            name: tpl_name.clone(),
            language: tpl_lang.clone(),
            components: build_components(&variables, contact_name.as_deref()),
        };

        let mut tx = pool.begin().await?;
        let conv = inbox::get_or_create_conversation_tx(
            &mut tx,
            org_id,
            acc.customer_id,
            acc.id,
            contact_id,
        )
        .await?;
        tx.commit().await?;

        let message = inbox::insert_outbound(
            pool,
            inbox::NewOutbound {
                org_id,
                customer_id: acc.customer_id,
                wa_account_id: acc.id,
                contact_id,
                conversation_id: conv.id,
                msg_type: "template",
                body: Some(&format!("[template: {tpl_name}]")),
                payload: serde_json::to_value(&content).unwrap_or_default(),
                template_id: Some(template_id),
                pricing_category: None,
                source: "broadcast",
                sent_by_user_id: None,
                api_key_id: None,
                idempotency_key: Some(&format!("bc:{broadcast_id}:{contact_id}")),
            },
        )
        .await?;

        let outbound = OutboundMessage {
            to: wa_id.clone(),
            content,
            reply_to: None,
        };

        match provider.send_message(&creds, &outbound).await {
            Ok(res) => {
                inbox::mark_sent(pool, message.id, &res.wa_message_id).await?;
                sqlx::query("UPDATE broadcast_recipients SET message_id=$2 WHERE id=$1")
                    .bind(recipient_id)
                    .bind(message.id)
                    .execute(pool)
                    .await?;
                sent += 1;
            }
            Err(e) => {
                let detail = e.to_string();
                inbox::mark_failed(pool, message.id, "send_failed", &detail).await?;
                sqlx::query(
                    r#"UPDATE broadcast_recipients
                       SET status='failed', error_code='send_failed', error_detail=$2
                       WHERE id=$1"#,
                )
                .bind(recipient_id)
                .bind(&detail)
                .execute(pool)
                .await?;
                failed += 1;
            }
        }
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE broadcasts SET sent_count = sent_count + $2,
                                  failed_count = failed_count + $3,
                                  updated_at = now()
           WHERE id = $1"#,
    )
    .bind(broadcast_id)
    .bind(sent)
    .bind(failed)
    .execute(&mut *tx)
    .await?;
    quota::increment(
        &mut tx,
        org_id,
        quota::METRIC_BROADCASTS_SENT,
        (sent + failed) as i64,
    )
    .await?;

    // Pace the next batch to respect the configured per-minute throttle.
    let delay_secs = if throttle > 0 {
        (BATCH as f64 / throttle as f64 * 60.0).ceil() as i64
    } else {
        1
    };
    jobs::enqueue_tx(
        &mut tx,
        Enqueue::new(
            jobs::KIND_BROADCAST_RUN,
            json!({ "broadcast_id": broadcast_id }),
        )
        .org(org_id)
        .at(Utc::now() + Duration::seconds(delay_secs.max(1)))
        .dedup(format!("broadcast:{broadcast_id}")),
    )
    .await?;
    tx.commit().await?;

    tracing::info!(%broadcast_id, sent, failed, next_in_secs = delay_secs, "broadcast batch done");
    Ok(())
}

/// Substitutes `{{contact.name}}` into the template's body variables.
fn build_components(
    variables: &serde_json::Value,
    contact_name: Option<&str>,
) -> serde_json::Value {
    let Some(map) = variables.as_object() else {
        return json!([]);
    };
    if map.is_empty() {
        return json!([]);
    }

    // Variables are keyed "1", "2", … matching the template's {{n}} slots.
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));

    let params: Vec<serde_json::Value> = keys
        .iter()
        .map(|k| {
            let raw = map[*k].as_str().unwrap_or_default();
            let resolved = raw.replace("{{contact.name}}", contact_name.unwrap_or("Pelanggan"));
            json!({ "type": "text", "text": resolved })
        })
        .collect();

    json!([{ "type": "body", "parameters": params }])
}

async fn finish(pool: &PgPool, broadcast_id: Uuid, status: &str) -> Result<()> {
    sqlx::query(
        r#"UPDATE broadcasts SET status=$2,
             completed_at = CASE WHEN $2 IN ('completed','failed') THEN now() ELSE completed_at END,
             updated_at = now()
           WHERE id=$1"#,
    )
    .bind(broadcast_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_variables_produce_no_components() {
        assert_eq!(build_components(&json!({}), None), json!([]));
    }

    #[test]
    fn substitutes_contact_name_in_order() {
        let vars = json!({ "2": "diskon", "1": "{{contact.name}}" });
        let c = build_components(&vars, Some("Budi"));
        let params = &c[0]["parameters"];
        assert_eq!(params[0]["text"], "Budi");
        assert_eq!(params[1]["text"], "diskon");
    }

    #[test]
    fn falls_back_when_contact_has_no_name() {
        let c = build_components(&json!({ "1": "{{contact.name}}" }), None);
        assert_eq!(c[0]["parameters"][0]["text"], "Pelanggan");
    }
}
