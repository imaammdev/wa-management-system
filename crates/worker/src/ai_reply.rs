//! Runs the AI agent for one inbound message.
//!
//! Guards before spending a token: the thread must still want an agent reply,
//! the agent must not already have been superseded by a human, and the
//! handoff rules get the last word.

use chrono::Utc;
use mms_core::repo::{ai, crm, inbox, wa};
use mms_core::{Error, PgPool, Result};
use mms_provider::{MessageContent, OutboundMessage, ProviderCreds};
use serde_json::json;
use std::time::Instant;
use uuid::Uuid;

use crate::broadcast::Providers;
use crate::claude::{self, Claude};

#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub pool: &'a PgPool,
    pub providers: &'a Providers,
    pub claude: Option<&'a Claude>,
    pub enc_key: &'a [u8; 32],
}

pub async fn run(
    ctx: Ctx<'_>,
    conversation_id: Uuid,
    trigger_message_id: Uuid,
    agent_id: Uuid,
) -> Result<()> {
    let Some(claude) = ctx.claude else {
        tracing::warn!("ANTHROPIC_API_KEY is not set; skipping AI reply");
        return Ok(());
    };

    let agent: Option<ai::AiAgent> = sqlx::query_as::<_, ai::AiAgent>(
        r#"SELECT id, org_id, wa_account_id, name, model, system_prompt, knowledge,
                  handoff_rules, enabled, max_turns, context_messages, reply_delay_secs,
                  fallback_message
           FROM ai_agents WHERE id = $1"#,
    )
    .bind(agent_id)
    .fetch_optional(ctx.pool)
    .await?;

    let Some(agent) = agent.filter(|a| a.enabled) else {
        return Ok(());
    };

    let conv = inbox::get_conversation(ctx.pool, agent.org_id, conversation_id).await?;

    // A human took the thread while the job was queued — stay out of it.
    if conv.assigned_to.is_some() || conv.status == "closed" {
        skip(ctx.pool, &agent, conversation_id, "assigned or closed").await;
        return Ok(());
    }

    // The customer kept typing; only answer the newest message.
    let newest: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM messages WHERE conversation_id=$1 AND direction='inbound'
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(conversation_id)
    .fetch_optional(ctx.pool)
    .await?;
    if newest != Some(trigger_message_id) {
        skip(ctx.pool, &agent, conversation_id, "superseded").await;
        return Ok(());
    }

    let turns = ai::turns_taken(ctx.pool, conversation_id).await?;
    if turns >= agent.max_turns {
        handoff(
            ctx.pool,
            &agent,
            conversation_id,
            "batas giliran agent tercapai",
        )
        .await?;
        return Ok(());
    }

    let history =
        ai::conversation_context(ctx.pool, conversation_id, agent.context_messages).await?;
    if history.is_empty() {
        return Ok(());
    }

    let org_name: String = sqlx::query_scalar("SELECT name FROM organizations WHERE id=$1")
        .bind(agent.org_id)
        .fetch_one(ctx.pool)
        .await?;
    let prompt = claude::system_prompt(&agent.system_prompt, &org_name, &agent.knowledge);

    let started = Instant::now();
    let completion = match claude.reply(&agent.model, &prompt, &history).await {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            tracing::error!(error = %msg, agent = %agent.id, "AI reply failed");
            let _ = ai::record_run(
                ctx.pool,
                ai::RunRecord {
                    org_id: agent.org_id,
                    agent_id: agent.id,
                    conversation_id,
                    trigger_message_id: Some(trigger_message_id),
                    reply_message_id: None,
                    model: &agent.model,
                    status: if matches!(&e, Error::Provider { code, .. } if code == "refusal") {
                        "refused"
                    } else {
                        "failed"
                    },
                    intent: None,
                    handoff_reason: None,
                    input_tokens: None,
                    output_tokens: None,
                    latency_ms: Some(started.elapsed().as_millis() as i32),
                    error: Some(&msg),
                },
            )
            .await;

            // The customer is waiting; a holding line beats silence.
            handoff(ctx.pool, &agent, conversation_id, "agent gagal menjawab").await?;
            return Ok(());
        }
    };
    let latency = started.elapsed().as_millis() as i32;

    // Rules first, then the model's own judgement.
    let rules = ai::HandoffRules::parse(&agent.handoff_rules);
    let last_customer_text = history
        .iter()
        .rev()
        .find(|(r, _)| r == "user")
        .map(|(_, t)| t.as_str())
        .unwrap_or("");
    let rule_handoff = rules.should_handoff(
        last_customer_text,
        Some(&completion.reply.intent),
        turns + 1,
    );
    let needs_human = completion.reply.needs_human || rule_handoff.is_some();

    let sent = send(ctx, &agent, &conv, &completion.reply.reply).await;

    let (status, reply_id, err) = match sent {
        Ok(id) => (if needs_human { "handoff" } else { "ok" }, Some(id), None),
        Err(e) => ("failed", None, Some(e.to_string())),
    };

    ai::record_run(
        ctx.pool,
        ai::RunRecord {
            org_id: agent.org_id,
            agent_id: agent.id,
            conversation_id,
            trigger_message_id: Some(trigger_message_id),
            reply_message_id: reply_id,
            model: &agent.model,
            status,
            intent: Some(&completion.reply.intent),
            handoff_reason: rule_handoff.as_deref(),
            input_tokens: Some(completion.input_tokens),
            output_tokens: Some(completion.output_tokens),
            latency_ms: Some(latency),
            error: err.as_deref(),
        },
    )
    .await?;

    if needs_human {
        let reason = rule_handoff.unwrap_or_else(|| "agent meminta bantuan manusia".to_string());
        mark_pending(ctx.pool, &agent, conversation_id, &reason).await?;
    }

    Ok(())
}

