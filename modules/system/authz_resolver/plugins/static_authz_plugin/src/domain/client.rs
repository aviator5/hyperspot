//! Client implementation for the static AuthZ resolver plugin.

use async_trait::async_trait;
use authz_resolver_sdk::{
    AuthZResolverError, AuthZResolverPluginClient, EvaluationRequest, EvaluationResponse,
};

use super::service::Service;

#[async_trait]
impl AuthZResolverPluginClient for Service {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        Ok(self.evaluate(&request))
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use authz_resolver_sdk::{Action, Context, Resource, Subject, TenantContext};
    use std::collections::HashMap;
    use uuid::Uuid;

    #[tokio::test]
    async fn plugin_trait_evaluates_successfully() {
        let service = Service::new();
        let plugin: &dyn AuthZResolverPluginClient = &service;

        let request = EvaluationRequest {
            subject: Subject {
                id: Uuid::nil(),
                tenant_id: None,
                subject_type: None,
                properties: HashMap::new(),
            },
            action: Action {
                name: "list".to_owned(),
            },
            resource: Resource {
                resource_type: "test".to_owned(),
                id: None,
                require_constraints: false,
            },
            context: Context {
                tenant: Some(TenantContext {
                    root_id: Uuid::nil(),
                }),
                token_scopes: vec![],
                properties: HashMap::new(),
            },
        };

        let result = plugin.evaluate(request).await;
        assert!(result.is_ok());
        assert!(result.unwrap().decision);
    }
}
