//! Shared-inbox endpoints used by the dashboard.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::repo::inbox::{Assignee, ConvFilter};
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
        .route("/counts", get(counts))
        .route("/canned-responses", get(canned).post(create_canned))
        .route("/conversations", get(list_conversations))
        .route("/conversations/{id}", get(get_conversation))
        .route("/conversations/{id}/messages", get(messages).post(reply))
        .route("/conversations/{id}/assign", post(assign))
        .route("/conversations/{id}/status", post(set_status))
        .route("/conversations/{id}/read", post(mark_read))
        .route("/conversations/{id}/notes", get(list_notes).post(add_note))
        .route("/conversations/{id}/priority", post(set_priority))
        .route("/conversations/{id}/labels", post(toggle_label))
        .route("/conversations/{id}/activities", get(list_activities))
}

#[derive(Deserialize)]
struct ConvQuery {
    /// Stored lifecycle value: open | pending | held | snoozed | closed.
    status: Option<String>,
    /// Derived view: all | unread | awaiting | urgent.
    bucket: Option<String>,
    priority: Option<String>,
    /// "me" resolves to the caller, "none" to unassigned, else a user id.
    assigned_to: Option<String>,
    label_id: Option<Uuid>,
    wa_account_id: Option<Uuid>,
    search: Option<String>,
    #[serde(default = "d30")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn d30() -> i64 {
    30
}
fn d50() -> i64 {
    50
}

/// Every number in the inbox filter rail, in one query.
async fn counts(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<ConvQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let c = repo::inbox::inbox_counts(
        &state.pool,
        actor.org_id(),
        actor.user_id(),
        q.wa_account_id,
    )
    .await?;
    Ok(Json(json!({ "counts": c })))
}

async fn list_conversations(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<ConvQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;

    let assignee = match q.assigned_to.as_deref() {
        None | Some("") | Some("any") => None,
        Some("me") => actor.user_id().map(Assignee::User),
        Some("none") => Some(Assignee::Unassigned),
        Some(other) => other.parse::<Uuid>().ok().map(Assignee::User),
    };

    let rows = repo::inbox::list_conversations(
        &state.pool,
        actor.org_id(),
        ConvFilter {
            status: q.status.as_deref().filter(|s| !s.is_empty()),
            bucket: q.bucket.as_deref().filter(|s| !s.is_empty() && *s != "all"),
            priority: q.priority.as_deref().filter(|s| !s.is_empty()),
            assignee,
            label_id: q.label_id,
            wa_account_id: q.wa_account_id,
            search: q.search.as_deref().filter(|s| !s.trim().is_empty()),
            limit: q.limit,
            offset: q.offset,
        },
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
    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::conversation(id, actor.user_id()),
        repo::crm::KIND_ASSIGNED,
        if body.user_id.is_some() {
            "Percakapan ditugaskan"
        } else {
            "Penugasan dilepas"
        },
        json!({ "user_id": body.user_id }),
    )
    .await;
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
    if !["open", "pending", "held", "snoozed", "closed"].contains(&body.status.as_str()) {
        return Err(mms_core::Error::validation("status tidak dikenal").into());
    }
    repo::inbox::set_status(&state.pool, actor.org_id(), id, &body.status).await?;
    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::conversation(id, actor.user_id()),
        repo::crm::KIND_STATUS,
        &format!("Status diubah ke {}", body.status),
        json!({ "status": body.status }),
    )
    .await;
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

#[derive(Deserialize)]
struct PriorityBody {
    priority: String,
}

async fn set_priority(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<PriorityBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    repo::inbox::set_priority(&state.pool, actor.org_id(), id, &body.priority).await?;

    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::conversation(id, actor.user_id()),
        repo::crm::KIND_PRIORITY,
        &format!("Prioritas diubah ke {}", body.priority),
        json!({ "priority": body.priority }),
    )
    .await;

    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct LabelToggle {
    label_id: Uuid,
    #[serde(default = "yes")]
    on: bool,
}
fn yes() -> bool {
    true
}

async fn toggle_label(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<LabelToggle>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    repo::inbox::set_conversation_label(&state.pool, actor.org_id(), id, body.label_id, body.on)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_notes(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::crm::notes(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct ActivityQuery {
    #[serde(default = "d50")]
    limit: i64,
}

async fn list_activities(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Query(q): Query<ActivityQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::crm::activities(&state.pool, actor.org_id(), Some(id), None, q.limit).await?;
    Ok(Json(json!({ "data": rows })))
}

async fn canned(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::crm::canned_responses(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct CannedBody {
    shortcut: String,
    body: String,
}

async fn create_canned(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(b): Json<CannedBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let r = repo::crm::create_canned_response(&state.pool, actor.org_id(), &b.shortcut, &b.body)
        .await?;
    Ok(Json(json!({ "data": r })))
}
