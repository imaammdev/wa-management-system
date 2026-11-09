//! Click-to-WhatsApp attribution.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::Result;

/// Stores the referral Meta attached to an ad-originated message, and stamps
/// the contact's acquisition source. Runs inside the inbound transaction.
pub async fn record_referral_tx(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    contact_id: Uuid,
    conversation_id: Option<Uuid>,
    wa_account_id: Option<Uuid>,
    referral: &Value,
) -> Result<()> {
    let s = |k: &str| referral.get(k).and_then(|v| v.as_str()).map(str::to_string);

    sqlx::query(
        r#"INSERT INTO ad_referrals
             (org_id, contact_id, conversation_id, wa_account_id, source_type,
              source_id, source_url, headline, body, media_type, ctwa_clid, raw)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
    )
    .bind(org_id)
    .bind(contact_id)
    .bind(conversation_id)
    .bind(wa_account_id)
    .bind(s("source_type"))
    .bind(s("source_id"))
    .bind(s("source_url"))
    .bind(s("headline"))
    .bind(s("body"))
    .bind(s("media_type"))
    .bind(s("ctwa_clid"))
    .bind(referral)
    .execute(&mut **tx)
    .await?;

    // First touch wins: a contact's acquisition source shouldn't be rewritten
    // by a later ad click.
    sqlx::query(
        r#"UPDATE contacts
           SET source = COALESCE(source, $2),
               referral = CASE WHEN referral = '{}'::jsonb THEN $3 ELSE referral END,
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(contact_id)
    .bind(s("source_type").unwrap_or_else(|| "ad".into()))
    .bind(referral)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdRow {
    pub source_id: Option<String>,
    pub headline: Option<String>,
    pub source_url: Option<String>,
    pub source_type: Option<String>,
    pub leads: i64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// Leads grouped by the ad that produced them.
pub async fn by_ad(pool: &PgPool, org_id: Uuid, limit: i64) -> Result<Vec<AdRow>> {
    Ok(sqlx::query_as::<_, AdRow>(
        r#"SELECT source_id,
                  max(headline)    AS headline,
                  max(source_url)  AS source_url,
                  max(source_type) AS source_type,
                  count(DISTINCT contact_id) AS leads,
                  min(created_at)  AS first_seen,
                  max(created_at)  AS last_seen
           FROM ad_referrals
           WHERE org_id = $1
           GROUP BY source_id
           ORDER BY leads DESC, last_seen DESC
           LIMIT $2"#,
    )
    .bind(org_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?)
}

#[derive(Debug, Default, Serialize, FromRow)]
pub struct AdSummary {
    pub referrals: i64,
    pub contacts: i64,
    pub ads: i64,
    /// Ad-sourced contacts that went on to a won deal.
    pub converted: i64,
}

pub async fn summary(pool: &PgPool, org_id: Uuid) -> Result<AdSummary> {
    Ok(sqlx::query_as::<_, AdSummary>(
        r#"SELECT
             (SELECT count(*) FROM ad_referrals WHERE org_id=$1)                      AS referrals,
             (SELECT count(DISTINCT contact_id) FROM ad_referrals WHERE org_id=$1)    AS contacts,
             (SELECT count(DISTINCT source_id) FROM ad_referrals WHERE org_id=$1)     AS ads,
             (SELECT count(DISTINCT d.contact_id) FROM deals d
                WHERE d.org_id=$1 AND d.status='won'
                  AND EXISTS (SELECT 1 FROM ad_referrals r
                              WHERE r.contact_id = d.contact_id))                     AS converted"#,
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?)
}
