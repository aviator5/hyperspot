//! Single-tenant resolver plugin module.

use std::sync::Arc;

use async_trait::async_trait;
use modkit::Module;
use modkit::client_hub::ClientScope;
use modkit::context::ModuleCtx;
use modkit::gts::PluginV1;
use tenant_resolver_sdk::{TenantResolverPluginClient, TenantResolverPluginSpecV1};
use tracing::info;
use types_registry_sdk::{RegisterResult, TypesRegistryClient};

use crate::domain::Service;

/// Hardcoded vendor name for GTS instance registration.
const VENDOR: &str = "cyberfabric";

/// Hardcoded priority (higher value = lower priority).
/// Set to 1000 so `static_tr_plugin` (priority 100) wins when both are enabled.
const PRIORITY: i16 = 1000;

/// Single-tenant resolver plugin module.
///
/// Zero-configuration plugin for single-tenant deployments.
/// Returns the tenant from security context as the only accessible tenant.
#[modkit::module(
    name = "single-tenant-tr-plugin",
    deps = ["types-registry"]
)]
pub struct SingleTenantTrPlugin;

impl Default for SingleTenantTrPlugin {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Module for SingleTenantTrPlugin {
    async fn init(&self, ctx: &ModuleCtx) -> anyhow::Result<()> {
        // Build registration payload and instance id for this plugin.
        let (instance_id, instance_json) =
            PluginV1::<TenantResolverPluginSpecV1>::build_registration(
                "cf.builtin.single_tenant_resolver.plugin.v1",
                VENDOR,
                PRIORITY,
            )?;

        // Publish to types-registry.
        let registry = ctx.client_hub().get::<dyn TypesRegistryClient>()?;
        let results = registry.register(vec![instance_json]).await?;
        RegisterResult::ensure_all_ok(&results)?;

        // Create service and register scoped client in ClientHub
        let service = Arc::new(Service);
        let api: Arc<dyn TenantResolverPluginClient> = service;
        ctx.client_hub()
            .register_scoped::<dyn TenantResolverPluginClient>(
                ClientScope::gts_id(&instance_id),
                api,
            );

        info!(
            instance_id = %instance_id,
            vendor = VENDOR,
            priority = PRIORITY
        );
        Ok(())
    }
}
