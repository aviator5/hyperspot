use std::sync::Arc;

use authz_resolver_sdk::PolicyEnforcer;
use modkit_macros::domain_model;

use crate::domain::repos::{ChatRepository, ReactionRepository};

use super::DbProvider;

/// Service handling message reaction operations.
#[domain_model]
pub struct ReactionService<CR: ChatRepository> {
    _db: Arc<DbProvider>,
    _reaction_repo: Arc<dyn ReactionRepository>,
    _chat_repo: Arc<CR>,
    _enforcer: PolicyEnforcer,
}

impl<CR: ChatRepository> ReactionService<CR> {
    pub(crate) fn new(
        db: Arc<DbProvider>,
        reaction_repo: Arc<dyn ReactionRepository>,
        chat_repo: Arc<CR>,
        enforcer: PolicyEnforcer,
    ) -> Self {
        Self {
            _db: db,
            _reaction_repo: reaction_repo,
            _chat_repo: chat_repo,
            _enforcer: enforcer,
        }
    }
}
