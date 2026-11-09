//! Meta WhatsApp Cloud API adapter (Graph API).

use async_trait::async_trait;
use mms_core::{Error, Result};
use serde_json::{json, Value};
use std::time::Duration;

use crate::types::*;
use crate::WaProvider;

pub struct MetaCloudProvider {
    http: reqwest::Client,
    api_base: String,
    api_version: String,
    /// Path template for lead reporting; `{waba_id}` and `{phone_number_id}`
    /// are substituted. `None` disables the feature.
    conversions_path: Option<String>,
}

impl MetaCloudProvider {
    pub fn new(api_base: &str, api_version: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            http,
            api_base: api_base.trim_end_matches('/').to_string(),
            api_version: api_version.to_string(),
            conversions_path: None,
        }
    }

    pub fn with_conversions_path(mut self, path: Option<String>) -> Self {
        self.conversions_path = path;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.api_base, self.api_version, path)
    }

    /// Maps a Graph API error body onto `Error::Provider`.
    async fn parse_error(resp: reqwest::Response) -> Error {
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        let err = &body["error"];
        let code = err["code"]
            .as_i64()
            .map(|c| c.to_string())
            .unwrap_or_else(|| status.as_u16().to_string());
        let message = err["message"]
            .as_str()
            .unwrap_or("unknown provider error")
            .to_string();
        let detail = err["error_data"]["details"].as_str().unwrap_or("");
        Error::Provider {
            code,
            message: if detail.is_empty() {
                message
            } else {
                format!("{message} ({detail})")
            },
        }
    }

    /// Builds the `/messages` request body in Cloud API shape.
    fn message_body(msg: &OutboundMessage) -> Value {
        let mut body = json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": msg.to,
        });

        match &msg.content {
            MessageContent::Text {
                body: text,
                preview_url,
            } => {
                body["type"] = json!("text");
                body["text"] = json!({ "body": text, "preview_url": preview_url });
            }
            MessageContent::Image { link, id, caption } => {
                body["type"] = json!("image");
                body["image"] = media_object(link, id, caption, &None);
            }
            MessageContent::Video { link, id, caption } => {
                body["type"] = json!("video");
                body["video"] = media_object(link, id, caption, &None);
            }
            MessageContent::Audio { link, id } => {
                body["type"] = json!("audio");
                body["audio"] = media_object(link, id, &None, &None);
            }
            MessageContent::Document {
                link,
                id,
                caption,
                filename,
            } => {
                body["type"] = json!("document");
                body["document"] = media_object(link, id, caption, filename);
            }
            MessageContent::Location {
                latitude,
                longitude,
                name,
                address,
            } => {
                body["type"] = json!("location");
                body["location"] = json!({
                    "latitude": latitude, "longitude": longitude,
                    "name": name, "address": address,
                });
            }
            MessageContent::Template {
                name,
                language,
                components,
            } => {
                body["type"] = json!("template");
                let mut t = json!({
                    "name": name,
                    "language": { "code": language },
                });
                if !components.is_null() {
                    t["components"] = components.clone();
                }
                body["template"] = t;
            }
            MessageContent::Interactive { interactive } => {
                body["type"] = json!("interactive");
                body["interactive"] = interactive.clone();
            }
            MessageContent::Reaction { message_id, emoji } => {
                body["type"] = json!("reaction");
                body["reaction"] = json!({ "message_id": message_id, "emoji": emoji });
            }
        }

        if let Some(reply_to) = &msg.reply_to {
            body["context"] = json!({ "message_id": reply_to });
        }
        body
    }
}

fn media_object(
    link: &Option<String>,
    id: &Option<String>,
    caption: &Option<String>,
    filename: &Option<String>,
) -> Value {
    let mut o = json!({});
    // Meta accepts either an uploaded media id or a public link, never both.
    if let Some(id) = id {
        o["id"] = json!(id);
    } else if let Some(link) = link {
        o["link"] = json!(link);
    }
    if let Some(c) = caption {
        o["caption"] = json!(c);
    }
    if let Some(f) = filename {
        o["filename"] = json!(f);
    }
    o
}

