//! Activity timeline, notes, deals, canned responses and Meta lead events —
//! everything behind the customer panel.

use chrono::NaiveDate;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::jobs::{self, Enqueue};
use crate::models::{Activity, CannedResponse, Deal, MetaLeadEvent, Note};
use crate::{Error, Result};

// --------------------------------------------------------------- activities

pub const KIND_STATUS: &str = "conversation.status";
pub const KIND_PRIORITY: &str = "conversation.priority";
pub const KIND_ASSIGNED: &str = "conversation.assigned";
pub const KIND_LABEL: &str = "conversation.label";
pub const KIND_NOTE: &str = "note.added";
pub const KIND_DEAL: &str = "deal.changed";
pub const KIND_META_EVENT: &str = "meta.event";

/// Who/what an activity is about, grouped so the logger reads well at call
/// sites and stays within a sane argument count.
#[derive(Debug, Clone, Copy, Default)]
pub struct Target {
    pub conversation_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub actor: Option<Uuid>,
}

impl Target {
    pub fn conversation(id: Uuid, actor: Option<Uuid>) -> Self {
        Self {
            conversation_id: Some(id),
            contact_id: None,
            actor,
        }
    }
    pub fn contact(id: Uuid, actor: Option<Uuid>) -> Self {
        Self {
            conversation_id: None,
            contact_id: Some(id),
            actor,
        }
    }
    pub fn with_conversation(mut self, id: Option<Uuid>) -> Self {
        self.conversation_id = id;
        self
    }
}

/// Appends to the timeline. Best-effort at the call site: a lost timeline
/// entry must never fail the action it describes, so callers log the error
/// and carry on.
pub async fn log(
    pool: &PgPool,
    org_id: Uuid,
    t: Target,
    kind: &str,
    summary: &str,
    metadata: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO activities
             (org_id, conversation_id, contact_id, actor_user_id, kind, summary, metadata)
           VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
    )
    .bind(org_id)
    .bind(t.conversation_id)
    .bind(t.contact_id)
    .bind(t.actor)
    .bind(kind)
    .bind(summary)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn log_tx(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    conversation_id: Option<Uuid>,
    contact_id: Option<Uuid>,
    actor: Option<Uuid>,
    kind: &str,
    summary: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO activities
             (org_id, conversation_id, contact_id, actor_user_id, kind, summary)
           VALUES ($1,$2,$3,$4,$5,$6)"#,
    )
    .bind(org_id)
    .bind(conversation_id)
    .bind(contact_id)
    .bind(actor)
    .bind(kind)
    .bind(summary)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Timeline for a conversation, or for the contact across all conversations.
