//! Subscription and plan management. Payment capture is left to a provider
//! adapter (Midtrans/Xendit); this exposes plan changes and the invoice list.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscription", get(subscription))
        .route("/subscription/change-plan", post(change_plan))
        .route("/invoices", get(invoices))
}

async fn subscription(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let sub = repo::billing::subscription_for_org(&state.pool, actor.org_id()).await?;
    let plans = repo::billing::list_public_plans(&state.pool).await?;
    Ok(Json(json!({ "subscription": sub, "plans": plans })))
}

#[derive(Deserialize)]
struct ChangePlan {
    plan_code: String,
    #[serde(default = "monthly")]
    interval: String,
}
fn monthly() -> String {
    "monthly".into()
}

async fn change_plan(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<ChangePlan>,
) -> ApiResult<Json<serde_json::Value>> {
    // Only an owner may move money.
    actor.require(Capability::OrgOwner)?;
    let sub =
        repo::billing::change_plan(&state.pool, actor.org_id(), &body.plan_code, &body.interval)
            .await?;

    // TODO(payments): hand off to Midtrans/Xendit and return a payment_url.
    // Until then the plan switches immediately, which is what a dev
    // environment wants.
    Ok(Json(json!({ "subscription": sub, "payment_url": null })))
}

/// Invoice list projection — the full row carries line items we don't need here.
#[derive(serde::Serialize, sqlx::FromRow)]
struct InvoiceRow {
    id: uuid::Uuid,
    number: String,
    status: String,
    total_idr: i64,
    paid_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn invoices(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let rows = sqlx::query_as::<_, InvoiceRow>(
        r#"SELECT id, number, status, total_idr, paid_at
               FROM invoices WHERE org_id=$1 ORDER BY created_at DESC LIMIT 100"#,
    )
    .bind(actor.org_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "data": rows })))
}
