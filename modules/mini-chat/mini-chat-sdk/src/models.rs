use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Current policy version metadata for a tenant.
#[derive(Debug, Clone)]
pub struct PolicyVersionInfo {
    pub tenant_id: Uuid,
    pub policy_version: u64,
    pub generated_at: OffsetDateTime,
}

/// Full policy snapshot for a given version, including the model catalog.
#[derive(Debug, Clone)]
pub struct PolicySnapshot {
    pub tenant_id: Uuid,
    pub policy_version: u64,
    pub model_catalog: Vec<ModelCatalogEntry>,
}

/// A single model in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalogEntry {
    pub model_id: String,
    pub display_name: String,
    pub tier: ModelTier,
    pub global_enabled: bool,
    pub is_default: bool,
}

/// Model pricing/capability tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    Standard,
    Premium,
}
