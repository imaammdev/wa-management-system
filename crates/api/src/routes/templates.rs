//! Message templates. Authoring happens in Meta's Business Manager, so our job
//! is to mirror their approval status and let the dashboard read it.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mms_core::repo;
use mms_core::tenancy::Capability;
use mms_core::Error;
use mms_provider::ProviderCreds;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::extract::DashboardActor;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/sync/{wa_account_id}", post(sync))
}

async fn list(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = sqlx::query_as::<_, mms_core::models::MessageTemplate>(
        r#"SELECT id, org_id, wa_account_id, name, language, category, components,
                  status, rejected_reason, provider_ref, created_at
           FROM message_templates WHERE org_id=$1 ORDER BY name, language"#,
    )
    .bind(actor.org_id())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "data": rows })))
}

/// Pulls templates from the provider and upserts them locally.
async fn sync(
    State(state): State<AppState>,
    DashboardActor(actor): DashboardActor,
    Path(wa_account_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require(Capability::ContentWrite)?;
    let acc = repo::wa::get(&state.pool, actor.org_id(), wa_account_id).await?;

    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .unwrap_or_else(|| format!("mock-{}", acc.id.simple())),
        waba_id: acc.waba_id.clone(),
        access_token: repo::wa::access_token(&state.cfg.encryption_key, &acc)
            .unwrap_or_else(|_| String::new()),
    };

    let remote = state
        .provider_for(&acc.provider)
        .list_templates(&creds)
        .await?;

    let mut synced = 0usize;
    for t in &remote {
        sqlx::query(
            r#"INSERT INTO message_templates
                 (org_id, wa_account_id, name, language, category, components,
                  status, rejected_reason, provider_ref, synced_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9, now())
               ON CONFLICT (wa_account_id, name, language) DO UPDATE SET
                 category = EXCLUDED.category,
                 components = EXCLUDED.components,
                 status = EXCLUDED.status,
                 rejected_reason = EXCLUDED.rejected_reason,
                 provider_ref = EXCLUDED.provider_ref,
                 synced_at = now(),
                 updated_at = now()"#,
        )
        .bind(actor.org_id())
        .bind(acc.id)
        .bind(&t.name)
        .bind(&t.language)
        .bind(&t.category)
        .bind(&t.components)
        .bind(&t.status)
        .bind(&t.rejected_reason)
        .bind(&t.provider_ref)
        .execute(&state.pool)
        .await?;
        synced += 1;
    }

    if synced == 0 {
        return Err(Error::validation("tidak ada template di provider").into());
    }
    Ok(Json(json!({ "synced": synced })))
}