pub async fn activities(
    pool: &PgPool,
    org_id: Uuid,
    conversation_id: Option<Uuid>,
    contact_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Activity>> {
    let rows = sqlx::query_as::<_, Activity>(
        r#"SELECT a.id, a.conversation_id, a.contact_id, a.actor_user_id,
                  u.name AS actor_name, a.kind, a.summary, a.metadata, a.created_at
           FROM activities a
           LEFT JOIN users u ON u.id = a.actor_user_id
           WHERE a.org_id = $1
             AND ($2::uuid IS NULL OR a.conversation_id = $2)
             AND ($3::uuid IS NULL OR a.contact_id = $3)
           ORDER BY a.created_at DESC
           LIMIT $4"#,
    )
    .bind(org_id)
    .bind(conversation_id)
    .bind(contact_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// -------------------------------------------------------------------- notes

pub async fn notes(pool: &PgPool, org_id: Uuid, conversation_id: Uuid) -> Result<Vec<Note>> {
    let rows = sqlx::query_as::<_, Note>(
        r#"SELECT n.id, n.conversation_id, n.author_id, u.name AS author_name,
                  n.body, n.created_at
           FROM conversation_notes n
           LEFT JOIN users u ON u.id = n.author_id
           WHERE n.org_id = $1 AND n.conversation_id = $2
           ORDER BY n.created_at DESC"#,
    )
    .bind(org_id)
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// -------------------------------------------------------------------- deals

const DEAL_COLS: &str = r#"d.id, d.org_id, d.contact_id, d.pipeline_id, d.stage_id,
    s.name AS stage_name, d.title, d.value_idr, d.status, d.owner_id,
    d.expected_close_at, d.lost_reason, d.created_at"#;

pub async fn deals_for_contact(pool: &PgPool, org_id: Uuid, contact_id: Uuid) -> Result<Vec<Deal>> {
    let sql = format!(
        "SELECT {DEAL_COLS} FROM deals d
         LEFT JOIN pipeline_stages s ON s.id = d.stage_id
         WHERE d.org_id=$1 AND d.contact_id=$2
         ORDER BY d.created_at DESC"
    );
    Ok(sqlx::query_as::<_, Deal>(&sql)
        .bind(org_id)
        .bind(contact_id)
        .fetch_all(pool)
        .await?)
}

pub struct NewDeal<'a> {
    pub contact_id: Uuid,
    pub title: &'a str,
    pub value_idr: i64,
    pub stage_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    pub expected_close_at: Option<NaiveDate>,
}

pub async fn create_deal(pool: &PgPool, org_id: Uuid, d: NewDeal<'_>) -> Result<Deal> {
    if d.title.trim().is_empty() {
        return Err(Error::validation("judul deal wajib diisi"));
    }
    // Fall back to the org's default pipeline so a deal always has a stage.
    let stage_id = match d.stage_id {
        Some(s) => Some(s),
        None => {
            sqlx::query_scalar::<_, Uuid>(
                r#"SELECT s.id FROM pipeline_stages s
               JOIN pipelines p ON p.id = s.pipeline_id
               WHERE p.org_id = $1 AND p.is_default
               ORDER BY s.position LIMIT 1"#,
            )
            .bind(org_id)
            .fetch_optional(pool)
            .await?
        }
    };

    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO deals (org_id, contact_id, title, value_idr, stage_id, owner_id,
                              expected_close_at, pipeline_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,
                   (SELECT pipeline_id FROM pipeline_stages WHERE id = $5))
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(d.contact_id)
    .bind(d.title.trim())
    .bind(d.value_idr)
    .bind(stage_id)
    .bind(d.owner_id)
    .bind(d.expected_close_at)
    .fetch_one(pool)
    .await?;

    get_deal(pool, org_id, id).await
}

pub async fn get_deal(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<Deal> {
    let sql = format!(
        "SELECT {DEAL_COLS} FROM deals d
         LEFT JOIN pipeline_stages s ON s.id = d.stage_id
         WHERE d.id=$1 AND d.org_id=$2"
    );
    sqlx::query_as::<_, Deal>(&sql)
        .bind(id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("deal"))
}

pub async fn update_deal_status(
    pool: &PgPool,
    org_id: Uuid,
    id: Uuid,
    status: &str,
    lost_reason: Option<&str>,
) -> Result<Deal> {
    if !["open", "won", "lost"].contains(&status) {
        return Err(Error::validation("status deal tidak dikenal"));
    }
    sqlx::query(
        "UPDATE deals SET status=$3, lost_reason=$4, updated_at=now() WHERE id=$1 AND org_id=$2",
    )
    .bind(id)
    .bind(org_id)
    .bind(status)
    .bind(lost_reason)
    .execute(pool)
    .await?;
    get_deal(pool, org_id, id).await
}

// -------------------------------------------------------- canned responses

pub async fn canned_responses(pool: &PgPool, org_id: Uuid) -> Result<Vec<CannedResponse>> {
    Ok(sqlx::query_as::<_, CannedResponse>(
        "SELECT id, shortcut, body FROM canned_responses WHERE org_id=$1 ORDER BY shortcut",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?)
}

pub async fn create_canned_response(
    pool: &PgPool,
    org_id: Uuid,
    shortcut: &str,
    body: &str,
) -> Result<CannedResponse> {
    let shortcut = shortcut.trim().trim_start_matches('/').to_lowercase();
    if shortcut.is_empty() || body.trim().is_empty() {
        return Err(Error::validation("shortcut dan isi wajib diisi"));
    }
    Ok(sqlx::query_as::<_, CannedResponse>(
        r#"INSERT INTO canned_responses (org_id, shortcut, body) VALUES ($1,$2,$3)
           ON CONFLICT (org_id, shortcut) DO UPDATE SET body = EXCLUDED.body
           RETURNING id, shortcut, body"#,
    )
    .bind(org_id)
    .bind(&shortcut)
    .bind(body.trim())
    .fetch_one(pool)
    .await?)
}

// ------------------------------------------------------- Meta lead events

/// Milestones Meta accepts for Click-to-WhatsApp lead reporting.
pub const META_EVENTS: [&str; 4] = [
    "LeadSubmitted",
    "QualifiedLead",
    "ConvertedLead",
    "DisqualifiedLead",
];

pub async fn meta_events(
    pool: &PgPool,
    org_id: Uuid,
    contact_id: Uuid,
) -> Result<Vec<MetaLeadEvent>> {
    Ok(sqlx::query_as::<_, MetaLeadEvent>(
        r#"SELECT id, contact_id, event_name, status, provider_ref, error, sent_at, created_at
           FROM meta_lead_events
           WHERE org_id=$1 AND contact_id=$2
           ORDER BY created_at DESC"#,
    )
    .bind(org_id)
    .bind(contact_id)
    .fetch_all(pool)
    .await?)
}

/// Records the intent to report a milestone. The actual Graph API call is the
/// worker's job, so a slow Meta never blocks the agent's click.
pub async fn queue_meta_event(
    pool: &PgPool,
    org_id: Uuid,
    contact_id: Uuid,
    conversation_id: Option<Uuid>,
    wa_account_id: Option<Uuid>,
    event_name: &str,
    actor: Option<Uuid>,
) -> Result<MetaLeadEvent> {
    if !META_EVENTS.contains(&event_name) {
        return Err(Error::validation(format!(
            "event '{event_name}' tidak dikenal"
        )));
    }

    let already: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM meta_lead_events
             WHERE contact_id=$1 AND event_name=$2 AND status='sent')"#,
    )
    .bind(contact_id)
    .bind(event_name)
    .fetch_one(pool)
    .await?;
    if already {
        return Err(Error::conflict(format!(
            "event '{event_name}' sudah pernah dikirim untuk kontak ini"
        )));
    }

    // Insert and enqueue together: an event row that nothing will ever
    // dispatch is worse than no row at all.
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO meta_lead_events
             (org_id, contact_id, conversation_id, wa_account_id, event_name, payload, sent_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id"#,
    )
    .bind(org_id)
    .bind(contact_id)
    .bind(conversation_id)
    .bind(wa_account_id)
    .bind(event_name)
    .bind(json!({ "source": "dashboard" }))
    .bind(actor)
    .fetch_one(&mut *tx)
    .await?;

    jobs::enqueue_tx(
        &mut tx,
        Enqueue::new(jobs::KIND_META_LEAD_EVENT, json!({ "event_id": id }))
            .org(org_id)
            .max_attempts(5)
            .dedup(format!("metaevt:{id}")),
    )
    .await?;
    tx.commit().await?;

    sqlx::query_as::<_, MetaLeadEvent>(
        r#"SELECT id, contact_id, event_name, status, provider_ref, error, sent_at, created_at
           FROM meta_lead_events WHERE id=$1"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(Error::Db)
}

pub async fn mark_meta_event(
    pool: &PgPool,
    id: Uuid,
    ok: bool,
    provider_ref: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE meta_lead_events
           SET status = CASE WHEN $2 THEN 'sent' ELSE 'failed' END,
               provider_ref = $3,
               error = $4,
               sent_at = CASE WHEN $2 THEN now() ELSE sent_at END
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(ok)
    .bind(provider_ref)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------ contact panel

/// Everything the right-hand customer panel shows in one round trip.
pub async fn contact_panel(
    pool: &PgPool,
    org_id: Uuid,
    contact_id: Uuid,
) -> Result<serde_json::Value> {
    #[derive(sqlx::FromRow)]
    struct PanelRow {
        id: Uuid,
        wa_id: String,
        name: Option<String>,
        profile_name: Option<String>,
        email: Option<String>,
        attributes: serde_json::Value,
        opt_in_status: String,
        last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let row = sqlx::query_as::<_, PanelRow>(
        r#"SELECT c.id, c.wa_id, c.name, c.profile_name, c.email::text AS email,
                  c.attributes, c.opt_in_status, c.last_seen_at, c.created_at
           FROM contacts c WHERE c.id=$1 AND c.org_id=$2"#,
    )
    .bind(contact_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let PanelRow {
        id,
        wa_id,
        name,
        profile_name,
        email,
        attributes,
        opt_in_status: opt_in,
        last_seen_at: last_seen,
        created_at: created,
    } = row.ok_or(Error::NotFound("contact"))?;

    let labels: Vec<(Uuid, String, String)> = sqlx::query_as(
        r#"SELECT l.id, l.name, l.color FROM contact_labels cl
           JOIN labels l ON l.id = cl.label_id
           WHERE cl.contact_id = $1 ORDER BY l.name"#,
    )
    .bind(contact_id)
    .fetch_all(pool)
    .await?;

    Ok(json!({
        "id": id,
        "wa_id": wa_id,
        "name": name,
        "profile_name": profile_name,
        "email": email,
        "attributes": attributes,
        "opt_in_status": opt_in,
        "last_seen_at": last_seen,
        "created_at": created,
        "labels": labels.iter().map(|(id, name, color)| json!({
            "id": id, "name": name, "color": color
        })).collect::<Vec<_>>(),
        "deals": deals_for_contact(pool, org_id, contact_id).await?,
        "meta_events": meta_events(pool, org_id, contact_id).await?,
    }))
}

/// Attach/detach a label on the contact itself (the customer panel's labels).
pub async fn set_contact_label(
    pool: &PgPool,
    org_id: Uuid,
    contact_id: Uuid,
    label_id: Uuid,
    on: bool,
) -> Result<()> {
    let ok: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM contacts c, labels l
             WHERE c.id=$2 AND c.org_id=$1 AND l.id=$3 AND l.org_id=$1)"#,
    )
    .bind(org_id)
    .bind(contact_id)
    .bind(label_id)
    .fetch_one(pool)
    .await?;
    if !ok {
        return Err(Error::NotFound("contact or label"));
    }
    if on {
        sqlx::query(
            "INSERT INTO contact_labels (contact_id, label_id) VALUES ($1,$2)
             ON CONFLICT DO NOTHING",
        )
        .bind(contact_id)
        .bind(label_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM contact_labels WHERE contact_id=$1 AND label_id=$2")
            .bind(contact_id)
            .bind(label_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}
