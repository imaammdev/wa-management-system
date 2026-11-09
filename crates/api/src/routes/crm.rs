//! Customer panel: contact detail, deals, timeline and Meta lead events.

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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/contacts/{id}/panel", get(panel))
        .route("/contacts/{id}/labels", post(toggle_label))
        .route("/contacts/{id}/activities", get(activities))
        .route("/contacts/{id}/deals", get(deals).post(create_deal))
        .route("/contacts/{id}/meta-events", post(send_meta_event))
        .route("/deals/{id}/status", post(deal_status))
        .route("/meta-events/catalog", get(meta_catalog))
}

async fn panel(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let p = repo::crm::contact_panel(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "contact": p })))
}

#[derive(Deserialize)]
struct LabelToggle {
    label_id: Uuid,
    #[serde(default = "yes")]
    on: bool,
}
fn yes() -> bool {
    true
}

async fn toggle_label(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(b): Json<LabelToggle>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    repo::crm::set_contact_label(&state.pool, actor.org_id(), id, b.label_id, b.on).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default = "d50")]
    limit: i64,
}
fn d50() -> i64 {
    50
}

async fn activities(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::crm::activities(&state.pool, actor.org_id(), None, Some(id), q.limit).await?;
    Ok(Json(json!({ "data": rows })))
}

async fn deals(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::crm::deals_for_contact(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct NewDealBody {
    title: String,
    #[serde(default)]
    value_idr: i64,
    stage_id: Option<Uuid>,
    expected_close_at: Option<chrono::NaiveDate>,
}

async fn create_deal(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(contact_id): Path<Uuid>,
    Json(b): Json<NewDealBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let deal = repo::crm::create_deal(
        &state.pool,
        actor.org_id(),
        repo::crm::NewDeal {
            contact_id,
            title: &b.title,
            value_idr: b.value_idr,
            stage_id: b.stage_id,
            owner_id: actor.user_id(),
            expected_close_at: b.expected_close_at,
        },
    )
    .await?;

    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::contact(contact_id, actor.user_id()),
        repo::crm::KIND_DEAL,
        &format!("Deal dibuat: {}", deal.title),
        json!({ "deal_id": deal.id, "value_idr": deal.value_idr }),
    )
    .await;

    Ok(Json(json!({ "data": deal })))
}

#[derive(Deserialize)]
struct DealStatusBody {
    status: String,
    lost_reason: Option<String>,
}

async fn deal_status(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(b): Json<DealStatusBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let deal = repo::crm::update_deal_status(
        &state.pool,
        actor.org_id(),
        id,
        &b.status,
        b.lost_reason.as_deref(),
    )
    .await?;

    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::contact(deal.contact_id, actor.user_id()),
        repo::crm::KIND_DEAL,
        &format!("Deal '{}' ditandai {}", deal.title, b.status),
        json!({ "deal_id": deal.id, "status": b.status }),
    )
    .await;

    Ok(Json(json!({ "data": deal })))
}

/// The milestones the panel offers, so the UI isn't hard-coding Meta's list.
async fn meta_catalog() -> Json<serde_json::Value> {
    Json(json!({
        "data": [
            { "name": "LeadSubmitted",
              "label": "LeadSubmitted",
              "description": "Tandai contact sebagai lead baru." },
            { "name": "QualifiedLead",
              "label": "QualifiedLead",
              "description": "Tandai contact sebagai lead berkualitas." },
            { "name": "ConvertedLead",
              "label": "ConvertedLead",
              "description": "Lead berhasil menjadi pelanggan." },
            { "name": "DisqualifiedLead",
              "label": "DisqualifiedLead",
              "description": "Lead tidak memenuhi syarat." }
        ]
    }))
}

#[derive(Deserialize)]
struct MetaEventBody {
    event_name: String,
    conversation_id: Option<Uuid>,
    wa_account_id: Option<Uuid>,
}

/// Queues a Click-to-WhatsApp milestone report. The Graph call itself is the
/// worker's job, so a slow Meta never blocks the agent.
async fn send_meta_event(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(contact_id): Path<Uuid>,
    Json(b): Json<MetaEventBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let ev = repo::crm::queue_meta_event(
        &state.pool,
        actor.org_id(),
        contact_id,
        b.conversation_id,
        b.wa_account_id,
        &b.event_name,
        actor.user_id(),
    )
    .await?;

    let _ = repo::crm::log(
        &state.pool,
        actor.org_id(),
        repo::crm::Target::contact(contact_id, actor.user_id())
            .with_conversation(b.conversation_id),
        repo::crm::KIND_META_EVENT,
        &format!("Event {} dikirim ke Meta", b.event_name),
        json!({ "event_name": b.event_name }),
    )
    .await;

    Ok(Json(json!({ "data": ev })))
}
