//! Postgres-backed job queue (`SELECT ... FOR UPDATE SKIP LOCKED`).
//!
//! Deliberately not Redis: the queue shares the database transaction with the
//! rows it acts on, so enqueueing a webhook delivery and writing the message
//! that triggered it either both happen or neither does.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::Result;

pub const KIND_WEBHOOK_DELIVER: &str = "webhook.deliver";
pub const KIND_BROADCAST_RUN: &str = "broadcast.run";
pub const KIND_TEMPLATE_SYNC: &str = "template.sync";
pub const KIND_AI_REPLY: &str = "ai.reply";
pub const KIND_KICHAT_NOTIFY: &str = "kichat.notify";
pub const KIND_RETENTION_PRUNE: &str = "retention.prune";
pub const KIND_META_LEAD_EVENT: &str = "meta.lead_event";

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub org_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub attempt: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
}

pub struct Enqueue<'a> {
    pub kind: &'a str,
    pub org_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub run_at: Option<DateTime<Utc>>,
    pub max_attempts: i32,
    pub priority: i32,
    /// When set, a second enqueue while the first is still pending is a no-op.
    pub dedup_key: Option<String>,
}

impl<'a> Enqueue<'a> {
    pub fn new(kind: &'a str, payload: serde_json::Value) -> Self {
        Self {
            kind,
            org_id: None,
            payload,
            run_at: None,
            max_attempts: 5,
            priority: 0,
            dedup_key: None,
        }
    }
    pub fn org(mut self, org_id: Uuid) -> Self {
        self.org_id = Some(org_id);
        self
    }
    pub fn at(mut self, when: DateTime<Utc>) -> Self {
        self.run_at = Some(when);
        self
    }
    pub fn dedup(mut self, key: impl Into<String>) -> Self {
        self.dedup_key = Some(key.into());
        self
    }
    pub fn max_attempts(mut self, n: i32) -> Self {
        self.max_attempts = n;
        self
    }
}

/// Enqueue inside an existing transaction (the common case).
pub async fn enqueue_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: Enqueue<'_>,
) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (kind, org_id, payload, run_at, max_attempts, priority, dedup_key)
        VALUES ($1, $2, $3, COALESCE($4, now()), $5, $6, $7)
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(job.kind)
    .bind(job.org_id)
    .bind(&job.payload)
    .bind(job.run_at)
    .bind(job.max_attempts)
    .bind(job.priority)
    .bind(job.dedup_key.as_deref())
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id)
}

pub async fn enqueue(pool: &PgPool, job: Enqueue<'_>) -> Result<Option<i64>> {
    let mut tx = pool.begin().await?;
    let id = enqueue_tx(&mut tx, job).await?;
    tx.commit().await?;
    Ok(id)
}

/// Atomically claims up to `limit` due jobs for this worker.
pub async fn claim(pool: &PgPool, worker_id: &str, limit: i64) -> Result<Vec<Job>> {
    let jobs = sqlx::query_as::<_, Job>(
        r#"
        WITH due AS (
            SELECT id FROM jobs
            WHERE status = 'pending' AND run_at <= now()
            ORDER BY priority DESC, run_at
            FOR UPDATE SKIP LOCKED
            LIMIT $2
        )
        UPDATE jobs j
        SET status = 'running',
            locked_at = now(),
            locked_by = $1,
            attempt = j.attempt + 1
        FROM due
        WHERE j.id = due.id
        RETURNING j.id, j.kind, j.org_id, j.payload, j.attempt, j.max_attempts, j.run_at
        "#,
    )
    .bind(worker_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

pub async fn complete(pool: &PgPool, job_id: i64) -> Result<()> {
    sqlx::query("UPDATE jobs SET status='done', finished_at=now(), dedup_key=NULL WHERE id=$1")
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Exponential backoff: 2^attempt seconds, capped at 1 hour.
pub fn backoff(attempt: i32) -> Duration {
    let secs = 2i64.saturating_pow(attempt.clamp(0, 12) as u32);
    Duration::seconds(secs.min(3600))
}

/// Reschedules the job, or marks it failed once attempts run out.
pub async fn fail(pool: &PgPool, job: &Job, err: &str) -> Result<()> {
    if job.attempt >= job.max_attempts {
        sqlx::query(
            "UPDATE jobs SET status='failed', last_error=$2, finished_at=now(), dedup_key=NULL WHERE id=$1",
        )
        .bind(job.id)
        .bind(err)
        .execute(pool)
        .await?;
    } else {
        let next = Utc::now() + backoff(job.attempt);
        sqlx::query(
            "UPDATE jobs SET status='pending', last_error=$2, run_at=$3, locked_at=NULL, locked_by=NULL WHERE id=$1",
        )
        .bind(job.id)
        .bind(err)
        .bind(next)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Releases jobs whose worker died mid-run.
pub async fn requeue_stale(pool: &PgPool, older_than_secs: i64) -> Result<u64> {
    let res = sqlx::query(
        r#"
        UPDATE jobs SET status='pending', locked_at=NULL, locked_by=NULL
        WHERE status='running' AND locked_at < now() - make_interval(secs => $1)
        "#,
    )
    .bind(older_than_secs as f64)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff(1).num_seconds(), 2);
        assert_eq!(backoff(5).num_seconds(), 32);
        assert_eq!(backoff(30).num_seconds(), 3600);
    }
}
