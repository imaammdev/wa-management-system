//! Processes a parsed Meta webhook batch into our own tables, then fans the
//! resulting events out to the customer's webhook endpoints.

use mms_core::jobs::{self, Enqueue};
use mms_core::repo::{self, inbox, wa, webhooks};
use mms_core::Result;
use mms_provider::{InboundBatch, InboundMessage, InboundStatus};
use serde_json::json;

use crate::state::AppState;

/// Returns (messages stored, statuses applied). Unknown numbers are skipped:
/// a webhook for a number we don't host is not an error on our side.
pub async fn process_batch(state: &AppState, batch: &InboundBatch) -> Result<(usize, usize)> {
    let mut stored = 0usize;
    let mut applied = 0usize;

    for m in &batch.messages {
        if process_message(state, m).await? {
            stored += 1;
        }
    }
    for s in &batch.statuses {
        if process_status(state, s).await? {
            applied += 1;
        }
    }
    Ok((stored, applied))
}

async fn process_message(state: &AppState, m: &InboundMessage) -> Result<bool> {
    let Some(acc) = wa::find_by_phone_number_id(&state.pool, &m.phone_number_id).await? else {
        tracing::warn!(
            phone_number_id = %m.phone_number_id,
            "inbound message for an unknown number, skipping"
        );
        return Ok(false);
    };

    let saved = inbox::record_inbound(
        &state.pool,
        inbox::NewInbound {
            org_id: acc.org_id,
            customer_id: acc.customer_id,
            wa_account_id: acc.id,
            wa_id: &m.from,
            profile_name: m.profile_name.as_deref(),
            wa_message_id: &m.wa_message_id,
            msg_type: &m.msg_type,
            body: m.text.as_deref(),
            payload: m.raw.clone(),
            reply_to: m.reply_to.as_deref(),
            referral: m.referral.as_ref(),
        },
    )
    .await?;

    // `None` means Meta re-delivered something we already have.
    let Some(msg) = saved else { return Ok(false) };

    // Hand off to the AI agent if the number has one enabled. Queued rather
    // than awaited: Meta's webhook must get its 200 back promptly.
    if let Some(agent) = repo::ai::for_account(&state.pool, acc.id).await? {
        let _ = jobs::enqueue(
            &state.pool,
            Enqueue::new(
                jobs::KIND_AI_REPLY,
                json!({
                    "conversation_id": msg.conversation_id,
                    "message_id": msg.id,
                    "agent_id": agent.id,
                }),
            )
            .org(acc.org_id)
            .at(chrono::Utc::now()
                + chrono::Duration::seconds(agent.reply_delay_secs.max(0) as i64))
            .max_attempts(2)
            .dedup(format!("ai:{}", msg.id)),
        )
        .await;
    }

    Ok(true)
}

async fn process_status(state: &AppState, s: &InboundStatus) -> Result<bool> {
    let Some(acc) = wa::find_by_phone_number_id(&state.pool, &s.phone_number_id).await? else {
        return Ok(false);
    };

    let updated = inbox::apply_status_update(
        &state.pool,
        acc.id,
        &s.wa_message_id,
        &s.status,
        s.pricing_category.as_deref(),
    )
    .await?;

    let Some((message_id, org_id)) = updated else {
        // A status for a message we never sent (or already pruned).
        return Ok(false);
    };

    webhooks::queue_event(
        &state.pool,
        org_id,
        acc.customer_id,
        "message.status",
        json!({
            "message_id": message_id,
            "wa_message_id": s.wa_message_id,
            "status": s.status,
            "pricing_category": s.pricing_category,
            "error": s.error_code.as_ref().map(|c| json!({
                "code": c, "title": s.error_title,
            })),
            "wa_account_id": acc.id,
        }),
    )
    .await?;

    Ok(true)
}
