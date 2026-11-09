//! Plan-limit enforcement and usage metering.
//!
//! Counters live in `usage_counters`, one row per (org, month, metric), and
//! are bumped in the same transaction as the write they account for, so a
//! failed send never bills.

use chrono::{Datelike, NaiveDate, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::models::Plan;
use crate::{Error, Result};

pub const METRIC_MESSAGES_SENT: &str = "messages_sent";
pub const METRIC_MESSAGES_RECEIVED: &str = "messages_received";
pub const METRIC_BROADCASTS_SENT: &str = "broadcasts_sent";
pub const METRIC_STORAGE_BYTES: &str = "storage_bytes";

/// First day of the current UTC month — the bucket key for counters.
pub fn current_period() -> NaiveDate {
    let now = Utc::now().date_naive();
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("day 1 always valid")
}

/// The plan backing an org's live subscription. Orgs without one get no quota.
pub async fn plan_for_org(pool: &PgPool, org_id: Uuid) -> Result<Plan> {
    let plan = sqlx::query_as::<_, Plan>(
        r#"
        SELECT p.id, p.code, p.name, p.tagline, p.price_idr_monthly, p.price_idr_yearly,
               p.is_public, p.is_popular, p.position,
               p.max_broadcasts_per_month, p.max_messages_per_month, p.max_storage_bytes,
               p.max_members, p.max_wa_accounts, p.history_retention_days, p.features
        FROM subscriptions s
        JOIN plans p ON p.id = s.plan_id
        WHERE s.org_id = $1 AND s.status IN ('trialing','active','past_due')
        LIMIT 1
        "#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::Forbidden("organization has no active subscription"))?;
    Ok(plan)
}

pub async fn usage(pool: &PgPool, org_id: Uuid, metric: &str) -> Result<i64> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT value FROM usage_counters WHERE org_id = $1 AND period_month = $2 AND metric = $3",
    )
    .bind(org_id)
    .bind(current_period())
    .bind(metric)
    .fetch_optional(pool)
    .await?;
    Ok(v.unwrap_or(0))
}

/// Bumps a counter inside the caller's transaction.
pub async fn increment(
    tx: &mut Transaction<'_, Postgres>,
    org_id: Uuid,
    metric: &str,
    by: i64,
) -> Result<i64> {
    let total: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO usage_counters (org_id, period_month, metric, value, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (org_id, period_month, metric)
        DO UPDATE SET value = usage_counters.value + EXCLUDED.value, updated_at = now()
        RETURNING value
        "#,
    )
    .bind(org_id)
    .bind(current_period())
    .bind(metric)
    .bind(by)
    .fetch_one(&mut **tx)
    .await?;
    Ok(total)
}

/// Rejects the call when the org is already at or over its monthly message cap.
pub async fn check_message_quota(pool: &PgPool, org_id: Uuid) -> Result<()> {
    let plan = plan_for_org(pool, org_id).await?;
    let used = usage(pool, org_id, METRIC_MESSAGES_SENT).await?;
    if used >= plan.max_messages_per_month {
        return Err(Error::QuotaExceeded {
            metric: "messages_per_month",
            used,
            limit: plan.max_messages_per_month,
        });
    }
    Ok(())
}

pub async fn check_broadcast_quota(pool: &PgPool, org_id: Uuid, adding: i64) -> Result<()> {
    let plan = plan_for_org(pool, org_id).await?;
    let used = usage(pool, org_id, METRIC_BROADCASTS_SENT).await?;
    if used + adding > plan.max_broadcasts_per_month {
        return Err(Error::QuotaExceeded {
            metric: "broadcasts_per_month",
            used,
            limit: plan.max_broadcasts_per_month,
        });
    }
    Ok(())
}

/// Called before connecting another number.
pub async fn check_wa_account_quota(pool: &PgPool, org_id: Uuid) -> Result<()> {
    let plan = plan_for_org(pool, org_id).await?;
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM whatsapp_accounts WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await?;
    // Add-ons raise the ceiling beyond the plan's bundled numbers.
    let extra: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(sa.quantity), 0)::bigint
        FROM subscription_addons sa
        JOIN addons a ON a.id = sa.addon_id
        JOIN subscriptions s ON s.id = sa.subscription_id
        WHERE s.org_id = $1
          AND s.status IN ('trialing','active','past_due')
          AND a.code = 'extra_wa_number'
        "#,
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;

    let limit = plan.max_wa_accounts as i64 + extra;
    if used >= limit {
        return Err(Error::QuotaExceeded {
            metric: "wa_accounts",
            used,
            limit,
        });
    }
    Ok(())
}

pub async fn check_member_quota(pool: &PgPool, org_id: Uuid) -> Result<()> {
    let plan = plan_for_org(pool, org_id).await?;
    let used: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM memberships WHERE org_id = $1)
              + (SELECT count(*) FROM invitations WHERE org_id = $1 AND accepted_at IS NULL)",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    if used >= plan.max_members as i64 {
        return Err(Error::QuotaExceeded {
            metric: "members",
            used,
            limit: plan.max_members as i64,
        });
    }
    Ok(())
}

/// What the dashboard's usage widget renders.
#[derive(Debug, serde::Serialize)]
pub struct UsageSummary {
    pub plan_code: String,
    pub period_month: NaiveDate,
    pub messages_sent: i64,
    pub messages_limit: i64,
    pub broadcasts_sent: i64,
    pub broadcasts_limit: i64,
    pub wa_accounts_used: i64,
    pub wa_accounts_limit: i64,
    pub members_used: i64,
    pub members_limit: i64,
    pub storage_bytes: i64,
    pub storage_limit: i64,
}

pub async fn summary(pool: &PgPool, org_id: Uuid) -> Result<UsageSummary> {
    let plan = plan_for_org(pool, org_id).await?;
    let wa_used: i64 = sqlx::query_scalar("SELECT count(*) FROM whatsapp_accounts WHERE org_id=$1")
        .bind(org_id)
        .fetch_one(pool)
        .await?;
    let members_used: i64 = sqlx::query_scalar("SELECT count(*) FROM memberships WHERE org_id=$1")
        .bind(org_id)
        .fetch_one(pool)
        .await?;

    Ok(UsageSummary {
        plan_code: plan.code.clone(),
        period_month: current_period(),
        messages_sent: usage(pool, org_id, METRIC_MESSAGES_SENT).await?,
        messages_limit: plan.max_messages_per_month,
        broadcasts_sent: usage(pool, org_id, METRIC_BROADCASTS_SENT).await?,
        broadcasts_limit: plan.max_broadcasts_per_month,
        wa_accounts_used: wa_used,
        wa_accounts_limit: plan.max_wa_accounts as i64,
        members_used,
        members_limit: plan.max_members as i64,
        storage_bytes: usage(pool, org_id, METRIC_STORAGE_BYTES).await?,
        storage_limit: plan.max_storage_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_is_first_of_month() {
        assert_eq!(current_period().day(), 1);
    }
}
