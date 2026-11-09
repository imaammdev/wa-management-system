use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Wraps `mms_core::Error` so it can be returned straight from handlers.
pub struct ApiError(pub mms_core::Error);

impl From<mms_core::Error> for ApiError {
    fn from(e: mms_core::Error) -> Self {
        ApiError(e)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError(mms_core::Error::Db(e))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let e = self.0;
        let status =
            StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // 5xx bodies are deliberately vague; the detail goes to the log only.
        let message = if status.is_server_error() {
            tracing::error!(error = %e, "request failed");
            "terjadi kesalahan internal".to_string()
        } else {
            e.to_string()
        };

        let mut body = json!({
            "error": {
                "code": e.code(),
                "message": message,
            }
        });

        if let mms_core::Error::QuotaExceeded {
            metric,
            used,
            limit,
        } = &e
        {
            body["error"]["details"] = json!({ "metric": metric, "used": used, "limit": limit });
        }
        if let mms_core::Error::RateLimited { retry_after_secs } = &e {
            body["error"]["details"] = json!({ "retry_after": retry_after_secs });
        }

        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
