//! Contact/CRM management for the dashboard (labels, attributes, opt-out).

use axum::extract::{Path, Query, State};
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
        .route("/", get(list))
        .route("/{id}", patch(update))
        .route("/{id}/opt-out", post(opt_out))
        .route("/labels", get(list_labels).post(create_label))
}

#[derive(Deserialize)]
struct ListQuery {
    search: Option<String>,
    #[serde(default = "d50")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn d50() -> i64 {
    50
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::inbox::list_contacts(
        &state.pool,
        actor.org_id(),
        None,
        q.search.as_deref(),
        q.limit,
        q.offset,
    )
    .await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct UpdateContact {
    name: Option<String>,
    email: Option<String>,
    attributes: Option<serde_json::Value>,
}

async fn update(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateContact>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    sqlx::query(
        r#"UPDATE contacts SET
             name = COALESCE($3, name),
             email = COALESCE($4::citext, email),
             attributes = COALESCE($5, attributes),
             updated_at = now()
           WHERE id = $1 AND org_id = $2"#,
    )
    .bind(id)
    .bind(actor.org_id())
    .bind(body.name)
    .bind(body.email)
    .bind(body.attributes)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Honouring an opt-out is a compliance requirement, not a preference —
/// broadcasts skip these contacts.
async fn opt_out(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    sqlx::query(
        r#"UPDATE contacts SET opt_in_status='opted_out', opt_in_source='manual',
                                updated_at=now()
           WHERE id=$1 AND org_id=$2"#,
    )
    .bind(id)
    .bind(actor.org_id())
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_labels(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, name, color FROM labels WHERE org_id=$1 ORDER BY name")
            .bind(actor.org_id())
            .fetch_all(&state.pool)
            .await?;
    Ok(Json(json!({
        "data": rows.iter().map(|(id, name, color)| json!({
            "id": id, "name": name, "color": color
        })).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct LabelBody {
    name: String,
    #[serde(default = "default_color")]
    color: String,
}
fn default_color() -> String {
    "#64748b".into()
}

async fn create_label(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<LabelBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO labels (org_id, name, color) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(actor.org_id())
    .bind(&body.name)
    .bind(&body.color)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "id": id })))
}
