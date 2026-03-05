use std::sync::Arc;

use async_trait::async_trait;
use authz_resolver_sdk::{
    AuthZResolverClient, AuthZResolverError, PolicyEnforcer,
    constraints::{Constraint, EqPredicate, Predicate},
    models::{DenyReason, EvaluationRequest, EvaluationResponse, EvaluationResponseContext},
};
use modkit_odata::ODataQuery;
use modkit_security::{AccessScope, pep_properties};
use uuid::Uuid;

use crate::domain::error::DomainError;
use crate::domain::models::NewChat;

use crate::domain::repos::{
    InsertAssistantMessageParams, InsertUserMessageParams, MessageRepository as MessageRepoTrait,
};
use crate::domain::service::test_helpers::{
    inmem_db, mock_db_provider, mock_enforcer, mock_model_resolver, mock_thread_summary_repo,
    test_security_ctx,
};
use crate::infra::db::repo::chat_repo::ChatRepository as OrmChatRepository;
use crate::infra::db::repo::message_repo::MessageRepository as OrmMessageRepository;

use super::MessageService;
use crate::domain::service::ChatService;

// ── Test Helpers ──

/// Mock AuthZ resolver that returns only `owner_tenant_id` constraints
/// (no `owner_id`). This is needed because message entities use `no_owner`,
/// so scopes containing `owner_id` filters would fail-closed to deny-all
/// during query resolution.
struct TenantOnlyAuthZResolver;

#[async_trait]
impl AuthZResolverClient for TenantOnlyAuthZResolver {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        let subject_tenant_id = request
            .subject
            .properties
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        // Deny when resource tenant_id differs from subject tenant_id
        if let Some(res_tenant) = request
            .resource
            .properties
            .get(pep_properties::OWNER_TENANT_ID)
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            && subject_tenant_id.is_some_and(|st| st != res_tenant)
        {
            return Ok(EvaluationResponse {
                decision: false,
                context: EvaluationResponseContext {
                    deny_reason: Some(DenyReason {
                        error_code: "tenant_mismatch".to_owned(),
                        details: Some("subject tenant does not match resource tenant".to_owned()),
                    }),
                    ..Default::default()
                },
            });
        }

        if request.context.require_constraints {
            let mut predicates = Vec::new();
            if let Some(tid) = subject_tenant_id {
                predicates.push(Predicate::Eq(EqPredicate::new(
                    pep_properties::OWNER_TENANT_ID,
                    tid,
                )));
            }
            let constraints = vec![Constraint { predicates }];
            Ok(EvaluationResponse {
                decision: true,
                context: EvaluationResponseContext {
                    constraints,
                    ..Default::default()
                },
            })
        } else {
            Ok(EvaluationResponse {
                decision: true,
                context: EvaluationResponseContext::default(),
            })
        }
    }
}

fn tenant_only_enforcer() -> PolicyEnforcer {
    let authz: Arc<dyn AuthZResolverClient> = Arc::new(TenantOnlyAuthZResolver);
    PolicyEnforcer::new(authz)
}

fn limit_cfg() -> modkit_db::odata::LimitCfg {
    modkit_db::odata::LimitCfg {
        default: 20,
        max: 100,
    }
}

fn build_chat_service(
    db_provider: Arc<crate::domain::service::DbProvider>,
    chat_repo: Arc<OrmChatRepository>,
) -> ChatService<OrmChatRepository> {
    ChatService::new(
        db_provider,
        chat_repo,
        mock_thread_summary_repo(),
        mock_enforcer(),
        mock_model_resolver(),
    )
}

fn build_message_service(
    db_provider: Arc<crate::domain::service::DbProvider>,
    chat_repo: Arc<OrmChatRepository>,
) -> MessageService<OrmMessageRepository, OrmChatRepository> {
    let message_repo = Arc::new(OrmMessageRepository::new(limit_cfg()));
    MessageService::new(db_provider, message_repo, chat_repo, tenant_only_enforcer())
}

// ── Tests ──

#[tokio::test]
async fn list_messages_empty_chat() {
    let db = inmem_db().await;
    let db_provider = mock_db_provider(db);
    let chat_repo = Arc::new(OrmChatRepository::new(limit_cfg()));

    let chat_svc = build_chat_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));
    let msg_svc = build_message_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));

    let tenant_id = Uuid::new_v4();
    let ctx = test_security_ctx(tenant_id);

    let chat = chat_svc
        .create_chat(
            &ctx,
            NewChat {
                model: String::new(),
                title: Some("Empty chat".to_owned()),
                is_temporary: false,
            },
        )
        .await
        .expect("create_chat failed");

    let page = msg_svc
        .list_messages(&ctx, chat.id, &ODataQuery::default())
        .await
        .expect("list_messages failed");

    assert!(page.items.is_empty(), "Expected no messages in new chat");
}

