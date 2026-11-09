//! AI agent configuration and run history.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

/// Models the dashboard offers. Opus 5.5 is the default; the cheaper tiers
/// are there for high-volume deployments that choose to trade quality.
const MODELS: [(&str, &str); 3] = [
    ("claude-opus-5-5", "Opus 5.5 — paling pintar (disarankan)"),
    ("claude-sonnet-5-5", "Sonnet 5.5 — seimbang"),
    ("claude-haiku-5-5", "Haiku 5.5 — paling murah & cepat"),
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents", get(list).post(upsert))
        .route("/agents/{id}", axum::routing::delete(remove))
        .route("/runs", get(runs))
        .route("/models", get(models))
        .route("/conversations/{id}/toggle", post(toggle_for_conversation))
}

async fn models() -> Json<serde_json::Value> {
    Json(json!({
        "data": MODELS.iter().map(|(id, label)| json!({ "id": id, "label": label }))
            .collect::<Vec<_>>()
    }))
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let agents = repo::ai::list(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({
        "data": agents,
        "ai_configured": state.cfg.anthropic_api_key.is_some(),
    })))
}

#[derive(Deserialize)]
struct UpsertBody {
    id: Option<Uuid>,
    wa_account_id: Option<Uuid>,
    name: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default)]
    system_prompt: String,
    #[serde(default)]
    handoff_rules: serde_json::Value,
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_fallback")]
    fallback_message: String,
}
fn default_model() -> String {
    "claude-opus-5-5".into()
}
fn default_fallback() -> String {
    "Mohon tunggu sebentar, tim kami akan segera membantu.".into()
}

async fn upsert(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(b): Json<UpsertBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    if !MODELS.iter().any(|(id, _)| *id == b.model) {
        return Err(mms_core::Error::validation("model tidak dikenal").into());
    }

    let agent = repo::ai::upsert(
        &state.pool,
        actor.org_id(),
        repo::ai::UpsertAgent {
            id: b.id,
            wa_account_id: b.wa_account_id,
            name: &b.name,
            model: &b.model,
            system_prompt: &b.system_prompt,
            handoff_rules: if b.handoff_rules.is_null() {
                json!({})
            } else {
                b.handoff_rules
            },
            enabled: b.enabled,
            fallback_message: &b.fallback_message,
        },
    )
    .await?;

    Ok(Json(json!({
        "data": agent,
        "ai_configured": state.cfg.anthropic_api_key.is_some(),
    })))
}

async fn remove(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::ai::delete(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RunQuery {
    #[serde(default = "d50")]
    limit: i64,
}
fn d50() -> i64 {
    50
}

async fn runs(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<RunQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::ai::recent_runs(&state.pool, actor.org_id(), q.limit).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct ToggleBody {
    enabled: bool,
}

/// Lets an agent hand a thread back to the AI, or take it away.
async fn toggle_for_conversation(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(b): Json<ToggleBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxWrite)?;
    sqlx::query(
        "UPDATE conversations SET ai_agent_enabled=$3, updated_at=now() WHERE id=$1 AND org_id=$2",
    )
    .bind(id)
    .bind(actor.org_id())
    .bind(b.enabled)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true, "enabled": b.enabled })))
}
