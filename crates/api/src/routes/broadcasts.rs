//! Broadcast campaigns. The API builds the recipient list and hands execution
//! to the worker, so a campaign of 50k survives an API restart.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::jobs::{self, Enqueue};
use mms_core::tenancy::Capability;
use mms_core::{quota, Error};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one))
        .route("/{id}/start", post(start))
        .route("/{id}/pause", post(pause))
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query_as::<_, mms_core::models::Broadcast>(
        r#"SELECT id, org_id, wa_account_id, template_id, segment_id, name, variables,
                  status, throttle_per_minute, scheduled_at, started_at, completed_at,
                  total_recipients, sent_count, delivered_count, read_count,
                  failed_count, created_at
           FROM broadcasts WHERE org_id=$1 ORDER BY created_at DESC LIMIT 100"#,
    )
    .bind(actor.org_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct CreateBroadcast {
    name: String,
    wa_account_id: Uuid,
    template_id: Uuid,
    /// Label ids to target; empty means every opted-in contact.
    #[serde(default)]
    label_ids: Vec<Uuid>,
    #[serde(default)]
    variables: serde_json::Value,
    scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "d600")]
    throttle_per_minute: i32,
}
fn d600() -> i32 {
    600
}

async fn create(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<CreateBroadcast>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;

    // Only an approved template may be broadcast — Meta rejects the rest.
    let tpl_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM message_templates WHERE id=$1 AND org_id=$2")
            .bind(body.template_id)
            .bind(actor.org_id())
            .fetch_optional(&state.pool)
            .await?;

    match tpl_status.as_deref() {
        Some("APPROVED") => {}
        Some(other) => {
            return Err(
                Error::validation(format!("template belum disetujui (status: {other})")).into(),
            )
        }
        None => return Err(Error::NotFound("template").into()),
    }

    let mut tx = state.pool.begin().await?;

    let broadcast_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO broadcasts
             (org_id, wa_account_id, template_id, name, variables, status,
              throttle_per_minute, scheduled_at, created_by)
           VALUES ($1,$2,$3,$4,$5,
                   CASE WHEN $8::timestamptz IS NULL THEN 'draft' ELSE 'scheduled' END,
                   $6,$8,$7)
           RETURNING id"#,
    )
    .bind(actor.org_id())
    .bind(body.wa_account_id)
    .bind(body.template_id)
    .bind(&body.name)
    .bind(&body.variables)
    .bind(body.throttle_per_minute)
    .bind(actor.user_id())
    .bind(body.scheduled_at)
    .fetch_one(&mut *tx)
    .await?;

    // Materialise the audience now, so the campaign is reproducible and the
    // recipient count can't drift between preview and send.
    let inserted: i64 = if body.label_ids.is_empty() {
        sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO broadcast_recipients (broadcast_id, contact_id)
               SELECT $1, c.id FROM contacts c
               WHERE c.org_id = $2 AND c.opt_in_status <> 'opted_out' AND c.blocked_at IS NULL
               ON CONFLICT DO NOTHING
               RETURNING 1"#,
        )
        .bind(broadcast_id)
        .bind(actor.org_id())
        .fetch_all(&mut *tx)
        .await?
        .len() as i64
    } else {
        sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO broadcast_recipients (broadcast_id, contact_id)
               SELECT DISTINCT $1, c.id
               FROM contacts c
               JOIN conversations cv ON cv.contact_id = c.id
               JOIN conversation_labels cl ON cl.conversation_id = cv.id
               WHERE c.org_id = $2
                 AND c.opt_in_status <> 'opted_out'
                 AND c.blocked_at IS NULL
                 AND cl.label_id = ANY($3)
               ON CONFLICT DO NOTHING
               RETURNING 1"#,
        )
        .bind(broadcast_id)
        .bind(actor.org_id())
        .bind(&body.label_ids)
        .fetch_all(&mut *tx)
        .await?
        .len() as i64
    };

    sqlx::query("UPDATE broadcasts SET total_recipients = $2 WHERE id = $1")
        .bind(broadcast_id)
        .bind(inserted as i32)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    quota::check_broadcast_quota(&state.pool, actor.org_id(), inserted).await?;

    Ok(Json(json!({
        "id": broadcast_id,
        "total_recipients": inserted,
        "status": if body.scheduled_at.is_some() { "scheduled" } else { "draft" },
    })))
}

async fn get_one(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let b = sqlx::query_as::<_, mms_core::models::Broadcast>(
        r#"SELECT id, org_id, wa_account_id, template_id, segment_id, name, variables,
                  status, throttle_per_minute, scheduled_at, started_at, completed_at,
                  total_recipients, sent_count, delivered_count, read_count,
                  failed_count, created_at
           FROM broadcasts WHERE id=$1 AND org_id=$2"#,
    )
    .bind(id)
    .bind(actor.org_id())
    .fetch_optional(&state.pool)
    .await?
    .ok_or(Error::NotFound("broadcast"))?;
    Ok(Json(json!({ "data": b })))
}

async fn start(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let r = sqlx::query(
        r#"UPDATE broadcasts SET status='running', started_at=COALESCE(started_at, now()),
                                  updated_at=now()
           WHERE id=$1 AND org_id=$2 AND status IN ('draft','scheduled','paused')"#,
    )
    .bind(id)
    .bind(actor.org_id())
    .execute(&state.pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Error::conflict("broadcast tidak bisa dijalankan dari status ini").into());
    }

    jobs::enqueue(
        &state.pool,
        Enqueue::new(jobs::KIND_BROADCAST_RUN, json!({ "broadcast_id": id }))
            .org(actor.org_id())
            .dedup(format!("broadcast:{id}")),
    )
    .await?;

    Ok(Json(json!({ "ok": true, "status": "running" })))
}

async fn pause(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    sqlx::query(
        "UPDATE broadcasts SET status='paused', updated_at=now() WHERE id=$1 AND org_id=$2 AND status='running'",
    )
    .bind(id)
    .bind(actor.org_id())
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true, "status": "paused" })))
}
