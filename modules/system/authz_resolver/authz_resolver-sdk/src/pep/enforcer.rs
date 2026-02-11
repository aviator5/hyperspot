//! Policy Enforcement Point (`PEP`) object.
//!
//! [`PolicyEnforcer`] encapsulates the full PEP flow for a single resource type:
//! build evaluation request → call PDP → compile constraints to `AccessScope`.
//!
//! Constructed once per resource type during service initialisation; stores
//! the `AuthZ` client reference and static resource config so call sites only
//! need to supply per-request data (security context, action, resource ID).

use std::collections::HashMap;
use std::sync::Arc;

use modkit_security::{AccessScope, SecurityContext};
use secrecy::SecretString;
use uuid::Uuid;

use crate::api::AuthZResolverGatewayClient;
use crate::error::AuthZResolverError;
use crate::models::{
    Action, BarrierMode, Capability, Context, EvaluationRequest, Resource, Subject, TenantContext,
    TenantMode,
};
use crate::pep::compiler::{ConstraintCompileError, compile_to_access_scope};

/// Error from the PEP enforcement flow.
///
/// Unifies both failure modes: the PDP call itself can fail
/// ([`AuthZResolverError`]) or the constraint compilation can fail
/// ([`ConstraintCompileError`]).
#[derive(Debug, thiserror::Error)]
pub enum EnforcerError {
    /// The `AuthZ` evaluation RPC failed.
    #[error("authorization evaluation failed: {0}")]
    EvaluationFailed(#[from] AuthZResolverError),

    /// Constraint compilation failed (denied, missing, or unsupported).
    #[error("constraint compilation failed: {0}")]
    CompileFailed(#[from] ConstraintCompileError),
}

/// Per-request evaluation parameters for advanced authorization scenarios.
///
/// Used with [`PolicyEnforcer::access_scope_with()`] when the simple
/// [`PolicyEnforcer::access_scope()`] defaults don't suffice (ABAC resource
/// properties, custom tenant mode, barrier bypass, etc.).
///
/// All fields default to "not overridden" — only set what you need.
///
/// # Examples
///
/// ```ignore
/// use authz_resolver_sdk::pep::{AccessRequest, PolicyEnforcer};
///
/// // S04: CREATE with target tenant + resource properties
/// let scope = enforcer.access_scope_with(
///     &ctx, "create", None, false,
///     &AccessRequest::new()
///         .context_tenant_id(target_tenant_id)
///         .tenant_mode(TenantMode::RootOnly)
///         .resource_property("owner_tenant_id", json!(target_tenant_id.to_string())),
/// ).await?;
///
/// // S05: Billing — ignore barriers
/// let scope = enforcer.access_scope_with(
///     &ctx, "list", None, true,
///     &AccessRequest::new().barrier_mode(BarrierMode::Ignore),
/// ).await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct AccessRequest {
    resource_properties: HashMap<String, serde_json::Value>,
    tenant_context: Option<TenantContext>,
}

impl AccessRequest {
    /// Create a new empty access request (all defaults).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a single resource property for ABAC evaluation.
    #[must_use]
    pub fn resource_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.resource_properties.insert(key.into(), value.into());
        self
    }

    /// Set all resource properties at once (replaces any previously set).
    #[must_use]
    pub fn resource_properties(mut self, props: HashMap<String, serde_json::Value>) -> Self {
        self.resource_properties = props;
        self
    }

    /// Override the context tenant ID (default: subject's tenant).
    #[must_use]
    pub fn context_tenant_id(mut self, id: Uuid) -> Self {
        self.tenant_context.get_or_insert_default().root_id = Some(id);
        self
    }

    /// Override the tenant hierarchy mode (default: `Subtree`).
    #[must_use]
    pub fn tenant_mode(mut self, mode: TenantMode) -> Self {
        self.tenant_context.get_or_insert_default().mode = mode;
        self
    }

    /// Override the barrier enforcement mode (default: `Respect`).
    #[must_use]
    pub fn barrier_mode(mut self, mode: BarrierMode) -> Self {
        self.tenant_context.get_or_insert_default().barrier_mode = mode;
        self
    }

