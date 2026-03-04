use mini_chat_sdk::{ModelCatalogEntry, ModelTier};
use serde::Deserialize;

/// Plugin configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticMiniChatPolicyPluginConfig {
    /// Vendor name for GTS instance registration.
    pub vendor: String,

    /// Plugin priority (lower = higher priority).
    pub priority: i16,

    /// Static model catalog entries.
    pub model_catalog: Vec<ModelCatalogEntry>,
}

impl Default for StaticMiniChatPolicyPluginConfig {
    fn default() -> Self {
        Self {
            vendor: "hyperspot".to_owned(),
            priority: 100,
            model_catalog: vec![
                ModelCatalogEntry {
                    model_id: "gpt-5.2".to_owned(),
                    display_name: "GPT-5.2".to_owned(),
                    tier: ModelTier::Premium,
                    global_enabled: true,
                    is_default: true,
                },
                ModelCatalogEntry {
                    model_id: "gpt-5-mini".to_owned(),
                    display_name: "GPT-5 Mini".to_owned(),
                    tier: ModelTier::Standard,
                    global_enabled: true,
                    is_default: false,
                },
            ],
        }
    }
}
