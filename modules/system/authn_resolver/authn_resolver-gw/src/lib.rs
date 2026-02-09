//! AuthN Resolver Gateway Module
//!
//! This module discovers AuthN resolver plugins via types-registry
//! and routes authentication calls to the selected plugin based on vendor configuration.
//!
//! The gateway provides the `AuthNResolverGatewayClient` trait registered
//! in `ClientHub` for consumption by other modules.
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod config;
pub mod domain;
pub mod module;
