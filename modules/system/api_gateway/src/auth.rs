use axum::http::Method;
use axum::response::IntoResponse;
use std::{collections::HashMap, sync::Arc};

use authn_resolver_sdk::{AuthNResolverError, AuthNResolverGatewayClient};
use modkit_security::SecurityContext;

/// Route matcher for a specific HTTP method (secured routes with requirements)
#[derive(Clone)]
pub struct RouteMatcher {
    matcher: matchit::Router<RouteRequirement>,
}

/// Route-level requirement: a resource + action pair registered by an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequirement {
    pub resource: String,
    pub action: String,
}

impl RouteMatcher {
    fn new() -> Self {
        Self {
            matcher: matchit::Router::new(),
        }
    }

    fn insert(
        &mut self,
        path: &str,
        requirement: RouteRequirement,
    ) -> Result<(), matchit::InsertError> {
        self.matcher.insert(path, requirement)
    }

    fn find(&self, path: &str) -> Option<&RouteRequirement> {
        self.matcher.at(path).ok().map(|m| m.value)
    }
}

/// Public route matcher for explicitly public routes
#[derive(Clone)]
pub struct PublicRouteMatcher {
    matcher: matchit::Router<()>,
}

impl PublicRouteMatcher {
    fn new() -> Self {
        Self {
            matcher: matchit::Router::new(),
        }
    }

    fn insert(&mut self, path: &str) -> Result<(), matchit::InsertError> {
        self.matcher.insert(path, ())
    }

    fn find(&self, path: &str) -> bool {
        self.matcher.at(path).is_ok()
    }
}

/// Convert Axum path syntax `:param` to matchit syntax `{param}`
///
/// Axum uses `:id` for path parameters, but matchit 0.8 uses `{id}`.
/// This function converts between the two syntaxes.
fn convert_axum_path_to_matchit(path: &str) -> String {
    // Simple regex-free approach: find :word and replace with {word}
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == ':' {
            // Start of a parameter - collect the parameter name
            result.push('{');
            while matches!(chars.peek(), Some(c) if c.is_alphanumeric() || *c == '_') {
                if let Some(c) = chars.next() {
                    result.push(c);
                }
            }
            result.push('}');
        } else {
            result.push(ch);
        }
    }

    result
}

/// Whether a route requires authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRequirement {
    /// No authentication required (public route).
    None,
    /// Authentication required; an optional requirement pair may apply.
    Required(Option<RouteRequirement>),
}

/// Gateway-specific route policy implementation
#[derive(Clone)]
pub struct GatewayRoutePolicy {
    route_matchers: Arc<HashMap<Method, RouteMatcher>>,
    public_matchers: Arc<HashMap<Method, PublicRouteMatcher>>,
    require_auth_by_default: bool,
}

impl GatewayRoutePolicy {
    pub fn new(
        route_matchers: Arc<HashMap<Method, RouteMatcher>>,
        public_matchers: Arc<HashMap<Method, PublicRouteMatcher>>,
        require_auth_by_default: bool,
    ) -> Self {
        Self {
            route_matchers,
            public_matchers,
            require_auth_by_default,
        }
    }

    /// Resolve the authentication requirement for a given (method, path).
    pub fn resolve(&self, method: &Method, path: &str) -> AuthRequirement {
        // Find requirement using pattern matching
        let requirement = self
            .route_matchers
            .get(method)
            .and_then(|matcher| matcher.find(path))
            .cloned();

        // Check if route is explicitly public using pattern matching
        let is_public = self
            .public_matchers
            .get(method)
            .is_some_and(|matcher| matcher.find(path));

        // Public routes should not be forced to auth by default
        let needs_authn = requirement.is_some() || (self.require_auth_by_default && !is_public);

        if needs_authn {
            AuthRequirement::Required(requirement)
        } else {
            AuthRequirement::None
        }
    }
}

/// Shared state for the authentication middleware.
#[derive(Clone)]
pub struct AuthState {
    pub authn_client: Arc<dyn AuthNResolverGatewayClient>,
    pub route_policy: GatewayRoutePolicy,
}

/// Helper to build `GatewayRoutePolicy` from operation requirements.
pub fn build_route_policy(
    cfg: &crate::config::ApiGatewayConfig,
    requirements: HashMap<(Method, String), RouteRequirement>,
    public_routes: std::collections::HashSet<(Method, String)>,
) -> Result<GatewayRoutePolicy, anyhow::Error> {
    // Build route matchers per HTTP method (secured routes with requirements)
    let mut route_matchers_map: HashMap<Method, RouteMatcher> = HashMap::new();

    for ((method, path), requirement) in requirements {
        let matcher = route_matchers_map
            .entry(method)
            .or_insert_with(RouteMatcher::new);
        // Convert Axum path syntax (:param) to matchit syntax ({param})
        let matchit_path = convert_axum_path_to_matchit(&path);
        matcher
            .insert(&matchit_path, requirement)
            .map_err(|e| anyhow::anyhow!("Failed to insert route pattern '{path}': {e}"))?;
    }

    // Build public matchers per HTTP method
    let mut public_matchers_map: HashMap<Method, PublicRouteMatcher> = HashMap::new();

    for (method, path) in public_routes {
        let matcher = public_matchers_map
            .entry(method)
            .or_insert_with(PublicRouteMatcher::new);
        // Convert Axum path syntax (:param) to matchit syntax ({param})
        let matchit_path = convert_axum_path_to_matchit(&path);
        matcher
            .insert(&matchit_path)
            .map_err(|e| anyhow::anyhow!("Failed to insert public route pattern '{path}': {e}"))?;
    }

    Ok(GatewayRoutePolicy::new(
        Arc::new(route_matchers_map),
        Arc::new(public_matchers_map),
        cfg.require_auth_by_default,
    ))
}

