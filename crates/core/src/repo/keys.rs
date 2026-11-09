use sqlx::PgPool;
use uuid::Uuid;

use crate::crypto;
use crate::models::ApiKey;
use crate::tenancy::Actor;
use crate::{Error, Result};

/// Returns the row plus the one-time plaintext key.
pub async fn create(
    pool: &PgPool,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    name: &str,
    scopes: &[String],
    created_by: Option<Uuid>,
    live: bool,
) -> Result<(ApiKey, String)> {
    let minted = crypto::mint_api_key(live);
    let key = sqlx::query_as::<_, ApiKey>(
        r#"INSERT INTO api_keys (org_id, customer_id, name, prefix, key_hash, scopes, created_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7)
           RETURNING id, org_id, customer_id, name, prefix, key_hash, scopes,
                     rate_limit_per_min, last_used_at, revoked_at, created_at"#,
    )
    .bind(org_id)
    .bind(customer_id)
    .bind(name.trim())
    .bind(&minted.prefix)
    .bind(&minted.hash)
    .bind(scopes)
    .bind(created_by)
    .fetch_one(pool)
    .await?;
    Ok((key, minted.plaintext))
}

pub async fn list(pool: &PgPool, org_id: Uuid) -> Result<Vec<ApiKey>> {
    let rows = sqlx::query_as::<_, ApiKey>(
        r#"SELECT id, org_id, customer_id, name, prefix, key_hash, scopes,
                  rate_limit_per_min, last_used_at, revoked_at, created_at
           FROM api_keys WHERE org_id=$1 ORDER BY created_at DESC"#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn revoke(pool: &PgPool, org_id: Uuid, key_id: Uuid) -> Result<()> {
    let r = sqlx::query(
        "UPDATE api_keys SET revoked_at = now() WHERE id=$1 AND org_id=$2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(org_id)
    .execute(pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("api key"));
    }
    Ok(())
}

/// The columns needed to authenticate a key, without exposing the hash
/// anywhere it could be serialized.
#[derive(sqlx::FromRow)]
struct AuthRow {
    id: Uuid,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    key_hash: String,
    scopes: Vec<String>,
    rate_limit_per_min: i32,
}

/// Authenticates `Authorization: Bearer mms_live_...`.
///
/// Looks the row up by prefix, then compares the SHA-256 of the whole key in
/// constant time, so a wrong secret with a valid prefix still fails closed.
pub async fn authenticate(pool: &PgPool, plaintext: &str) -> Result<(Actor, i32)> {
    let prefix = crypto::api_key_prefix(plaintext).ok_or(Error::Unauthorized)?;

    let row = sqlx::query_as::<_, AuthRow>(
        r#"SELECT id, org_id, customer_id, key_hash, scopes, rate_limit_per_min
           FROM api_keys WHERE prefix = $1 AND revoked_at IS NULL"#,
    )
    .bind(&prefix)
    .fetch_optional(pool)
    .await?;

    let AuthRow {
        id,
        org_id,
        customer_id,
        key_hash,
        scopes,
        rate_limit_per_min: rate_limit,
    } = row.ok_or(Error::Unauthorized)?;

    if !crypto::constant_time_eq(&crypto::sha256_hex(plaintext), &key_hash) {
        return Err(Error::Unauthorized);
    }

    // Best-effort; a failed touch must not fail the request.
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await;

    Ok((
        Actor::ApiKey {
            api_key_id: id,
            org_id,
            customer_id,
            scopes,
        },
        rate_limit,
    ))
}
