//! Configuration for the static tenant resolver plugin.

use anyhow::{Context, bail};
use serde::Deserialize;
use tenant_resolver_sdk::TenantStatus;
use uuid::Uuid;

/// Plugin configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticTrPluginConfig {
    /// Vendor name for GTS instance registration.
    pub vendor: String,

    /// Plugin priority (lower = higher priority).
    pub priority: i16,

    /// Static tenant definitions.
    pub tenants: Vec<TenantConfig>,
}

impl Default for StaticTrPluginConfig {
    fn default() -> Self {
        Self {
            vendor: "hyperspot".to_owned(),
            priority: 100,
            tenants: Vec::new(),
        }
    }
}

impl StaticTrPluginConfig {
    /// Validates the single-root tree topology invariant.
    ///
    /// Enforces that the configured tenants form a single-root tree:
    /// - exactly one tenant has `parent_id == None`;
    /// - tenant ids are unique;
    /// - no tenant references itself via `parent_id`;
    /// - every referenced `parent_id` belongs to a configured tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the invariants above are violated.
    pub fn validate(&self) -> anyhow::Result<()> {
        let roots: Vec<Uuid> = self
            .tenants
            .iter()
            .filter(|t| t.parent_id.is_none())
            .map(|t| t.id)
            .collect();

        match roots.len() {
            0 => bail!(
                "static-tr-plugin: no root tenant configured -- exactly one tenant must have \
                 no parent_id (single-root tree topology)"
            ),
            1 => {}
            n => bail!(
                "static-tr-plugin: {n} root tenants configured ({roots:?}) -- exactly one \
                 tenant must have no parent_id (single-root tree topology)"
            ),
        }

        let mut ids: std::collections::HashSet<Uuid> =
            std::collections::HashSet::with_capacity(self.tenants.len());
        for tenant in &self.tenants {
            if !ids.insert(tenant.id) {
                return Err(anyhow::anyhow!(
                    "static-tr-plugin: duplicate tenant id {} in configuration",
                    tenant.id,
                ))
                .context("invalid tenant hierarchy configuration");
            }
        }

        for tenant in &self.tenants {
            let Some(parent_id) = tenant.parent_id else {
                continue;
            };
            if parent_id == tenant.id {
                return Err(anyhow::anyhow!(
                    "static-tr-plugin: tenant {} lists itself as parent_id",
                    tenant.id,
                ))
                .context("invalid tenant hierarchy configuration");
            }
            if !ids.contains(&parent_id) {
                return Err(anyhow::anyhow!(
                    "static-tr-plugin: tenant {} references parent_id {parent_id} which is not \
                     in the configured tenants",
                    tenant.id,
                ))
                .context("invalid tenant hierarchy configuration");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
    const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";
    const TENANT_C: &str = "33333333-3333-3333-3333-333333333333";

    fn tenant(id: &str, parent: Option<&str>) -> TenantConfig {
        TenantConfig {
            id: Uuid::parse_str(id).unwrap(),
            name: id.to_owned(),
            status: TenantStatus::Active,
            tenant_type: None,
            parent_id: parent.map(|p| Uuid::parse_str(p).unwrap()),
            self_managed: false,
        }
    }

    #[test]
    fn validate_accepts_single_root_with_children() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![
                tenant(TENANT_A, None),
                tenant(TENANT_B, Some(TENANT_A)),
                tenant(TENANT_C, Some(TENANT_B)),
            ],
            ..Default::default()
        };
        cfg.validate().expect("valid single-root tree should pass");
    }

    #[test]
    fn validate_accepts_single_root_alone() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![tenant(TENANT_A, None)],
            ..Default::default()
        };
        cfg.validate().expect("lone root should pass");
    }

    #[test]
    fn validate_rejects_zero_roots() {
        let cfg = StaticTrPluginConfig {
            tenants: Vec::new(),
            ..Default::default()
        };
        let err = cfg.validate().expect_err("empty config must fail");
        assert!(err.to_string().contains("no root tenant"));
    }

    #[test]
    fn validate_rejects_multiple_roots() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![tenant(TENANT_A, None), tenant(TENANT_B, None)],
            ..Default::default()
        };
        let err = cfg.validate().expect_err("two roots must fail");
        assert!(err.to_string().contains("2 root tenants"));
    }

    #[test]
    fn validate_rejects_dangling_parent_reference() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![tenant(TENANT_A, None), tenant(TENANT_B, Some(TENANT_C))],
            ..Default::default()
        };
        let err = cfg.validate().expect_err("dangling parent must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("invalid tenant hierarchy configuration"));
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![
                tenant(TENANT_A, None),
                // Same id, different parent — HashMap would silently drop this
                // without validation.
                tenant(TENANT_A, Some(TENANT_A)),
            ],
            ..Default::default()
        };
        let err = cfg.validate().expect_err("duplicate ids must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("duplicate tenant id"), "got: {msg}");
    }

    #[test]
    fn validate_rejects_self_referential_parent() {
        let cfg = StaticTrPluginConfig {
            tenants: vec![tenant(TENANT_A, None), tenant(TENANT_B, Some(TENANT_B))],
            ..Default::default()
        };
        let err = cfg
            .validate()
            .expect_err("self-referential parent must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("lists itself as parent_id"), "got: {msg}");
    }
}

/// Configuration for a single tenant.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantConfig {
    /// Tenant ID.
    pub id: Uuid,

    /// Tenant name.
    pub name: String,

    /// Tenant status (defaults to Active).
    #[serde(default)]
    pub status: TenantStatus,

    /// Tenant type classification.
    #[serde(rename = "type", default)]
    pub tenant_type: Option<String>,

    /// Parent tenant ID. `None` for the root tenant. Exactly one configured
    /// tenant is expected to have `parent_id == None` (single-root tree).
    #[serde(default)]
    pub parent_id: Option<Uuid>,

    /// Whether this tenant is self-managed (barrier).
    /// When `true`, parent tenants cannot traverse into this subtree
    /// unless `BarrierMode::Ignore` is used.
    #[serde(default)]
    pub self_managed: bool,
}
