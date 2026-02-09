//! Domain models for the `AuthZ` resolver module.
//!
//! Based on `AuthZEN` 1.0 evaluation model with constraint extensions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constraints::Constraint;

/// Authorization evaluation request.
///
/// Follows the `AuthZEN` 1.0 model: Subject + Action + Resource + Context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationRequest {
    /// The subject (who is making the request).
    pub subject: Subject,
    /// The action being performed.
    pub action: Action,
    /// The resource being accessed.
    pub resource: Resource,
    /// Additional context for the evaluation.
    pub context: Context,
}

/// The authenticated subject making the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    /// Subject identifier (user ID, service ID).
    pub id: Uuid,
    /// Subject's home tenant.
    pub tenant_id: Option<Uuid>,
    /// Subject type (e.g., "user", "service").
    pub subject_type: Option<String>,
    /// Additional subject properties for policy evaluation.
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// The action being performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action name (e.g., "list", "get", "create", "update", "delete").
    pub name: String,
}

/// The resource being accessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource type identifier (e.g., "`users_info.user`").
    pub resource_type: String,
    /// Specific resource ID (for GET/UPDATE/DELETE on a single resource).
    pub id: Option<Uuid>,
    /// Whether the PDP should return row-level constraints.
    /// - `true` for LIST/GET/UPDATE/DELETE (need scope filtering)
    /// - `false` for CREATE (just need decision)
    pub require_constraints: bool,
}

/// Tenant context for the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantContext {
    /// The context tenant ID (tenant being operated on).
    pub root_id: Uuid,
}

/// Additional evaluation context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Tenant context for multi-tenant scoping.
    pub tenant: Option<TenantContext>,
    /// Token scopes from the `AuthN` result.
    #[serde(default)]
    pub token_scopes: Vec<String>,
    /// Additional context properties for policy evaluation.
    #[serde(default)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// Authorization evaluation response.
///
/// The PDP returns a decision (allow/deny) and optionally constraints
/// that must be compiled into an `AccessScope` by the PEP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResponse {
    /// Whether access is granted.
    pub decision: bool,
    /// Row-level constraints to apply when `decision` is `true`.
    /// Empty when `require_constraints` was `false` or when access is unrestricted.
    /// Multiple constraints are `ORed` (any one matching is sufficient).
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}
