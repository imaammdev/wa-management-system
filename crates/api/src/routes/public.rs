//! Unauthenticated endpoints that back the marketing site.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mms_core::repo;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/plans", get(plans))
        .route("/meta-rates", get(meta_rates))
        .route("/estimate", get(estimate))
}

async fn plans(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let plans = repo::billing::list_public_plans(&state.pool).await?;
    Ok(Json(json!({ "data": plans })))
}

async fn meta_rates(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let rates = repo::billing::meta_rates(&state.pool, "ID").await?;
    Ok(Json(json!({ "data": rates })))
}

#[derive(Deserialize)]
struct EstimateQuery {
    #[serde(default = "default_plan")]
    plan: String,
    #[serde(default)]
    marketing: i64,
    #[serde(default)]
    utility: i64,
    #[serde(default)]
    authentication: i64,
    #[serde(default)]
    service: i64,
    /// Number of connected phone numbers — the service free tier is per number.
    #[serde(default = "one")]
    numbers: i64,
    #[serde(default = "default_interval")]
    interval: String,
}

fn default_plan() -> String {
    "starter".into()
}
fn default_interval() -> String {
    "monthly".into()
}
fn one() -> i64 {
    1
}

/// Server-side version of the pricing calculator, so the marketing page and
/// the in-app estimate can never disagree.
async fn estimate(
    State(state): State<AppState>,
    Query(q): Query<EstimateQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let plan = repo::billing::plan_by_code(&state.pool, &q.plan).await?;
    let rates = repo::billing::meta_rates(&state.pool, "ID").await?;

    let rate_of = |cat: &str| -> (f64, i64) {
        rates
            .iter()
            .find(|r| r.category == cat)
            .map(|r| (r.rate_idr, r.free_tier_per_number_per_month as i64))
            .unwrap_or((0.0, 0))
    };

    let mut lines = Vec::new();
    let mut meta_total = 0f64;

    for (cat, volume) in [
        ("marketing", q.marketing),
        ("utility", q.utility),
        ("authentication", q.authentication),
        ("service", q.service),
    ] {
        let (rate, free_per_number) = rate_of(cat);
        let free = free_per_number.saturating_mul(q.numbers.max(1));
        let billable = (volume - free).max(0);
        let cost = billable as f64 * rate;
        meta_total += cost;
        lines.push(json!({
            "category": cat,
            "volume": volume,
            "free_allowance": free.min(volume),
            "billable": billable,
            "rate_idr": rate,
            "cost_idr": cost.round(),
        }));
    }

    let platform = if q.interval == "yearly" {
        // Yearly is billed up front; show the monthly equivalent.
        plan.price_idr_yearly as f64 / 12.0
    } else {
        plan.price_idr_monthly as f64
    };

    Ok(Json(json!({
        "plan": { "code": plan.code, "name": plan.name, "interval": q.interval },
        "platform_cost_idr": platform.round(),
        "meta_lines": lines,
        "meta_total_idr": meta_total.round(),
        "total_idr": (platform + meta_total).round(),
        "notes": [
            "Biaya pesan ditagih langsung oleh Meta ke akun WhatsApp Business Anda.",
            "Estimasi belum termasuk PPN."
        ]
    })))
}
