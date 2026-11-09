//! Dispatches queued Click-to-WhatsApp lead milestones to Meta.

use mms_core::repo::{crm, wa};
use mms_core::{Error, PgPool, Result};
use mms_provider::ProviderCreds;
use uuid::Uuid;

use crate::broadcast::Providers;

pub async fn dispatch(
    pool: &PgPool,
    providers: &Providers,
    enc_key: &[u8; 32],
    event_id: Uuid,
) -> Result<()> {
    let row: Option<(Uuid, Uuid, Option<Uuid>, String, String)> = sqlx::query_as(
        r#"SELECT id, org_id, wa_account_id, event_name, status
           FROM meta_lead_events WHERE id = $1"#,
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    let Some((id, org_id, wa_account_id, event_name, status)) = row else {
        tracing::warn!(%event_id, "lead event vanished, dropping job");
        return Ok(());
    };
    if status == "sent" {
        return Ok(());
    }

    // Without a number we have no credentials and no WABA to report against.
    let Some(wa_account_id) = wa_account_id else {
        crm::mark_meta_event(
            pool,
            id,
            false,
            None,
            Some("nomor WhatsApp tidak diketahui"),
        )
        .await?;
        return Ok(());
    };

    let acc = wa::get(pool, org_id, wa_account_id).await?;
    let creds = ProviderCreds {
        wa_account_id: acc.id,
        phone_number_id: acc
            .phone_number_id
            .clone()
            .unwrap_or_else(|| format!("mock-{}", acc.id.simple())),
        waba_id: acc.waba_id.clone(),
        access_token: wa::access_token(enc_key, &acc).unwrap_or_default(),
    };

    let payload = serde_json::json!({ "wa_id": acc.phone_number_id, "org_id": org_id });

    match providers
        .for_name_pub(&acc.provider)
        .report_lead_event(&creds, &event_name, &payload)
        .await
    {
        Ok(reference) => {
            crm::mark_meta_event(pool, id, true, Some(&reference), None).await?;
            tracing::info!(%id, event_name, "lead event reported");
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            crm::mark_meta_event(pool, id, false, None, Some(&msg)).await?;
            // A missing configuration will not fix itself on retry.
            if matches!(&e, Error::Provider { code, .. } if code == "not_configured") {
                tracing::warn!(%id, "lead reporting is not configured; not retrying");
                return Ok(());
            }
            Err(e)
        }
    }
}
