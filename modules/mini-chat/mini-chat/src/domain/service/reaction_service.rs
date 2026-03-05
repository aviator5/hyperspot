use std::sync::Arc;

use authz_resolver_sdk::PolicyEnforcer;
use modkit_db::secure::SecureEntityExt;
use modkit_macros::domain_model;
use modkit_security::SecurityContext;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter};
use tracing::instrument;
use uuid::Uuid;

use crate::domain::error::DomainError;
use crate::domain::models::{Reaction, ReactionDeleted};
use crate::domain::repos::{ChatRepository, MessageRepository, ReactionRepository};
use crate::infra::db::entity::message::{
    Column as MsgCol, Entity as MsgEntity, MessageRole,
};

use super::{DbProvider, actions, resources};

/// Service handling message reaction operations.
#[domain_model]
pub struct ReactionService<MR: MessageRepository, CR: ChatRepository> {
    db: Arc<DbProvider>,
    reaction_repo: Arc<dyn ReactionRepository>,
    _message_repo: Arc<MR>,
    chat_repo: Arc<CR>,
    enforcer: PolicyEnforcer,
}

impl<MR: MessageRepository, CR: ChatRepository> ReactionService<MR, CR> {
    pub(crate) fn new(
        db: Arc<DbProvider>,
        reaction_repo: Arc<dyn ReactionRepository>,
        _message_repo: Arc<MR>,
        chat_repo: Arc<CR>,
        enforcer: PolicyEnforcer,
    ) -> Self {
        Self {
            db,
            reaction_repo,
            _message_repo,
            chat_repo,
            enforcer,
        }
    }

    /// Set or update a reaction on an assistant message.
    #[instrument(skip(self, ctx, reaction), fields(chat_id = %chat_id, msg_id = %msg_id))]
    pub async fn set_reaction(
        &self,
        ctx: &SecurityContext,
        chat_id: Uuid,
        msg_id: Uuid,
        reaction: &str,
    ) -> Result<Reaction, DomainError> {
        tracing::debug!("Setting reaction on message");

        // Validate reaction value
        if reaction != "like" && reaction != "dislike" {
            return Err(DomainError::validation(
                "Reaction must be 'like' or 'dislike'",
            ));
        }

        let conn = self.db.conn().map_err(DomainError::from)?;

        let scope = self
            .enforcer
            .access_scope(ctx, &resources::CHAT, actions::REACT, Some(chat_id))
            .await?;

        // Verify chat exists (scoped)
        self.chat_repo
            .get(&conn, &scope, chat_id)
            .await?
            .ok_or_else(|| DomainError::chat_not_found(chat_id))?;

        // Verify message exists in this chat and is an assistant message
        let message = MsgEntity::find()
            .filter(
                Condition::all()
                    .add(MsgCol::Id.eq(msg_id))
                    .add(MsgCol::ChatId.eq(chat_id))
                    .add(MsgCol::DeletedAt.is_null()),
            )
            .secure()
            .scope_with(&scope)
            .one(&conn)
            .await
            .map_err(|e| DomainError::database(e.to_string()))?
            .ok_or_else(|| DomainError::message_not_found(msg_id))?;

        if message.role != MessageRole::Assistant {
            return Err(DomainError::invalid_reaction_target(msg_id));
        }

        let id = Uuid::now_v7();
        let tenant_id = ctx.subject_tenant_id();
        let user_id = ctx.subject_id();

        let model = self
            .reaction_repo
            .upsert(&conn, &scope, id, tenant_id, msg_id, user_id, reaction)
            .await?;

        tracing::debug!("Successfully set reaction");
        Ok(Reaction {
            message_id: model.message_id,
            reaction: model.reaction,
            created_at: model.created_at,
        })
    }

    /// Delete a reaction from a message (idempotent).
    #[instrument(skip(self, ctx), fields(chat_id = %chat_id, msg_id = %msg_id))]
    pub async fn delete_reaction(
        &self,
        ctx: &SecurityContext,
        chat_id: Uuid,
        msg_id: Uuid,
    ) -> Result<ReactionDeleted, DomainError> {
        tracing::debug!("Deleting reaction from message");

        let conn = self.db.conn().map_err(DomainError::from)?;

        let scope = self
            .enforcer
            .access_scope(
                ctx,
                &resources::CHAT,
                actions::DELETE_REACTION,
                Some(chat_id),
            )
            .await?;

        // Verify chat exists (scoped)
        self.chat_repo
            .get(&conn, &scope, chat_id)
            .await?
            .ok_or_else(|| DomainError::chat_not_found(chat_id))?;

        // Verify message exists in this chat
        MsgEntity::find()
            .filter(
                Condition::all()
                    .add(MsgCol::Id.eq(msg_id))
                    .add(MsgCol::ChatId.eq(chat_id))
                    .add(MsgCol::DeletedAt.is_null()),
            )
            .secure()
            .scope_with(&scope)
            .one(&conn)
            .await
            .map_err(|e| DomainError::database(e.to_string()))?
            .ok_or_else(|| DomainError::message_not_found(msg_id))?;

        let user_id = ctx.subject_id();

        self.reaction_repo
            .delete_by_message_and_user(&conn, &scope, msg_id, user_id)
            .await?;

        tracing::debug!("Successfully deleted reaction");
        Ok(ReactionDeleted {
            message_id: msg_id,
            deleted: true,
        })
    }
}

#[cfg(test)]
#[path = "reaction_service_test.rs"]
mod tests;
