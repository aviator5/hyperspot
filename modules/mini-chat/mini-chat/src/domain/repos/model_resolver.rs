use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::error::DomainError;

/// Resolves and validates model IDs against the tenant's policy catalog.
///
/// If `model` is empty, returns the default model for the tenant.
/// If `model` is non-empty, validates it exists and is enabled in the catalog.
#[async_trait]
pub trait ModelResolver: Send + Sync {
    async fn resolve_model(&self, tenant_id: Uuid, model: &str) -> Result<String, DomainError>;
}
