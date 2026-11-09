use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use mms_core::{quota, Error};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::{DashboardActor, SessionUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/current", get(current))
        .route("/current/usage", get(usage))
        .route("/current/members", get(members).post(invite))
        .route(
            "/current/members/{user_id}",
            patch(update_role).delete(remove),
        )
        .route(
            "/current/customers",
            get(list_customers).post(create_customer),
        )
        .route("/invitations/accept", post(accept_invite))
}

#[derive(Deserialize)]
struct CreateOrg {
    name: String,
}

async fn create(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Json(body): Json<CreateOrg>,
) -> ApiResult<Json<serde_json::Value>> {
    let org = repo::orgs::create_with_owner(&state.pool, &body.name, user.id).await?;
    Ok(Json(json!({ "organization": org })))
}

async fn list(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
) -> ApiResult<Json<serde_json::Value>> {
    let orgs = repo::orgs::list_for_user(&state.pool, user.id).await?;
    Ok(Json(json!({
        "data": orgs.iter().map(|(o, role)| json!({
            "id": o.id, "name": o.name, "slug": o.slug, "role": role,
        })).collect::<Vec<_>>()
    })))
}

async fn current(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let org = repo::orgs::get(&state.pool, actor.org_id()).await?;
    let sub = repo::billing::subscription_for_org(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "organization": org, "subscription": sub })))
}

async fn usage(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let s = quota::summary(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "usage": s })))
}

async fn members(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::InboxRead)?;
    let rows = repo::orgs::list_members(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({
        "data": rows.iter().map(|(id, name, email, role)| json!({
            "id": id, "name": name, "email": email, "role": role,
        })).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct InviteBody {
    email: String,
    role: String,
}

async fn invite(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<InviteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    quota::check_member_quota(&state.pool, actor.org_id()).await?;
    mms_core::tenancy::Role::parse(&body.role)?;

    let user_id = actor.user_id().ok_or(Error::Unauthorized)?;
    let token = repo::orgs::invite_member(
        &state.pool,
        actor.org_id(),
        &body.email,
        &body.role,
        user_id,
    )
    .await?;

    // No mailer yet — the dashboard shows the link for the admin to share.
    Ok(Json(json!({
        "invite_url": format!("{}/invite/{}", state.cfg.app_url, token),
        "expires_in_days": 7,
    })))
}

#[derive(Deserialize)]
struct RoleBody {
    role: String,
}

async fn update_role(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(user_id): Path<Uuid>,
    Json(body): Json<RoleBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    mms_core::tenancy::Role::parse(&body.role)?;
    repo::orgs::update_member_role(&state.pool, actor.org_id(), user_id, &body.role).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn remove(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::orgs::remove_member(&state.pool, actor.org_id(), user_id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct AcceptBody {
    token: String,
}

async fn accept_invite(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Json(body): Json<AcceptBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let org_id = repo::orgs::accept_invitation(&state.pool, &body.token, user.id).await?;
    Ok(Json(json!({ "organization_id": org_id })))
}

// --------------------------------------------------------- agency sub-clients

async fn list_customers(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = repo::orgs::list_customers(&state.pool, actor.org_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct CreateCustomer {
    name: String,
    external_ref: Option<String>,
}

async fn create_customer(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<CreateCustomer>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let (customer, setup_token) = repo::orgs::create_customer(
        &state.pool,
        actor.org_id(),
        &body.name,
        body.external_ref.as_deref(),
    )
    .await?;

    Ok(Json(json!({
        "customer": customer,
        // Self-service onboarding link for the sub-client.
        "setup_url": format!("{}/setup/{}", state.cfg.app_url, setup_token),
    })))
}
