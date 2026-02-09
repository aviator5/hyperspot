//! Service implementation for the static AuthN resolver plugin.

use std::collections::HashMap;

use modkit_security::SecurityContext;

use crate::config::{AuthnMode, IdentityConfig, StaticAuthnPluginConfig};
use authn_resolver_sdk::AuthenticationResult;

/// Static AuthN resolver service.
///
/// Provides token-to-identity mapping based on configuration mode:
/// - `accept_all`: Any non-empty token maps to the default identity
/// - `static_tokens`: Specific tokens map to specific identities
pub struct Service {
    mode: AuthnMode,
    default_identity: IdentityConfig,
    token_map: HashMap<String, IdentityConfig>,
}

impl Service {
    /// Create a service from plugin configuration.
    #[must_use]
    pub fn from_config(cfg: &StaticAuthnPluginConfig) -> Self {
        let token_map: HashMap<String, IdentityConfig> = cfg
            .tokens
            .iter()
            .map(|m| (m.token.clone(), m.identity.clone()))
            .collect();

        Self {
            mode: cfg.mode.clone(),
            default_identity: cfg.default_identity.clone(),
            token_map,
        }
    }

    /// Authenticate a bearer token and return the identity.
    ///
    /// Returns `None` if the token is not recognized (in `static_tokens` mode)
    /// or empty.
    pub fn authenticate(&self, bearer_token: &str) -> Option<AuthenticationResult> {
        if bearer_token.is_empty() {
            return None;
        }

        let identity = match &self.mode {
            AuthnMode::AcceptAll => &self.default_identity,
            AuthnMode::StaticTokens => self.token_map.get(bearer_token)?,
        };

        Some(build_result(identity, bearer_token))
    }
}

fn build_result(identity: &IdentityConfig, bearer_token: &str) -> AuthenticationResult {
    let tenant_id = identity.tenant_id.unwrap_or(identity.subject_tenant_id);

    let ctx = SecurityContext::builder()
        .tenant_id(tenant_id)
        .subject_id(identity.subject_id)
        .subject_tenant_id(identity.subject_tenant_id)
        .token_scopes(identity.token_scopes.clone())
        .bearer_token(bearer_token.to_owned())
        .build();

    AuthenticationResult {
        security_context: ctx,
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::config::TokenMapping;
    use uuid::Uuid;

    fn default_config() -> StaticAuthnPluginConfig {
        StaticAuthnPluginConfig::default()
    }

    #[test]
    fn accept_all_mode_returns_default_identity() {
        let service = Service::from_config(&default_config());

        let result = service.authenticate("any-token-value");
        assert!(result.is_some());

        let auth = result.unwrap();
        let ctx = &auth.security_context;
        assert_eq!(
            ctx.subject_id(),
            modkit_security::constants::DEFAULT_SUBJECT_ID
        );
        assert_eq!(
            ctx.subject_tenant_id(),
            Some(modkit_security::constants::DEFAULT_TENANT_ID)
        );
        assert_eq!(ctx.token_scopes(), &["*"]);
        assert_eq!(ctx.bearer_token(), Some("any-token-value"));
    }

    #[test]
    fn accept_all_mode_rejects_empty_token() {
        let service = Service::from_config(&default_config());

        let result = service.authenticate("");
        assert!(result.is_none());
    }

    #[test]
    fn static_tokens_mode_returns_mapped_identity() {
        let user_a_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let tenant_a = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let cfg = StaticAuthnPluginConfig {
            mode: AuthnMode::StaticTokens,
            tokens: vec![TokenMapping {
                token: "token-user-a".to_owned(),
                identity: IdentityConfig {
                    subject_id: user_a_id,
                    subject_tenant_id: tenant_a,
                    tenant_id: None,
                    token_scopes: vec!["read:data".to_owned()],
                },
            }],
            ..default_config()
        };

        let service = Service::from_config(&cfg);

        let result = service.authenticate("token-user-a");
        assert!(result.is_some());

        let auth = result.unwrap();
        let ctx = &auth.security_context;
        assert_eq!(ctx.subject_id(), user_a_id);
        assert_eq!(ctx.subject_tenant_id(), Some(tenant_a));
        assert_eq!(ctx.tenant_id(), tenant_a);
        assert_eq!(ctx.token_scopes(), &["read:data"]);
        assert_eq!(ctx.bearer_token(), Some("token-user-a"));
    }

    #[test]
    fn static_tokens_mode_rejects_unknown_token() {
        let cfg = StaticAuthnPluginConfig {
            mode: AuthnMode::StaticTokens,
            tokens: vec![TokenMapping {
                token: "known-token".to_owned(),
                identity: IdentityConfig::default(),
            }],
            ..default_config()
        };

        let service = Service::from_config(&cfg);

        let result = service.authenticate("unknown-token");
        assert!(result.is_none());
    }

    #[test]
    fn static_tokens_mode_rejects_empty_token() {
        let cfg = StaticAuthnPluginConfig {
            mode: AuthnMode::StaticTokens,
            tokens: vec![],
            ..default_config()
        };

        let service = Service::from_config(&cfg);

        let result = service.authenticate("");
        assert!(result.is_none());
    }

    #[test]
    fn custom_tenant_id_in_identity() {
        let subject_tenant = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap();
        let context_tenant = Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000000").unwrap();

        let cfg = StaticAuthnPluginConfig {
            default_identity: IdentityConfig {
                subject_tenant_id: subject_tenant,
                tenant_id: Some(context_tenant),
                ..IdentityConfig::default()
            },
            ..default_config()
        };

        let service = Service::from_config(&cfg);

        let result = service.authenticate("test").unwrap();
        let ctx = &result.security_context;
        assert_eq!(ctx.tenant_id(), context_tenant);
        assert_eq!(ctx.subject_tenant_id(), Some(subject_tenant));
    }
}
