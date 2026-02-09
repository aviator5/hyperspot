//! Configuration for the AuthZ resolver gateway.

use serde::Deserialize;

/// Gateway configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthZResolverGwConfig {
    /// Vendor selector used to pick a plugin implementation.
    pub vendor: String,
}

impl Default for AuthZResolverGwConfig {
    fn default() -> Self {
        Self {
            vendor: "hyperspot".to_owned(),
        }
    }
}
