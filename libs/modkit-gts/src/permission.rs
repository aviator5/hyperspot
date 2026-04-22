//! Base GTS type for Cyber Fabric authorization permissions.
//!
//! Permissions are declared by modules as **well-known GTS instances** of
//! this base type and registered with `types-registry` during module init
//! (preferably at compile time via the `gts_instance!` macro). The future
//! `AuthZ` Management module / admin UI lists permissions by querying
//! `types-registry` for instances of `gts.cf.modkit.authz.permission.v1~`.
//!
//! ## `resource_type` semantics
//!
//! The `resource_type` field accepts a GTS expression:
//!
//! - **Concrete GTS type ID** — `gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.v1~`
//! - **Wildcard pattern** (GTS §3.5) — `gts.cf.core.am.tenant.*`
//! - **Query Language predicates** (GTS §3.3) — `gts.cf.core.ai_chat.chat.v1~[category='support']`
//!
//! Attribute Selector (GTS §3.4, `@path.nested`) is NOT accepted; it is for
//! single-value reads, not set expressions.
//!
//! ## Well-known instance ID convention
//!
//! ```text
//! gts.cf.modkit.authz.permission.v1~<vendor>.<package>.<namespace>.<permission_name>.v1
//! ```
//!
//! The right-hand segment encodes the declaring module's ownership
//! (`<vendor>.<package>.<namespace>`) — use `_` as a placeholder when a slot
//! has no meaningful value — and an internal handle for the permission
//! (`<permission_name>`). Examples:
//!
//! - `gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_create.v1`
//! - `gts.cf.modkit.authz.permission.v1~cf.am._.tenant_create.v1`
//!
//! ## Extending with per-permission metadata
//!
//! If a module needs ABAC-style per-permission attributes (audit category,
//! MFA requirement, risk class, …), it declares a derived schema with
//! `#[gts_macros::struct_to_gts_schema(base = AuthzPermissionV1, ...)]` and
//! registers instances against that derived schema. This path is reserved
//! for concrete consumers with real need — YAGNI governs today's shape.

use crate::gts_schema;
use gts::GtsInstanceId;

/// Base GTS type for authorization permissions.
///
/// Permissions are well-known GTS instances of this type; declaring modules
/// register them via the `gts_instance!` macro (preferred, compile-time) or
/// `TypesRegistryClient::register` (runtime).
///
/// GTS ID: `gts.cf.modkit.authz.permission.v1~`
#[gts_schema(
    schema_id = "gts.cf.modkit.authz.permission.v1~",
    description = "Cyber Fabric authorization permission",
    properties = "id,resource_type,action,display_name"
)]
pub struct AuthzPermissionV1 {
    /// Full GTS instance ID of this permission (e.g.
    /// `gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_read.v1`).
    pub id: GtsInstanceId,
    /// GTS expression identifying the set of resources this permission
    /// applies to. Accepts concrete IDs, wildcard patterns (GTS §3.5), or
    /// Query Language predicates (GTS §3.3).
    pub resource_type: String,
    /// Concrete action name (lowercase `snake_case`). No wildcard, no list.
    /// Examples: `create`, `read`, `list`, `retry_turn`, `upload_attachment`.
    pub action: String,
    /// Human-readable label for admin UIs. Examples: "Create tenant",
    /// "Retry chat turn".
    pub display_name: String,
}
