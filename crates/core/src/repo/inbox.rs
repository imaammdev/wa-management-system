//! Contacts, conversations and messages — the write paths that everything
//! else funnels through.

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::{Contact, Conversation, Message};
use crate::{quota, repo::webhooks, Error, Result};

const CONTACT_COLS: &str = r#"id, org_id, customer_id, wa_id, name, profile_name, email::text as email,
    attributes, opt_in_status, blocked_at, last_seen_at, created_at"#;
const CONV_COLS: &str = r#"id, org_id, customer_id, wa_account_id, contact_id, status, assigned_to,
    assigned_team_id, stage_id, service_window_expires_at, ai_agent_enabled,
    unread_count, last_message_at, last_message_preview, created_at"#;
const MSG_COLS: &str = r#"id, org_id, conversation_id, wa_account_id, contact_id, direction,
    wa_message_id, msg_type, body, payload, status, error_code, error_detail,
    pricing_category, source, sent_by_user_id, sent_at, delivered_at, read_at, created_at"#;

/// Strips `+`, spaces and leading zeros into the `628…` form Meta expects.
pub fn normalize_wa_id(raw: &str) -> Result<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return Err(Error::validation(format!("nomor '{raw}' tidak valid")));
    }
    // Indonesian local format: 08xx -> 628xx
    let normalized = if let Some(rest) = digits.strip_prefix('0') {
        format!("62{rest}")
    } else {
        digits
    };
    Ok(normalized)
}

// ------------------------------------------------------------------ contacts

pub async fn upsert_contact_tx(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    wa_id: &str,
    profile_name: Option<&str>,
) -> Result<Contact> {
    let sql = format!(
        r#"
        INSERT INTO contacts (org_id, customer_id, wa_id, profile_name, last_seen_at)
        VALUES ($1,$2,$3,$4, now())
        ON CONFLICT (org_id, COALESCE(customer_id,'00000000-0000-0000-0000-000000000000'::uuid), wa_id)
        DO UPDATE SET
            profile_name = COALESCE(EXCLUDED.profile_name, contacts.profile_name),
            last_seen_at = now(),
            updated_at = now()
        RETURNING {CONTACT_COLS}
        "#
    );
    let c = sqlx::query_as::<_, Contact>(&sql)
        .bind(org_id)
        .bind(customer_id)
        .bind(wa_id)
        .bind(profile_name)
        .fetch_one(&mut **tx)
        .await?;
    Ok(c)
}