    /// Set a tenant status filter (e.g., `["active"]`).
    #[must_use]
    pub fn tenant_status(mut self, statuses: Vec<String>) -> Self {
        self.tenant_context.get_or_insert_default().tenant_status = Some(statuses);
        self
    }

    /// Set the entire tenant context at once.
    #[must_use]
    pub fn tenant_context(mut self, tc: TenantContext) -> Self {
        self.tenant_context = Some(tc);
        self
    }
}

/// Per-resource-type Policy Enforcement Point.
///
/// Holds the `AuthZ` client and static resource configuration.
/// Constructed once per resource type during service init; cloneable and
/// cheap to pass around (`Arc` inside).
///
/// # Example
///
/// ```ignore
/// let enforcer = PolicyEnforcer::new("users_info.user", authz.clone());
///
/// // In a request handler:
/// let scope = enforcer.access_scope(&ctx, "get", Some(id), true).await?;
/// ```
#[derive(Clone)]
pub struct PolicyEnforcer {
    authz: Arc<dyn AuthZResolverGatewayClient>,
    resource_type: String,
    capabilities: Vec<Capability>,
    supported_properties: Vec<String>,
}

impl PolicyEnforcer {
    /// Create a new enforcer for the given resource type.
    pub fn new(
        resource_type: impl Into<String>,
        authz: Arc<dyn AuthZResolverGatewayClient>,
    ) -> Self {
        Self {
            authz,
            resource_type: resource_type.into(),
            capabilities: Vec::new(),
            supported_properties: Vec::new(),
        }
    }

    /// Set PEP capabilities advertised to the PDP.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: Vec<Capability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Set supported constraint properties advertised to the PDP.
    #[must_use]
    pub fn with_supported_properties(mut self, properties: Vec<String>) -> Self {
        self.supported_properties = properties;
        self
    }

    /// The resource type this enforcer is configured for.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    // ── Low-level: build request only ────────────────────────────────

    /// Build an evaluation request using the subject's tenant as context tenant
    /// and default settings.
    #[must_use]
    pub fn build_request(
        &self,
        ctx: &SecurityContext,
        action: &str,
        resource_id: Option<Uuid>,
        require_constraints: bool,
    ) -> EvaluationRequest {
        self.build_request_with(
            ctx,
            action,
            resource_id,
            require_constraints,
            &AccessRequest::default(),
        )
    }

    /// Build an evaluation request with per-request overrides from [`AccessRequest`].
    #[must_use]
    pub fn build_request_with(
        &self,
        ctx: &SecurityContext,
        action: &str,
        resource_id: Option<Uuid>,
        require_constraints: bool,
        request: &AccessRequest,
    ) -> EvaluationRequest {
        // Resolve root_id: explicit override > subject_tenant_id > None
        let resolved_root_id = request
            .tenant_context
            .as_ref()
            .and_then(|tc| tc.root_id)
            .or(ctx.subject_tenant_id())
            .filter(|id| *id != Uuid::default());

        let tenant_context = resolved_root_id.map(|root_id| {
            let base = request.tenant_context.clone().unwrap_or_default();
            TenantContext {
                root_id: Some(root_id),
                ..base
            }
        });

        // Put subject's tenant_id into properties per AuthZEN spec
        let mut subject_properties = HashMap::new();
        if let Some(tid) = ctx.subject_tenant_id() {
            subject_properties.insert(
                "tenant_id".to_owned(),
                serde_json::Value::String(tid.to_string()),
            );
        }

        let bearer_token = ctx.bearer_token().map(|t| SecretString::from(t.to_owned()));

        EvaluationRequest {
            subject: Subject {
                id: ctx.subject_id(),
                subject_type: ctx.subject_type().map(ToOwned::to_owned),
                properties: subject_properties,
            },
            action: Action {
                name: action.to_owned(),
            },
            resource: Resource {
                resource_type: self.resource_type.clone(),
                id: resource_id,
                properties: request.resource_properties.clone(),
            },
            context: Context {
                tenant_context,
                token_scopes: ctx.token_scopes().to_vec(),
                require_constraints,
                capabilities: self.capabilities.clone(),
                supported_properties: self.supported_properties.clone(),
                bearer_token,
                properties: HashMap::new(),
            },
        }
    }

