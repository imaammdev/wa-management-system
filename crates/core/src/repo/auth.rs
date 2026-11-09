use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::crypto;
use crate::models::User;
use crate::tenancy::{Actor, Role};
use crate::{Error, Result};

pub async fn create_user(pool: &PgPool, email: &str, name: &str, password: &str) -> Result<User> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(Error::validation("email tidak valid"));
    }
    if password.chars().count() < 8 {
        return Err(Error::validation("password minimal 8 karakter"));
    }
    let hash = crypto::hash_password(password)?;

    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (email, name, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id, email, name, password_hash, avatar_url,
                  email_verified_at, last_login_at, created_at
        "#,
    )
    .bind(&email)
    .bind(name.trim())
    .bind(&hash)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            Error::conflict("email sudah terdaftar")
        }
        _ => Error::Db(e),
    })?;
    Ok(user)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let u = sqlx::query_as::<_, User>(
        r#"SELECT id, email, name, password_hash, avatar_url,
                  email_verified_at, last_login_at, created_at
           FROM users WHERE email = $1"#,
    )
    .bind(email.trim().to_lowercase())
    .fetch_optional(pool)
    .await?;
    Ok(u)
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>> {
    let u = sqlx::query_as::<_, User>(
        r#"SELECT id, email, name, password_hash, avatar_url,
                  email_verified_at, last_login_at, created_at
           FROM users WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(u)
}

/// Verifies credentials and returns the user, without issuing a session.
pub async fn verify_login(pool: &PgPool, email: &str, password: &str) -> Result<User> {
    let user = find_by_email(pool, email)
        .await?
        .ok_or(Error::Unauthorized)?;
    if !crypto::verify_password(password, &user.password_hash) {
        return Err(Error::Unauthorized);
    }
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(pool)
        .await?;
    Ok(user)
}

/// Returns the opaque session token to set as a cookie. Only the hash is stored.
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    ttl_days: i64,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Result<String> {
    let token = format!("sess_{}", crypto::random_string(48));
    let expires = Utc::now() + Duration::days(ttl_days);
    sqlx::query(
        r#"INSERT INTO sessions (user_id, token_hash, ip, user_agent, expires_at)
           VALUES ($1, $2, $3::text::inet, $4, $5)"#,
    )
    .bind(user_id)
    .bind(crypto::sha256_hex(&token))
    .bind(ip)
    .bind(user_agent)
    .bind(expires)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn delete_session(pool: &PgPool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(crypto::sha256_hex(token))
        .execute(pool)
        .await?;
    Ok(())
}

/// Resolves a session cookie to the signed-in user plus their role in `org_id`.
/// When `org_id` is None, falls back to the user's first membership.
pub async fn actor_from_session(pool: &PgPool, token: &str, org_id: Option<Uuid>) -> Result<Actor> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        r#"
        SELECT s.user_id, m.org_id, m.role
        FROM sessions s
        JOIN memberships m ON m.user_id = s.user_id
        WHERE s.token_hash = $1
          AND s.expires_at > now()
          AND ($2::uuid IS NULL OR m.org_id = $2)
        ORDER BY (m.org_id = $2::uuid) DESC, m.created_at
        LIMIT 1
        "#,
    )
    .bind(crypto::sha256_hex(token))
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let (user_id, org_id, role) = row.ok_or(Error::Unauthorized)?;
    Ok(Actor::User {
        user_id,
        org_id,
        role: Role::parse(&role)?,
    })
}

/// A valid session that is not yet attached to any organization — used by the
/// onboarding screen, which must work before the first org exists.
pub async fn user_from_session(pool: &PgPool, token: &str) -> Result<User> {
    let u = sqlx::query_as::<_, User>(
        r#"
        SELECT u.id, u.email, u.name, u.password_hash, u.avatar_url,
               u.email_verified_at, u.last_login_at, u.created_at
        FROM sessions s JOIN users u ON u.id = s.user_id
        WHERE s.token_hash = $1 AND s.expires_at > now()
        "#,
    )
    .bind(crypto::sha256_hex(token))
    .fetch_optional(pool)
    .await?
    .ok_or(Error::Unauthorized)?;
    Ok(u)
}

pub async fn purge_expired_sessions(pool: &PgPool) -> Result<u64> {
    let r = sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}
