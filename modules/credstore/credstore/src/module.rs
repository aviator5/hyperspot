//! `CredStore` module.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use credstore_sdk::CredStoreClientV1;
use modkit::contracts::SystemCapability;
use modkit::{Module, ModuleCtx};
use tracing::info;

use crate::config::CredStoreConfig;
use crate::domain::{CredStoreLocalClient, Service};

/// `CredStore` gateway module.
///
/// This module:
/// 1. Discovers plugin instances via types-registry (lazy, first-use)
/// 2. Routes secret operations through the selected plugin
/// 3. Registers `Arc<dyn CredStoreClientV1>` in `ClientHub` for consumers
///
/// The `CredStorePluginSpecV1` schema itself reaches `types-registry`
/// automatically via the `modkit-gts` link-time inventory — no per-init
/// registration is needed.
#[modkit::module(
    name = "credstore",
    deps = ["types-registry"],
    capabilities = [system]
)]
pub struct CredStoreModule {
    service: OnceLock<Arc<Service>>,
}

impl Default for CredStoreModule {
    fn default() -> Self {
        Self {
            service: OnceLock::new(),
        }
    }
}

#[async_trait]
impl Module for CredStoreModule {
    #[tracing::instrument(skip_all, fields(vendor))]
    async fn init(&self, ctx: &ModuleCtx) -> anyhow::Result<()> {
        let cfg: CredStoreConfig = ctx.config_or_default()?;
        tracing::Span::current().record("vendor", cfg.vendor.as_str());
        info!(vendor = %cfg.vendor);

        // Create domain service
        let hub = ctx.client_hub();
        let svc = Arc::new(Service::new(hub, cfg.vendor));
        self.service
            .set(svc.clone())
            .map_err(|_| anyhow::anyhow!("{} module already initialized", Self::MODULE_NAME))?;

        // Register local client in ClientHub
        let api: Arc<dyn CredStoreClientV1> = Arc::new(CredStoreLocalClient::new(svc));
        ctx.client_hub().register::<dyn CredStoreClientV1>(api);

        Ok(())
    }
}

#[async_trait]
impl SystemCapability for CredStoreModule {}
