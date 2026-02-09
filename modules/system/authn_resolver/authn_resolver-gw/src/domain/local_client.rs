//! Local (in-process) client for the AuthN resolver gateway.

use std::sync::Arc;

use async_trait::async_trait;
use authn_resolver_sdk::{AuthNResolverError, AuthNResolverGatewayClient, AuthenticationResult};

use super::{DomainError, Service};

/// Local client wrapping the gateway service.
///
/// Registered in `ClientHub` by the gateway module during `init()`.
pub struct AuthNResolverGwLocalClient {
    svc: Arc<Service>,
}

impl AuthNResolverGwLocalClient {
    #[must_use]
    pub fn new(svc: Arc<Service>) -> Self {
        Self { svc }
    }
}

fn log_and_convert(op: &str, e: DomainError) -> AuthNResolverError {
    tracing::error!(operation = op, error = ?e, "authn_resolver gateway call failed");
    e.into()
}

#[async_trait]
impl AuthNResolverGatewayClient for AuthNResolverGwLocalClient {
    async fn authenticate(
        &self,
        bearer_token: &str,
    ) -> Result<AuthenticationResult, AuthNResolverError> {
        self.svc
            .authenticate(bearer_token)
            .await
            .map_err(|e| log_and_convert("authenticate", e))
    }
}