    // ── High-level: full PEP flow ────────────────────────────────────

    /// Execute the full PEP flow: build request → evaluate → compile to `AccessScope`.
    ///
    /// Uses `ctx.subject_tenant_id()` as the context tenant and default settings.
    ///
    /// # Errors
    ///
    /// - [`EnforcerError::EvaluationFailed`] if the PDP call fails
    /// - [`EnforcerError::CompileFailed`] if constraint compilation fails (denied, missing, etc.)
    pub async fn access_scope(
        &self,
        ctx: &SecurityContext,
        action: &str,
        resource_id: Option<Uuid>,
        require_constraints: bool,
    ) -> Result<AccessScope, EnforcerError> {
        self.access_scope_with(
            ctx,
            action,
            resource_id,
            require_constraints,
            &AccessRequest::default(),
        )
        .await
    }

    /// Execute the full PEP flow with per-request overrides from [`AccessRequest`].
    ///
    /// # Errors
    ///
    /// - [`EnforcerError::EvaluationFailed`] if the PDP call fails
    /// - [`EnforcerError::CompileFailed`] if constraint compilation fails (denied, missing, etc.)
    pub async fn access_scope_with(
        &self,
        ctx: &SecurityContext,
        action: &str,
        resource_id: Option<Uuid>,
        require_constraints: bool,
        request: &AccessRequest,
    ) -> Result<AccessScope, EnforcerError> {
        let eval_request =
            self.build_request_with(ctx, action, resource_id, require_constraints, request);
        let response = self.authz.evaluate(eval_request).await?;
        Ok(compile_to_access_scope(&response, require_constraints)?)
    }
}

