//! AuthZ Resolver SDK
//!
//! This crate provides the public API for the `authz_resolver` module:
//!
//! - [`AuthZResolverGatewayClient`] - Public API trait for consumers
//! - [`AuthZResolverPluginClient`] - Plugin API trait for implementations
//! - [`EvaluationRequest`], [`EvaluationResponse`] - Evaluation models
//! - [`Constraint`], [`Predicate`] - Constraint types
//! - [`AuthZResolverError`] - Error types
//! - [`AuthZResolverPluginSpecV1`] - GTS schema for plugin discovery
//! - [`pep`] - PEP helpers (compiler, request builder)
//!
//! ## Usage
//!
//! ```ignore
//! use authz_resolver_sdk::{AuthZResolverGatewayClient, pep};
//!
//! // Get the client from ClientHub
//! let authz = hub.get::<dyn AuthZResolverGatewayClient>()?;
//!
//! // Build evaluation request
//! let request = pep::build_evaluation_request(&ctx, "list", "users_info.user", None, true);
//!
//! // Evaluate
//! let response = authz.evaluate(request).await?;
//!
//! // Compile to AccessScope
//! let scope = pep::compile_to_access_scope(&response, true)?;
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
    Action, Context, EvaluationRequest, EvaluationResponse, Resource, Subject, TenantContext,
};
pub use plugin_api::AuthZResolverPluginClient;
