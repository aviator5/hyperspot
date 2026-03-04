use std::sync::Arc;

use async_trait::async_trait;
use mini_chat_sdk::{MiniChatModelPolicyPluginClientV1, MiniChatModelPolicyPluginSpecV1};
use modkit::client_hub::{ClientHub, ClientScope};
use modkit::plugins::{GtsPluginSelector, choose_plugin_instance};
use types_registry_sdk::{ListQuery, TypesRegistryClient};
use uuid::Uuid;

use crate::domain::error::DomainError;
use crate::domain::repos::ModelResolver;

/// Resolves model IDs by querying the policy plugin discovered via GTS.
pub struct ModelPolicyGateway {
    hub: Arc<ClientHub>,
    vendor: String,
    policy_selector: GtsPluginSelector,
}

impl ModelPolicyGateway {
    pub(crate) fn new(hub: Arc<ClientHub>, vendor: String) -> Self {
        Self {
            hub,
            vendor,
            policy_selector: GtsPluginSelector::new(),
        }
    }

    /// Lazily resolve the policy plugin from `ClientHub`.
    async fn get_policy_plugin(
        &self,
    ) -> Result<Arc<dyn MiniChatModelPolicyPluginClientV1>, DomainError> {
        let instance_id = self
            .policy_selector
            .get_or_init(|| self.resolve_policy_plugin())
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        let scope = ClientScope::gts_id(instance_id.as_ref());
        self.hub
            .try_get_scoped::<dyn MiniChatModelPolicyPluginClientV1>(&scope)
            .ok_or_else(|| {
                DomainError::internal(format!(
                    "Policy plugin client not registered: {instance_id}"
                ))
            })
    }

    /// Resolve the policy plugin instance from types-registry.
    async fn resolve_policy_plugin(&self) -> Result<String, anyhow::Error> {
        let registry = self.hub.get::<dyn TypesRegistryClient>()?;
        let plugin_type_id = MiniChatModelPolicyPluginSpecV1::gts_schema_id().clone();
        let instances = registry
            .list(
                ListQuery::new()
                    .with_pattern(format!("{plugin_type_id}*"))
                    .with_is_type(false),
            )
            .await?;

        let gts_id = choose_plugin_instance::<MiniChatModelPolicyPluginSpecV1>(
            &self.vendor,
            instances.iter().map(|e| (e.gts_id.as_str(), &e.content)),
        )?;

        Ok(gts_id)
    }
}

#[async_trait]
impl ModelResolver for ModelPolicyGateway {
    async fn resolve_model(&self, tenant_id: Uuid, model: &str) -> Result<String, DomainError> {
        let plugin = self.get_policy_plugin().await?;
        let version_info = plugin
            .get_current_policy_version(tenant_id)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;
        let snapshot = plugin
            .get_policy_snapshot(tenant_id, version_info.policy_version)
            .await
            .map_err(|e| DomainError::internal(e.to_string()))?;

        if model.is_empty() {
            // Find default model (prefer is_default + enabled, else first enabled)
            let default = snapshot
                .model_catalog
                .iter()
                .find(|m| m.is_default && m.global_enabled)
                .or_else(|| snapshot.model_catalog.iter().find(|m| m.global_enabled));

            match default {
                Some(entry) => Ok(entry.model_id.clone()),
                None => Err(DomainError::invalid_model("no models available in catalog")),
            }
        } else {
            // Validate provided model exists in catalog
            let found = snapshot
                .model_catalog
                .iter()
                .any(|m| m.model_id == model && m.global_enabled);

            if found {
                Ok(model.to_owned())
            } else {
                Err(DomainError::invalid_model(model))
            }
        }
    }
}
