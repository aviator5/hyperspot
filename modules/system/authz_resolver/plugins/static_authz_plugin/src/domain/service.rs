//! Service implementation for the static `AuthZ` resolver plugin.

use authz_resolver_sdk::{
    Constraint, EvaluationRequest, EvaluationResponse, InPredicate, Predicate,
};
use uuid::Uuid;

/// Static `AuthZ` resolver service.
///
/// In `allow_all` mode:
/// - Always returns `decision: true`
/// - When `require_constraints=true`, returns `in` predicate on `owner_tenant_id`
///   scoped to the context tenant from the request.
/// - When `require_constraints=false`, returns no constraints (for CREATE).
pub struct Service;

impl Service {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Evaluate an authorization request.
    #[must_use]
    pub fn evaluate(&self, request: &EvaluationRequest) -> EvaluationResponse {
        if !request.resource.require_constraints {
            // CREATE operations: just grant access, no row-level constraints
            return EvaluationResponse {
                decision: true,
                constraints: vec![],
            };
        }

        // For constrained operations: scope to context tenant
        let tenant_id = request
            .context
            .tenant
            .as_ref()
            .map(|t| t.root_id)
            .or(request.subject.tenant_id);

        let constraints = if let Some(tid) = tenant_id {
            if tid == Uuid::default() {
                // Anonymous/nil tenant: no constraints (will result in allow_all)
                vec![]
            } else {
                vec![Constraint {
                    predicates: vec![Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![tid],
                    })],
                }]
            }
        } else {
            vec![]
        };

        EvaluationResponse {
            decision: true,
            constraints,
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use authz_resolver_sdk::{Action, Context, Resource, Subject, TenantContext};
    use std::collections::HashMap;

    fn make_request(require_constraints: bool, tenant_id: Option<Uuid>) -> EvaluationRequest {
        EvaluationRequest {
            subject: Subject {
                id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                tenant_id: Some(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()),
                subject_type: None,
                properties: HashMap::new(),
            },
            action: Action {
                name: "list".to_owned(),
            },
            resource: Resource {
                resource_type: "users_info.user".to_owned(),
                id: None,
                require_constraints,
            },
            context: Context {
                tenant: tenant_id.map(|id| TenantContext { root_id: id }),
                token_scopes: vec!["*".to_owned()],
                properties: HashMap::new(),
            },
        }
    }

    #[test]
    fn create_operation_no_constraints() {
        let service = Service::new();
        let response = service.evaluate(&make_request(false, None));

        assert!(response.decision);
        assert!(response.constraints.is_empty());
    }

    #[test]
    fn list_operation_with_tenant_context() {
        let tenant_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let service = Service::new();
        let response = service.evaluate(&make_request(true, Some(tenant_id)));

        assert!(response.decision);
        assert_eq!(response.constraints.len(), 1);

        let constraint = &response.constraints[0];
        assert_eq!(constraint.predicates.len(), 1);

        match &constraint.predicates[0] {
            Predicate::In(in_pred) => {
                assert_eq!(in_pred.property, "owner_tenant_id");
                assert_eq!(in_pred.values, vec![tenant_id]);
            }
            other => panic!("Expected In predicate, got: {other:?}"),
        }
    }

    #[test]
    fn list_operation_without_tenant_falls_back_to_subject_tenant() {
        let service = Service::new();
        let response = service.evaluate(&make_request(true, None));

        // Falls back to subject.tenant_id
        assert!(response.decision);
        assert_eq!(response.constraints.len(), 1);

        match &response.constraints[0].predicates[0] {
            Predicate::In(in_pred) => {
                assert_eq!(
                    in_pred.values,
                    vec![Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()]
                );
            }
            other => panic!("Expected In predicate, got: {other:?}"),
        }
    }

    #[test]
    fn list_operation_with_nil_tenant_returns_no_constraints() {
        let service = Service::new();
        let response = service.evaluate(&make_request(true, Some(Uuid::default())));

        assert!(response.decision);
        assert!(response.constraints.is_empty());
    }
}
