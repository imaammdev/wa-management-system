use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use mms_core::repo;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiResult;
use crate::extract::{SessionUser, SESSION_COOKIE};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct RegisterBody {
    name: String,
    email: String,
    password: String,
    /// Optional: creates the first organization in the same step.
    org_name: Option<String>,
}

fn session_cookie(token: String, ttl_days: i64) -> Cookie<'static> {
    let mut c = Cookie::new(SESSION_COOKIE, token);
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_path("/");
    c.set_max_age(time::Duration::days(ttl_days));
    // Set Secure in production; left off so http://localhost works in dev.
    c
}

async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<RegisterBody>,
) -> ApiResult<(CookieJar, Json<serde_json::Value>)> {
    let user =
        repo::auth::create_user(&state.pool, &body.email, &body.name, &body.password).await?;

    let org = match body.org_name.as_deref() {
        Some(name) if !name.trim().is_empty() => {
            Some(repo::orgs::create_with_owner(&state.pool, name, user.id).await?)
        }
        _ => None,
    };

    let token =
        repo::auth::create_session(&state.pool, user.id, state.cfg.session_ttl_days, None, None)
            .await?;

    Ok((
        jar.add(session_cookie(token, state.cfg.session_ttl_days)),
        Json(json!({ "user": user, "organization": org })),
    ))
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> ApiResult<(CookieJar, Json<serde_json::Value>)> {
    let user = repo::auth::verify_login(&state.pool, &body.email, &body.password).await?;
    let orgs = repo::orgs::list_for_user(&state.pool, user.id).await?;
    let token =
        repo::auth::create_session(&state.pool, user.id, state.cfg.session_ttl_days, None, None)
            .await?;

    Ok((
        jar.add(session_cookie(token, state.cfg.session_ttl_days)),
        Json(json!({
            "user": user,
            "organizations": orgs.iter().map(|(o, role)| json!({
                "id": o.id, "name": o.name, "slug": o.slug, "role": role
            })).collect::<Vec<_>>(),
        })),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<serde_json::Value>)> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        repo::auth::delete_session(&state.pool, c.value()).await?;
    }
    Ok((
        jar.remove(Cookie::from(SESSION_COOKIE)),
        Json(json!({ "ok": true })),
    ))
}

async fn me(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
) -> ApiResult<Json<serde_json::Value>> {
    let orgs = repo::orgs::list_for_user(&state.pool, user.id).await?;
    Ok(Json(json!({
        "user": user,
        "organizations": orgs.iter().map(|(o, role)| json!({
            "id": o.id, "name": o.name, "slug": o.slug, "role": role,
        })).collect::<Vec<_>>(),
    })))
}
