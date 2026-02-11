//! PEP (Policy Enforcement Point) helpers.
//!
//! - [`PolicyEnforcer`] — Per-resource-type PEP object (build → evaluate → compile)
//! - [`compile_to_access_scope`] — Low-level: compile evaluation response into `AccessScope`

pub mod compiler;
pub mod enforcer;

pub use compiler::{ConstraintCompileError, compile_to_access_scope};
pub use enforcer::{AccessRequest, EnforcerError, PolicyEnforcer};
