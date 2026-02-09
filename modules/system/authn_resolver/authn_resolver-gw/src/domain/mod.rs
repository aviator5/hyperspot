//! Domain layer for the AuthN resolver gateway.

pub mod error;
pub mod local_client;
pub mod service;

pub use error::DomainError;
pub use local_client::AuthNResolverGwLocalClient;
pub use service::Service;
