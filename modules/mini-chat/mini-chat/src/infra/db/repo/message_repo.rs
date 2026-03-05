use async_trait::async_trait;
use modkit_db::odata::{LimitCfg, paginate_odata};
use modkit_db::secure::{DBRunner, SecureEntityExt, secure_insert};
use modkit_odata::{ODataQuery, Page, SortDir};
use modkit_security::AccessScope;
use sea_orm::{ColumnTrait, Condition, EntityTrait, Order, QueryFilter, Set};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::error::DomainError;
use crate::domain::repos::{InsertAssistantMessageParams, InsertUserMessageParams};
use crate::infra::db::entity::message::{
    ActiveModel, Column, Entity as MessageEntity, MessageRole, Model as MessageModel,
};
use crate::infra::db::odata_mapper::{MessageField, MessageODataMapper};

pub struct MessageRepository {
    limit_cfg: LimitCfg,
}

impl MessageRepository {
    #[must_use]
    pub fn new(limit_cfg: LimitCfg) -> Self {
        Self { limit_cfg }
    }
}

#[async_trait]
impl crate::domain::repos::MessageRepository for MessageRepository {
    async fn insert_user_message<C: DBRunner>(
        &self,
        runner: &C,
        scope: &AccessScope,
        params: InsertUserMessageParams,
    ) -> Result<MessageModel, DomainError> {
        let now = OffsetDateTime::now_utc();
        let am = ActiveModel {
            id: Set(params.id),
            tenant_id: Set(params.tenant_id),
            chat_id: Set(params.chat_id),
            request_id: Set(Some(params.request_id)),
            role: Set(MessageRole::User),
            content: Set(params.content),
            content_type: Set("text".to_owned()),
            token_estimate: Set(0),
            provider_response_id: Set(None),
            request_kind: Set(Some("chat".to_owned())),
            features_used: Set(serde_json::json!([])),
            input_tokens: Set(0),
            output_tokens: Set(0),
            model: Set(None),
            is_compressed: Set(false),
            created_at: Set(now),
            deleted_at: Set(None),
        };
        Ok(secure_insert::<MessageEntity>(am, scope, runner).await?)
    }

    async fn insert_assistant_message<C: DBRunner>(
        &self,
        runner: &C,
        scope: &AccessScope,
        params: InsertAssistantMessageParams,
    ) -> Result<MessageModel, DomainError> {
        let now = OffsetDateTime::now_utc();
        let am = ActiveModel {
            id: Set(params.id),
            tenant_id: Set(params.tenant_id),
            chat_id: Set(params.chat_id),
            request_id: Set(Some(params.request_id)),
            role: Set(MessageRole::Assistant),
            content: Set(params.content),
            content_type: Set("text".to_owned()),
            token_estimate: Set(0),
            provider_response_id: Set(params.provider_response_id),
            request_kind: Set(Some("chat".to_owned())),
            features_used: Set(serde_json::json!([])),
            input_tokens: Set(params.input_tokens.unwrap_or(0)),
            output_tokens: Set(params.output_tokens.unwrap_or(0)),
            model: Set(params.model),
            is_compressed: Set(false),
            created_at: Set(now),
            deleted_at: Set(None),
        };
        Ok(secure_insert::<MessageEntity>(am, scope, runner).await?)
    }

    async fn find_by_chat_and_request_id<C: DBRunner>(
        &self,
        runner: &C,
        scope: &AccessScope,
        chat_id: Uuid,
        request_id: Uuid,
    ) -> Result<Vec<MessageModel>, DomainError> {
        Ok(MessageEntity::find()
            .filter(
                Condition::all()
                    .add(Column::ChatId.eq(chat_id))
                    .add(Column::RequestId.eq(request_id))
                    .add(Column::DeletedAt.is_null()),
            )
            .secure()
            .scope_with(scope)
            .order_by(Column::CreatedAt, Order::Asc)
            .all(runner)
            .await?)
    }

    async fn list_by_chat<C: DBRunner>(
        &self,
        runner: &C,
        scope: &AccessScope,
        chat_id: Uuid,
        query: &ODataQuery,
    ) -> Result<Page<MessageModel>, DomainError> {
        let base_query = MessageEntity::find()
            .filter(
                Condition::all()
                    .add(Column::ChatId.eq(chat_id))
                    .add(Column::RequestId.is_not_null())
                    .add(Column::DeletedAt.is_null()),
            )
            .secure()
            .scope_with(scope);

        let page = paginate_odata::<MessageField, MessageODataMapper, _, _, _, _>(
            base_query,
            runner,
            query,
            ("created_at", SortDir::Asc),
            self.limit_cfg,
            std::convert::identity,
        )
        .await
        .map_err(|e| DomainError::database(e.to_string()))?;

        Ok(page)
    }
}
