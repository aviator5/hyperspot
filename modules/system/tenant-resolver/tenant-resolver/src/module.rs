//! Tenant resolver module.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use modkit::Module;
use modkit::context::ModuleCtx;
use modkit::contracts::SystemCapability;
use tenant_resolver_sdk::TenantResolverClient;
use tracing::info;

use crate::config::TenantResolverConfig;
use crate::domain::{Service, TenantResolverLocalClient};

/// Tenant Resolver module.
///
/// This module:
/// 1. Discovers plugin instances via types-registry
/// 2. Routes requests to the selected plugin based on vendor configuration
///
/// The `TenantResolverPluginSpecV1` schema itself reaches `types-registry`
/// automatically via the `modkit-gts` link-time inventory — no per-init
/// registration is needed. Plugin discovery is lazy: happens on first API
/// call after types-registry is ready.
#[modkit::module(
    name = "tenant-resolver",
    deps = ["types-registry"],
    capabilities = [system]
)]
pub(crate) struct TenantResolver {
    service: OnceLock<Arc<Service>>,
}

impl Default for TenantResolver {
    fn default() -> Self {
        Self {
            service: OnceLock::new(),
        }
    }
}

// Marked as `system` so that init() runs in the system-module phase.
// This ensures the TenantResolver client is available in ClientHub before
// other system modules that depend on it.
impl SystemCapability for TenantResolver {}

#[async_trait]
impl Module for TenantResolver {
    #[tracing::instrument(skip_all, fields(vendor))]
    async fn init(&self, ctx: &ModuleCtx) -> anyhow::Result<()> {
        let cfg: TenantResolverConfig = ctx.config_or_default()?;
        tracing::Span::current().record("vendor", cfg.vendor.as_str());
        info!(vendor = %cfg.vendor);

        // Create service
        let hub = ctx.client_hub();
        let svc = Arc::new(Service::new(hub, cfg.vendor));
        self.service
            .set(svc.clone())
            .map_err(|_| anyhow::anyhow!("{} module already initialized", Self::MODULE_NAME))?;

        // Register local client in ClientHub
        let api: Arc<dyn TenantResolverClient> = Arc::new(TenantResolverLocalClient::new(svc));
        ctx.client_hub().register::<dyn TenantResolverClient>(api);

        Ok(())
    }
}
