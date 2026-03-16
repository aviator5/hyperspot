//! Gateway Scope Enforcement Middleware
//!
//! Performs coarse-grained early rejection of requests based on token scopes
//! without calling the PDP. This is an optimization for performance-critical routes.
//!
//! See `docs/arch/authorization/DESIGN.md` section "Gateway Scope Enforcement" for details.

use std::sync::Arc;

use axum::response::IntoResponse;
use glob::{MatchOptions, Pattern};

use crate::config::RoutePoliciesConfig;
use crate::middleware::common;
use modkit::api::Problem;
use modkit_security::SecurityContext;

/// Compiled scope enforcement rules for efficient runtime matching.
#[derive(Clone, Debug)]
pub struct ScopeEnforcementRules {
    /// Compiled glob patterns with their required scopes.
    rules: Arc<[CompiledRule]>,
    /// Whether scope enforcement is enabled.
    enabled: bool,
}

/// A single compiled rule: glob pattern + required scopes.
#[derive(Clone, Debug)]
struct CompiledRule {
    pattern: Pattern,
    required_scopes: Vec<String>,
}

impl ScopeEnforcementRules {
    /// Build scope enforcement rules from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any glob pattern is invalid or if any rule has empty `required_scopes`.
    pub fn from_config(config: &RoutePoliciesConfig) -> Result<Self, anyhow::Error> {
        if !config.enabled {
            return Ok(Self {
                rules: Arc::from([]),
                enabled: false,
            });
        }

        let mut rules = Vec::with_capacity(config.rules.len());

        for rule in &config.rules {
            // Validate: empty required_scopes is likely a config mistake
            if rule.required_scopes.is_empty() {
                return Err(anyhow::anyhow!(
                    "Route policy rule for path '{}' has empty required_scopes. \
                     This would match all tokens and is likely a config mistake.",
                    rule.path
                ));
            }

            let pattern = Pattern::new(&rule.path).map_err(|e| {
                anyhow::anyhow!(
                    "Invalid glob pattern '{}' in route_policies: {e}",
                    rule.path
                )
            })?;

            rules.push(CompiledRule {
                pattern,
                required_scopes: rule.required_scopes.clone(),
            });
        }

        tracing::info!(
            rules_count = rules.len(),
            "Route policy enforcement enabled with {} rules",
            rules.len()
        );

        Ok(Self {
            rules: Arc::from(rules),
            enabled: true,
        })
    }

    /// Check if the given path matches any protected route.
    ///
    /// Returns `true` if the path matches at least one scope enforcement rule.
    fn matches_protected_route(&self, path: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let match_opts = MatchOptions {
            require_literal_separator: true,
            ..MatchOptions::default()
        };

        self.rules
            .iter()
            .any(|rule| rule.pattern.matches_with(path, match_opts))
    }

