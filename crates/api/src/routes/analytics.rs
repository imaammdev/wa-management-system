//! Reporting endpoints. One request fills the whole Analytics page.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mms_core::repo::analytics as an;
use mms_core::tenancy::Capability;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/overview", get(overview))
}

#[derive(Deserialize)]
struct RangeQuery {
    #[serde(default = "d30")]
    days: i64,
}
fn d30() -> i64 {
    30
}

async fn overview(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let org = actor.org_id();
    let (from, to) = an::window(q.days);

    // Run the independent aggregates concurrently — they hit different
    // indexes and the page is useless until all of them land.
    let (volume, delivery, categories, convs, agents, labels, broadcasts) = tokio::try_join!(
        an::volume_by_day(&state.pool, org, from, to),
        an::delivery_stats(&state.pool, org, from, to),
        an::by_category(&state.pool, org, from, to),
        an::conversation_stats(&state.pool, org, from, to),
        an::agent_performance(&state.pool, org, from, to),
        an::top_labels(&state.pool, org),
        an::broadcast_stats(&state.pool, org, from, to),
    )?;

    Ok(Json(json!({
        "range": { "days": q.days.clamp(1, 365), "from": from, "to": to },
        "volume": volume,
        "delivery": {
            "counts": delivery,
            "delivery_rate": delivery.delivery_rate(),
            "read_rate": delivery.read_rate(),
        },
        "categories": categories,
        "conversations": convs,
        "agents": agents,
        "labels": labels,
        "broadcasts": broadcasts,
    })))
}