pub async fn upsert_contact(
    pool: &PgPool,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    wa_id: &str,
    name: Option<&str>,
) -> Result<Contact> {
    let wa_id = normalize_wa_id(wa_id)?;
    let mut tx = pool.begin().await?;
    let c = upsert_contact_tx(&mut tx, org_id, customer_id, &wa_id, None).await?;
    if let Some(n) = name {
        sqlx::query("UPDATE contacts SET name=$2, updated_at=now() WHERE id=$1")
            .bind(c.id)
            .bind(n)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(c)
}

pub async fn list_contacts(
    pool: &PgPool,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Contact>> {
    let sql = format!(
        r#"SELECT {CONTACT_COLS} FROM contacts
           WHERE org_id=$1
             AND ($2::uuid IS NULL OR customer_id=$2)
             AND ($3::text IS NULL OR wa_id ILIKE '%'||$3||'%'
                  OR COALESCE(name,'') ILIKE '%'||$3||'%'
                  OR COALESCE(profile_name,'') ILIKE '%'||$3||'%')
           ORDER BY last_seen_at DESC NULLS LAST, created_at DESC
           LIMIT $4 OFFSET $5"#
    );
    let rows = sqlx::query_as::<_, Contact>(&sql)
        .bind(org_id)
        .bind(customer_id)
        .bind(search)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

// ------------------------------------------------------------- conversations

pub async fn get_or_create_conversation_tx(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    wa_account_id: Uuid,
    contact_id: Uuid,
) -> Result<Conversation> {
    let sql = format!(
        r#"
        INSERT INTO conversations (org_id, customer_id, wa_account_id, contact_id)
        VALUES ($1,$2,$3,$4)
        ON CONFLICT (wa_account_id, contact_id)
        DO UPDATE SET updated_at = now()
        RETURNING {CONV_COLS}
        "#
    );
    let c = sqlx::query_as::<_, Conversation>(&sql)
        .bind(org_id)
        .bind(customer_id)
        .bind(wa_account_id)
        .bind(contact_id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(c)
}

pub async fn list_conversations(
    pool: &PgPool,
    org_id: Uuid,
    status: Option<&str>,
    assigned_to: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query_as::<_, ConvListRow>(
        r#"
            SELECT c.id, c.status, c.assigned_to, ct.name, ct.profile_name, ct.wa_id,
                   c.last_message_at, c.last_message_preview, c.unread_count,
                   c.service_window_expires_at, c.wa_account_id
            FROM conversations c
            JOIN contacts ct ON ct.id = c.contact_id
            WHERE c.org_id = $1
              AND ($2::text IS NULL OR c.status = $2)
              AND ($3::uuid IS NULL OR c.assigned_to = $3)
            ORDER BY c.last_message_at DESC NULLS LAST
            LIMIT $4 OFFSET $5
            "#,
    )
    .bind(org_id)
    .bind(status)
    .bind(assigned_to)
    .bind(limit.clamp(1, 100))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(ConvListRow::into_json).collect())
}

/// The inbox list needs the contact joined onto every conversation; this is
/// that denormalized row.
#[derive(sqlx::FromRow)]
struct ConvListRow {
    id: Uuid,
    status: String,
    assigned_to: Option<Uuid>,
    name: Option<String>,
    profile_name: Option<String>,
    wa_id: String,
    last_message_at: Option<DateTime<Utc>>,
    last_message_preview: Option<String>,
    unread_count: i32,
    service_window_expires_at: Option<DateTime<Utc>>,
    wa_account_id: Uuid,
}

impl ConvListRow {
    fn into_json(self) -> serde_json::Value {
        json!({
            "id": self.id,
            "status": self.status,
            "assigned_to": self.assigned_to,
            "wa_account_id": self.wa_account_id,
            "contact": {
                "wa_id": self.wa_id,
                "name": self.name.or(self.profile_name),
            },
            "last_message_at": self.last_message_at,
            "last_message_preview": self.last_message_preview,
            "unread_count": self.unread_count,
            "service_window_expires_at": self.service_window_expires_at,
            "service_window_open": self
                .service_window_expires_at
                .map(|t| t > Utc::now())
                .unwrap_or(false),
        })
    }
}

pub async fn get_conversation(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<Conversation> {
    let sql = format!("SELECT {CONV_COLS} FROM conversations WHERE id=$1 AND org_id=$2");
    sqlx::query_as::<_, Conversation>(&sql)
        .bind(id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("conversation"))
}

pub async fn list_messages(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Uuid,
    limit: i64,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<Message>> {
    let sql = format!(
        r#"SELECT {MSG_COLS} FROM messages
           WHERE org_id=$1 AND conversation_id=$2
             AND ($3::timestamptz IS NULL OR created_at < $3)
           ORDER BY created_at DESC LIMIT $4"#
    );
    let rows = sqlx::query_as::<_, Message>(&sql)
        .bind(org_id)
        .bind(conversation_id)
        .bind(before)
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn assign(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Uuid,
    user_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "UPDATE conversations SET assigned_to=$3, updated_at=now() WHERE id=$1 AND org_id=$2",
    )
    .bind(conversation_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_status(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Uuid,
    status: &str,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE conversations
           SET status=$3,
               closed_at = CASE WHEN $3='closed' THEN now() ELSE NULL END,
               updated_at=now()
           WHERE id=$1 AND org_id=$2"#,
    )
    .bind(conversation_id)
    .bind(org_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_read(pool: &PgPool, org_id: Uuid, conversation_id: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE conversations SET unread_count=0, updated_at=now() WHERE id=$1 AND org_id=$2",
    )
    .bind(conversation_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------- message writes

pub struct NewOutbound<'a> {
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub wa_account_id: Uuid,
    pub contact_id: Uuid,
    pub conversation_id: Uuid,
    pub msg_type: &'a str,
    pub body: Option<&'a str>,
    pub payload: serde_json::Value,
    pub template_id: Option<Uuid>,
    pub pricing_category: Option<&'a str>,
    pub source: &'a str,
    pub sent_by_user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub idempotency_key: Option<&'a str>,
}

/// Records an outbound message as `queued` and meters it, in one transaction.
/// Returns `Err(Conflict)` carrying the existing id when the idempotency key
/// has already been used, so the caller can replay the original response.
pub async fn insert_outbound(pool: &PgPool, m: NewOutbound<'_>) -> Result<Message> {
    let mut tx = pool.begin().await?;

    if let Some(key) = m.idempotency_key {
        let existing = sqlx::query_as::<_, Message>(&format!(
            "SELECT {MSG_COLS} FROM messages WHERE org_id=$1 AND idempotency_key=$2"
        ))
        .bind(m.org_id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(found) = existing {
            return Ok(found);
        }
    }

    let sql = format!(
        r#"INSERT INTO messages
             (org_id, customer_id, conversation_id, wa_account_id, contact_id, direction,
              msg_type, body, payload, template_id, pricing_category, source,
              sent_by_user_id, api_key_id, idempotency_key, status)
           VALUES ($1,$2,$3,$4,$5,'outbound',$6,$7,$8,$9,$10,$11,$12,$13,$14,'queued')
           RETURNING {MSG_COLS}"#
    );
    let msg = sqlx::query_as::<_, Message>(&sql)
        .bind(m.org_id)
        .bind(m.customer_id)
        .bind(m.conversation_id)
        .bind(m.wa_account_id)
        .bind(m.contact_id)
        .bind(m.msg_type)
        .bind(m.body)
        .bind(&m.payload)
        .bind(m.template_id)
        .bind(m.pricing_category)
        .bind(m.source)
        .bind(m.sent_by_user_id)
        .bind(m.api_key_id)
        .bind(m.idempotency_key)
        .fetch_one(&mut *tx)
        .await?;

    quota::increment(&mut tx, m.org_id, quota::METRIC_MESSAGES_SENT, 1).await?;

    sqlx::query(
        r#"UPDATE conversations
           SET last_message_at=now(), last_message_preview=$2, updated_at=now()
           WHERE id=$1"#,
    )
    .bind(m.conversation_id)
    .bind(m.body.map(|b| b.chars().take(140).collect::<String>()))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(msg)
}

/// Marks a queued message as accepted by the provider.
pub async fn mark_sent(pool: &PgPool, id: Uuid, wa_message_id: &str) -> Result<()> {
    sqlx::query("UPDATE messages SET status='sent', wa_message_id=$2, sent_at=now() WHERE id=$1")
        .bind(id)
        .bind(wa_message_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, id: Uuid, code: &str, detail: &str) -> Result<()> {
    sqlx::query(
        r#"UPDATE messages SET status='failed', error_code=$2, error_detail=$3,
                                failed_at=now() WHERE id=$1"#,
    )
    .bind(id)
    .bind(code)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

/// Applies a delivery receipt from Meta. Monotonic: a late `sent` never
/// overwrites a `read` we already recorded.
pub async fn apply_status_update(
    pool: &PgPool,
    wa_account_id: Uuid,
    wa_message_id: &str,
    status: &str,
    pricing_category: Option<&str>,
) -> Result<Option<(Uuid, Uuid)>> {
    let rank = |s: &str| match s {
        "queued" => 0,
        "sent" => 1,
        "delivered" => 2,
        "read" => 3,
        "failed" => 4,
        _ => 0,
    };
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT id, org_id, status FROM messages WHERE wa_account_id=$1 AND wa_message_id=$2",
    )
    .bind(wa_account_id)
    .bind(wa_message_id)
    .fetch_optional(pool)
    .await?;

    let Some((id, org_id, current)) = row else {
        return Ok(None);
    };
    if rank(status) <= rank(&current) && status != "failed" {
        return Ok(Some((id, org_id)));
    }

    sqlx::query(
        r#"UPDATE messages SET
             status = $2,
             delivered_at = CASE WHEN $2 IN ('delivered','read') THEN COALESCE(delivered_at, now()) ELSE delivered_at END,
             read_at      = CASE WHEN $2 = 'read' THEN COALESCE(read_at, now()) ELSE read_at END,
             pricing_category = COALESCE($3, pricing_category)
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(status)
    .bind(pricing_category)
    .execute(pool)
    .await?;

    Ok(Some((id, org_id)))
}

pub struct NewInbound<'a> {
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub wa_account_id: Uuid,
    pub wa_id: &'a str,
    pub profile_name: Option<&'a str>,
    pub wa_message_id: &'a str,
    pub msg_type: &'a str,
    pub body: Option<&'a str>,
    pub payload: serde_json::Value,
    pub reply_to: Option<&'a str>,
}

/// The inbound path: upsert contact, open/refresh the conversation, store the
/// message, reset the 24h service window, meter it, and queue the customer's
/// `message.received` webhook — atomically.
pub async fn record_inbound(pool: &PgPool, m: NewInbound<'_>) -> Result<Option<Message>> {
    let wa_id = normalize_wa_id(m.wa_id)?;
    let mut tx = pool.begin().await?;

    let contact =
        upsert_contact_tx(&mut tx, m.org_id, m.customer_id, &wa_id, m.profile_name).await?;
    let conv = get_or_create_conversation_tx(
        &mut tx,
        m.org_id,
        m.customer_id,
        m.wa_account_id,
        contact.id,
    )
    .await?;

    let sql = format!(
        r#"INSERT INTO messages
             (org_id, customer_id, conversation_id, wa_account_id, contact_id, direction,
              wa_message_id, reply_to_wa_message_id, msg_type, body, payload, status)
           VALUES ($1,$2,$3,$4,$5,'inbound',$6,$7,$8,$9,$10,'delivered')
           ON CONFLICT (wa_account_id, wa_message_id) DO NOTHING
           RETURNING {MSG_COLS}"#
    );
    let inserted = sqlx::query_as::<_, Message>(&sql)
        .bind(m.org_id)
        .bind(m.customer_id)
        .bind(conv.id)
        .bind(m.wa_account_id)
        .bind(contact.id)
        .bind(m.wa_message_id)
        .bind(m.reply_to)
        .bind(m.msg_type)
        .bind(m.body)
        .bind(&m.payload)
        .fetch_optional(&mut *tx)
        .await?;

    // Duplicate webhook delivery from Meta — nothing new to do.
    let Some(msg) = inserted else {
        tx.rollback().await?;
        return Ok(None);
    };

    let window_until = Utc::now() + Duration::hours(24);
    sqlx::query(
        r#"UPDATE conversations SET
             service_window_expires_at = $2,
             unread_count = unread_count + 1,
             last_message_at = now(),
             last_message_preview = $3,
             status = CASE WHEN status = 'closed' THEN 'open' ELSE status END,
             updated_at = now()
           WHERE id = $1"#,
    )
    .bind(conv.id)
    .bind(window_until)
    .bind(m.body.map(|b| b.chars().take(140).collect::<String>()))
    .execute(&mut *tx)
    .await?;

    quota::increment(&mut tx, m.org_id, quota::METRIC_MESSAGES_RECEIVED, 1).await?;

    webhooks::queue_event_tx(
        &mut tx,
        m.org_id,
        m.customer_id,
        "message.received",
        json!({
            "message": {
                "id": msg.id,
                "wa_message_id": m.wa_message_id,
                "conversation_id": conv.id,
                "type": m.msg_type,
                "text": m.body,
                "from": wa_id,
                "timestamp": msg.created_at,
            },
            "contact": { "wa_id": wa_id, "profile_name": m.profile_name },
            "wa_account_id": m.wa_account_id,
        }),
    )
    .await?;

    tx.commit().await?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_indonesian_numbers() {
        assert_eq!(normalize_wa_id("081234567890").unwrap(), "6281234567890");
        assert_eq!(
            normalize_wa_id("+62 812-3456-7890").unwrap(),
            "6281234567890"
        );
        assert_eq!(normalize_wa_id("6281234567890").unwrap(), "6281234567890");
        assert!(normalize_wa_id("123").is_err());
    }
}
