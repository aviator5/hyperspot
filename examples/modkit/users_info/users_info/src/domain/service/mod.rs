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
//! All operations use the `AuthZ` Resolver PEP (Policy Enforcement Point) pattern
//! via [`PolicyEnforcer`](authz_resolver_sdk::PolicyEnforcer):
//! 1. Construct a `PolicyEnforcer` per resource type (once, during init)
//! 2. Call `enforcer.access_scope(&ctx, action, resource_id, require_constraints)`
//! 3. The enforcer builds the request, evaluates via PDP, and compiles to `AccessScope`
//! 4. Pass scope to repository methods for tenant-isolated queries
//!
//! ### Subtree authorization (no closure table)
//!
//! Enforcers are created with empty `capabilities` (no `tenant_hierarchy`).
//! This means the PDP must expand the subtree into explicit tenant IDs in
//! the constraints it returns.
//!
//! For **point operations** (GET/UPDATE/DELETE by ID), a prefetch pattern
//! would be more efficient: PEP fetches the resource first, sends its
//! `owner_tenant_id` as a resource property, and PDP returns a narrow `eq`
//! constraint instead of an expanded subtree. This also improves TOCTOU
//! protection for mutations.
//!
//! Reference: `docs/arch/authorization/AUTHZ_USAGE_SCENARIOS.md`
//! - **S06** — LIST without closure (current approach for list operations)
//! - **S07** — GET with prefetch (optimal for point reads)
//! - **S08** — UPDATE/DELETE with prefetch + TOCTOU protection
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

use crate::domain::events::UserDomainEvent;
use crate::domain::ports::{AuditPort, EventPublisher};
use crate::domain::repos::{AddressesRepository, CitiesRepository, UsersRepository};
use authz_resolver_sdk::AuthZResolverGatewayClient;
use authz_resolver_sdk::PolicyEnforcer;
use modkit_db::DBProvider;
use modkit_db::odata::LimitCfg;

mod addresses;
mod cities;
mod users;

pub(crate) use addresses::AddressesService;
pub(crate) use cities::CitiesService;
pub(crate) use users::UsersService;

pub(crate) type DbProvider = DBProvider<modkit_db::DbError>;

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

        let default_props = vec![
            modkit_security::properties::OWNER_TENANT_ID.to_owned(),
            modkit_security::properties::RESOURCE_ID.to_owned(),
        ];

        let cities = Arc::new(CitiesService::new(
            Arc::clone(&db),
            Arc::clone(&cities_repo),
            PolicyEnforcer::new("users_info.city", authz.clone())
                .with_supported_properties(default_props.clone()),
        ));
        let addresses = Arc::new(AddressesService::new(
            Arc::clone(&db),
            Arc::clone(&addresses_repo),
            Arc::clone(&users_repo),
            PolicyEnforcer::new("users_info.address", authz.clone())
                .with_supported_properties(default_props.clone()),
        ));

        Self {
            users: UsersService::new(
                db,
                Arc::clone(&users_repo),
                events,
                audit,
                PolicyEnforcer::new("users_info.user", authz)
                    .with_supported_properties(default_props),
                config,
                cities.clone(),
                addresses.clone(),
            ),
            cities,
            addresses,
        }
    }
}
