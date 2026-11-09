//! Minimal Anthropic Messages API client.
//!
//! Rust has no official Anthropic SDK, so this speaks the REST API directly
//! with `reqwest`. It asks for a structured reply (`output_config.format`)
//! rather than free text, so the worker never has to parse prose to decide
//! whether to hand off to a human.

use mms_core::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

/// Pinned: the API is versioned by header, and an unpinned client would
/// change behaviour under us on a server-side release.
const API_VERSION: &str = "2026-10-21";

/// Routes a safety decline to a fallback model inside the same call instead
/// of returning nothing.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub struct Claude {
    http: reqwest::Client,
    api_key: String,
    api_base: String,
}

/// What the model is required to return.
#[derive(Debug, Deserialize)]
pub struct AgentReply {
    pub reply: String,
    #[serde(default)]
    pub intent: String,
    #[serde(default)]
    pub needs_human: bool,
}

pub struct Completion {
    pub reply: AgentReply,
    pub input_tokens: i32,
    pub output_tokens: i32,
}

impl Claude {
    pub fn new(api_key: String, api_base: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client builds"),
            api_key,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// One turn. `history` is oldest-first `(role, text)` pairs.
    pub async fn reply(
        &self,
        model: &str,
        system_prompt: &str,
        history: &[(String, String)],
    ) -> Result<Completion> {
        let messages: Vec<Value> = history
            .iter()
            .map(|(role, text)| json!({ "role": role, "content": text }))
            .collect();

        // The API rejects a conversation that doesn't start with a user turn.
        if messages.first().and_then(|m| m["role"].as_str()) != Some("user") {
            return Err(Error::validation(
                "riwayat percakapan harus diawali pesan pelanggan",
            ));
        }

        let body = json!({
            "model": model,
            // Deliberately small: these are WhatsApp replies, not essays.
            "max_tokens": 1024,
            "system": system_prompt,
            "messages": messages,
            "output_config": {
                // A customer-service reply doesn't need deep reasoning, and
                // latency is visible to the customer.
                "effort": "low",
                "format": {
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "reply": {
                                "type": "string",
                                "description": "Balasan untuk pelanggan, dalam Bahasa Indonesia."
                            },
                            "intent": {
                                "type": "string",
                                "description": "Satu kata intent, mis. pricing, shipping, refund, other."
                            },
                            "needs_human": {
                                "type": "boolean",
                                "description": "True jika pertanyaan ini harus ditangani manusia."
                            }
                        },
                        "required": ["reply", "intent", "needs_human"],
                        "additionalProperties": false
                    }
                }
            },
            "fallbacks": "default"
        });

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.api_base))
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider {
                code: "network".into(),
                message: e.to_string(),
            })?;

        let status = resp.status();
        let payload: Value = resp.json().await.map_err(|e| Error::Provider {
            code: "bad_response".into(),
            message: e.to_string(),
        })?;

        if !status.is_success() {
            return Err(Error::Provider {
                code: payload["error"]["type"]
                    .as_str()
                    .unwrap_or("api_error")
                    .to_string(),
                message: payload["error"]["message"]
                    .as_str()
                    .unwrap_or("permintaan ke model gagal")
                    .to_string(),
            });
        }

        // A safety decline returns HTTP 200 with no usable content, so this
        // has to be checked before reading `content`.
        if payload["stop_reason"].as_str() == Some("refusal") {
            return Err(Error::Provider {
                code: "refusal".into(),
                message: format!(
                    "model menolak permintaan ({})",
                    payload["stop_details"]["category"]
                        .as_str()
                        .unwrap_or("tidak diketahui")
                ),
            });
        }

        let text = payload["content"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b["type"] == "text")
                    .and_then(|b| b["text"].as_str())
            })
            .ok_or_else(|| Error::Provider {
                code: "bad_response".into(),
                message: "respons model tidak berisi teks".into(),
            })?;

        let reply: AgentReply = serde_json::from_str(text).map_err(|e| Error::Provider {
            code: "bad_response".into(),
            message: format!("respons model bukan JSON yang diharapkan: {e}"),
        })?;

        if reply.reply.trim().is_empty() {
            return Err(Error::Provider {
                code: "empty_reply".into(),
                message: "model mengembalikan balasan kosong".into(),
            });
        }

        Ok(Completion {
            reply,
            input_tokens: payload["usage"]["input_tokens"].as_i64().unwrap_or(0) as i32,
            output_tokens: payload["usage"]["output_tokens"].as_i64().unwrap_or(0) as i32,
        })
    }
}

/// Builds the system prompt from the agent's configuration.
pub fn system_prompt(base: &str, business: &str, knowledge: &Value) -> String {
    let mut p = String::new();
    p.push_str(
        "Anda adalah asisten layanan pelanggan WhatsApp untuk sebuah bisnis di Indonesia.\n\
         Jawab dalam Bahasa Indonesia yang ramah, singkat, dan jelas — maksimal 3 kalimat.\n\
         Jangan mengarang informasi harga, stok, atau status pesanan yang tidak Anda ketahui;\n\
         jika tidak tahu, setel needs_human menjadi true dan katakan bahwa tim akan membantu.\n\n",
    );
    if !business.trim().is_empty() {
        p.push_str(&format!("Nama bisnis: {business}\n\n"));
    }
    if !base.trim().is_empty() {
        p.push_str("Instruksi dari pemilik bisnis:\n");
        p.push_str(base.trim());
        p.push_str("\n\n");
    }
    if let Some(items) = knowledge.as_array() {
        if !items.is_empty() {
            p.push_str("Informasi yang boleh Anda gunakan:\n");
            for item in items {
                if let Some(t) = item.as_str() {
                    p.push_str(&format!("- {t}\n"));
                }
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_business_and_knowledge() {
        let p = system_prompt(
            "Selalu tawarkan gratis ongkir.",
            "Toko Berkah",
            &json!(["Jam buka 09.00-17.00", "Pengiriman 1-2 hari"]),
        );
        assert!(p.contains("Toko Berkah"));
        assert!(p.contains("gratis ongkir"));
        assert!(p.contains("Jam buka"));
        assert!(p.contains("Bahasa Indonesia"));
    }

    #[test]
    fn prompt_survives_empty_configuration() {
        let p = system_prompt("", "", &json!(null));
        assert!(p.contains("asisten layanan pelanggan"));
        assert!(!p.contains("Nama bisnis"));
    }

    #[test]
    fn reply_deserializes_with_defaults() {
        let r: AgentReply = serde_json::from_str(r#"{"reply":"Halo!"}"#).unwrap();
        assert_eq!(r.reply, "Halo!");
        assert!(!r.needs_human);
        assert_eq!(r.intent, "");
    }
}
