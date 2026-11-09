//! A local stand-in for Meta's Cloud API.
//!
//! Lets the whole platform — send, delivery receipts, templates, inbox — be
//! exercised end to end with no Meta app, no WABA and no billing. Sent
//! messages are kept in memory so tests and the dev dashboard can assert on
//! them, and `simulate_inbound` produces a webhook body shaped exactly like
//! Meta's, so the same parser is used in dev and in production.

use async_trait::async_trait;
use chrono::Utc;
use mms_core::Result;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::types::*;
use crate::WaProvider;

#[derive(Debug, Clone)]
pub struct SentRecord {
    pub wa_message_id: String,
    pub to: String,
    pub kind: String,
    pub preview: Option<String>,
    pub at: chrono::DateTime<Utc>,
}

#[derive(Default)]
pub struct MockProvider {
    sent: Arc<Mutex<Vec<SentRecord>>>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn sent(&self) -> Vec<SentRecord> {
        self.sent.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Builds a Meta-shaped inbound webhook body, so dev traffic exercises the
    /// real parsing and persistence path.
    pub fn simulate_inbound(phone_number_id: &str, from: &str, text: &str) -> Value {
        json!({
          "object": "whatsapp_business_account",
          "entry": [{
            "id": "MOCK_WABA",
            "changes": [{
              "field": "messages",
              "value": {
                "messaging_product": "whatsapp",
                "metadata": {
                  "display_phone_number": "628000000000",
                  "phone_number_id": phone_number_id
                },
                "contacts": [{
                  "wa_id": from,
                  "profile": { "name": "Pelanggan Uji" }
                }],
                "messages": [{
                  "from": from,
                  "id": format!("wamid.mock.{}", Uuid::new_v4().simple()),
                  "timestamp": Utc::now().timestamp().to_string(),
                  "type": "text",
                  "text": { "body": text }
                }]
              }
            }]
          }]
        })
    }

    /// Builds a Meta-shaped status callback, for testing delivery receipts.
    pub fn simulate_status(phone_number_id: &str, wa_message_id: &str, status: &str) -> Value {
        json!({
          "object": "whatsapp_business_account",
          "entry": [{
            "id": "MOCK_WABA",
            "changes": [{
              "field": "messages",
              "value": {
                "messaging_product": "whatsapp",
                "metadata": { "phone_number_id": phone_number_id },
                "statuses": [{
                  "id": wa_message_id,
                  "status": status,
                  "timestamp": Utc::now().timestamp().to_string(),
                  "recipient_id": "6281234567890",
                  "pricing": { "billable": true, "category": "service" }
                }]
              }
            }]
          }]
        })
    }
}

#[async_trait]
impl WaProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn send_message(
        &self,
        _creds: &ProviderCreds,
        msg: &OutboundMessage,
    ) -> Result<SendResult> {
        let wa_message_id = format!("wamid.mock.{}", Uuid::new_v4().simple());
        if let Ok(mut guard) = self.sent.lock() {
            guard.push(SentRecord {
                wa_message_id: wa_message_id.clone(),
                to: msg.to.clone(),
                kind: msg.content.kind().to_string(),
                preview: msg.content.preview(),
                at: Utc::now(),
            });
        }
        tracing::info!(
            to = %msg.to,
            kind = msg.content.kind(),
            "mock provider accepted message"
        );
        Ok(SendResult {
            wa_message_id,
            pricing_category: Some(if msg.content.is_template() {
                "utility".into()
            } else {
                "service".into()
            }),
        })
    }

    async fn mark_read(&self, _creds: &ProviderCreds, _wa_message_id: &str) -> Result<()> {
        Ok(())
    }

    async fn list_templates(&self, _creds: &ProviderCreds) -> Result<Vec<RemoteTemplate>> {
        Ok(vec![
            RemoteTemplate {
                provider_ref: "mock_tpl_otp".into(),
                name: "kode_verifikasi".into(),
                language: "id".into(),
                category: "AUTHENTICATION".into(),
                status: "APPROVED".into(),
                components: json!([{
                    "type": "BODY",
                    "text": "Kode verifikasi Anda adalah {{1}}. Jangan bagikan ke siapa pun."
                }]),
                rejected_reason: None,
            },
            RemoteTemplate {
                provider_ref: "mock_tpl_promo".into(),
                name: "promo_bulanan".into(),
                language: "id".into(),
                category: "MARKETING".into(),
                status: "APPROVED".into(),
                components: json!([{
                    "type": "BODY",
                    "text": "Hai {{1}}, ada promo spesial untuk Anda bulan ini!"
                }]),
                rejected_reason: None,
            },
        ])
    }

    async fn media_url(&self, _creds: &ProviderCreds, media_id: &str) -> Result<String> {
        Ok(format!("https://mock.local/media/{media_id}"))
    }

    async fn phone_number_info(&self, _creds: &ProviderCreds) -> Result<PhoneNumberInfo> {
        Ok(PhoneNumberInfo {
            display_phone_number: Some("+62 800-0000-0000".into()),
            verified_name: Some("Mock Business".into()),
            quality_rating: Some("GREEN".into()),
            messaging_limit_tier: Some("TIER_1K".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_cloud::parse_webhook;

    #[tokio::test]
    async fn records_sent_messages() {
        let p = MockProvider::new();
        let creds = ProviderCreds {
            wa_account_id: Uuid::nil(),
            phone_number_id: "PNID".into(),
            waba_id: None,
            access_token: "t".into(),
        };
        let msg = OutboundMessage {
            to: "6281234567890".into(),
            content: MessageContent::Text {
                body: "tes".into(),
                preview_url: false,
            },
            reply_to: None,
        };
        let res = p.send_message(&creds, &msg).await.unwrap();
        assert!(res.wa_message_id.starts_with("wamid.mock."));
        assert_eq!(p.sent().len(), 1);
        assert_eq!(p.sent()[0].to, "6281234567890");
    }

    #[test]
    fn simulated_inbound_parses_with_the_real_parser() {
        let body = MockProvider::simulate_inbound("PNID1", "6281234567890", "halo");
        let batch = parse_webhook(&body);
        assert_eq!(batch.messages.len(), 1);
        assert_eq!(batch.messages[0].phone_number_id, "PNID1");
        assert_eq!(batch.messages[0].text.as_deref(), Some("halo"));
    }

    #[test]
    fn simulated_status_parses() {
        let body = MockProvider::simulate_status("PNID1", "wamid.X", "read");
        let batch = parse_webhook(&body);
        assert_eq!(batch.statuses.len(), 1);
        assert_eq!(batch.statuses[0].status, "read");
    }
}
