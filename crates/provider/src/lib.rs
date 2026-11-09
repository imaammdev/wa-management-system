//! WhatsApp provider adapters.
//!
//! `WaProvider` is the seam between our API and whoever actually delivers the
//! message. `meta_cloud` talks to Meta's Graph API; `mock` fakes the whole
//! thing locally so the platform is testable before Meta access is approved.

pub mod meta_cloud;
pub mod mock;
pub mod types;

use async_trait::async_trait;
use mms_core::Result;

pub use meta_cloud::MetaCloudProvider;
pub use mock::MockProvider;
pub use types::*;

#[async_trait]
pub trait WaProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Sends one message. `creds` carries the unsealed token and phone id.
    async fn send_message(
        &self,
        creds: &ProviderCreds,
        msg: &OutboundMessage,
    ) -> Result<SendResult>;

    /// Marks an inbound message read, which is what makes the blue ticks show.
    async fn mark_read(&self, creds: &ProviderCreds, wa_message_id: &str) -> Result<()>;

    /// Lists templates upstream so we can mirror approval status locally.
    async fn list_templates(&self, creds: &ProviderCreds) -> Result<Vec<RemoteTemplate>>;

    /// Resolves a media id to a short-lived download URL.
    async fn media_url(&self, creds: &ProviderCreds, media_id: &str) -> Result<String>;

    /// Reads the number's profile, used to confirm a connection works.
    async fn phone_number_info(&self, creds: &ProviderCreds) -> Result<PhoneNumberInfo>;
}

/// Builds the adapter named by `WA_PROVIDER` / the account's `provider` column.
pub fn build(provider: &str, api_base: &str, api_version: &str) -> Box<dyn WaProvider> {
    match provider {
        "meta_cloud" => Box::new(MetaCloudProvider::new(api_base, api_version)),
        _ => Box::new(MockProvider::new()),
    }
}