/// Authentication middleware that uses the AuthN Resolver to validate bearer tokens.
///
/// For each request:
/// 1. Skips CORS preflight requests
/// 2. Resolves the route's auth requirement via `GatewayRoutePolicy`
/// 3. For public routes: inserts anonymous `SecurityContext`
/// 4. For required routes: extracts bearer token, calls AuthN Resolver, inserts `SecurityContext`
pub async fn authn_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Skip CORS preflight
    if is_preflight_request(req.method(), req.headers()) {
        return next.run(req).await;
    }

    let requirement = state.route_policy.resolve(req.method(), req.uri().path());

    match requirement {
        AuthRequirement::None => {
            req.extensions_mut().insert(SecurityContext::anonymous());
            next.run(req).await
        }
        AuthRequirement::Required(_sec_requirement) => {
            let Some(token) = extract_bearer_token(req.headers()) else {
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    "Missing or invalid Authorization header",
                )
                    .into_response();
            };

            match state.authn_client.authenticate(token).await {
                Ok(result) => {
                    req.extensions_mut().insert(result.security_context);
                    next.run(req).await
                }
                Err(err) => authn_error_to_response(err),
            }
        }
    }
}

/// Convert `AuthNResolverError` to an HTTP response.
fn authn_error_to_response(err: AuthNResolverError) -> axum::response::Response {
    use axum::response::IntoResponse;

    match err {
        AuthNResolverError::Unauthorized(msg) => {
            tracing::debug!("AuthN rejected: {msg}");
            (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
        }
        AuthNResolverError::NoPluginAvailable => {
            tracing::error!("No AuthN plugin available");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Authentication service unavailable",
            )
                .into_response()
        }
        AuthNResolverError::ServiceUnavailable(msg) => {
            tracing::error!("AuthN service unavailable: {msg}");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Authentication service unavailable",
            )
                .into_response()
        }
        AuthNResolverError::Internal(msg) => {
            tracing::error!("AuthN internal error: {msg}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal authentication error",
            )
                .into_response()
        }
    }
}

/// Extract Bearer token from Authorization header
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::trim))
}

