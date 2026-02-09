//! Configuration for the static AuthZ resolver plugin.

use serde::Deserialize;

/// Plugin configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticAuthzPluginConfig {
    /// Vendor name for GTS instance registration.
    pub vendor: String,

    /// Plugin priority (lower = higher priority).
    pub priority: i16,

    /// Authorization mode.
    pub mode: AuthzMode,
}

impl Default for StaticAuthzPluginConfig {
    fn default() -> Self {
        Self {
            vendor: "hyperspot".to_owned(),
            priority: 100,
            mode: AuthzMode::AllowAll,
        }
    }
}

/// Authorization mode.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthzMode {
    /// Allow all requests. For constrained operations, scope to context tenant.
    #[default]
    AllowAll,
}
