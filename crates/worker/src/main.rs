//! mms-worker — drains the `jobs` table.
//!
//! Handles webhook delivery, broadcast batches, template sync and retention
//! pruning. Safe to run several instances: jobs are claimed with
//! `FOR UPDATE SKIP LOCKED`.

mod ai_reply;
mod broadcast;
mod claude;
mod lead_event;
mod webhook;

use mms_core::jobs::{self, Job};
use mms_core::{db, repo, Config};
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

use crate::broadcast::Providers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = Config::from_env()?;
    let pool = db::connect(&cfg.database_url, 10).await?;

    let worker_id = format!(
        "{}-{}",
        hostname(),
        Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(6)
            .collect::<String>()
    );
    tracing::info!(%worker_id, concurrency = cfg.worker_concurrency, "starting mms-worker");

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        // Customer endpoints are arbitrary; don't follow redirects into
        // somewhere we didn't agree to post to.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let claude = cfg.anthropic_api_key.clone().map(|key| {
        Arc::new(crate::claude::Claude::new(
            key,
            cfg.anthropic_api_base.clone(),
        ))
    });
    if claude.is_none() {
        tracing::warn!("ANTHROPIC_API_KEY not set — AI agent replies are disabled");
    }

    let providers = Arc::new(Providers {
        meta: Arc::from(mms_provider::build_with(
            "meta_cloud",
            &cfg.meta_api_base,
            &cfg.meta_api_version,
            cfg.meta_conversions_path.clone(),
        )),
        mock: Arc::from(mms_provider::build("mock", "", "")),
    });

    // Housekeeping: release jobs from dead workers, drop expired sessions,
    // and prune history past each plan's retention window.
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                match jobs::requeue_stale(&pool, 300).await {
                    Ok(n) if n > 0 => tracing::warn!(n, "requeued stale jobs"),
                    Err(e) => tracing::error!(error = %e, "requeue_stale failed"),
                    _ => {}
                }
                if let Err(e) = repo::auth::purge_expired_sessions(&pool).await {
                    tracing::error!(error = %e, "session purge failed");
                }
            }
        });
    }

    let concurrency = cfg.worker_concurrency.max(1);
    let cfg = Arc::new(cfg);

    loop {
        let claimed = match jobs::claim(&pool, &worker_id, concurrency as i64).await {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, "claim failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        if claimed.is_empty() {
            // Nothing due. Poll rather than LISTEN/NOTIFY for now — simpler,
            // and a one-second floor on latency is fine for these jobs.
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        let mut handles = Vec::with_capacity(claimed.len());
        for job in claimed {
            let pool = pool.clone();
            let http = http.clone();
            let providers = providers.clone();
            let claude = claude.clone();
            let cfg = cfg.clone();
            handles.push(tokio::spawn(async move {
                let id = job.id;
                let kind = job.kind.clone();
                match handle(&pool, &http, &providers, &claude, &cfg, &job).await {
                    Ok(()) => {
                        if let Err(e) = jobs::complete(&pool, id).await {
                            tracing::error!(error = %e, job = id, "could not mark job done");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, job = id, %kind, "job failed");
                        if let Err(e2) = jobs::fail(&pool, &job, &e.to_string()).await {
                            tracing::error!(error = %e2, job = id, "could not record failure");
                        }
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.await;
        }
    }
}

async fn handle(
    pool: &mms_core::PgPool,
    http: &reqwest::Client,
    providers: &Providers,
    claude: &Option<Arc<crate::claude::Claude>>,
    cfg: &Config,
    job: &Job,
) -> mms_core::Result<()> {
    match job.kind.as_str() {
        jobs::KIND_WEBHOOK_DELIVER => {
            let id = uuid_field(job, "delivery_id")?;
            webhook::deliver(pool, http, id).await
        }
        jobs::KIND_BROADCAST_RUN => {
            let id = uuid_field(job, "broadcast_id")?;
            broadcast::run(pool, providers, &cfg.encryption_key, id).await
        }
        jobs::KIND_AI_REPLY => {
            let conversation_id = uuid_field(job, "conversation_id")?;
            let message_id = uuid_field(job, "message_id")?;
            let agent_id = uuid_field(job, "agent_id")?;
            ai_reply::run(
                ai_reply::Ctx {
                    pool,
                    providers,
                    claude: claude.as_deref(),
                    enc_key: &cfg.encryption_key,
                },
                conversation_id,
                message_id,
                agent_id,
            )
            .await
        }
        jobs::KIND_META_LEAD_EVENT => {
            let id = uuid_field(job, "event_id")?;
            lead_event::dispatch(pool, providers, &cfg.encryption_key, id).await
        }
        jobs::KIND_RETENTION_PRUNE => {
            let org_id = job
                .org_id
                .ok_or_else(|| mms_core::Error::validation("retention job needs an org_id"))?;
            let n = repo::billing::prune_history(pool, org_id).await?;
            tracing::info!(%org_id, pruned = n, "retention prune done");
            Ok(())
        }
        other => Err(mms_core::Error::validation(format!(
            "unknown job kind '{other}'"
        ))),
    }
}

fn uuid_field(job: &Job, key: &str) -> mms_core::Result<Uuid> {
    job.payload
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| mms_core::Error::validation(format!("job payload missing '{key}'")))
}

fn hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_string())
}
