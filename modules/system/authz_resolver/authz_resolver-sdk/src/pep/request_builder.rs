//! PEP request builder.
//!
//! Convenience function for building `EvaluationRequest` from a `SecurityContext`.

use std::collections::HashMap;

use modkit_security::SecurityContext;
use uuid::Uuid;

use crate::models::{Action, Context, EvaluationRequest, Resource, Subject, TenantContext};

/// Build an evaluation request from the security context and action metadata.
///
/// Populates the `Subject` from `SecurityContext` fields and sets up
/// the `TenantContext` from the explicit `context_tenant_id` parameter.
///
/// # Arguments
///
/// * `ctx` - The authenticated security context
/// * `action_name` - The action being performed (e.g., "list", "get", "create")
/// * `resource_type` - The resource type (e.g., "`users_info.user`")
/// * `resource_id` - Specific resource ID (for GET/UPDATE/DELETE)
/// * `require_constraints` - Whether to request row-level constraints from the PDP
/// * `context_tenant_id` - The context tenant for this operation (determined by the module)
#[must_use]
pub fn build_evaluation_request(
    ctx: &SecurityContext,
    action_name: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    require_constraints: bool,
    context_tenant_id: Option<Uuid>,
) -> EvaluationRequest {
    let tenant_context = context_tenant_id
        .filter(|id| *id != Uuid::default())
        .map(|id| TenantContext { root_id: id });

    EvaluationRequest {
        subject: Subject {
            id: ctx.subject_id(),
            tenant_id: ctx.subject_tenant_id(),
            subject_type: None,
            properties: HashMap::new(),
        },
        action: Action {
            name: action_name.to_owned(),
        },
        resource: Resource {
            resource_type: resource_type.to_owned(),
            id: resource_id,
            require_constraints,
        },
        context: Context {
            tenant: tenant_context,
            token_scopes: ctx.token_scopes().to_vec(),
            properties: HashMap::new(),
        },
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn builds_request_with_all_fields() {
        let context_tenant_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let subject_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let subject_tenant_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let resource_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();

        let ctx = SecurityContext::builder()
            .subject_id(subject_id)
            .subject_tenant_id(subject_tenant_id)
            .token_scopes(vec!["admin".to_owned()])
            .build();

        let request = build_evaluation_request(
            &ctx,
            "get",
            "users_info.user",
            Some(resource_id),
            true,
            Some(context_tenant_id),
        );

        assert_eq!(request.subject.id, subject_id);
        assert_eq!(request.subject.tenant_id, Some(subject_tenant_id));
        assert_eq!(request.action.name, "get");
        assert_eq!(request.resource.resource_type, "users_info.user");
        assert_eq!(request.resource.id, Some(resource_id));
        assert!(request.resource.require_constraints);
        assert_eq!(
            request.context.tenant.as_ref().unwrap().root_id,
            context_tenant_id
        );
        assert_eq!(request.context.token_scopes, vec!["admin"]);
    }

    #[test]
    fn builds_request_without_tenant_context() {
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .build();

        let request =
            build_evaluation_request(&ctx, "create", "users_info.user", None, false, None);

        assert!(request.context.tenant.is_none());
        assert!(!request.resource.require_constraints);
        assert_eq!(request.resource.id, None);
    }
}
