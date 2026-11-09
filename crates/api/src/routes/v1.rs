//! The public, documented API. Shapes mirror Meta's Cloud API closely enough
//! that an existing integration can be repointed by changing the base URL and
//! the bearer token.

use axum::extract::{Path, Query, State};
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::Error;
use mms_provider::MessageContent;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::{idempotency_key, AnyActor};
use crate::service::send::{self, SendRequest};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/messages", post(send_message))
        .route("/messages/{id}", get(get_message))
        .route("/contacts", get(list_contacts).post(upsert_contact))
        .route("/templates", get(list_templates))
        .route("/numbers", get(list_numbers))
}

#[derive(Deserialize)]
struct SendBody {
    /// Which hosted number to send from.
    wa_account_id: Uuid,
    to: String,
    #[serde(flatten)]
    content: MessageContent,
    #[serde(default)]
    reply_to: Option<String>,
}

async fn send_message(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
    parts: Parts,
    Json(body): Json<SendBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let idem = idempotency_key(&parts);

    let outcome = send::send(
        &state,
        &actor,
        SendRequest {
            wa_account_id: body.wa_account_id,
            to: &body.to,
            content: body.content,
            reply_to: body.reply_to,
            source: if actor.api_key_id().is_some() {
                "api"
            } else {
                "dashboard"
            },
            idempotency_key: idem.as_deref(),
        },
    )
    .await?;

    Ok(Json(json!({
        "messaging_product": "whatsapp",
        "id": outcome.message_id,
        "wa_message_id": outcome.wa_message_id,
        "conversation_id": outcome.conversation_id,
        "status": outcome.status,
    })))
}

async fn get_message(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require_scope("messages:read")?;
    let msg = sqlx::query_as::<_, mms_core::models::Message>(
        r#"SELECT id, org_id, conversation_id, wa_account_id, contact_id, direction,
                  wa_message_id, msg_type, body, payload, status, error_code, error_detail,
                  pricing_category, source, sent_by_user_id, sent_at, delivered_at,
                  read_at, created_at
           FROM messages WHERE id = $1 AND org_id = $2"#,
    )
    .bind(id)
    .bind(actor.org_id())
    .fetch_optional(&state.pool)
    .await?
    .ok_or(Error::NotFound("message"))?;

    Ok(Json(json!({ "data": msg })))
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    search: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_contacts(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require_scope("contacts:read")?;
    let rows = repo::inbox::list_contacts(
        &state.pool,
        actor.org_id(),
        actor.customer_id(),
        q.search.as_deref(),
        q.limit,
        q.offset,
    )
    .await?;
    Ok(Json(
        json!({ "data": rows, "has_more": rows.len() as i64 == q.limit }),
    ))
}

#[derive(Deserialize)]
struct UpsertContact {
    wa_id: String,
    name: Option<String>,
}

async fn upsert_contact(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
    Json(body): Json<UpsertContact>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require_scope("contacts:write")?;
    let c = repo::inbox::upsert_contact(
        &state.pool,
        actor.org_id(),
        actor.customer_id(),
        &body.wa_id,
        body.name.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "data": c })))
}

async fn list_templates(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require_scope("templates:read")?;
    let rows = sqlx::query_as::<_, mms_core::models::MessageTemplate>(
        r#"SELECT id, org_id, wa_account_id, name, language, category, components,
                  status, rejected_reason, provider_ref, created_at
           FROM message_templates WHERE org_id = $1 ORDER BY name"#,
    )
    .bind(actor.org_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "data": rows })))
}

async fn list_numbers(
    State(state): State<AppState>,
    AnyActor(actor): AnyActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = repo::wa::list(&state.pool, actor.org_id(), actor.customer_id()).await?;
    Ok(Json(json!({
        "data": rows.iter().map(|a| json!({
            "id": a.id,
            "label": a.label,
            "display_phone_number": a.display_phone_number,
            "status": a.status,
            "quality_rating": a.quality_rating,
        })).collect::<Vec<_>>()
    })))
}
