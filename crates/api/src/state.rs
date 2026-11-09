use mms_core::{Config, PgPool};
use mms_provider::WaProvider;
use std::sync::Arc;

use crate::ratelimit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    /// Real Meta adapter, used when an account's `provider` is `meta_cloud`.
    pub meta: Arc<dyn WaProvider>,
    /// Local stand-in, used when it is `mock`.
    pub mock: Arc<dyn WaProvider>,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    /// Picks the adapter for a given account's provider column, so one
    /// deployment can serve mock numbers alongside real ones.
    pub fn provider_for(&self, provider: &str) -> Arc<dyn WaProvider> {
        match provider {
            "meta_cloud" => self.meta.clone(),
            _ => self.mock.clone(),
        }
    }
}
