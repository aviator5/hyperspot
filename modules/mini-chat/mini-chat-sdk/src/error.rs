use thiserror::Error;

/// Errors returned by `MiniChatModelPolicyPluginClientV1` methods.
#[derive(Debug, Error)]
pub enum MiniChatModelPolicyPluginError {
    #[error("policy not found for the given tenant/version")]
    NotFound,

    #[error("internal policy plugin error: {0}")]
    Internal(String),
}