#[async_trait]
impl WaProvider for MetaCloudProvider {
    fn name(&self) -> &'static str {
        "meta_cloud"
    }

    async fn send_message(
        &self,
        creds: &ProviderCreds,
        msg: &OutboundMessage,
    ) -> Result<SendResult> {
        let url = self.url(&format!("{}/messages", creds.phone_number_id));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&creds.access_token)
            .json(&Self::message_body(msg))
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }

        let body: Value = resp.json().await.map_err(|e| Error::Provider {
            code: "bad_response".into(),
            message: e.to_string(),
        })?;

        let wa_message_id = body["messages"][0]["id"]
            .as_str()
            .ok_or_else(|| Error::Provider {
                code: "bad_response".into(),
                message: "response had no message id".into(),
            })?
            .to_string();

        Ok(SendResult {
            wa_message_id,
            pricing_category: body["messages"][0]["message_status"]
                .as_str()
                .map(str::to_string),
        })
    }

    async fn mark_read(&self, creds: &ProviderCreds, wa_message_id: &str) -> Result<()> {
        let url = self.url(&format!("{}/messages", creds.phone_number_id));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&creds.access_token)
            .json(&json!({
                "messaging_product": "whatsapp",
                "status": "read",
                "message_id": wa_message_id,
            }))
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    async fn list_templates(&self, creds: &ProviderCreds) -> Result<Vec<RemoteTemplate>> {
        let waba_id = creds.waba_id.as_deref().ok_or_else(|| Error::Provider {
            code: "missing_waba".into(),
            message: "WABA id belum diset untuk nomor ini".into(),
        })?;
        let url = self.url(&format!("{waba_id}/message_templates"));
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&creds.access_token)
            .query(&[("limit", "200")])
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let body: Value = resp.json().await.unwrap_or_else(|_| json!({"data": []}));

        Ok(body["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|t| RemoteTemplate {
                provider_ref: t["id"].as_str().unwrap_or_default().to_string(),
                name: t["name"].as_str().unwrap_or_default().to_string(),
                language: t["language"].as_str().unwrap_or("id").to_string(),
                category: t["category"].as_str().unwrap_or("UTILITY").to_string(),
                status: t["status"].as_str().unwrap_or("PENDING").to_string(),
                components: t["components"].clone(),
                rejected_reason: t["rejected_reason"].as_str().map(str::to_string),
            })
            .collect())
    }

    async fn media_url(&self, creds: &ProviderCreds, media_id: &str) -> Result<String> {
        let resp = self
            .http
            .get(self.url(media_id))
            .bearer_auth(&creds.access_token)
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        body["url"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Error::Provider {
                code: "bad_response".into(),
                message: "media response had no url".into(),
            })
    }

    async fn phone_number_info(&self, creds: &ProviderCreds) -> Result<PhoneNumberInfo> {
        let resp = self
            .http
            .get(self.url(&creds.phone_number_id))
            .bearer_auth(&creds.access_token)
            .query(&[(
                "fields",
                "display_phone_number,verified_name,quality_rating,throughput",
            )])
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let b: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        Ok(PhoneNumberInfo {
            display_phone_number: b["display_phone_number"].as_str().map(str::to_string),
            verified_name: b["verified_name"].as_str().map(str::to_string),
            quality_rating: b["quality_rating"].as_str().map(str::to_string),
            messaging_limit_tier: b["throughput"]["level"].as_str().map(str::to_string),
        })
    }

    async fn report_lead_event(
        &self,
        creds: &ProviderCreds,
        event_name: &str,
        payload: &Value,
    ) -> Result<String> {
        // Meta exposes lead reporting through more than one product and the
        // path differs per enrollment, so we require it to be configured
        // rather than guessing and silently reporting nothing.
        let template = self
            .conversions_path
            .as_deref()
            .ok_or_else(|| Error::Provider {
                code: "not_configured".into(),
                message: "META_CONVERSIONS_PATH belum diset — lead event tidak dikirim".into(),
            })?;

        let waba_id = creds.waba_id.as_deref().unwrap_or_default();
        let path = template
            .replace("{waba_id}", waba_id)
            .replace("{phone_number_id}", &creds.phone_number_id);

        let body = json!({
            "event_name": event_name,
            "action_source": "business_messaging",
            "messaging_channel": "whatsapp",
            "event_time": chrono::Utc::now().timestamp(),
            "custom_data": payload,
        });

        let resp = self
            .http
            .post(self.url(&path))
            .bearer_auth(&creds.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let b: Value = resp.json().await.unwrap_or_else(|_| json!({}));
        Ok(b["id"].as_str().unwrap_or("accepted").to_string())
    }
}