/// Sends the agent's reply. Kichat is not bound by Meta's 24-hour window, so
/// a thread with an open room can always be answered.
async fn send(
    ctx: Ctx<'_>,
    agent: &ai::AiAgent,
    conv: &mms_core::models::Conversation,
    text: &str,
) -> Result<Uuid> {
    if !conv.service_window_open() && !conv.kichat_active {
        return Err(Error::validation(
            "jendela 24 jam tertutup dan Kichat tidak aktif",
        ));
    }

    let contact: (String,) = sqlx::query_as("SELECT wa_id FROM contacts WHERE id=$1")
        .bind(conv.contact_id)
        .fetch_one(ctx.pool)
        .await?;

    let content = MessageContent::Text {
        body: text.to_string(),
        preview_url: false,
    };

    let message = inbox::insert_outbound(
        ctx.pool,
        inbox::NewOutbound {
            org_id: agent.org_id,
            customer_id: None,
            wa_account_id: conv.wa_account_id,
            contact_id: conv.contact_id,
            conversation_id: conv.id,
            msg_type: "text",
            body: Some(text),
            payload: serde_json::to_value(&content).unwrap_or_default(),
            template_id: None,
            pricing_category: Some("service"),
            source: "ai_agent",
            sent_by_user_id: None,
            api_key_id: None,
            idempotency_key: None,
        },
    )
    .await?;

    // Inside Kichat the message is already delivered — there is no carrier.
    if conv.kichat_active && !conv.service_window_open() {
        sqlx::query(
            "UPDATE messages SET status='delivered', channel='kichat', sent_at=now() WHERE id=$1",
        )
        .bind(message.id)
        .execute(ctx.pool)
        .await?;
        return Ok(message.id);
    }

    let acc = wa::get(ctx.pool, agent.org_id, conv.wa_account_id).await?;
    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .unwrap_or_else(|| format!("mock-{}", acc.id.simple())),
        waba_id: acc.waba_id.clone(),
        access_token: wa::access_token(ctx.enc_key, &acc).unwrap_or_default(),
    };

    match ctx
        .providers
        .for_name_pub(&acc.provider)
        .send_message(
            &creds,
            &OutboundMessage {
                to: contact.0,
                content,
                reply_to: None,
            },
        )
        .await
    {
        Ok(res) => {
            inbox::mark_sent(ctx.pool, message.id, &res.wa_message_id).await?;
            Ok(message.id)
        }
        Err(e) => {
            inbox::mark_failed(ctx.pool, message.id, "ai_send_failed", &e.to_string()).await?;
            Err(e)
        }
    }
}

async fn skip(pool: &PgPool, agent: &ai::AiAgent, conversation_id: Uuid, why: &str) {
    tracing::info!(%conversation_id, why, "AI reply skipped");
    let _ = ai::record_run(
        pool,
        ai::RunRecord {
            org_id: agent.org_id,
            agent_id: agent.id,
            conversation_id,
            trigger_message_id: None,
            reply_message_id: None,
            model: &agent.model,
            status: "skipped",
            intent: None,
            handoff_reason: Some(why),
            input_tokens: None,
            output_tokens: None,
            latency_ms: None,
            error: None,
        },
    )
    .await;
}

/// Sends the configured holding line, then parks the thread for a human.
async fn handoff(
    pool: &PgPool,
    agent: &ai::AiAgent,
    conversation_id: Uuid,
    reason: &str,
) -> Result<()> {
    mark_pending(pool, agent, conversation_id, reason).await
}

async fn mark_pending(
    pool: &PgPool,
    agent: &ai::AiAgent,
    conversation_id: Uuid,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE conversations
           SET status = CASE WHEN status = 'open' THEN 'pending' ELSE status END,
               ai_agent_enabled = false,
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(conversation_id)
    .execute(pool)
    .await?;

    let _ = crm::log(
        pool,
        agent.org_id,
        crm::Target {
            conversation_id: Some(conversation_id),
            contact_id: None,
            actor: None,
        },
        crm::KIND_STATUS,
        &format!("AI Agent menyerahkan ke tim: {reason}"),
        json!({ "reason": reason, "agent": agent.name }),
    )
    .await;

    tracing::info!(%conversation_id, reason, "handed off to a human");
    let _ = Utc::now();
    Ok(())
}
