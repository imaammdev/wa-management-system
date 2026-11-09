use sqlx::PgPool;
use uuid::Uuid;

use crate::crypto;
use crate::models::WhatsappAccount;
use crate::{Error, Result};

const COLS: &str = r#"id, org_id, customer_id, provider, label, waba_id, phone_number_id,
    display_phone_number, verified_name, access_token_enc, status, quality_rating,
    messaging_limit_tier, mode, provider_meta, last_error, connected_at, created_at"#;

pub async fn create(
    pool: &PgPool,
    org_id: Uuid,
    customer_id: Option<Uuid>,
    provider: &str,
    label: &str,
) -> Result<WhatsappAccount> {
    let verify_token = crypto::random_string(24);
    let sql = format!(
        "INSERT INTO whatsapp_accounts (org_id, customer_id, provider, label, webhook_verify_token)
         VALUES ($1,$2,$3,$4,$5) RETURNING {COLS}"
    );
    let acc = sqlx::query_as::<_, WhatsappAccount>(&sql)
        .bind(org_id)
        .bind(customer_id)
        .bind(provider)
        .bind(label.trim())
        .bind(verify_token)
        .fetch_one(pool)
        .await?;
    Ok(acc)
}

/// Everything needed to attach real Meta credentials to a pending number.
pub struct ConnectParams<'a> {
    pub org_id: Uuid,
    pub id: Uuid,
    pub waba_id: &'a str,
    pub phone_number_id: &'a str,
    pub display_phone_number: Option<&'a str>,
    pub verified_name: Option<&'a str>,
    /// Plaintext; sealed before it touches the database.
    pub access_token: &'a str,
}

/// Stores the Meta identifiers and seals the system-user token at rest.
pub async fn connect(
    pool: &PgPool,
    enc_key: &[u8; 32],
    p: ConnectParams<'_>,
) -> Result<WhatsappAccount> {
    let sealed = crypto::seal(enc_key, p.access_token)?;
    let sql = format!(
        r#"UPDATE whatsapp_accounts SET
             waba_id=$3, phone_number_id=$4, display_phone_number=$5,
             verified_name=$6, access_token_enc=$7, status='connected',
             connected_at=now(), last_error=NULL, updated_at=now()
           WHERE id=$1 AND org_id=$2 RETURNING {COLS}"#
    );
    let acc = sqlx::query_as::<_, WhatsappAccount>(&sql)
        .bind(p.id)
        .bind(p.org_id)
        .bind(p.waba_id)
        .bind(p.phone_number_id)
        .bind(p.display_phone_number)
        .bind(p.verified_name)
        .bind(&sealed)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("whatsapp account"))?;
    Ok(acc)
}

pub async fn list(
    pool: &PgPool,
    org_id: Uuid,
    customer_id: Option<Uuid>,
) -> Result<Vec<WhatsappAccount>> {
    let sql = format!(
        "SELECT {COLS} FROM whatsapp_accounts
         WHERE org_id=$1 AND ($2::uuid IS NULL OR customer_id=$2)
         ORDER BY created_at"
    );
    let rows = sqlx::query_as::<_, WhatsappAccount>(&sql)
        .bind(org_id)
        .bind(customer_id)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<WhatsappAccount> {
    let sql = format!("SELECT {COLS} FROM whatsapp_accounts WHERE id=$1 AND org_id=$2");
    sqlx::query_as::<_, WhatsappAccount>(&sql)
        .bind(id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("whatsapp account"))
}

/// Inbound webhooks arrive keyed by Meta's phone_number_id, with no org context.
pub async fn find_by_phone_number_id(
    pool: &PgPool,
    phone_number_id: &str,
) -> Result<Option<WhatsappAccount>> {
    let sql = format!("SELECT {COLS} FROM whatsapp_accounts WHERE phone_number_id=$1");
    let acc = sqlx::query_as::<_, WhatsappAccount>(&sql)
        .bind(phone_number_id)
        .fetch_optional(pool)
        .await?;
    Ok(acc)
}

/// Unseals the provider token for an outbound call.
pub fn access_token(enc_key: &[u8; 32], acc: &WhatsappAccount) -> Result<String> {
    match &acc.access_token_enc {
        Some(sealed) => crypto::open(enc_key, sealed),
        None => Err(Error::validation(
            "nomor WhatsApp belum terhubung (token kosong)",
        )),
    }
}

pub async fn mark_error(pool: &PgPool, id: Uuid, err: &str) -> Result<()> {
    sqlx::query(
        "UPDATE whatsapp_accounts SET status='error', last_error=$2, updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .bind(err)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<()> {
    let r = sqlx::query("DELETE FROM whatsapp_accounts WHERE id=$1 AND org_id=$2")
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("whatsapp account"));
    }
    Ok(())
}
