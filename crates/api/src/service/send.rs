//! The single outbound path. Every send — API, dashboard reply, broadcast,
//! AI agent — goes through `send`, so quota, the 24h window rule and delivery
//! bookkeeping can't be bypassed by a new caller.

use mms_core::models::WhatsappAccount;
use mms_core::repo::{inbox, wa};
use mms_core::tenancy::Actor;
use mms_core::{quota, Error, Result};
use mms_provider::{MessageContent, OutboundMessage, ProviderCreds};
use uuid::Uuid;

use crate::state::AppState;

pub struct SendRequest<'a> {
    pub wa_account_id: Uuid,
    pub to: &'a str,
    pub content: MessageContent,
    pub reply_to: Option<String>,
    /// 'api' | 'dashboard' | 'broadcast' | 'ai_agent'
    pub source: &'a str,
    pub idempotency_key: Option<&'a str>,
}

pub struct SendOutcome {
    pub message_id: Uuid,
    pub wa_message_id: Option<String>,
    pub conversation_id: Uuid,
    pub status: String,
}

/// Resolves the account and asserts the actor is allowed to use it.
async fn resolve_account(
    state: &AppState,
    actor: &Actor,
    wa_account_id: Uuid,
) -> Result<WhatsappAccount> {
    let acc = wa::get(&state.pool, actor.org_id(), wa_account_id).await?;
    actor.assert_customer_visible(acc.customer_id)?;
    if acc.status != "connected" {
        return Err(Error::validation(format!(
            "nomor WhatsApp '{}' belum terhubung (status: {})",
            acc.label, acc.status
        )));
    }
    Ok(acc)
}

pub async fn send(state: &AppState, actor: &Actor, req: SendRequest<'_>) -> Result<SendOutcome> {
    actor.require_scope("messages:write")?;

    let acc = resolve_account(state, actor, req.wa_account_id).await?;
    quota::check_message_quota(&state.pool, actor.org_id()).await?;

    let to = inbox::normalize_wa_id(req.to)?;

    // Contact + conversation must exist before we can attribute the message.
    let contact =
        inbox::upsert_contact(&state.pool, actor.org_id(), acc.customer_id, &to, None).await?;

    let mut tx = state.pool.begin().await?;
    let conv = inbox::get_or_create_conversation_tx(
        &mut tx,
        actor.org_id(),
        acc.customer_id,
        acc.id,
        contact.id,
    )
    .await?;
    tx.commit().await?;

    // Meta's rule: outside 24h of the customer's last message, only an
    // approved template may be sent. Rejecting here turns an opaque upstream
    // error into something the caller can act on.
    if !req.content.is_template() && !conv.service_window_open() {
        return Err(Error::validation(
            "jendela layanan 24 jam sudah tertutup — kirim template yang sudah disetujui",
        ));
    }

    let pricing = if req.content.is_template() {
        None // filled in from the status callback
    } else {
        Some("service")
    };

    let message = inbox::insert_outbound(
        &state.pool,
        inbox::NewOutbound {
            org_id: actor.org_id(),
            customer_id: acc.customer_id,
            wa_account_id: acc.id,
            contact_id: contact.id,
            conversation_id: conv.id,
            msg_type: req.content.kind(),
            body: req.content.preview().as_deref(),
            payload: serde_json::to_value(&req.content).unwrap_or_default(),
            template_id: None,
            pricing_category: pricing,
            source: req.source,
            sent_by_user_id: actor.user_id(),
            api_key_id: actor.api_key_id(),
            idempotency_key: req.idempotency_key,
        },
    )
    .await?;

    // Replay of an already-processed idempotent request.
    if message.status != "queued" {
        return Ok(SendOutcome {
            message_id: message.id,
            wa_message_id: message.wa_message_id,
            conversation_id: conv.id,
            status: message.status,
        });
    }

    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .ok_or_else(|| Error::validation("nomor belum punya phone_number_id"))?,
        waba_id: acc.waba_id.clone(),
        access_token: wa::access_token(&state.cfg.encryption_key, &acc)?,
    };

    let provider = state.provider_for(&acc.provider);
    let outbound = OutboundMessage {
        to: to.clone(),
        content: req.content,
        reply_to: req.reply_to,
    };

    match provider.send_message(&creds, &outbound).await {
        Ok(res) => {
            inbox::mark_sent(&state.pool, message.id, &res.wa_message_id).await?;
            Ok(SendOutcome {
                message_id: message.id,
                wa_message_id: Some(res.wa_message_id),
                conversation_id: conv.id,
                status: "sent".into(),
            })
        }
        Err(e) => {
            // The row stays, marked failed, so the customer can see why.
            let (code, detail) = match &e {
                Error::Provider { code, message } => (code.clone(), message.clone()),
                other => ("internal".to_string(), other.to_string()),
            };
            inbox::mark_failed(&state.pool, message.id, &code, &detail).await?;
            Err(e)
        }
    }
}
