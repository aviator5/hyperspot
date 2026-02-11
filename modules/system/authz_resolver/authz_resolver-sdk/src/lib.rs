//! `AuthZ` Resolver SDK
//!
//! This crate provides the public API for the `authz_resolver` module:
//!
//! - [`AuthZResolverGatewayClient`] - Public API trait for consumers
//! - [`AuthZResolverPluginClient`] - Plugin API trait for implementations
//! - [`EvaluationRequest`], [`EvaluationResponse`] - Evaluation models
//! - [`Constraint`], [`Predicate`] - Constraint types
//! - [`AuthZResolverError`] - Error types
//! - [`AuthZResolverPluginSpecV1`] - GTS schema for plugin discovery
//! - [`pep`] - PEP helpers ([`PolicyEnforcer`], [`AccessRequest`], compiler)
//!
//! ## Usage
//!
//! ```ignore
//! use authz_resolver_sdk::{AuthZResolverGatewayClient, pep::{AccessRequest, PolicyEnforcer}};
//!
//! // Get the client from ClientHub
//! let authz = hub.get::<dyn AuthZResolverGatewayClient>()?;
//!
//! // Create a per-resource-type enforcer (once, during init)
//! let enforcer = PolicyEnforcer::new("users_info.user", authz);
//!
//! // Simple case — full PEP flow in one call
//! let scope = enforcer.access_scope(&ctx, "get", Some(id), true).await?;
//!
//! // Advanced case — with per-request overrides
//! let scope = enforcer.access_scope_with(
//!     &ctx, "create", None, false,
//!     &AccessRequest::new()
//!         .context_tenant_id(target_tenant_id)
//!         .resource_property("owner_tenant_id", json!(target_tenant_id.to_string())),
//! ).await?;
//! ```

pub mod api;
pub mod constraints;
pub mod error;
pub mod gts;
pub mod models;
pub mod pep;
pub mod plugin_api;

// Re-export main types at crate root
pub use api::AuthZResolverGatewayClient;
pub use constraints::{Constraint, EqPredicate, InPredicate, Predicate};
pub use error::AuthZResolverError;
pub use gts::AuthZResolverPluginSpecV1;
pub use models::{
    Action, BarrierMode, Capability, Context, DenyReason, EvaluationRequest, EvaluationResponse,
    Resource, Subject, TenantContext, TenantMode,
};
pub use pep::{AccessRequest, EnforcerError, PolicyEnforcer};
pub use plugin_api::AuthZResolverPluginClient;