    /// Check if the given path and token scopes satisfy the scope requirements.
    ///
    /// Returns `Ok(())` if access is allowed, or `Err(problem)` if denied.
    #[allow(clippy::result_large_err)]
    fn check(&self, path: &str, token_scopes: &[String]) -> Result<(), Problem> {
        if !self.enabled {
            return Ok(());
        }

        // Only wildcard scope `["*"]` is unrestricted (first-party apps).
        // Empty scopes = no permissions (fail-closed).
        if token_scopes.iter().any(|s| s == "*") {
            return Ok(());
        }

        // Match options: require `/` to be matched literally so `*` doesn't cross path segments
        let match_opts = MatchOptions {
            require_literal_separator: true,
            ..MatchOptions::default()
        };

        // Find matching rules and check scopes
        for rule in self.rules.iter() {
            if rule.pattern.matches_with(path, match_opts) {
                // Check if token has ANY of the required scopes
                let has_required_scope = rule
                    .required_scopes
                    .iter()
                    .any(|required| token_scopes.contains(required));

                if !has_required_scope {
                    tracing::warn!(
                        path = %path,
                        pattern = %rule.pattern,
                        required_scopes = ?rule.required_scopes,
                        token_scopes = ?token_scopes,
                        "Route policy enforcement denied: insufficient scopes"
                    );

                    return Err(Problem::new(
                        axum::http::StatusCode::FORBIDDEN,
                        "Forbidden",
                        "Insufficient token scopes for this resource",
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Scope enforcement middleware state.
#[derive(Clone)]
pub struct ScopeEnforcementState {
    pub rules: ScopeEnforcementRules,
}

/// Scope enforcement middleware.
///
/// Checks if the request's token scopes satisfy the configured requirements
/// for the matched route pattern. Returns 403 Forbidden if scopes are insufficient.
///
/// This middleware MUST run AFTER the auth middleware (which populates `SecurityContext`).
pub async fn scope_enforcement_middleware(
    axum::extract::State(state): axum::extract::State<ScopeEnforcementState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Skip if enforcement is disabled
    if !state.rules.enabled {
        return next.run(req).await;
    }

    // Use the concrete URI path for glob pattern matching (not MatchedPath which
    // returns the route template like "/{*path}" for catch-all routes).
    let path = req.uri().path();
    let path = common::resolve_path(&req, path);

    // Get SecurityContext from request extensions (populated by auth middleware)
    let Some(security_context) = req.extensions().get::<SecurityContext>() else {
        // No SecurityContext means auth middleware didn't run or request is unauthenticated.
        // If the path matches a protected route, reject with 401 Unauthorized.
        // Otherwise, let it pass through for public/unprotected routes.
        if state.rules.matches_protected_route(&path) {
            tracing::warn!(
                path = %path,
                "Route policy enforcement denied: no SecurityContext for protected route"
            );
            return Problem::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "Unauthorized",
                "Authentication required for this resource",
            )
            .into_response();
        }
        return next.run(req).await;
    };

    // Check scopes
    if let Err(problem) = state.rules.check(&path, security_context.token_scopes()) {
        return problem.into_response();
    }

    next.run(req).await
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::config::RoutePolicyRule;

    fn build_config(enabled: bool, routes: Vec<(&str, Vec<&str>)>) -> RoutePoliciesConfig {
        let rules = routes
            .into_iter()
            .map(|(path, scopes)| RoutePolicyRule {
                path: path.to_owned(),
                required_scopes: scopes.into_iter().map(String::from).collect(),
            })
            .collect();

        RoutePoliciesConfig { enabled, rules }
    }

    #[test]
    fn disabled_enforcement_always_passes() {
        let config = build_config(false, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Even with no scopes, should pass when disabled
        assert!(rules.check("/admin/users", &[]).is_ok());
    }

    #[test]
    fn first_party_app_always_passes() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // First-party apps have ["*"] scope
        let scopes = vec!["*".to_owned()];
        assert!(rules.check("/admin/users", &scopes).is_ok());
    }

    #[test]
    fn matching_scope_passes() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        let scopes = vec!["admin".to_owned()];
        assert!(rules.check("/admin/users", &scopes).is_ok());
    }

    #[test]
    fn any_of_required_scopes_passes() {
        let config = build_config(
            true,
            vec![("/events/v1/*", vec!["read:events", "write:events"])],
        );
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Having just one of the required scopes should pass
        let scopes = vec!["read:events".to_owned()];
        assert!(rules.check("/events/v1/list", &scopes).is_ok());

        let scopes = vec!["write:events".to_owned()];
        assert!(rules.check("/events/v1/create", &scopes).is_ok());
    }

    #[test]
    fn missing_scope_returns_forbidden() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // No matching scope
        let scopes = vec!["read:events".to_owned()];
        let result = rules.check("/admin/users", &scopes);
        assert!(result.is_err());

        let problem = result.unwrap_err();
        assert_eq!(problem.status, axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn empty_scopes_returns_forbidden() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Empty scopes = no permissions (fail-closed)
        let result = rules.check("/admin/users", &[]);
        assert!(result.is_err());

        let problem = result.unwrap_err();
        assert_eq!(problem.status, axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn unmatched_route_passes() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Route doesn't match any pattern, should pass even with unrelated scope
        let scopes = vec!["unrelated:scope".to_owned()];
        assert!(rules.check("/public/health", &scopes).is_ok());
    }

    #[test]
    fn glob_single_star_matches_single_segment_only() {
        let config = build_config(true, vec![("/api/*/items", vec!["api:read"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        let scopes = vec!["api:read".to_owned()];

        // Single * matches exactly one path segment (doesn't cross `/`)
        assert!(rules.check("/api/v1/items", &scopes).is_ok());
        assert!(rules.check("/api/v2/items", &scopes).is_ok());

        // Multiple segments do NOT match single * pattern (no scope check triggered)
        let unrelated_scopes = vec!["unrelated:scope".to_owned()];
        assert!(
            rules
                .check("/api/v1/nested/items", &unrelated_scopes)
                .is_ok()
        ); // doesn't match pattern
    }

    #[test]
    fn glob_double_star_matches_multiple_segments() {
        let config = build_config(true, vec![("/api/**", vec!["api:access"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        let scopes = vec!["api:access".to_owned()];

        // ** matches any number of path segments
        assert!(rules.check("/api/v1", &scopes).is_ok());
        assert!(rules.check("/api/v1/items", &scopes).is_ok());
        assert!(rules.check("/api/v1/items/123/details", &scopes).is_ok());
    }

    #[test]
    fn invalid_glob_pattern_returns_error() {
        let config = build_config(true, vec![("/admin/[invalid", vec!["admin"])]);
        let result = ScopeEnforcementRules::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn empty_required_scopes_returns_error() {
        let config = build_config(true, vec![("/admin/*", vec![])]);
        let result = ScopeEnforcementRules::from_config(&config);
        let err = result.expect_err("should fail with empty required_scopes");
        assert!(
            err.to_string().contains("empty required_scopes"),
            "error should mention empty required_scopes: {err}"
        );
    }

    #[test]
    fn multiple_rules_all_checked() {
        let config = build_config(
            true,
            vec![
                ("/admin/*", vec!["admin"]),
                ("/events/**", vec!["events:read"]),
            ],
        );
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Admin route needs admin scope
        let admin_scopes = vec!["admin".to_owned()];
        assert!(rules.check("/admin/users", &admin_scopes).is_ok());

        // Events route needs events:read scope
        let events_scopes = vec!["events:read".to_owned()];
        assert!(rules.check("/events/v1/list", &events_scopes).is_ok());

        // Wrong scope for admin route
        assert!(rules.check("/admin/users", &events_scopes).is_err());

        // Wrong scope for events route
        assert!(rules.check("/events/v1/list", &admin_scopes).is_err());
    }

    #[test]
    fn matches_protected_route_returns_true_for_matching_path() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        assert!(rules.matches_protected_route("/admin/users"));
        assert!(rules.matches_protected_route("/admin/settings"));
    }

    #[test]
    fn matches_protected_route_returns_false_for_non_matching_path() {
        let config = build_config(true, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        assert!(!rules.matches_protected_route("/public/health"));
        assert!(!rules.matches_protected_route("/api/v1/users"));
    }

    #[test]
    fn matches_protected_route_returns_false_when_disabled() {
        let config = build_config(false, vec![("/admin/*", vec!["admin"])]);
        let rules = ScopeEnforcementRules::from_config(&config).unwrap();

        // Even matching paths return false when enforcement is disabled
        assert!(!rules.matches_protected_route("/admin/users"));
    }
}
