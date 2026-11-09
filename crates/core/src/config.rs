use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub app_url: String,
    /// 32-byte key, hex-encoded. Seals provider access tokens at rest.
    pub encryption_key: [u8; 32],
    /// Default provider for newly connected numbers: "meta_cloud" | "mock".
    pub wa_provider: String,
    pub meta_api_base: String,
    pub meta_api_version: String,
    /// Shared verify token for Meta's webhook subscription handshake.
    pub meta_webhook_verify_token: String,
    /// Meta app secret, used to verify X-Hub-Signature-256 on inbound calls.
    pub meta_app_secret: Option<String>,
    pub session_ttl_days: i64,
    pub worker_concurrency: usize,
}

fn env(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| Error::validation(format!("missing env var {key}")))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    /// Reads `.env` if present, then the process environment.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let key_hex = env("ENCRYPTION_KEY")?;
        let raw = hex::decode(key_hex.trim())
            .map_err(|_| Error::validation("ENCRYPTION_KEY must be hex"))?;
        if raw.len() != 32 {
            return Err(Error::validation(
                "ENCRYPTION_KEY must be 32 bytes (64 hex chars)",
            ));
        }
        let mut encryption_key = [0u8; 32];
        encryption_key.copy_from_slice(&raw);

        Ok(Config {
            database_url: env("DATABASE_URL")?,
            bind_addr: env_or("BIND_ADDR", "127.0.0.1:8080"),
            app_url: env_or("APP_URL", "http://localhost:5173"),
            encryption_key,
            wa_provider: env_or("WA_PROVIDER", "mock"),
            meta_api_base: env_or("META_API_BASE", "https://graph.facebook.com"),
            meta_api_version: env_or("META_API_VERSION", "v23.0"),
            meta_webhook_verify_token: env_or("META_WEBHOOK_VERIFY_TOKEN", "dev-verify-token"),
            meta_app_secret: std::env::var("META_APP_SECRET").ok(),
            session_ttl_days: env_or("SESSION_TTL_DAYS", "30").parse().unwrap_or(30),
            worker_concurrency: env_or("WORKER_CONCURRENCY", "4").parse().unwrap_or(4),
        })
    }
}
