use async_trait::async_trait;
use modkit_db::secure::DbConn;
use modkit_security::AccessScope;
use uuid::Uuid;

use crate::domain::error::DomainError;
use crate::infra::db::entity::message_reaction::Model as ReactionModel;

/// Repository trait for reaction persistence operations.
///
/// Unlike other repos, `ReactionRepository` takes a concrete `DbConn` instead
/// of generic `DBRunner` so the trait stays dyn-compatible (reactions are stored
/// as `Arc<dyn ReactionRepository>`). Access-control is enforced by the caller
/// at the parent-chat level before invoking these methods.
#[async_trait]
pub trait ReactionRepository: Send + Sync {
    /// Upsert a reaction for (message_id, user_id). Returns the model.
    async fn upsert(
        &self,
        runner: &DbConn<'_>,
        scope: &AccessScope,
        id: Uuid,
        tenant_id: Uuid,
        message_id: Uuid,
        user_id: Uuid,
        reaction: &str,
    ) -> Result<ReactionModel, DomainError>;

    /// Delete reaction for (message_id, user_id). Returns true if deleted.
    async fn delete_by_message_and_user(
        &self,
        runner: &DbConn<'_>,
        scope: &AccessScope,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, DomainError>;
}
