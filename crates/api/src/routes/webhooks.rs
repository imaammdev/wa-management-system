//! Customer-facing webhook configuration, delivery log and replay.

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
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
        .route("/endpoints", get(list_endpoints).post(create_endpoint))
        .route("/endpoints/{id}", delete(delete_endpoint))
        .route("/endpoints/{id}/rotate-secret", post(rotate_secret))
        .route("/deliveries", get(list_deliveries))
        .route("/deliveries/{id}/replay", post(replay))
}

async fn list_endpoints(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let rows = repo::webhooks::list_endpoints(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct CreateEndpoint {
    url: String,
    #[serde(default = "default_events")]
    events: Vec<String>,
    description: Option<String>,
    customer_id: Option<Uuid>,
}

fn default_events() -> Vec<String> {
    vec!["message.received".into(), "message.status".into()]
}

async fn create_endpoint(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<CreateEndpoint>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let ep = repo::webhooks::create_endpoint(
        &state.pool,
        actor.org_id(),
        body.customer_id,
        &body.url,
        &body.events,
        body.description.as_deref(),
    )
    .await?;

    // The signing secret is shown once, here.
    let secret = ep.secret.clone();
    Ok(Json(json!({ "endpoint": ep, "secret": secret })))
}

async fn delete_endpoint(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::webhooks::delete_endpoint(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn rotate_secret(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let secret = repo::webhooks::rotate_secret(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "secret": secret })))
}

#[derive(Deserialize)]
struct DeliveryQuery {
    endpoint_id: Option<Uuid>,
    status: Option<String>,
    #[serde(default = "d50")]
    limit: i64,
}
fn d50() -> i64 {
    50
}

async fn list_deliveries(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<DeliveryQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let rows = repo::webhooks::list_deliveries(
        &state.pool,
        actor.org_id(),
        q.endpoint_id,
        q.status.as_deref(),
        q.limit,
    )
    .await?;
    Ok(Json(json!({ "data": rows })))
}

/// Re-sends a past event — the "Replay event" affordance.
async fn replay(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::webhooks::replay(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true, "status": "pending" })))
}
