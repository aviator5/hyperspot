#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use authz_resolver_sdk::{
    AuthZResolverError, AuthZResolverGatewayClient,
    constraints::{Constraint, InPredicate, Predicate},
    models::{EvaluationRequest, EvaluationResponse},
};
use modkit::config::ConfigProvider;
use modkit::{ClientHub, DatabaseCapability, Module, ModuleCtx};
use modkit_db::migration_runner::run_migrations_for_module;
use modkit_db::{ConnectOpts, DBProvider, Db, DbError, connect_db};
use modkit_security::SecurityContext;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use user_info_sdk::{NewUser, UsersInfoClientV1};
use users_info::UsersInfo;

/// Mock `AuthZ` resolver for tests (`allow_all` mode).
struct MockAuthZResolver;

#[async_trait::async_trait]
impl AuthZResolverGatewayClient for MockAuthZResolver {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        let constraints = if request.resource.require_constraints {
            if let Some(ref tenant_ctx) = request.context.tenant {
                vec![Constraint {
                    predicates: vec![Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![tenant_ctx.root_id],
                    })],
                }]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok(EvaluationResponse {
            decision: true,
            constraints,
        })
    }
}

struct MockConfigProvider {
    modules: HashMap<String, serde_json::Value>,
}

impl MockConfigProvider {
    fn new_users_info_default() -> Self {
        let mut modules = HashMap::new();
        // ModuleCtx::raw_config expects: modules.<name> = { database: ..., config: ... }
        // For this test we supply config only; DB handle is injected directly.
        modules.insert(
            "users_info".to_owned(),
            json!({
                "config": {
                    "default_page_size": 50,
                    "max_page_size": 1000,
                    "audit_base_url": "http://audit.local",
                    "notifications_base_url": "http://notifications.local",
                }
            }),
        );
        Self { modules }
    }
}

impl ConfigProvider for MockConfigProvider {
    fn get_module_config(&self, module_name: &str) -> Option<&serde_json::Value> {
        self.modules.get(module_name)
    }
}

#[tokio::test]
async fn users_info_registers_sdk_client_and_handles_basic_crud() {
    // Arrange: build a real Db for sqlite in-memory, run module migrations, then init module.
    let db: Db = connect_db(
        "sqlite::memory:",
        ConnectOpts {
            max_conns: Some(1),
            ..Default::default()
        },
    )
    .await
    .expect("db connect");
    let dbp: DBProvider<DbError> = DBProvider::new(db.clone());

    let hub = Arc::new(ClientHub::new());

    // Register mock AuthZ resolver before initializing the module
    hub.register::<dyn AuthZResolverGatewayClient>(Arc::new(MockAuthZResolver));

    let ctx = ModuleCtx::new(
        "users_info",
        Uuid::new_v4(),
        Arc::new(MockConfigProvider::new_users_info_default()),
        hub.clone(),
        CancellationToken::new(),
        Some(dbp),
    );

    let module = UsersInfo::default();
    run_migrations_for_module(&db, "users_info", module.migrations())
        .await
        .expect("migrate");
    module.init(&ctx).await.expect("init");

    // Act: resolve SDK client from hub and do basic CRUD.
    let client = ctx
        .client_hub()
        .get::<dyn UsersInfoClientV1>()
        .expect("UsersInfoClientV1 must be registered");

    // Create a security context with tenant access
    let tenant_id = Uuid::new_v4();
    let sec = SecurityContext::builder()
        .subject_id(Uuid::new_v4())
        .subject_tenant_id(tenant_id)
        .build();

    let created = client
        .create_user(
            sec.clone(),
            NewUser {
                id: None,
                tenant_id,
                email: "test@example.com".to_owned(),
                display_name: "Test".to_owned(),
            },
        )
        .await
        .unwrap();

    let fetched = client.get_user(sec.clone(), created.id).await.unwrap();
    assert_eq!(fetched.email, "test@example.com");

    client.delete_user(sec, created.id).await.unwrap();
}