#[tokio::test]
async fn list_messages_returns_messages_chronologically() {
    let db = inmem_db().await;
    let db_provider = mock_db_provider(db);
    let chat_repo = Arc::new(OrmChatRepository::new(limit_cfg()));

    let chat_svc = build_chat_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));
    let msg_svc = build_message_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));

    let tenant_id = Uuid::new_v4();
    let ctx = test_security_ctx(tenant_id);

    let chat = chat_svc
        .create_chat(
            &ctx,
            NewChat {
                model: String::new(),
                title: Some("With messages".to_owned()),
                is_temporary: false,
            },
        )
        .await
        .expect("create_chat failed");

    // Insert messages via the repo directly using tenant-scoped access
    let scope = AccessScope::for_tenant(tenant_id);
    let conn = db_provider.conn().expect("conn failed");
    let message_repo = OrmMessageRepository::new(limit_cfg());

    let request_id = Uuid::new_v4();

    message_repo
        .insert_user_message(
            &conn,
            &scope,
            InsertUserMessageParams {
                id: Uuid::now_v7(),
                tenant_id,
                chat_id: chat.id,
                request_id,
                content: "Hello".to_owned(),
            },
        )
        .await
        .expect("insert_user_message failed");

    message_repo
        .insert_assistant_message(
            &conn,
            &scope,
            InsertAssistantMessageParams {
                id: Uuid::now_v7(),
                tenant_id,
                chat_id: chat.id,
                request_id,
                content: "Hi there!".to_owned(),
                input_tokens: Some(10),
                output_tokens: Some(20),
                model: Some("gpt-5.2".to_owned()),
                provider_response_id: None,
            },
        )
        .await
        .expect("insert_assistant_message failed");

    let page = msg_svc
        .list_messages(&ctx, chat.id, &ODataQuery::default())
        .await
        .expect("list_messages failed");

    assert_eq!(page.items.len(), 2, "Expected 2 messages");
    assert_eq!(page.items[0].role, "user", "First message should be user");
    assert_eq!(
        page.items[1].role, "assistant",
        "Second message should be assistant"
    );
    assert!(
        page.items[0].created_at <= page.items[1].created_at,
        "Messages should be in chronological order"
    );
}

#[tokio::test]
async fn list_messages_chat_not_found() {
    let db = inmem_db().await;
    let db_provider = mock_db_provider(db);
    let chat_repo = Arc::new(OrmChatRepository::new(limit_cfg()));

    let msg_svc = build_message_service(db_provider, chat_repo);

    let ctx = test_security_ctx(Uuid::new_v4());
    let random_chat_id = Uuid::new_v4();

    let result = msg_svc
        .list_messages(&ctx, random_chat_id, &ODataQuery::default())
        .await;

    assert!(result.is_err(), "Expected error for non-existent chat");
    assert!(
        matches!(result.unwrap_err(), DomainError::ChatNotFound { .. }),
        "Expected ChatNotFound"
    );
}

#[tokio::test]
async fn list_messages_cross_tenant_returns_not_found() {
    let db = inmem_db().await;
    let db_provider = mock_db_provider(db);
    let chat_repo = Arc::new(OrmChatRepository::new(limit_cfg()));

    let chat_svc = build_chat_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));
    let msg_svc = build_message_service(Arc::clone(&db_provider), Arc::clone(&chat_repo));

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let ctx_a = test_security_ctx(tenant_a);
    let ctx_b = test_security_ctx(tenant_b);

    // Tenant A creates a chat
    let chat = chat_svc
        .create_chat(
            &ctx_a,
            NewChat {
                model: String::new(),
                title: Some("Tenant A chat".to_owned()),
                is_temporary: false,
            },
        )
        .await
        .expect("create_chat failed");

    // Tenant B tries to list messages in Tenant A's chat
    let result = msg_svc
        .list_messages(&ctx_b, chat.id, &ODataQuery::default())
        .await;

    assert!(result.is_err(), "Cross-tenant list must fail");
    assert!(
        matches!(result.unwrap_err(), DomainError::ChatNotFound { .. }),
        "Expected ChatNotFound for cross-tenant access"
    );
}
