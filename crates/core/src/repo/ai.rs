//! AI agent configuration and run history.

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Error, Result};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AiAgent {
    pub id: Uuid,
    pub org_id: Uuid,
    pub wa_account_id: Option<Uuid>,
    pub name: String,
    pub model: String,
    pub system_prompt: String,
    pub knowledge: serde_json::Value,
    pub handoff_rules: serde_json::Value,
    pub enabled: bool,
    pub max_turns: i32,
    pub context_messages: i32,
    pub reply_delay_secs: i32,
    pub fallback_message: String,
}

const COLS: &str = r#"id, org_id, wa_account_id, name, model, system_prompt, knowledge,
    handoff_rules, enabled, max_turns, context_messages, reply_delay_secs, fallback_message"#;

/// Handoff triggers, read by the worker after the model replies.
#[derive(Debug, Default, Deserialize)]
pub struct HandoffRules {
    #[serde(default)]
    pub on_keyword: Vec<String>,
    /// Hand off once the agent has replied this many times in one thread.
    #[serde(default)]
    pub after_turns: Option<i32>,
    #[serde(default)]
    pub on_intent: Vec<String>,
}

impl HandoffRules {
    pub fn parse(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    /// Whether this exchange should leave the agent and go to a human.
    pub fn should_handoff(&self, text: &str, intent: Option<&str>, turns: i32) -> Option<String> {
        let lower = text.to_lowercase();
        if let Some(k) = self
            .on_keyword
            .iter()
            .find(|k| lower.contains(&k.to_lowercase()))
        {
            return Some(format!("kata kunci '{k}'"));
        }
        if let Some(i) = intent {
            if self.on_intent.iter().any(|x| x.eq_ignore_ascii_case(i)) {
                return Some(format!("intent '{i}'"));
            }
        }
        if let Some(max) = self.after_turns {
            if turns >= max {
                return Some(format!("sudah {turns} giliran"));
            }
        }
        None
    }
}

pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<AiAgent>> {
    let sql = format!("SELECT {COLS} FROM ai_agents WHERE org_id=$1 ORDER BY name");
    Ok(sqlx::query_as::<_, AiAgent>(&sql)
        .bind(org_id)
        .fetch_all(pool)
        .await?)
}

/// The enabled agent attached to a number, if any.
pub async fn for_account(pool: &PgPool, wa_account_id: Uuid) -> Result<Option<AiAgent>> {
    let sql = format!("SELECT {COLS} FROM ai_agents WHERE wa_account_id=$1 AND enabled");
    Ok(sqlx::query_as::<_, AiAgent>(&sql)
        .bind(wa_account_id)
        .fetch_optional(pool)
        .await?)
}

pub struct UpsertAgent<'a> {
    pub id: Option<Uuid>,
    pub wa_account_id: Option<Uuid>,
    pub name: &'a str,
    pub model: &'a str,
    pub system_prompt: &'a str,
    pub handoff_rules: serde_json::Value,
    pub enabled: bool,
    pub fallback_message: &'a str,
}

pub async fn upsert(pool: &PgPool, org_id: Uuid, a: UpsertAgent<'_>) -> Result<AiAgent> {
    if a.name.trim().is_empty() {
        return Err(Error::validation("nama agent wajib diisi"));
    }
    let sql = match a.id {
        Some(_) => format!(
            r#"UPDATE ai_agents SET wa_account_id=$3, name=$4, model=$5, system_prompt=$6,
                     handoff_rules=$7, enabled=$8, fallback_message=$9, updated_at=now()
               WHERE id=$2 AND org_id=$1 RETURNING {COLS}"#
        ),
        None => format!(
            r#"INSERT INTO ai_agents
                 (org_id, id, wa_account_id, name, model, system_prompt,
                  handoff_rules, enabled, fallback_message)
               VALUES ($1, COALESCE($2, gen_random_uuid()), $3,$4,$5,$6,$7,$8,$9)
               RETURNING {COLS}"#
        ),
    };

    sqlx::query_as::<_, AiAgent>(&sql)
        .bind(org_id)
        .bind(a.id)
        .bind(a.wa_account_id)
        .bind(a.name.trim())
        .bind(a.model)
        .bind(a.system_prompt)
        .bind(&a.handoff_rules)
        .bind(a.enabled)
        .bind(a.fallback_message)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("ai agent"))
}

