#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use async_trait::async_trait;
use authz_resolver_sdk::{
    AuthZResolverError, AuthZResolverGatewayClient,
    constraints::{Constraint, InPredicate, Predicate},
    models::{EvaluationRequest, EvaluationResponse},
};
use modkit_db::migration_runner::run_migrations_for_testing;
use modkit_db::secure::DBRunner;
use modkit_db::secure::{AccessScope, secure_insert};
use modkit_db::{ConnectOpts, DBProvider, Db, DbError, connect_db};
use modkit_security::SecurityContext;
use sea_orm_migration::MigratorTrait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::events::UserDomainEvent;
use crate::domain::ports::{AuditPort, EventPublisher};
use crate::domain::service::ServiceConfig;
use crate::infra::storage::{OrmAddressesRepository, OrmCitiesRepository, OrmUsersRepository};
use crate::module::ConcreteAppServices;

#[must_use]
pub fn ctx_allow_tenants(tenants: &[Uuid]) -> SecurityContext {
    let tenant_id = tenants.first().copied().unwrap_or_else(Uuid::new_v4);
    SecurityContext::builder()
        .subject_id(Uuid::new_v4())
        .subject_tenant_id(tenant_id)
        .build()
}

#[must_use]
pub fn ctx_deny_all() -> SecurityContext {
    SecurityContext::anonymous()
}

/// Create an in-memory database for testing.
pub async fn inmem_db() -> Db {
    let opts = ConnectOpts {
        max_conns: Some(1),
        min_conns: Some(1),
        ..Default::default()
    };
    let db = connect_db("sqlite::memory:", opts)
        .await
        .expect("Failed to connect to in-memory database");

    run_migrations_for_testing(
        &db,
        crate::infra::storage::migrations::Migrator::migrations(),
    )
    .await
    .map_err(|e| e.to_string())
    .expect("Failed to run migrations");

    db
}

pub async fn seed_user(
    db: &impl DBRunner,
    id: Uuid,
    tenant_id: Uuid,
    email: &str,
    display_name: &str,
) {
    use crate::infra::storage::entity::user::ActiveModel;
    use crate::infra::storage::entity::user::Entity as UserEntity;
    use sea_orm::Set;

    let now = OffsetDateTime::now_utc();
    let user = ActiveModel {
        id: Set(id),
        tenant_id: Set(tenant_id),
        email: Set(email.to_owned()),
        display_name: Set(display_name.to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    let scope = AccessScope::for_tenants(vec![tenant_id]);
    let _ = secure_insert::<UserEntity>(user, &scope, db)
        .await
        .expect("Failed to seed user");
}

pub struct MockEventPublisher;
pub struct MockAuditPort;

impl EventPublisher<UserDomainEvent> for MockEventPublisher {
    fn publish(&self, _event: &UserDomainEvent) {}
}

#[async_trait::async_trait]
impl AuditPort for MockAuditPort {
    async fn get_user_access(&self, _id: Uuid) -> Result<(), crate::domain::error::DomainError> {
        Ok(())
    }

    async fn notify_user_created(&self) -> Result<(), crate::domain::error::DomainError> {
        Ok(())
    }
}

/// Mock `AuthZ` resolver that allows all requests and returns the context's tenant
/// as a constraint, mimicking the `static_authz_plugin` `allow_all` behavior.
pub struct MockAuthZResolver;

#[async_trait]
impl AuthZResolverGatewayClient for MockAuthZResolver {
    async fn evaluate(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationResponse, AuthZResolverError> {
        // allow_all mode: decision=true with tenant constraint from context
        let constraints = if request.context.require_constraints {
            if let Some(ref tenant_ctx) = request.context.tenant_context {
                if let Some(root_id) = tenant_ctx.root_id {
                    vec![Constraint {
                        predicates: vec![Predicate::In(InPredicate {
                            property: "owner_tenant_id".to_owned(),
                            values: vec![root_id],
                        })],
                    }]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok(EvaluationResponse {
            decision: true,
            constraints,
            deny_reason: None,
        })
    }
}

pub fn build_services(db: Db, config: ServiceConfig) -> Arc<ConcreteAppServices> {
    let limit_cfg = config.limit_cfg();

    let users_repo = OrmUsersRepository::new(limit_cfg);
    let cities_repo = OrmCitiesRepository::new(limit_cfg);
    let addresses_repo = OrmAddressesRepository::new(limit_cfg);

    let db: Arc<DBProvider<DbError>> = Arc::new(DBProvider::new(db));

    Arc::new(ConcreteAppServices::new(
        users_repo,
        cities_repo,
        addresses_repo,
        db,
        Arc::new(MockEventPublisher),
        Arc::new(MockAuditPort),
        Arc::new(MockAuthZResolver),
        config,
    ))
}