// ----------------------------------------------------------- webhook parsing

/// Parses one Meta webhook POST body into messages and status updates.
/// Unknown shapes are skipped rather than failing the whole batch — Meta adds
/// fields without notice and a 500 from us means they retry the lot.
pub fn parse_webhook(body: &Value) -> InboundBatch {
    let mut batch = InboundBatch::default();

    let entries = body["entry"].as_array().cloned().unwrap_or_default();
    for entry in entries {
        let changes = entry["changes"].as_array().cloned().unwrap_or_default();
        for change in changes {
            let value = &change["value"];
            let phone_number_id = value["metadata"]["phone_number_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if phone_number_id.is_empty() {
                continue;
            }

            // Profile names arrive in a sibling array keyed by wa_id.
            let contacts = value["contacts"].as_array().cloned().unwrap_or_default();
            let name_for = |wa_id: &str| -> Option<String> {
                contacts
                    .iter()
                    .find(|c| c["wa_id"].as_str() == Some(wa_id))
                    .and_then(|c| c["profile"]["name"].as_str())
                    .map(str::to_string)
            };

            for m in value["messages"].as_array().cloned().unwrap_or_default() {
                let from = m["from"].as_str().unwrap_or_default().to_string();
                let wa_message_id = m["id"].as_str().unwrap_or_default().to_string();
                if from.is_empty() || wa_message_id.is_empty() {
                    continue;
                }
                let msg_type = m["type"].as_str().unwrap_or("unsupported").to_string();
                let text = match msg_type.as_str() {
                    "text" => m["text"]["body"].as_str().map(str::to_string),
                    "button" => m["button"]["text"].as_str().map(str::to_string),
                    "interactive" => m["interactive"]["button_reply"]["title"]
                        .as_str()
                        .or_else(|| m["interactive"]["list_reply"]["title"].as_str())
                        .map(str::to_string),
                    "image" | "video" | "document" => {
                        m[&msg_type]["caption"].as_str().map(str::to_string)
                    }
                    "reaction" => m["reaction"]["emoji"].as_str().map(str::to_string),
                    _ => None,
                };

                batch.messages.push(InboundMessage {
                    phone_number_id: phone_number_id.clone(),
                    wa_message_id,
                    profile_name: name_for(&from),
                    from,
                    msg_type,
                    text,
                    reply_to: m["context"]["id"].as_str().map(str::to_string),
                    referral: m.get("referral").filter(|r| r.is_object()).cloned(),
                    raw: m.clone(),
                });
            }

            for s in value["statuses"].as_array().cloned().unwrap_or_default() {
                let wa_message_id = s["id"].as_str().unwrap_or_default().to_string();
                if wa_message_id.is_empty() {
                    continue;
                }
                let err = s["errors"][0].clone();
                batch.statuses.push(InboundStatus {
                    phone_number_id: phone_number_id.clone(),
                    wa_message_id,
                    status: s["status"].as_str().unwrap_or("sent").to_string(),
                    pricing_category: s["pricing"]["category"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| {
                            s["conversation"]["origin"]["type"]
                                .as_str()
                                .map(str::to_string)
                        }),
                    error_code: err["code"].as_i64().map(|c| c.to_string()),
                    error_title: err["title"].as_str().map(str::to_string),
                });
            }
        }
    }

    batch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_text_body() {
        let msg = OutboundMessage {
            to: "6281234567890".into(),
            content: MessageContent::Text {
                body: "Halo".into(),
                preview_url: false,
            },
            reply_to: None,
        };
        let b = MetaCloudProvider::message_body(&msg);
        assert_eq!(b["type"], "text");
        assert_eq!(b["text"]["body"], "Halo");
        assert_eq!(b["messaging_product"], "whatsapp");
    }

    #[test]
    fn media_prefers_id_over_link() {
        let o = media_object(
            &Some("https://x/y.jpg".into()),
            &Some("media-123".into()),
            &None,
            &None,
        );
        assert_eq!(o["id"], "media-123");
        assert!(o["link"].is_null());
    }

    #[test]
    fn parses_inbound_text_and_status() {
        let body = json!({
          "entry": [{
            "changes": [{
              "value": {
                "metadata": { "phone_number_id": "PNID1" },
                "contacts": [{ "wa_id": "6281", "profile": { "name": "Budi" } }],
                "messages": [{
                  "from": "6281", "id": "wamid.AAA", "type": "text",
                  "text": { "body": "Pesan masuk" }
                }],
                "statuses": [{
                  "id": "wamid.BBB", "status": "delivered",
                  "pricing": { "category": "utility" }
                }]
              }
            }]
          }]
        });
        let batch = parse_webhook(&body);
        assert_eq!(batch.messages.len(), 1);
        assert_eq!(batch.messages[0].text.as_deref(), Some("Pesan masuk"));
        assert_eq!(batch.messages[0].profile_name.as_deref(), Some("Budi"));
        assert_eq!(batch.statuses.len(), 1);
        assert_eq!(batch.statuses[0].status, "delivered");
        assert_eq!(
            batch.statuses[0].pricing_category.as_deref(),
            Some("utility")
        );
    }

    #[test]
    fn captures_click_to_whatsapp_referral() {
        let body = json!({
          "entry": [{
            "changes": [{
              "value": {
                "metadata": { "phone_number_id": "PNID1" },
                "messages": [{
                  "from": "6281", "id": "wamid.CTWA", "type": "text",
                  "text": { "body": "Saya lihat iklan ini" },
                  "referral": {
                    "source_url": "https://fb.me/abc",
                    "source_id": "1203948",
                    "source_type": "ad",
                    "headline": "Diskon 50%",
                    "ctwa_clid": "clid_xyz"
                  }
                }]
              }
            }]
          }]
        });
        let batch = parse_webhook(&body);
        let r = batch.messages[0]
            .referral
            .as_ref()
            .expect("referral captured");
        assert_eq!(r["source_type"], "ad");
        assert_eq!(r["ctwa_clid"], "clid_xyz");
    }

    #[test]
    fn ordinary_messages_have_no_referral() {
        let body = MockProviderShim::inbound();
        let batch = parse_webhook(&body);
        assert!(batch.messages[0].referral.is_none());
    }

    struct MockProviderShim;
    impl MockProviderShim {
        fn inbound() -> Value {
            json!({"entry":[{"changes":[{"value":{
                "metadata": {"phone_number_id":"P"},
                "messages":[{"from":"62811","id":"wamid.X","type":"text",
                             "text":{"body":"halo"}}]
            }}]}]})
        }
    }

    #[test]
    fn skips_malformed_entries_without_failing() {
        let body = json!({ "entry": [{ "changes": [{ "value": {} }] }] });
        let batch = parse_webhook(&body);
        assert!(batch.messages.is_empty() && batch.statuses.is_empty());
    }
}
