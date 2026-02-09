//! Domain layer for the AuthZ resolver gateway.

pub mod error;
pub mod local_client;
pub mod service;

pub use error::DomainError;
pub use local_client::AuthZResolverGwLocalClient;
pub use service::Service;
