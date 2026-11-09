//! Row structs. These map 1:1 onto the tables in `migrations/`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub avatar_url: Option<String>,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub timezone: String,
    pub locale: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Customer {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub external_ref: Option<String>,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub name: String,
    pub prefix: String,
    #[serde(skip_serializing)]
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub rate_limit_per_min: i32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WhatsappAccount {
    pub id: Uuid,
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub provider: String,
    pub label: String,
    pub waba_id: Option<String>,
    pub phone_number_id: Option<String>,
    pub display_phone_number: Option<String>,
    pub verified_name: Option<String>,
    /// Sealed at rest; never serialized out of the API.
    #[serde(skip_serializing)]
    pub access_token_enc: Option<String>,
    pub status: String,
    pub quality_rating: Option<String>,
    pub messaging_limit_tier: Option<String>,
    pub mode: String,
    pub provider_meta: serde_json::Value,
    pub last_error: Option<String>,
    pub connected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Contact {
    pub id: Uuid,
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub wa_id: String,
    pub name: Option<String>,
    pub profile_name: Option<String>,
    pub email: Option<String>,
    pub attributes: serde_json::Value,
    pub opt_in_status: String,
    pub blocked_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Conversation {
    pub id: Uuid,
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub wa_account_id: Uuid,
    pub contact_id: Uuid,
    pub status: String,
    pub assigned_to: Option<Uuid>,
    pub assigned_team_id: Option<Uuid>,
    pub stage_id: Option<Uuid>,
    pub service_window_expires_at: Option<DateTime<Utc>>,
    pub ai_agent_enabled: bool,
    pub unread_count: i32,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_preview: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Conversation {
    /// Meta only allows free-form replies inside 24h of the customer's last
    /// message. Outside it, you must use an approved template.
    pub fn service_window_open(&self) -> bool {
        self.service_window_expires_at
            .map(|t| t > Utc::now())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub org_id: Uuid,
    pub conversation_id: Uuid,
    pub wa_account_id: Uuid,
    pub contact_id: Uuid,
    pub direction: String,
    pub wa_message_id: Option<String>,
    pub msg_type: String,
    pub body: Option<String>,
    pub payload: serde_json::Value,
    pub status: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub pricing_category: Option<String>,
    pub source: String,
    pub sent_by_user_id: Option<Uuid>,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MessageTemplate {
    pub id: Uuid,
    pub org_id: Uuid,
    pub wa_account_id: Uuid,
    pub name: String,
    pub language: String,
    pub category: String,
    pub components: serde_json::Value,
    pub status: String,
    pub rejected_reason: Option<String>,
    pub provider_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub org_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub url: String,
    /// Returned once on create so the customer can store it; hidden on list.
    #[serde(skip_serializing)]
    pub secret: String,
    pub events: Vec<String>,
    pub active: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub org_id: Uuid,
    pub endpoint_id: Uuid,
    pub event_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempt: i32,
    pub max_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub duration_ms: Option<i32>,
    pub last_error: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Broadcast {
    pub id: Uuid,
    pub org_id: Uuid,
    pub wa_account_id: Uuid,
    pub template_id: Option<Uuid>,
    pub segment_id: Option<Uuid>,
    pub name: String,
    pub variables: serde_json::Value,
    pub status: String,
    pub throttle_per_minute: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub total_recipients: i32,
    pub sent_count: i32,
    pub delivered_count: i32,
    pub read_count: i32,
    pub failed_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Plan {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub tagline: Option<String>,
    pub price_idr_monthly: i64,
    pub price_idr_yearly: i64,
    pub is_public: bool,
    pub is_popular: bool,
    pub position: i32,
    pub max_broadcasts_per_month: i64,
    pub max_messages_per_month: i64,
    pub max_storage_bytes: i64,
    pub max_members: i32,
    pub max_wa_accounts: i32,
    pub history_retention_days: i32,
    pub features: serde_json::Value,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Subscription {
    pub id: Uuid,
    pub org_id: Uuid,
    pub plan_id: Uuid,
    pub status: String,
    pub interval: String,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub payment_provider: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MetaRate {
    pub country: String,
    pub category: String,
    /// Queries must select `rate_idr::float8` — NUMERIC has no f64 decoder.
    pub rate_idr: f64,
    pub free_tier_per_number_per_month: i32,
    pub effective_from: NaiveDate,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UsageRow {
    pub metric: String,
    pub value: i64,
}
