use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{MetaRate, Plan, Subscription};
use crate::{Error, Result};

const PLAN_COLS: &str = r#"id, code, name, tagline, price_idr_monthly, price_idr_yearly,
    is_public, is_popular, position, max_broadcasts_per_month, max_messages_per_month,
    max_storage_bytes, max_members, max_wa_accounts, history_retention_days, features"#;

pub async fn list_public_plans(pool: &PgPool) -> Result<Vec<Plan>> {
    let sql = format!("SELECT {PLAN_COLS} FROM plans WHERE is_public ORDER BY position");
    Ok(sqlx::query_as::<_, Plan>(&sql).fetch_all(pool).await?)
}

pub async fn plan_by_code(pool: &PgPool, code: &str) -> Result<Plan> {
    let sql = format!("SELECT {PLAN_COLS} FROM plans WHERE code=$1");
    sqlx::query_as::<_, Plan>(&sql)
        .bind(code)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::NotFound("plan"))
}

pub async fn subscription_for_org(pool: &PgPool, org_id: Uuid) -> Result<Option<Subscription>> {
    let s = sqlx::query_as::<_, Subscription>(
        r#"SELECT id, org_id, plan_id, status, interval, current_period_start,
                  current_period_end, trial_ends_at, cancel_at_period_end,
                  payment_provider, created_at
           FROM subscriptions
           WHERE org_id=$1 AND status IN ('trialing','active','past_due')
           LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;
    Ok(s)
}

/// Switches plan on the live subscription, keeping the current period.
pub async fn change_plan(
    pool: &PgPool,
    org_id: Uuid,
    plan_code: &str,
    interval: &str,
) -> Result<Subscription> {
    if interval != "monthly" && interval != "yearly" {
        return Err(Error::validation("interval harus monthly atau yearly"));
    }
    let plan = plan_by_code(pool, plan_code).await?;
    let s = sqlx::query_as::<_, Subscription>(
        r#"UPDATE subscriptions
           SET plan_id=$2, interval=$3, updated_at=now()
           WHERE org_id=$1 AND status IN ('trialing','active','past_due')
           RETURNING id, org_id, plan_id, status, interval, current_period_start,
                     current_period_end, trial_ends_at, cancel_at_period_end,
                     payment_provider, created_at"#,
    )
    .bind(org_id)
    .bind(plan.id)
    .bind(interval)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound("subscription"))?;
    Ok(s)
}

pub async fn meta_rates(pool: &PgPool, country: &str) -> Result<Vec<MetaRate>> {
    // Latest effective row per category, so the calculator reflects the
    // schedule in force today.
    let rows = sqlx::query_as::<_, MetaRate>(
        r#"SELECT DISTINCT ON (category)
                  country, category, rate_idr::float8 AS rate_idr,
                  free_tier_per_number_per_month, effective_from
           FROM meta_rates
           WHERE country = $1 AND effective_from <= CURRENT_DATE
           ORDER BY category, effective_from DESC"#,
    )
    .bind(country)
    .fetch_all(pool)
    .await?;

    // Before the first effective date there is nothing to show; fall back to
    // the earliest published schedule rather than an empty calculator.
    if rows.is_empty() {
        let fallback = sqlx::query_as::<_, MetaRate>(
            r#"SELECT DISTINCT ON (category)
                      country, category, rate_idr::float8 AS rate_idr,
                      free_tier_per_number_per_month, effective_from
               FROM meta_rates WHERE country = $1
               ORDER BY category, effective_from"#,
        )
        .bind(country)
        .fetch_all(pool)
        .await?;
        return Ok(fallback);
    }
    Ok(rows)
}

/// Deletes messages past the plan's retention window. Called by the worker.
pub async fn prune_history(pool: &PgPool, org_id: Uuid) -> Result<u64> {
    let days: Option<i32> = sqlx::query_scalar(
        r#"SELECT p.history_retention_days FROM subscriptions s
           JOIN plans p ON p.id = s.plan_id
           WHERE s.org_id=$1 AND s.status IN ('trialing','active','past_due') LIMIT 1"#,
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let Some(days) = days else { return Ok(0) };
    let r = sqlx::query(
        "DELETE FROM messages WHERE org_id=$1 AND created_at < now() - make_interval(days => $2)",
    )
    .bind(org_id)
    .bind(days)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
