use axum::extract::{Path, State};
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}/revoke", post(revoke))
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let keys = repo::keys::list(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": keys })))
}

#[derive(Deserialize)]
struct CreateKey {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
    /// Scope the key to one agency sub-client.
    customer_id: Option<Uuid>,
    #[serde(default = "default_live")]
    live: bool,
}

fn default_live() -> bool {
    true
}

async fn create(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<CreateKey>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;

    let scopes = if body.scopes.is_empty() {
        vec![
            "messages:write".to_string(),
            "messages:read".to_string(),
            "contacts:write".to_string(),
            "contacts:read".to_string(),
            "templates:read".to_string(),
        ]
    } else {
        body.scopes
    };

    let (key, plaintext) = repo::keys::create(
        &state.pool,
        actor.org_id(),
        body.customer_id,
        &body.name,
        &scopes,
        actor.user_id(),
        body.live,
    )
    .await?;

    // The only time the full key is ever returned.
    Ok(Json(json!({ "api_key": key, "secret": plaintext })))
}

async fn revoke(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::keys::revoke(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}
