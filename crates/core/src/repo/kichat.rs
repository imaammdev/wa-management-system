//! Kichat — a private browser chat room for one customer.
//!
//! The customer opens a tokenized link; messages they type land in the same
//! conversation as their WhatsApp ones, tagged `channel = 'kichat'`. Because
//! Kichat is our own transport, replies there are not bound by Meta's 24-hour
//! service window.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::crypto;
use crate::{Error, Result};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct KichatSession {
    pub id: Uuid,
    pub org_id: Uuid,
    pub conversation_id: Uuid,
    pub contact_id: Uuid,
    pub status: String,
    pub opened_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

const COLS: &str = r#"id, org_id, conversation_id, contact_id, status, opened_at,
    last_seen_at, expires_at, created_at"#;

/// Issues an invite. Returns the session and the raw token for the link —
/// the token is never stored, only its hash.
pub async fn invite(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Uuid,
    created_by: Option<Uuid>,
    ttl_days: i64,
) -> Result<(KichatSession, String)> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT contact_id FROM conversations WHERE id=$1 AND org_id=$2")
            .bind(conversation_id)
            .bind(org_id)
            .fetch_optional(pool)
            .await?;
    let (contact_id,) = row.ok_or(Error::NotFound("conversation"))?;

    let token = format!("kc_{}", crypto::random_string(40));
    let sql = format!(
        r#"INSERT INTO kichat_sessions
             (org_id, conversation_id, contact_id, token_hash, expires_at, created_by)
           VALUES ($1,$2,$3,$4,$5,$6) RETURNING {COLS}"#
    );
    let session = sqlx::query_as::<_, KichatSession>(&sql)
        .bind(org_id)
        .bind(conversation_id)
        .bind(contact_id)
        .bind(crypto::sha256_hex(&token))
        .bind(Utc::now() + Duration::days(ttl_days.clamp(1, 90)))
        .bind(created_by)
        .fetch_one(pool)
        .await?;

    Ok((session, token))
}

/// Resolves a link token. Expired sessions are reported as such rather than
/// as "not found", so the room can say so plainly.
pub async fn resolve(pool: &PgPool, token: &str) -> Result<KichatSession> {
    let sql = format!("SELECT {COLS} FROM kichat_sessions WHERE token_hash=$1");
    let s = sqlx::query_as::<_, KichatSession>(&sql)
        .bind(crypto::sha256_hex(token))
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("kichat session"))?;

    if s.status == "closed" {
        return Err(Error::Forbidden("ruang chat sudah ditutup"));
    }
    if s.expires_at <= Utc::now() {
        sqlx::query("UPDATE kichat_sessions SET status='expired' WHERE id=$1")
            .bind(s.id)
            .execute(pool)
            .await?;
        return Err(Error::Forbidden("tautan ruang chat sudah kedaluwarsa"));
    }
    Ok(s)
}

/// Marks the room opened the first time the customer loads it.
pub async fn touch(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query(
        r#"UPDATE kichat_sessions SET
             status = CASE WHEN status = 'invited' THEN 'opened' ELSE status END,
             opened_at = COALESCE(opened_at, now()),
             last_seen_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn close(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<()> {
    let r = sqlx::query("UPDATE kichat_sessions SET status='closed' WHERE id=$1 AND org_id=$2")
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("kichat session"));
    }
    Ok(())
}

pub async fn for_conversation(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Uuid,
) -> Result<Vec<KichatSession>> {
    let sql = format!(
        "SELECT {COLS} FROM kichat_sessions
         WHERE org_id=$1 AND conversation_id=$2 ORDER BY created_at DESC"
    );
    Ok(sqlx::query_as::<_, KichatSession>(&sql)
        .bind(org_id)
        .bind(conversation_id)
        .fetch_all(pool)
        .await?)
}

/// Stores a message the customer typed in the browser.
pub async fn record_customer_message(
    pool: &PgPool,
    session: &KichatSession,
    body: &str,
) -> Result<Uuid> {
    let body = body.trim();
    if body.is_empty() {
        return Err(Error::validation("pesan tidak boleh kosong"));
    }
    if body.chars().count() > 4000 {
        return Err(Error::validation("pesan terlalu panjang"));
    }

    let mut tx = pool.begin().await?;

    let wa_account_id: Uuid =
        sqlx::query_scalar("SELECT wa_account_id FROM conversations WHERE id=$1")
            .bind(session.conversation_id)
            .fetch_one(&mut *tx)
            .await?;

    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO messages
             (org_id, conversation_id, wa_account_id, contact_id, direction,
              msg_type, body, status, source, channel)
           VALUES ($1,$2,$3,$4,'inbound','text',$5,'delivered','kichat','kichat')
           RETURNING id"#,
    )
    .bind(session.org_id)
    .bind(session.conversation_id)
    .bind(wa_account_id)
    .bind(session.contact_id)
    .bind(body)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"UPDATE conversations SET
             unread_count = unread_count + 1,
             last_message_at = now(),
             last_message_preview = $2,
             last_message_direction = 'inbound',
             kichat_active = true,
             status = CASE WHEN status IN ('closed','snoozed') THEN 'open' ELSE status END,
             updated_at = now()
           WHERE id = $1"#,
    )
    .bind(session.conversation_id)
    .bind(body.chars().take(140).collect::<String>())
    .execute(&mut *tx)
    .await?;

    crate::quota::increment(
        &mut tx,
        session.org_id,
        crate::quota::METRIC_MESSAGES_RECEIVED,
        1,
    )
    .await?;

    crate::repo::webhooks::queue_event_tx(
        &mut tx,
        session.org_id,
        None,
        "message.received",
        serde_json::json!({
            "message": { "id": id, "channel": "kichat", "text": body,
                         "conversation_id": session.conversation_id },
            "wa_account_id": wa_account_id,
        }),
    )
    .await?;

    tx.commit().await?;
    Ok(id)
}

#[derive(Debug, Serialize, FromRow)]
pub struct KichatMessage {
    pub id: Uuid,
    pub direction: String,
    pub body: Option<String>,
    pub channel: String,
    pub created_at: DateTime<Utc>,
}

/// The transcript the customer sees. Internal notes are not messages, so
/// there is nothing here to leak.
pub async fn transcript(
    pool: &PgPool,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<KichatMessage>> {
    let rows = sqlx::query_as::<_, KichatMessage>(
        r#"SELECT id, direction, body, channel, created_at FROM (
             SELECT id, direction, body, channel, created_at FROM messages
             WHERE conversation_id = $1 AND msg_type = 'text' AND body IS NOT NULL
             ORDER BY created_at DESC LIMIT $2
           ) t ORDER BY created_at"#,
    )
    .bind(conversation_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
