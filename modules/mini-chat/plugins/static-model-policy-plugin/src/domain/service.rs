use mini_chat_sdk::ModelCatalogEntry;
use modkit_macros::domain_model;

/// Service holding the model catalog loaded from configuration.
#[domain_model]
pub struct Service {
    pub catalog: Vec<ModelCatalogEntry>,
}

impl Service {
    /// Create a service with the given model catalog.
    #[must_use]
    pub fn new(catalog: Vec<ModelCatalogEntry>) -> Self {
        Self { catalog }
    }
}