/// Check if this is a CORS preflight request
///
/// Preflight requests are OPTIONS requests with:
/// - Origin header present
/// - Access-Control-Request-Method header present
fn is_preflight_request(method: &Method, headers: &axum::http::HeaderMap) -> bool {
    method == Method::OPTIONS
        && headers.contains_key(axum::http::header::ORIGIN)
        && headers.contains_key(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use axum::http::Method;

    /// Helper to build `GatewayRoutePolicy` with given matchers
    fn build_test_policy(
        route_matchers: HashMap<Method, RouteMatcher>,
        public_matchers: HashMap<Method, PublicRouteMatcher>,
        require_auth_by_default: bool,
    ) -> GatewayRoutePolicy {
        GatewayRoutePolicy::new(
            Arc::new(route_matchers),
            Arc::new(public_matchers),
            require_auth_by_default,
        )
    }

    #[test]
    fn test_convert_axum_path_to_matchit() {
        assert_eq!(convert_axum_path_to_matchit("/users/:id"), "/users/{id}");
        assert_eq!(
            convert_axum_path_to_matchit("/posts/:post_id/comments/:comment_id"),
            "/posts/{post_id}/comments/{comment_id}"
        );
        assert_eq!(convert_axum_path_to_matchit("/health"), "/health"); // No params
        assert_eq!(
            convert_axum_path_to_matchit("/api/v1/:resource/:id/status"),
            "/api/v1/{resource}/{id}/status"
        );
    }

    #[test]
    fn test_matchit_router_with_params() {
        // matchit 0.8 uses {param} syntax for path parameters (NOT :param)
        let mut router = matchit::Router::new();
        router.insert("/users/{id}", "user_route").unwrap();

        let result = router.at("/users/42");
        assert!(
            result.is_ok(),
            "matchit should match /users/{{id}} against /users/42"
        );
        assert_eq!(*result.unwrap().value, "user_route");
    }

    #[test]
    fn explicit_public_route_with_path_params_returns_none() {
        let mut public_matchers = HashMap::new();
        let mut matcher = PublicRouteMatcher::new();
        // matchit 0.8 uses {param} syntax (Axum uses :param, so conversion needed in production)
        matcher.insert("/users/{id}").unwrap();

        public_matchers.insert(Method::GET, matcher);

        let policy = build_test_policy(HashMap::new(), public_matchers, true);

        // Path parameters should match concrete values
        let result = policy.resolve(&Method::GET, "/users/42");
        assert_eq!(result, AuthRequirement::None);
    }

    #[test]
    fn explicit_public_route_exact_match_returns_none() {
        let mut public_matchers = HashMap::new();
        let mut matcher = PublicRouteMatcher::new();
        matcher.insert("/health").unwrap();
        public_matchers.insert(Method::GET, matcher);

        let policy = build_test_policy(HashMap::new(), public_matchers, true);

        let result = policy.resolve(&Method::GET, "/health");
        assert_eq!(result, AuthRequirement::None);
    }

    #[test]
    fn explicit_secured_route_with_requirement_returns_required() {
        let mut route_matchers = HashMap::new();
        let mut matcher = RouteMatcher::new();
        let req = RouteRequirement {
            resource: "admin".to_owned(),
            action: "access".to_owned(),
        };
        matcher.insert("/admin/metrics", req).unwrap();
        route_matchers.insert(Method::GET, matcher);

        let policy = build_test_policy(route_matchers, HashMap::new(), false);

        let result = policy.resolve(&Method::GET, "/admin/metrics");
        match result {
            AuthRequirement::Required(Some(req)) => {
                assert_eq!(req.resource, "admin");
                assert_eq!(req.action, "access");
            }
            _ => panic!("Expected Required with RouteRequirement"),
        }
    }

    #[test]
    fn route_without_requirement_with_require_auth_by_default_returns_required_none() {
        let policy = build_test_policy(HashMap::new(), HashMap::new(), true);

        let result = policy.resolve(&Method::GET, "/profile");
        assert_eq!(result, AuthRequirement::Required(None));
    }

    #[test]
    fn route_without_requirement_without_require_auth_by_default_returns_none() {
        let policy = build_test_policy(HashMap::new(), HashMap::new(), false);

        let result = policy.resolve(&Method::GET, "/profile");
        assert_eq!(result, AuthRequirement::None);
    }

    #[test]
    fn unknown_route_with_require_auth_by_default_true_returns_required() {
        let policy = build_test_policy(HashMap::new(), HashMap::new(), true);

        let result = policy.resolve(&Method::POST, "/unknown");
        assert_eq!(result, AuthRequirement::Required(None));
    }

    #[test]
    fn unknown_route_with_require_auth_by_default_false_returns_none() {
        let policy = build_test_policy(HashMap::new(), HashMap::new(), false);

        let result = policy.resolve(&Method::POST, "/unknown");
        assert_eq!(result, AuthRequirement::None);
    }

    #[test]
    fn public_route_overrides_require_auth_by_default() {
        let mut public_matchers = HashMap::new();
        let mut matcher = PublicRouteMatcher::new();
        matcher.insert("/public").unwrap();
        public_matchers.insert(Method::GET, matcher);

        let policy = build_test_policy(HashMap::new(), public_matchers, true);

        let result = policy.resolve(&Method::GET, "/public");
        assert_eq!(result, AuthRequirement::None);
    }

    #[test]
    fn secured_route_has_priority_over_default() {
        let mut route_matchers = HashMap::new();
        let mut matcher = RouteMatcher::new();
        let req = RouteRequirement {
            resource: "users".to_owned(),
            action: "read".to_owned(),
        };
        // matchit 0.8 uses {param} syntax
        matcher.insert("/users/{id}", req).unwrap();
        route_matchers.insert(Method::GET, matcher);

        let policy = build_test_policy(route_matchers, HashMap::new(), false);

        let result = policy.resolve(&Method::GET, "/users/123");
        match result {
            AuthRequirement::Required(Some(req)) => {
                assert_eq!(req.resource, "users");
                assert_eq!(req.action, "read");
            }
            _ => panic!("Expected Required with RouteRequirement"),
        }
    }

    #[test]
    fn different_methods_resolve_independently() {
        let mut route_matchers = HashMap::new();

        // GET /users is secured
        let mut get_matcher = RouteMatcher::new();
        let req = RouteRequirement {
            resource: "users".to_owned(),
            action: "read".to_owned(),
        };
        get_matcher
            .insert("/user-management/v1/users", req)
            .unwrap();
        route_matchers.insert(Method::GET, get_matcher);

        // POST /users is not in matchers
        let policy = build_test_policy(route_matchers, HashMap::new(), false);

        // GET should be secured
        let get_result = policy.resolve(&Method::GET, "/user-management/v1/users");
        assert!(matches!(get_result, AuthRequirement::Required(Some(_))));

        // POST should be public (no requirement, require_auth_by_default=false)
        let post_result = policy.resolve(&Method::POST, "/user-management/v1/users");
        assert_eq!(post_result, AuthRequirement::None);
    }
}
