//! Request extractors that resolve the caller into a `mms_core::Actor`.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use mms_core::models::User;
use mms_core::repo;
use mms_core::tenancy::Actor;
use mms_core::Error;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "mms_session";

fn bearer(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_string)
}

fn session_token(parts: &Parts) -> Option<String> {
    let jar = CookieJar::from_headers(&parts.headers);
    jar.get(SESSION_COOKIE).map(|c| c.value().to_string())
}

/// The org the dashboard is currently acting in, from `X-Org-Id`.
fn requested_org(parts: &Parts) -> Option<Uuid> {
    parts.headers.get("x-org-id")?.to_str().ok()?.parse().ok()
}

/// Dashboard caller: a session cookie, scoped to one organization.
pub struct DashboardActor(pub Actor);

impl FromRequestParts<AppState> for DashboardActor {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = session_token(parts).ok_or(ApiError(Error::Unauthorized))?;
        let actor =
            repo::auth::actor_from_session(&state.pool, &token, requested_org(parts)).await?;
        Ok(DashboardActor(actor))
    }
}

/// A signed-in user who may not belong to any organization yet.
pub struct SessionUser(pub User);

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = session_token(parts).ok_or(ApiError(Error::Unauthorized))?;
        let user = repo::auth::user_from_session(&state.pool, &token).await?;
        Ok(SessionUser(user))
    }
}

/// Public `/v1` caller: `Authorization: Bearer mms_live_…`, rate limited.
pub struct ApiActor(pub Actor);

impl FromRequestParts<AppState> for ApiActor {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let key = bearer(parts).ok_or(ApiError(Error::Unauthorized))?;
        let (actor, rate_limit) = repo::keys::authenticate(&state.pool, &key).await?;

        if let Some(id) = actor.api_key_id() {
            if let Err(retry) = state.limiter.check(&id.to_string(), rate_limit) {
                return Err(ApiError(Error::RateLimited {
                    retry_after_secs: retry,
                }));
            }
        }
        Ok(ApiActor(actor))
    }
}

/// Accepts either credential. Lets the dashboard reuse the public endpoints,
/// which keeps the published API honest — it is the same code path.
pub struct AnyActor(pub Actor);

impl FromRequestParts<AppState> for AnyActor {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if bearer(parts).is_some() {
            return ApiActor::from_request_parts(parts, state)
                .await
                .map(|a| AnyActor(a.0));
        }
        DashboardActor::from_request_parts(parts, state)
            .await
            .map(|a| AnyActor(a.0))
    }
}

/// Idempotency-Key header, honoured on message sends.
pub fn idempotency_key(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}
