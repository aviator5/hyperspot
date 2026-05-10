//! Mock PDP plumbing for service-level `#[tokio::test]` blocks. Two
//! shapes are exposed:
//!
//! * [`mock_enforcer`] wires [`MockAuthZResolver`], an always-permit
//!   PDP that returns no constraints. Account Management calls
//!   `access_scope_with(... require_constraints(false))`, so the
//!   compiled scope is `AccessScope::allow_all` — sufficient for
//!   the majority of saga tests that don't care about row-level
//!   filtering semantics.
//! * [`constraint_bearing_enforcer`] wires
//!   [`ConstraintBearingAuthZResolver`], which DOES model
//!   constraint-bearing PDP output (single `OWNER_TENANT_ID Eq`
//!   constraint). It exists specifically to regression-pin the
//!   `tenants`-entity scope-discard contract enforced in
//!   `service::TenantService` (cyberware-rust#1813): an
//!   authorized read / update / soft-delete must NOT compile a
//!   PDP-narrowed permit into a `WHERE false` denial at the
//!   secure-extension layer until `InTenantSubtree` lands.

#![allow(dead_code, clippy::must_use_candidate, clippy::missing_panics_doc)]

use std::sync::Arc;

use async_trait::async_trait;
use authz_resolver_sdk::{
    AuthZResolverClient, AuthZResolverError, PolicyEnforcer,
    models::{EvaluationRequest, EvaluationResponse, EvaluationResponseContext},
};
use modkit_macros::domain_model;

/// Always-permit mock PDP for service / handler tests.
///
/// Returns `decision: true` with no constraints (i.e. compiles to
/// [`AccessScope::allow_all`]). Cross-tenant denial in production is
/// owned by the PDP behind a real `PolicyEnforcer` fed by the Tenant
/// Resolver Plugin (separate PR in this stack); this mock therefore
/// stays minimal — tests that need cross-tenant behaviour land
/// alongside the resolver plugin, not here.
#[domain_model]
pub struct MockAuthZResolver;

#[async_trait]
impl AuthZResolverClient for MockAuthZResolver {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        // AM service tests exercise the current decision-only path:
        // `require_constraints(false)` compiles an empty constraint
        // set to `AccessScope::allow_all`.
        let _ = request;
        Ok(EvaluationResponse {
            decision: true,
            context: EvaluationResponseContext::default(),
        })
    }
}

/// Build a permissive [`PolicyEnforcer`] for tests. Pairs with
/// [`make_service`] and the inline `make_service` helpers used by the
/// service-level `#[tokio::test]` blocks.
#[must_use]
pub fn mock_enforcer() -> PolicyEnforcer {
    let authz: Arc<dyn AuthZResolverClient> = Arc::new(MockAuthZResolver);
    PolicyEnforcer::new(authz)
}

/// Constraint-bearing PDP fake used to regression-pin the
/// `tenants`-entity scope-discard contract: an authorized read /
/// update / soft-delete must NOT compile a PDP-narrowed permit into
/// a `WHERE false` denial at the secure-extension layer. Returns
/// `decision: true` plus a single `OWNER_TENANT_ID Eq` constraint —
/// representative of what the static `AuthZ` resolver emits in
/// production for tenant-scoped policies. Pairs with
/// [`constraint_bearing_enforcer`].
#[domain_model]
pub struct ConstraintBearingAuthZResolver {
    /// Owner-tenant value the synthetic constraint will pin to.
    /// The actual UUID is irrelevant for the contract under test —
    /// what matters is that the compiled `AccessScope` is
    /// **constrained** rather than `allow_all`, which would have
    /// turned the `tenants`-entity repo read into a deny-all
    /// before the P1 fix.
    pub owner_tenant_id: uuid::Uuid,
}

#[async_trait]
impl AuthZResolverClient for ConstraintBearingAuthZResolver {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        use authz_resolver_sdk::constraints::{Constraint, EqPredicate, Predicate};
        use modkit_security::pep_properties;

        let _ = request;
        Ok(EvaluationResponse {
            decision: true,
            context: EvaluationResponseContext {
                constraints: vec![Constraint {
                    predicates: vec![Predicate::Eq(EqPredicate::new(
                        pep_properties::OWNER_TENANT_ID,
                        self.owner_tenant_id,
                    ))],
                }],
                deny_reason: None,
            },
        })
    }
}

/// Build a [`PolicyEnforcer`] backed by [`ConstraintBearingAuthZResolver`].
/// Used by tests that pin the contract: AM service methods must
/// **not** plumb a PDP-narrowed scope into the `tenants` repo until
/// `InTenantSubtree` lands (cyberware-rust#1813), because the
/// `tenants` entity is `no_tenant, no_resource, no_owner, no_type`
/// and any narrowed scope would compile to `WHERE false`.
#[must_use]
pub fn constraint_bearing_enforcer(owner_tenant_id: uuid::Uuid) -> PolicyEnforcer {
    let authz: Arc<dyn AuthZResolverClient> =
        Arc::new(ConstraintBearingAuthZResolver { owner_tenant_id });
    PolicyEnforcer::new(authz)
}
