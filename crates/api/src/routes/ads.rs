//! Click-to-WhatsApp ad attribution.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use serde_json::json;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/overview", get(overview))
}

async fn overview(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let org = actor.org_id();
    let (summary, ads) = tokio::try_join!(
        repo::ads::summary(&state.pool, org),
        repo::ads::by_ad(&state.pool, org, 50),
    )?;
    Ok(Json(json!({ "summary": summary, "ads": ads })))
}
