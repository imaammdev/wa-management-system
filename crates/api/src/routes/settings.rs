//! Konfigurasi & Workspace: org profile, labels, canned responses, pipelines.

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_all).patch(update))
        .route("/labels", get(labels))
        .route("/labels/{id}", patch(update_label).delete(delete_label))
        .route(
            "/canned-responses/{id}",
            axum::routing::delete(delete_canned),
        )
        .route("/pipelines", get(pipelines))
        .route("/pipelines/{id}/stages", post(add_stage))
}

/// Everything the settings screen renders, in one round trip.
async fn get_all(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let org = actor.org_id();
    let (settings, labels, canned, pipelines, members) = tokio::try_join!(
        repo::settings::get_settings(&state.pool, org),
        repo::settings::labels(&state.pool, org),
        repo::crm::canned_responses(&state.pool, org),
        repo::settings::pipelines(&state.pool, org),
        repo::orgs::list_members(&state.pool, org),
    )?;

    Ok(Json(json!({
        "organization": settings,
        "labels": labels,
        "canned_responses": canned,
        "pipelines": pipelines,
        "members": members.iter().map(|(id, name, email, role)| json!({
            "id": id, "name": name, "email": email, "role": role,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    timezone: Option<String>,
    /// Merged into `settings`, not replacing it.
    settings: Option<serde_json::Value>,
}

async fn update(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(b): Json<UpdateBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let s = repo::settings::update_settings(
        &state.pool,
        actor.org_id(),
        b.name.as_deref(),
        b.timezone.as_deref(),
        b.settings.as_ref(),
    )
    .await?;
    Ok(Json(json!({ "organization": s })))
}

async fn labels(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::settings::labels(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct LabelPatch {
    name: Option<String>,
    color: Option<String>,
}

async fn update_label(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(b): Json<LabelPatch>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    repo::settings::update_label(
        &state.pool,
        actor.org_id(),
        id,
        b.name.as_deref(),
        b.color.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_label(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::settings::delete_label(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_canned(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    repo::settings::delete_canned(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn pipelines(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::settings::pipelines(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct StageBody {
    name: String,
}

async fn add_stage(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(pipeline_id): Path<Uuid>,
    Json(b): Json<StageBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    repo::settings::add_stage(&state.pool, actor.org_id(), pipeline_id, &b.name).await?;
    Ok(Json(json!({ "ok": true })))
}
