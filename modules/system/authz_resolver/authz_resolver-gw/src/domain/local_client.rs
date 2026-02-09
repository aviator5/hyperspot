//! Local (in-process) client for the `AuthZ` resolver gateway.

use std::sync::Arc;

use async_trait::async_trait;
use authz_resolver_sdk::{
    AuthZResolverError, AuthZResolverGatewayClient, EvaluationRequest, EvaluationResponse,
};

use super::{DomainError, Service};

/// Local client wrapping the gateway service.
pub struct AuthZResolverGwLocalClient {
    svc: Arc<Service>,
}

impl AuthZResolverGwLocalClient {
    #[must_use]
    pub fn new(svc: Arc<Service>) -> Self {
        Self { svc }
    }
}

fn log_and_convert(op: &str, e: DomainError) -> AuthZResolverError {
    tracing::error!(operation = op, error = ?e, "authz_resolver gateway call failed");
    e.into()
}

#[async_trait]
impl AuthZResolverGatewayClient for AuthZResolverGwLocalClient {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        self.svc
            .evaluate(request)
            .await
            .map_err(|e| log_and_convert("evaluate", e))
    }
}
