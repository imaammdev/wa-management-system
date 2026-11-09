//! Kichat: a private browser chat room for one customer.
//!
//! Two surfaces. The dashboard half issues and closes invites and requires a
//! session. The public half is reached with the link token alone — no account
//! — so it is rate limited and returns only that one conversation's text.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::{repo, Error};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::DashboardActor;
use crate::state::AppState;

/// Dashboard-side routes, mounted under /api.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/conversations/{id}/invite", post(invite).get(sessions))
        .route("/sessions/{id}/close", post(close))
}

/// Public routes, mounted at the root — no authentication.
pub fn public_router() -> Router<AppState> {
    Router::new()
        // Namespaced under /room so it does not collide with the SPA route
        // that renders the chat page at /kichat/{token}.
        .route("/{token}/room", get(room))
        .route("/{token}/messages", get(poll).post(send))
}

#[derive(Deserialize)]
struct InviteBody {
    #[serde(default = "d14")]
    ttl_days: i64,
}
fn d14() -> i64 {
    14
}

async fn invite(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(conversation_id): Path<Uuid>,
    Json(b): Json<InviteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(mms_core::tenancy::Capability::InboxWrite)?;
    let (session, token) = repo::kichat::invite(
        &state.pool,
        actor.org_id(),
        conversation_id,
        actor.user_id(),
        b.ttl_days,
    )
    .await?;

    Ok(Json(json!({
        "session": session,
        // Shown once. Only the hash is stored.
        "url": format!("{}/kichat/{}", state.cfg.app_url, token),
    })))
}

async fn sessions(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(conversation_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(mms_core::tenancy::Capability::InboxRead)?;
    let rows = repo::kichat::for_conversation(&state.pool, actor.org_id(), conversation_id).await?;
    Ok(Json(json!({ "data": rows })))
}

async fn close(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(mms_core::tenancy::Capability::InboxWrite)?;
    repo::kichat::close(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------ public half

/// Buckets public traffic per link, so one shared token can't be used to
/// hammer the API.
fn throttle(state: &AppState, token: &str, per_minute: i32) -> Result<(), ApiError> {
    // Key on a hash, never the raw token — these end up in memory dumps.
    let key = format!("kichat:{}", mms_core::crypto::sha256_hex(token));
    state
        .limiter
        .check(&key, per_minute)
        .map_err(|retry_after_secs| ApiError(Error::RateLimited { retry_after_secs }))
}

async fn room(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    throttle(&state, &token, 120)?;
    let session = repo::kichat::resolve(&state.pool, &token).await?;
    repo::kichat::touch(&state.pool, session.id).await?;

    let (org_name, contact_name): (String, Option<String>) = sqlx::query_as(
        r#"SELECT o.name, COALESCE(c.name, c.profile_name)
           FROM kichat_sessions k
           JOIN organizations o ON o.id = k.org_id
           JOIN contacts c ON c.id = k.contact_id
           WHERE k.id = $1"#,
    )
    .bind(session.id)
    .fetch_one(&state.pool)
    .await?;

    let messages = repo::kichat::transcript(&state.pool, session.conversation_id, 100).await?;

    Ok(Json(json!({
        "business": org_name,
        "customer": contact_name,
        "expires_at": session.expires_at,
        "messages": messages,
    })))
}

#[derive(Deserialize)]
struct PollQuery {
    /// Newest message the browser already has.
    after: Option<chrono::DateTime<chrono::Utc>>,
}

async fn poll(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<PollQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    throttle(&state, &token, 120)?;
    let session = repo::kichat::resolve(&state.pool, &token).await?;
    repo::kichat::touch(&state.pool, session.id).await?;

    let all = repo::kichat::transcript(&state.pool, session.conversation_id, 100).await?;
    let fresh: Vec<_> = match q.after {
        Some(t) => all.into_iter().filter(|m| m.created_at > t).collect(),
        None => all,
    };
    Ok(Json(json!({ "messages": fresh })))
}

#[derive(Deserialize)]
struct SendBody {
    body: String,
}

async fn send(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(b): Json<SendBody>,
) -> ApiResult<Json<serde_json::Value>> {
    // Writes are rarer than polls, and cheaper to abuse.
    throttle(&state, &token, 20)?;
    let session = repo::kichat::resolve(&state.pool, &token).await?;
    let id = repo::kichat::record_customer_message(&state.pool, &session, &b.body).await?;
    Ok(Json(json!({ "id": id })))
}