impl std::fmt::Debug for PolicyEnforcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyEnforcer")
            .field("resource_type", &self.resource_type)
            .field("capabilities", &self.capabilities)
            .field("supported_properties", &self.supported_properties)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::constraints::{Constraint, InPredicate, Predicate};
    use crate::models::EvaluationResponse;

    fn uuid(s: &str) -> Uuid {
        Uuid::parse_str(s).expect("valid test UUID")
    }

    const TENANT: &str = "11111111-1111-1111-1111-111111111111";
    const SUBJECT: &str = "22222222-2222-2222-2222-222222222222";
    const RESOURCE: &str = "33333333-3333-3333-3333-333333333333";

    /// Mock that returns `decision=true` with a tenant constraint from
    /// the request's `TenantContext.root_id`.
    struct AllowAllMock;

    #[async_trait]
    impl AuthZResolverGatewayClient for AllowAllMock {
        async fn evaluate(
            &self,
            req: EvaluationRequest,
        ) -> Result<EvaluationResponse, AuthZResolverError> {
            let constraints = if req.context.require_constraints {
                if let Some(ref tc) = req.context.tenant_context {
                    if let Some(root_id) = tc.root_id {
                        vec![Constraint {
                            predicates: vec![Predicate::In(InPredicate {
                                property: "owner_tenant_id".to_owned(),
                                values: vec![root_id],
                            })],
                        }]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
            Ok(EvaluationResponse {
                decision: true,
                constraints,
                deny_reason: None,
            })
        }
    }

    /// Mock that always returns an RPC error.
    struct FailMock;

    #[async_trait]
    impl AuthZResolverGatewayClient for FailMock {
        async fn evaluate(
            &self,
            _req: EvaluationRequest,
        ) -> Result<EvaluationResponse, AuthZResolverError> {
            Err(AuthZResolverError::Internal("boom".to_owned()))
        }
    }

    fn test_ctx() -> SecurityContext {
        SecurityContext::builder()
            .subject_id(uuid(SUBJECT))
            .subject_tenant_id(uuid(TENANT))
            .build()
    }

    fn enforcer(mock: impl AuthZResolverGatewayClient + 'static) -> PolicyEnforcer {
        PolicyEnforcer::new("test.resource", Arc::new(mock))
    }

    // ── build_request ────────────────────────────────────────────────

    #[test]
    fn build_request_populates_fields() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let req = e.build_request(&ctx, "get", Some(uuid(RESOURCE)), true);

        assert_eq!(req.resource.resource_type, "test.resource");
        assert_eq!(req.action.name, "get");
        assert_eq!(req.resource.id, Some(uuid(RESOURCE)));
        assert!(req.context.require_constraints);
        assert_eq!(
            req.context
                .tenant_context
                .as_ref()
                .and_then(|tc| tc.root_id),
            Some(uuid(TENANT)),
        );
    }

    #[test]
    fn build_request_with_overrides_tenant() {
        let custom_tenant = uuid("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let req = e.build_request_with(
            &ctx,
            "list",
            None,
            false,
            &AccessRequest::new().context_tenant_id(custom_tenant),
        );

        assert_eq!(
            req.context
                .tenant_context
                .as_ref()
                .and_then(|tc| tc.root_id),
            Some(custom_tenant),
        );
        assert!(!req.context.require_constraints);
    }

    // ── access_scope ─────────────────────────────────────────────────

    #[tokio::test]
    async fn access_scope_returns_tenant_scope() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let scope = e
            .access_scope(&ctx, "get", Some(uuid(RESOURCE)), true)
            .await;

        let scope = scope.expect("should succeed");
        assert_eq!(scope.tenant_ids(), &[uuid(TENANT)]);
    }

    #[tokio::test]
    async fn access_scope_without_constraints_returns_allow_all() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let scope = e
            .access_scope(&ctx, "create", None, false)
            .await
            .expect("should succeed");

        assert!(scope.is_unconstrained());
    }

    #[tokio::test]
    async fn access_scope_evaluation_failure() {
        let e = enforcer(FailMock);
        let ctx = test_ctx();
        let result = e.access_scope(&ctx, "get", None, true).await;

        assert!(matches!(result, Err(EnforcerError::EvaluationFailed(_))));
    }

    #[tokio::test]
    async fn access_scope_anonymous_no_tenant_returns_compile_error() {
        let e = enforcer(AllowAllMock);
        let ctx = SecurityContext::anonymous();
        // Anonymous has no tenant → mock returns empty constraints → ConstraintsRequiredButAbsent
        let result = e.access_scope(&ctx, "list", None, true).await;

        assert!(matches!(result, Err(EnforcerError::CompileFailed(_))));
    }

    // ── builder methods ──────────────────────────────────────────────

    #[test]
    fn with_capabilities_and_properties() {
        let e = enforcer(AllowAllMock)
            .with_capabilities(vec![Capability::TenantHierarchy])
            .with_supported_properties(vec!["owner_tenant_id".to_owned()]);

        assert_eq!(e.capabilities, vec![Capability::TenantHierarchy]);
        assert_eq!(e.supported_properties, vec!["owner_tenant_id"]);
    }

    #[test]
    fn resource_type_accessor() {
        let e = enforcer(AllowAllMock);
        assert_eq!(e.resource_type(), "test.resource");
    }

    #[test]
    fn debug_impl() {
        let e = enforcer(AllowAllMock);
        let dbg = format!("{e:?}");
        assert!(dbg.contains("test.resource"));
    }

    // ── AccessRequest builder ─────────────────────────────────────────

    #[test]
    fn access_request_default_is_empty() {
        let req = AccessRequest::new();
        assert!(req.resource_properties.is_empty());
        assert!(req.tenant_context.is_none());
    }

    #[test]
    fn access_request_builder_chain() {
        let tid = uuid(TENANT);
        let req = AccessRequest::new()
            .resource_property("owner_tenant_id", serde_json::json!(tid.to_string()))
            .context_tenant_id(tid)
            .tenant_mode(TenantMode::RootOnly)
            .barrier_mode(BarrierMode::Ignore)
            .tenant_status(vec!["active".to_owned()]);

        assert_eq!(req.resource_properties.len(), 1);
        let tc = req.tenant_context.as_ref().expect("tenant context");
        assert_eq!(tc.root_id, Some(tid));
        assert_eq!(tc.mode, TenantMode::RootOnly);
        assert_eq!(tc.barrier_mode, BarrierMode::Ignore);
        assert_eq!(tc.tenant_status, Some(vec!["active".to_owned()]));
    }

    #[test]
    fn access_request_tenant_context_setter() {
        let tid = uuid(TENANT);
        let req = AccessRequest::new().tenant_context(TenantContext {
            root_id: Some(tid),
            mode: TenantMode::RootOnly,
            ..Default::default()
        });

        let tc = req.tenant_context.as_ref().expect("tenant context");
        assert_eq!(tc.root_id, Some(tid));
        assert_eq!(tc.mode, TenantMode::RootOnly);
        assert_eq!(tc.barrier_mode, BarrierMode::Respect);
    }

    #[test]
    fn access_request_resource_properties_replaces() {
        let mut props = HashMap::new();
        props.insert("a".to_owned(), serde_json::json!("1"));
        props.insert("b".to_owned(), serde_json::json!("2"));

        let req = AccessRequest::new()
            .resource_property("old_key", serde_json::json!("old"))
            .resource_properties(props);

        assert_eq!(req.resource_properties.len(), 2);
        assert!(!req.resource_properties.contains_key("old_key"));
    }

    // ── build_request_with ────────────────────────────────────────────

    #[test]
    fn build_request_with_applies_resource_properties() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let tid = uuid(TENANT);
        let req = e.build_request_with(
            &ctx,
            "create",
            None,
            false,
            &AccessRequest::new()
                .resource_property("owner_tenant_id", serde_json::json!(tid.to_string())),
        );

        assert_eq!(
            req.resource.properties.get("owner_tenant_id"),
            Some(&serde_json::json!(tid.to_string())),
        );
    }

    #[test]
    fn build_request_with_applies_tenant_mode_and_barrier() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let req = e.build_request_with(
            &ctx,
            "list",
            None,
            true,
            &AccessRequest::new()
                .tenant_mode(TenantMode::RootOnly)
                .barrier_mode(BarrierMode::Ignore)
                .tenant_status(vec!["active".to_owned()]),
        );

        let tc = req.context.tenant_context.as_ref().expect("tenant context");
        assert_eq!(tc.mode, TenantMode::RootOnly);
        assert_eq!(tc.barrier_mode, BarrierMode::Ignore);
        assert_eq!(tc.tenant_status, Some(vec!["active".to_owned()]));
    }

    #[test]
    fn build_request_with_default_delegates_to_subject_tenant() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let req = e.build_request_with(&ctx, "get", None, true, &AccessRequest::default());

        assert_eq!(
            req.context
                .tenant_context
                .as_ref()
                .and_then(|tc| tc.root_id),
            Some(uuid(TENANT)),
        );
    }

    // ── access_scope_with ─────────────────────────────────────────────

    #[tokio::test]
    async fn access_scope_with_custom_tenant() {
        let custom_tenant = uuid("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let scope = e
            .access_scope_with(
                &ctx,
                "list",
                None,
                true,
                &AccessRequest::new().context_tenant_id(custom_tenant),
            )
            .await
            .expect("should succeed");

        assert_eq!(scope.tenant_ids(), &[custom_tenant]);
    }

    #[tokio::test]
    async fn access_scope_with_resource_properties() {
        let e = enforcer(AllowAllMock);
        let ctx = test_ctx();
        let scope = e
            .access_scope_with(
                &ctx,
                "create",
                None,
                true,
                &AccessRequest::new()
                    .resource_property(
                        "owner_tenant_id",
                        serde_json::json!(uuid(TENANT).to_string()),
                    )
                    .tenant_mode(TenantMode::RootOnly),
            )
            .await
            .expect("should succeed");

        assert_eq!(scope.tenant_ids(), &[uuid(TENANT)]);
    }

    // ── request builder internals ────────────────────────────────────

    #[test]
    fn builds_request_with_all_fields() {
        let context_tenant_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let subject_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let subject_tenant_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let resource_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();

        let ctx = SecurityContext::builder()
            .subject_id(subject_id)
            .subject_tenant_id(subject_tenant_id)
            .subject_type("user")
            .token_scopes(vec!["admin".to_owned()])
            .bearer_token("test-token".to_owned())
            .build();

        let e = PolicyEnforcer::new("users_info.user", Arc::new(AllowAllMock))
            .with_capabilities(vec![Capability::TenantHierarchy])
            .with_supported_properties(vec!["owner_tenant_id".to_owned()]);

        let access_req = AccessRequest::new().tenant_context(TenantContext {
            root_id: Some(context_tenant_id),
            ..Default::default()
        });

        let request = e.build_request_with(&ctx, "get", Some(resource_id), true, &access_req);

        assert_eq!(request.subject.id, subject_id);
        assert_eq!(
            request.subject.properties.get("tenant_id").unwrap(),
            &serde_json::Value::String(subject_tenant_id.to_string())
        );
        assert_eq!(request.subject.subject_type.as_deref(), Some("user"));
        assert_eq!(request.action.name, "get");
        assert_eq!(request.resource.resource_type, "users_info.user");
        assert_eq!(request.resource.id, Some(resource_id));
        assert!(request.context.require_constraints);
        assert_eq!(
            request.context.tenant_context.as_ref().unwrap().root_id,
            Some(context_tenant_id)
        );
        assert_eq!(request.context.token_scopes, vec!["admin"]);
        assert_eq!(
            request.context.capabilities,
            vec![Capability::TenantHierarchy]
        );
        assert!(request.context.bearer_token.is_some());
        assert_eq!(
            request.context.supported_properties,
            vec!["owner_tenant_id"]
        );
    }

    #[test]
    fn builds_request_without_tenant_context() {
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .build();

        let e = enforcer(AllowAllMock);

        let request = e.build_request_with(&ctx, "create", None, false, &AccessRequest::default());

        assert!(request.context.tenant_context.is_none());
        assert!(!request.context.require_constraints);
        assert_eq!(request.resource.id, None);
        assert!(request.context.capabilities.is_empty());
        assert!(request.context.bearer_token.is_none());
    }

    #[test]
    fn applies_resource_properties() {
        let tenant_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .subject_tenant_id(tenant_id)
            .build();

        let e = enforcer(AllowAllMock);
        let access_req = AccessRequest::new()
            .resource_property(
                "owner_tenant_id",
                serde_json::Value::String(tenant_id.to_string()),
            )
            .context_tenant_id(tenant_id);

        let request = e.build_request_with(&ctx, "create", None, false, &access_req);

        assert_eq!(
            request.resource.properties.get("owner_tenant_id"),
            Some(&serde_json::Value::String(tenant_id.to_string())),
        );
    }

    #[test]
    fn applies_tenant_mode_and_barrier_mode() {
        let tenant_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .subject_tenant_id(tenant_id)
            .build();

        let e = enforcer(AllowAllMock);
        let access_req = AccessRequest::new().tenant_context(TenantContext {
            root_id: Some(tenant_id),
            mode: TenantMode::RootOnly,
            barrier_mode: BarrierMode::Ignore,
            tenant_status: Some(vec!["active".to_owned()]),
        });

        let request = e.build_request_with(&ctx, "list", None, true, &access_req);

        let tc = request.context.tenant_context.as_ref().unwrap();
        assert_eq!(tc.mode, TenantMode::RootOnly);
        assert_eq!(tc.barrier_mode, BarrierMode::Ignore);
        assert_eq!(tc.tenant_status, Some(vec!["active".to_owned()]));
    }

    #[test]
    fn falls_back_to_subject_tenant_id() {
        let subject_tenant = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .subject_tenant_id(subject_tenant)
            .build();

        let e = enforcer(AllowAllMock);

        // No tenant_context provided — should fall back to subject_tenant_id
        let request = e.build_request_with(&ctx, "list", None, true, &AccessRequest::default());

        let tc = request.context.tenant_context.as_ref().unwrap();
        assert_eq!(tc.root_id, Some(subject_tenant));
    }

    #[test]
    fn explicit_root_id_overrides_subject_tenant() {
        let subject_tenant = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let explicit_tenant = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let ctx = SecurityContext::builder()
            .subject_id(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
            .subject_tenant_id(subject_tenant)
            .build();

        let e = enforcer(AllowAllMock);
        let access_req = AccessRequest::new().context_tenant_id(explicit_tenant);

        let request = e.build_request_with(&ctx, "get", None, true, &access_req);

        let tc = request.context.tenant_context.as_ref().unwrap();
        assert_eq!(tc.root_id, Some(explicit_tenant));
    }
}
