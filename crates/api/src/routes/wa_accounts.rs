use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use mms_core::{quota, Error};
use mms_provider::ProviderCreds;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).delete(remove))
        .route("/{id}/connect", post(connect))
        .route("/{id}/verify", post(verify))
        .route("/{id}/simulate-inbound", post(simulate_inbound))
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = repo::wa::list(&state.pool, actor.org_id(), actor.customer_id()).await?;
    Ok(Json(json!({ "data": rows })))
}

#[derive(Deserialize)]
struct CreateAccount {
    label: String,
    customer_id: Option<Uuid>,
    /// 'meta_cloud' | 'mock'. Defaults to the deployment's WA_PROVIDER.
    provider: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Json(body): Json<CreateAccount>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    quota::check_wa_account_quota(&state.pool, actor.org_id()).await?;

    let provider = body
        .provider
        .unwrap_or_else(|| state.cfg.wa_provider.clone());
    if provider != "meta_cloud" && provider != "mock" {
        return Err(Error::validation("provider harus meta_cloud atau mock").into());
    }

    let acc = repo::wa::create(
        &state.pool,
        actor.org_id(),
        body.customer_id,
        &provider,
        &body.label,
    )
    .await?;

    // Meta must be pointed at this exact URL for the number to receive.
    Ok(Json(json!({
        "whatsapp_account": acc,
        "webhook_setup": {
            "callback_url": format!("{}/webhooks/meta", state.cfg.app_url),
            "verify_token": state.cfg.meta_webhook_verify_token,
        }
    })))
}

async fn get_one(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let acc = repo::wa::get(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "whatsapp_account": acc })))
}

#[derive(Deserialize)]
struct ConnectBody {
    waba_id: String,
    phone_number_id: String,
    /// Meta system-user token. Sealed with AES-256-GCM before it is stored.
    access_token: String,
}

async fn connect(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<ConnectBody>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    let existing = repo::wa::get(&state.pool, actor.org_id(), id).await?;

    // Confirm the credentials actually work before persisting them, so a
    // number never sits in 'connected' with a token that cannot send.
    let provider = state.provider_for(&existing.provider);
    let probe = ProviderCreds {
        wa_account_id: existing.id,
        phone_number_id: body.phone_number_id.clone(),
        waba_id: Some(body.waba_id.clone()),
        access_token: body.access_token.clone(),
    };
    let info = provider.phone_number_info(&probe).await?;

    let acc = repo::wa::connect(
        &state.pool,
        &state.cfg.encryption_key,
        repo::wa::ConnectParams {
            org_id: actor.org_id(),
            id,
            waba_id: &body.waba_id,
            phone_number_id: &body.phone_number_id,
            display_phone_number: info.display_phone_number.as_deref(),
            verified_name: info.verified_name.as_deref(),
            access_token: &body.access_token,
        },
    )
    .await?;

    Ok(Json(
        json!({ "whatsapp_account": acc, "provider_info": info }),
    ))
}

/// Re-reads the number's profile from the provider — the "is it still healthy"
/// button in the dashboard.
async fn verify(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let acc = repo::wa::get(&state.pool, actor.org_id(), id).await?;
    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .ok_or_else(|| Error::validation("nomor belum terhubung"))?,
        waba_id: acc.waba_id.clone(),
        access_token: repo::wa::access_token(&state.cfg.encryption_key, &acc)?,
    };

    match state
        .provider_for(&acc.provider)
        .phone_number_info(&creds)
        .await
    {
        Ok(info) => Ok(Json(json!({ "ok": true, "provider_info": info }))),
        Err(e) => {
            repo::wa::mark_error(&state.pool, acc.id, &e.to_string()).await?;
            Err(e.into())
        }
    }
}

async fn remove(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::OrgAdmin)?;
    repo::wa::delete(&state.pool, actor.org_id(), id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SimulateBody {
    from: String,
    text: String,
}

/// Dev-only: pushes a Meta-shaped inbound message through the real parsing and
/// persistence path, so the inbox can be exercised without a live number.
async fn simulate_inbound(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(id): Path<Uuid>,
    Json(body): Json<SimulateBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let acc = repo::wa::get(&state.pool, actor.org_id(), id).await?;
    if acc.provider != "mock" {
        return Err(Error::validation("simulasi hanya untuk nomor dengan provider 'mock'").into());
    }
    let phone_number_id = acc
        .phone_number_id
        .clone()
        .unwrap_or_else(|| format!("mock-{}", acc.id.simple()));

    let payload =
        mms_provider::MockProvider::simulate_inbound(&phone_number_id, &body.from, &body.text);
    let batch = mms_provider::meta_cloud::parse_webhook(&payload);
    let (stored, _) = crate::service::inbound::process_batch(&state, &batch).await?;

    Ok(Json(json!({ "stored": stored })))
}
