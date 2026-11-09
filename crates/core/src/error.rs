use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(&'static str),

    #[error("{0}")]
    Validation(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    #[error("conflict: {0}")]
    Conflict(String),

    /// A plan limit was hit. `metric` names the limit, e.g. "messages_per_month".
    #[error("quota exceeded: {metric} ({used}/{limit})")]
    QuotaExceeded {
        metric: &'static str,
        used: i64,
        limit: i64,
    },

    #[error("rate limited, retry in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// The upstream WhatsApp provider rejected the call.
    #[error("provider error {code}: {message}")]
    Provider { code: String, message: String },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Error::Conflict(msg.into())
    }

    /// Stable machine-readable code returned to API clients.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::Validation(_) => "invalid_request",
            Error::Unauthorized => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::Conflict(_) => "conflict",
            Error::QuotaExceeded { .. } => "quota_exceeded",
            Error::RateLimited { .. } => "rate_limited",
            Error::Provider { .. } => "provider_error",
            Error::Db(_) | Error::Crypto(_) | Error::Other(_) => "internal_error",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Error::NotFound(_) => 404,
            Error::Validation(_) => 422,
            Error::Unauthorized => 401,
            Error::Forbidden(_) => 403,
            Error::Conflict(_) => 409,
            Error::QuotaExceeded { .. } => 402,
            Error::RateLimited { .. } => 429,
            Error::Provider { .. } => 502,
            _ => 500,
        }
    }
}
