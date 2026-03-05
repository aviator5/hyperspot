use async_trait::async_trait;
use modkit_db::secure::{DbConn, SecureDeleteExt, SecureEntityExt, secure_insert};
use modkit_security::AccessScope;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Set};
use time::OffsetDateTime;

use crate::domain::error::DomainError;
use crate::domain::repos::UpsertReactionParams;
use crate::infra::db::entity::message_reaction::{
    ActiveModel, Column, Entity as ReactionEntity, Model as ReactionModel,
};

pub struct ReactionRepository;

#[async_trait]
impl crate::domain::repos::ReactionRepository for ReactionRepository {
    async fn upsert(
        &self,
        runner: &DbConn<'_>,
        scope: &AccessScope,
        params: UpsertReactionParams,
    ) -> Result<ReactionModel, DomainError> {
        let now = OffsetDateTime::now_utc();

        // Find existing
        let existing = ReactionEntity::find()
            .filter(
                Condition::all()
                    .add(Column::MessageId.eq(params.message_id))
                    .add(Column::UserId.eq(params.user_id)),
            )
            .secure()
            .scope_with(scope)
            .one(runner)
            .await
            .map_err(|e| DomainError::database(e.to_string()))?;

        if let Some(existing) = existing {
            // Delete + re-insert to update (secure ORM doesn't have single-entity update).
            ReactionEntity::delete_many()
                .filter(Column::Id.eq(existing.id))
                .secure()
                .scope_with(scope)
                .exec(runner)
                .await
                .map_err(|e| DomainError::database(e.to_string()))?;

            let am = ActiveModel {
                id: Set(existing.id),
                tenant_id: Set(existing.tenant_id),
                message_id: Set(existing.message_id),
                user_id: Set(existing.user_id),
                reaction: Set(params.reaction),
                created_at: Set(now),
            };
            Ok(secure_insert::<ReactionEntity>(am, scope, runner).await?)
        } else {
            let am = ActiveModel {
                id: Set(params.id),
                tenant_id: Set(params.tenant_id),
                message_id: Set(params.message_id),
                user_id: Set(params.user_id),
                reaction: Set(params.reaction),
                created_at: Set(now),
            };
            Ok(secure_insert::<ReactionEntity>(am, scope, runner).await?)
        }
    }

    async fn delete_by_message_and_user(
        &self,
        runner: &DbConn<'_>,
        scope: &AccessScope,
        message_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> Result<bool, DomainError> {
        let result = ReactionEntity::delete_many()
            .filter(
                Condition::all()
                    .add(Column::MessageId.eq(message_id))
                    .add(Column::UserId.eq(user_id)),
            )
            .secure()
            .scope_with(scope)
            .exec(runner)
            .await
            .map_err(|e| DomainError::database(e.to_string()))?;

        Ok(result.rows_affected > 0)
    }
}