pub async fn delete(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<()> {
    let r = sqlx::query("DELETE FROM ai_agents WHERE id=$1 AND org_id=$2")
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("ai agent"));
    }
    Ok(())
}

/// How many times the agent has already answered in this thread.
pub async fn turns_taken(pool: &PgPool, conversation_id: Uuid) -> Result<i32> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages WHERE conversation_id=$1 AND source='ai_agent'",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await?;
    Ok(n as i32)
}

pub struct RunRecord<'a> {
    pub org_id: Uuid,
    pub agent_id: Uuid,
    pub conversation_id: Uuid,
    pub trigger_message_id: Option<Uuid>,
    pub reply_message_id: Option<Uuid>,
    pub model: &'a str,
    pub status: &'a str,
    pub intent: Option<&'a str>,
    pub handoff_reason: Option<&'a str>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<&'a str>,
}

pub async fn record_run(pool: &PgPool, r: RunRecord<'_>) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO ai_runs
             (org_id, agent_id, conversation_id, trigger_message_id, reply_message_id,
              model, status, intent, handoff_reason, input_tokens, output_tokens,
              latency_ms, error)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
    )
    .bind(r.org_id)
    .bind(r.agent_id)
    .bind(r.conversation_id)
    .bind(r.trigger_message_id)
    .bind(r.reply_message_id)
    .bind(r.model)
    .bind(r.status)
    .bind(r.intent)
    .bind(r.handoff_reason)
    .bind(r.input_tokens)
    .bind(r.output_tokens)
    .bind(r.latency_ms)
    .bind(r.error)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize, FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub model: String,
    pub status: String,
    pub intent: Option<String>,
    pub handoff_reason: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn recent_runs(pool: &PgPool, org_id: Uuid, limit: i64) -> Result<Vec<RunRow>> {
    Ok(sqlx::query_as::<_, RunRow>(
        r#"SELECT id, model, status, intent, handoff_reason, input_tokens,
                  output_tokens, latency_ms, error, created_at
           FROM ai_runs WHERE org_id=$1 ORDER BY created_at DESC LIMIT $2"#,
    )
    .bind(org_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?)
}

/// Recent turns, oldest first, for the model's context window.
pub async fn conversation_context(
    pool: &PgPool,
    conversation_id: Uuid,
    limit: i32,
) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT direction, body FROM (
             SELECT direction, body, created_at FROM messages
             WHERE conversation_id = $1 AND body IS NOT NULL AND body <> ''
             ORDER BY created_at DESC LIMIT $2
           ) t ORDER BY created_at"#,
    )
    .bind(conversation_id)
    .bind(limit.clamp(2, 50))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(dir, body)| {
            body.map(|b| {
                let role = if dir == "inbound" {
                    "user"
                } else {
                    "assistant"
                };
                (role.to_string(), b)
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keyword_triggers_handoff() {
        let r = HandoffRules::parse(&json!({ "on_keyword": ["manusia", "agent"] }));
        assert!(r
            .should_handoff("saya mau bicara dengan MANUSIA", None, 1)
            .is_some());
        assert!(r.should_handoff("berapa harganya?", None, 1).is_none());
    }

    #[test]
    fn turn_limit_triggers_handoff() {
        let r = HandoffRules::parse(&json!({ "after_turns": 3 }));
        assert!(r.should_handoff("halo", None, 2).is_none());
        assert!(r.should_handoff("halo", None, 3).is_some());
    }

    #[test]
    fn intent_triggers_handoff_case_insensitively() {
        let r = HandoffRules::parse(&json!({ "on_intent": ["refund"] }));
        assert!(r.should_handoff("x", Some("REFUND"), 0).is_some());
        assert!(r.should_handoff("x", Some("pricing"), 0).is_none());
    }

    #[test]
    fn empty_rules_never_hand_off() {
        let r = HandoffRules::parse(&json!({}));
        assert!(r.should_handoff("apa pun", Some("apa"), 99).is_none());
    }
}
