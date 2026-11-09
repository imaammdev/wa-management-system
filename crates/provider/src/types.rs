use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Everything an adapter needs to act on one connected number.
#[derive(Debug, Clone)]
pub struct ProviderCreds {
    pub wa_account_id: Uuid,
    pub phone_number_id: String,
    pub waba_id: Option<String>,
    /// Already unsealed by the caller.
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    Text {
        body: String,
        #[serde(default)]
        preview_url: bool,
    },
    Image {
        link: Option<String>,
        id: Option<String>,
        caption: Option<String>,
    },
    Video {
        link: Option<String>,
        id: Option<String>,
        caption: Option<String>,
    },
    Audio {
        link: Option<String>,
        id: Option<String>,
    },
    Document {
        link: Option<String>,
        id: Option<String>,
        caption: Option<String>,
        filename: Option<String>,
    },
    Location {
        latitude: f64,
        longitude: f64,
        name: Option<String>,
        address: Option<String>,
    },
    /// An approved template, the only thing sendable outside the 24h window.
    Template {
        name: String,
        language: String,
        #[serde(default)]
        components: serde_json::Value,
    },
    /// Buttons / lists. Passed through to Meta as-is.
    Interactive {
        interactive: serde_json::Value,
    },
    Reaction {
        message_id: String,
        emoji: String,
    },
}

impl MessageContent {
    /// The value stored in `messages.msg_type`.
    pub fn kind(&self) -> &'static str {
        match self {
            MessageContent::Text { .. } => "text",
            MessageContent::Image { .. } => "image",
            MessageContent::Video { .. } => "video",
            MessageContent::Audio { .. } => "audio",
            MessageContent::Document { .. } => "document",
            MessageContent::Location { .. } => "location",
            MessageContent::Template { .. } => "template",
            MessageContent::Interactive { .. } => "interactive",
            MessageContent::Reaction { .. } => "reaction",
        }
    }

    /// Short text for the inbox preview line.
    pub fn preview(&self) -> Option<String> {
        match self {
            MessageContent::Text { body, .. } => Some(body.clone()),
            MessageContent::Image { caption, .. }
            | MessageContent::Video { caption, .. }
            | MessageContent::Document { caption, .. } => caption.clone(),
            MessageContent::Template { name, .. } => Some(format!("[template: {name}]")),
            MessageContent::Reaction { emoji, .. } => Some(emoji.clone()),
            MessageContent::Location { name, .. } => {
                Some(name.clone().unwrap_or_else(|| "[lokasi]".into()))
            }
            MessageContent::Audio { .. } => Some("[audio]".into()),
            MessageContent::Interactive { .. } => Some("[interactive]".into()),
        }
    }

    /// Templates are billed by their own category; free-form replies inside
    /// the 24h window are 'service'.
    pub fn is_template(&self) -> bool {
        matches!(self, MessageContent::Template { .. })
    }
}

#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Recipient in `628…` form.
    pub to: String,
    pub content: MessageContent,
    /// Quote another message in the thread.
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SendResult {
    pub wa_message_id: String,
    /// Meta echoes the conversation pricing category on some responses.
    pub pricing_category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteTemplate {
    pub provider_ref: String,
    pub name: String,
    pub language: String,
    pub category: String,
    pub status: String,
    pub components: serde_json::Value,
    pub rejected_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhoneNumberInfo {
    pub display_phone_number: Option<String>,
    pub verified_name: Option<String>,
    pub quality_rating: Option<String>,
    pub messaging_limit_tier: Option<String>,
}

// ---------------------------------------------------- inbound (Meta webhooks)

/// The parsed shape of one Meta webhook POST. One POST can carry several
/// entries, each with several messages and/or statuses.
#[derive(Debug, Clone, Default)]
pub struct InboundBatch {
    pub messages: Vec<InboundMessage>,
    pub statuses: Vec<InboundStatus>,
}

#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub phone_number_id: String,
    pub wa_message_id: String,
    pub from: String,
    pub profile_name: Option<String>,
    pub msg_type: String,
    pub text: Option<String>,
    pub reply_to: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct InboundStatus {
    pub phone_number_id: String,
    pub wa_message_id: String,
    /// 'sent' | 'delivered' | 'read' | 'failed'
    pub status: String,
    pub pricing_category: Option<String>,
    pub error_code: Option<String>,
    pub error_title: Option<String>,
}
