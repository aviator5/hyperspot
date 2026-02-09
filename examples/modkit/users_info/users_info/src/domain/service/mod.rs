//! Domain service layer - business logic and rules.
//!
//! ## Architecture
//!
//! This module implements the domain service pattern with per-resource submodules:
//! - `users` - User CRUD and business rules (email/display name validation)
//! - `cities` - City CRUD operations
//! - `addresses` - Address management (1-to-1 with users)
//!
//! ## Layering Rules
//!
//! The domain layer:
//! - **MAY** import: `user_info_sdk` (contract types), `infra` (data access), `modkit` libs
//! - **MUST NOT** import: `api::*` (one-way dependency: API → Domain)
//! - **Uses**: SDK contract types (`User`, `NewUser`, etc.) as primary domain models
//! - **Uses**: `OData` filter schemas from `user_info_sdk::odata` (not defined here)
//!
//! ## `OData` Integration
//!
//! The service uses type-safe `OData` filtering via SDK filter enums:
//! - Filter schemas: `user_info_sdk::odata::{UserFilterField, CityFilterField, ...}`
//! - Pagination: `modkit_db::odata::paginate_odata` with filter type parameter
//! - Mapping: Infrastructure layer (`odata_mapper`) maps filters to `SeaORM` columns
//!
//! ## Security
//!
//! All operations use the AuthZ Resolver PEP (Policy Enforcement Point) pattern:
//! 1. Build evaluation request from `SecurityContext` + operation details
//! 2. Call AuthZ Resolver to evaluate the request
//! 3. Compile PDP response into `AccessScope` for row-level filtering
//! 4. Pass scope to repository methods for tenant-isolated queries
//!
//! ## Connection Management
//!
//! Services acquire database connections internally via `DBProvider`. Handlers
//! do NOT touch database objects - they simply call service methods with
//! business parameters only.
//!
//! This design:
//! - Keeps handlers clean and focused on HTTP concerns
//! - Maintains transaction safety via the task-local guard

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::events::UserDomainEvent;
use crate::domain::ports::{AuditPort, EventPublisher};
use crate::domain::repos::{AddressesRepository, CitiesRepository, UsersRepository};
use authz_resolver_sdk::pep::{compile_to_access_scope, build_evaluation_request};
use authz_resolver_sdk::AuthZResolverGatewayClient;
use modkit_db::DBProvider;
use modkit_db::odata::LimitCfg;
use modkit_security::{AccessScope, SecurityContext};
use uuid::Uuid;

mod addresses;
mod cities;
mod users;

pub(crate) use addresses::AddressesService;
pub(crate) use cities::CitiesService;
pub(crate) use users::UsersService;

pub(crate) type DbProvider = DBProvider<modkit_db::DbError>;

/// Resolve access scope for the current security context using the AuthZ Resolver.
///
/// This is the PEP (Policy Enforcement Point) helper that:
/// 1. Builds an `EvaluationRequest` from the security context + operation details
/// 2. Calls the AuthZ Resolver PDP to evaluate the request
/// 3. Compiles the PDP response into an `AccessScope` for row-level filtering
pub(crate) async fn authz_scope(
    authz: &dyn AuthZResolverGatewayClient,
    ctx: &SecurityContext,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    require_constraints: bool,
) -> Result<AccessScope, DomainError> {
    let eval_request =
        build_evaluation_request(ctx, action, resource_type, resource_id, require_constraints);
    let eval_response = authz.evaluate(eval_request).await.map_err(|e| {
        tracing::error!(error = %e, "AuthZ evaluation failed");
        DomainError::InternalError
    })?;
    compile_to_access_scope(&eval_response, require_constraints).map_err(|e| {
        tracing::error!(error = %e, "Failed to compile AuthZ constraints to access scope");
        DomainError::Forbidden
    })
}

/// Configuration for the domain service
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub max_display_name_length: usize,
    pub default_page_size: u32,
    pub max_page_size: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            max_display_name_length: 100,
            default_page_size: 50,
            max_page_size: 1000,
        }
    }
}

impl ServiceConfig {
    #[must_use]
    pub fn limit_cfg(&self) -> LimitCfg {
        LimitCfg {
            default: u64::from(self.default_page_size),
            max: u64::from(self.max_page_size),
        }
    }
}

// DI Container - aggregates all domain services
//
// # Database Access
//
// Services acquire database connections internally via `DBProvider`. Handlers
// do NOT touch database objects - they call service methods with business
// parameters only (e.g., `svc.users.get_user(&ctx, id)`).
//
// **Security**: A task-local guard prevents `Db::conn()` from being called
// inside transaction closures, eliminating the factory bypass vulnerability.
pub(crate) struct AppServices<UR, CR, AR>
where
    UR: UsersRepository + 'static,
    CR: CitiesRepository,
    AR: AddressesRepository,
{
    pub(crate) users: UsersService<UR, CR, AR>,
    pub(crate) cities: Arc<CitiesService<CR>>,
    pub(crate) addresses: Arc<AddressesService<AR, UR>>,
}

#[cfg(test)]
mod tests_security_scoping;

#[cfg(test)]
mod tests_entities;

#[cfg(test)]
mod tests_cursor_pagination;

impl<UR, CR, AR> AppServices<UR, CR, AR>
where
    UR: UsersRepository + 'static,
    CR: CitiesRepository,
    AR: AddressesRepository,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        users_repo: UR,
        cities_repo: CR,
        addresses_repo: AR,
        db: Arc<DbProvider>,
        events: Arc<dyn EventPublisher<UserDomainEvent>>,
        audit: Arc<dyn AuditPort>,
        authz: Arc<dyn AuthZResolverGatewayClient>,
        config: ServiceConfig,
    ) -> Self {
        let users_repo = Arc::new(users_repo);
        let cities_repo = Arc::new(cities_repo);
        let addresses_repo = Arc::new(addresses_repo);

        let cities = Arc::new(CitiesService::new(
            Arc::clone(&db),
            Arc::clone(&cities_repo),
            authz.clone(),
        ));
        let addresses = Arc::new(AddressesService::new(
            Arc::clone(&db),
            Arc::clone(&addresses_repo),
            Arc::clone(&users_repo),
            authz.clone(),
        ));

        Self {
            users: UsersService::new(
                db,
                Arc::clone(&users_repo),
                events,
                audit,
                authz,
                config,
                cities.clone(),
                addresses.clone(),
            ),
            cities,
            addresses,
        }
    }
}
