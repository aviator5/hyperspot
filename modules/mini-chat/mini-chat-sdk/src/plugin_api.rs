use async_trait::async_trait;
use uuid::Uuid;

use crate::error::MiniChatModelPolicyPluginError;
use crate::models::{PolicySnapshot, PolicyVersionInfo};

/// Plugin API trait for mini-chat model policy implementations.
///
/// Plugins implement this trait to provide model catalog and policy data.
/// The mini-chat module discovers plugins via GTS types-registry and
/// delegates policy queries to the selected plugin.
#[async_trait]
pub trait MiniChatModelPolicyPluginClientV1: Send + Sync {
    /// Get the current policy version for a tenant.
    async fn get_current_policy_version(
        &self,
        tenant_id: Uuid,
    ) -> Result<PolicyVersionInfo, MiniChatModelPolicyPluginError>;

    /// Get the policy snapshot for a given version.
    async fn get_policy_snapshot(
        &self,
        tenant_id: Uuid,
        policy_version: u64,
    ) -> Result<PolicySnapshot, MiniChatModelPolicyPluginError>;
}
