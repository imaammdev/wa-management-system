//! Reporting aggregates.
//!
//! Every query is bounded by an explicit date window and an org, so a large
//! tenant can't accidentally scan its whole message history.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::Result;

/// Clamps a requested window to something a dashboard can actually render.
pub fn window(days: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    let days = days.clamp(1, 365);
    let to = Utc::now();
    (to - Duration::days(days), to)
}

#[derive(Debug, Serialize, FromRow)]
pub struct DayPoint {
    pub day: NaiveDate,
    pub inbound: i64,
    pub outbound: i64,
}

/// Message volume per day. `generate_series` fills gaps so the chart has no
/// holes on quiet days.
pub async fn volume_by_day(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DayPoint>> {
    let rows = sqlx::query_as::<_, DayPoint>(
        r#"
        SELECT d::date AS day,
               COALESCE(m.inbound, 0)  AS inbound,
               COALESCE(m.outbound, 0) AS outbound
        FROM generate_series($2::date, $3::date, interval '1 day') AS d
        LEFT JOIN (
            SELECT created_at::date AS day,
                   count(*) FILTER (WHERE direction = 'inbound')  AS inbound,
                   count(*) FILTER (WHERE direction = 'outbound') AS outbound
            FROM messages
            WHERE org_id = $1 AND created_at >= $2 AND created_at < $3
            GROUP BY 1
        ) m ON m.day = d::date
        ORDER BY day
        "#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Default, Serialize, FromRow)]
pub struct DeliveryStats {
    pub sent: i64,
    pub delivered: i64,
    pub read: i64,
    pub failed: i64,
    pub queued: i64,
}

impl DeliveryStats {
    /// Share of attempted sends that reached the handset.
    pub fn delivery_rate(&self) -> f64 {
        let attempted = self.sent + self.delivered + self.read + self.failed;
        if attempted == 0 {
            return 0.0;
        }
        (self.delivered + self.read) as f64 / attempted as f64
    }

    pub fn read_rate(&self) -> f64 {
        let reached = self.delivered + self.read;
        if reached == 0 {
            return 0.0;
        }
        self.read as f64 / reached as f64
    }
}

/// Outbound delivery funnel. Statuses are terminal-per-message, so these
/// buckets don't overlap.
pub async fn delivery_stats(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<DeliveryStats> {
    let s = sqlx::query_as::<_, DeliveryStats>(
        r#"
        SELECT count(*) FILTER (WHERE status = 'sent')      AS sent,
               count(*) FILTER (WHERE status = 'delivered') AS delivered,
               count(*) FILTER (WHERE status = 'read')      AS read,
               count(*) FILTER (WHERE status = 'failed')    AS failed,
               count(*) FILTER (WHERE status = 'queued')    AS queued
        FROM messages
        WHERE org_id = $1 AND direction = 'outbound'
          AND created_at >= $2 AND created_at < $3
        "#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(s)
}

#[derive(Debug, Serialize, FromRow)]
pub struct CategoryCount {
    pub category: Option<String>,
    pub total: i64,
}

/// Billable split, which is what drives the Meta invoice.
pub async fn by_category(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CategoryCount>> {
    let rows = sqlx::query_as::<_, CategoryCount>(
        r#"SELECT pricing_category AS category, count(*) AS total
           FROM messages
           WHERE org_id=$1 AND direction='outbound'
             AND created_at >= $2 AND created_at < $3
           GROUP BY 1 ORDER BY total DESC"#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Default, Serialize, FromRow)]
pub struct ConversationStats {
    pub opened: i64,
    pub resolved: i64,
    pub still_open: i64,
    /// Seconds from the customer's first message to the first agent reply.
    pub avg_first_response_secs: Option<f64>,
    pub avg_resolution_secs: Option<f64>,
}

pub async fn conversation_stats(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<ConversationStats> {
    let s = sqlx::query_as::<_, ConversationStats>(
        r#"
        SELECT
          count(*) FILTER (WHERE created_at >= $2 AND created_at < $3)      AS opened,
          count(*) FILTER (WHERE resolved_at >= $2 AND resolved_at < $3)    AS resolved,
          count(*) FILTER (WHERE status <> 'closed')                        AS still_open,
          avg(EXTRACT(EPOCH FROM (first_response_at - created_at)))
            FILTER (WHERE first_response_at IS NOT NULL
                      AND first_response_at >= $2 AND first_response_at < $3)
                                                                            AS avg_first_response_secs,
          avg(EXTRACT(EPOCH FROM (resolved_at - created_at)))
            FILTER (WHERE resolved_at IS NOT NULL
                      AND resolved_at >= $2 AND resolved_at < $3)           AS avg_resolution_secs
        FROM conversations
        WHERE org_id = $1
        "#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(s)
}

#[derive(Debug, Serialize, FromRow)]
pub struct AgentRow {
    pub user_id: Uuid,
    pub name: String,
    pub replies: i64,
    pub resolved: i64,
    pub avg_first_response_secs: Option<f64>,
}

/// Per-agent workload. Counts replies the agent actually sent, not messages
/// that merely happened in a thread assigned to them.
pub async fn agent_performance(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<AgentRow>> {
    let rows = sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT u.id AS user_id,
               u.name,
               COALESCE(r.replies, 0)  AS replies,
               COALESCE(c.resolved, 0) AS resolved,
               c.avg_first_response_secs
        FROM memberships mem
        JOIN users u ON u.id = mem.user_id
        LEFT JOIN (
            SELECT sent_by_user_id AS uid, count(*) AS replies
            FROM messages
            WHERE org_id = $1 AND direction = 'outbound'
              AND sent_by_user_id IS NOT NULL
              AND created_at >= $2 AND created_at < $3
            GROUP BY 1
        ) r ON r.uid = u.id
        LEFT JOIN (
            SELECT assigned_to AS uid,
                   count(*) FILTER (WHERE resolved_at IS NOT NULL) AS resolved,
                   avg(EXTRACT(EPOCH FROM (first_response_at - created_at)))
                     FILTER (WHERE first_response_at IS NOT NULL) AS avg_first_response_secs
            FROM conversations
            WHERE org_id = $1 AND assigned_to IS NOT NULL
              AND COALESCE(resolved_at, first_response_at) >= $2
              AND COALESCE(resolved_at, first_response_at) < $3
            GROUP BY 1
        ) c ON c.uid = u.id
        WHERE mem.org_id = $1
        ORDER BY replies DESC, u.name
        "#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Serialize, FromRow)]
pub struct LabelCount {
    pub name: String,
    pub color: String,
    pub total: i64,
}

pub async fn top_labels(pool: &PgPool, org_id: Uuid) -> Result<Vec<LabelCount>> {
    let rows = sqlx::query_as::<_, LabelCount>(
        r#"SELECT l.name, l.color, count(cl.conversation_id) AS total
           FROM labels l
           LEFT JOIN conversation_labels cl ON cl.label_id = l.id
           WHERE l.org_id = $1
           GROUP BY l.id, l.name, l.color
           ORDER BY total DESC, l.name
           LIMIT 10"#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Default, Serialize, FromRow)]
pub struct BroadcastStats {
    pub campaigns: i64,
    pub recipients: i64,
    pub sent: i64,
    pub failed: i64,
}

pub async fn broadcast_stats(
    pool: &PgPool,
    org_id: Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<BroadcastStats> {
    let s = sqlx::query_as::<_, BroadcastStats>(
        r#"SELECT count(*)                          AS campaigns,
                  COALESCE(sum(total_recipients),0) AS recipients,
                  COALESCE(sum(sent_count),0)       AS sent,
                  COALESCE(sum(failed_count),0)     AS failed
           FROM broadcasts
           WHERE org_id=$1 AND created_at >= $2 AND created_at < $3"#,
    )
    .bind(org_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_is_clamped() {
        let (a, b) = window(99999);
        assert!((b - a).num_days() <= 365);
        let (c, d) = window(0);
        assert!((d - c).num_days() >= 1);
    }

    #[test]
    fn delivery_rate_counts_read_as_delivered() {
        let s = DeliveryStats {
            sent: 10,
            delivered: 60,
            read: 30,
            failed: 0,
            queued: 5,
        };
        // read messages reached the handset too
        assert!((s.delivery_rate() - 0.9).abs() < 1e-9);
        assert!((s.read_rate() - (30.0 / 90.0)).abs() < 1e-9);
    }

    #[test]
    fn rates_are_zero_when_nothing_sent() {
        let s = DeliveryStats::default();
        assert_eq!(s.delivery_rate(), 0.0);
        assert_eq!(s.read_rate(), 0.0);
    }
}
