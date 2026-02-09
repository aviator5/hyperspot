//! PEP (Policy Enforcement Point) helpers.
//!
//! Convenience functions for modules acting as PEPs:
//! - [`compiler::compile_to_access_scope`] — Compiles evaluation response into AccessScope
//! - [`request_builder::build_evaluation_request`] — Builds EvaluationRequest from SecurityContext

pub mod compiler;
pub mod request_builder;

pub use compiler::{ConstraintCompileError, compile_to_access_scope};
pub use request_builder::build_evaluation_request;
