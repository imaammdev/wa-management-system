//! Shared-inbox endpoints used by the dashboard.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use mms_provider::MessageContent;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::service::send::{self, SendRequest};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list_conversations))
        .route("/conversations/{id}", get(get_conversation))
        .route("/conversations/{id}/messages", get(messages).post(reply))
        .route("/conversations/{id}/assign", post(assign))
        .route("/conversations/{id}/status", post(set_status))
        .route("/conversations/{id}/read", post(mark_read))
        .route("/conversations/{id}/notes", post(add_note))
}

#[derive(Deserialize)]
struct ConvQuery {
    status: Option<String>,
    /// "me" resolves to the caller; otherwise a user id.
    assigned_to: Option<String>,
    #[serde(default = "d50")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn d50() -> i64 {
    50
}

async fn list_conversations(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<ConvQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;

    let assigned = match q.assigned_to.as_deref() {
        Some("me") => actor.user_id(),
        Some(other) => other.parse::<Uuid>().ok(),
        None => None,
    };

    let rows = repo::inbox::list_conversations(
        &state.pool,
        actor.org_id(),
        q.status.as_deref(),
        assigned,
        q.limit,
        q.offset,
    )
    .await?;
    Ok(Json(json!({ "data": rows })))
}

async fn get_conversation(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let conv = repo::inbox::get_conversation(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({
        "conversation": conv,
        "service_window_open": conv.service_window_open(),
    })))
}

#[derive(Deserialize)]
struct MsgQuery {
    #[serde(default = "d50")]
    limit: i64,
    before: Option<chrono::DateTime<chrono::Utc>>,
}

async fn messages(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Query(q): Query<MsgQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows =
        repo::inbox::list_messages(&state.pool, actor.org_id(), id, q.limit, q.before).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct ReplyBody {
    #[serde(flatten)]
    content: MessageContent,
}

/// Replying from the inbox goes through the same send path as the public API.
async fn reply(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<ReplyBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;

    let conv = repo::inbox::get_conversation(&state.pool, actor.org_id(), id).await?;
    let contact: (String,) = sqlx::query_as("SELECT wa_id FROM contacts WHERE id = $1")
        .bind(conv.contact_id)
        .fetch_one(&state.pool)
        .await?;

    let outcome = send::send(
        &state,
        &actor,
        SendRequest {
            wa_account_id: conv.wa_account_id,
            to: &contact.0,
            content: body.content,
            reply_to: None,
            source: "dashboard",
            idempotency_key: None,
        },
    )
    .await?;

    Ok(Json(json!({
        "id": outcome.message_id,
        "wa_message_id": outcome.wa_message_id,
        "status": outcome.status,
    })))
}

#[derive(Deserialize)]
struct AssignBody {
    /// null unassigns.
    user_id: Option<Uuid>,
}

async fn assign(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    repo::inbox::assign(&state.pool, actor.org_id(), id, body.user_id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
}

async fn set_status(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    if !["open", "pending", "snoozed", "closed"].contains(&body.status.as_str()) {
        return Err(mms_core::Error::validation("status tidak dikenal").into());
    }
    repo::inbox::set_status(&state.pool, actor.org_id(), id, &body.status).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn mark_read(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    repo::inbox::mark_read(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct NoteBody {
    body: String,
}

async fn add_note(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(note): Json<NoteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO conversation_notes (org_id, conversation_id, author_id, body)
           VALUES ($1,$2,$3,$4) RETURNING id"#,
    )
    .bind(actor.org_id())
    .bind(id)
    .bind(actor.user_id())
    .bind(&note.body)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "id": id })))
}
