use std::sync::Arc;
use tracing::{debug, info, instrument};

use crate::domain::error::DomainError;
use crate::domain::repos::{AddressesRepository, UsersRepository};
use crate::domain::service::DbProvider;
use authz_resolver_sdk::PolicyEnforcer;
use authz_resolver_sdk::pep::AccessRequest;

use super::{actions, resources};
use modkit_odata::{ODataQuery, Page};
use modkit_security::{AccessScope, SecurityContext, properties};
use time::OffsetDateTime;
use user_info_sdk::{Address, AddressPatch, NewAddress};
use uuid::Uuid;

pub struct AddressesService<R: AddressesRepository, U: UsersRepository> {
    db: Arc<DbProvider>,
    repo: Arc<R>,
    users_repo: Arc<U>,
    enforcer: PolicyEnforcer,
}

impl<R: AddressesRepository, U: UsersRepository> AddressesService<R, U> {
    pub fn new(
        db: Arc<DbProvider>,
        repo: Arc<R>,
        users_repo: Arc<U>,
        enforcer: PolicyEnforcer,
    ) -> Self {
        Self {
            db,
            repo,
            users_repo,
            enforcer,
        }
    }
}

// Business logic methods
impl<R: AddressesRepository, U: UsersRepository> AddressesService<R, U> {
    #[instrument(skip(self, ctx), fields(address_id = %id))]
    pub async fn get_address(
        &self,
        ctx: &SecurityContext,
        id: Uuid,
    ) -> Result<Address, DomainError> {
        debug!("Getting address by id");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        // TODO: consider prefetch pattern (AUTHZ_USAGE_SCENARIOS.md).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::GET, Some(id))
            .await?;

        let found = self.repo.get(&conn, &scope, id).await?;

        found.ok_or_else(|| DomainError::not_found("Address", id))
    }

    /// List addresses with cursor-based pagination
    #[instrument(skip(self, ctx, query))]
    pub async fn list_addresses_page(
        &self,
        ctx: &SecurityContext,
        query: &ODataQuery,
    ) -> Result<Page<Address>, DomainError> {
        debug!("Listing addresses with cursor pagination");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::LIST, None)
            .await?;

        let page = self.repo.list_page(&conn, &scope, query).await?;

        debug!("Successfully listed {} addresses in page", page.items.len());
        Ok(page)
    }

    #[instrument(skip(self, ctx), fields(user_id = %user_id))]
    pub async fn get_user_address(
        &self,
        ctx: &SecurityContext,
        user_id: Uuid,
    ) -> Result<Option<Address>, DomainError> {
        debug!("Getting address by user_id");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::GET, None)
            .await?;

        let found = self.repo.get_by_user_id(&conn, &scope, user_id).await?;

        Ok(found)
    }

    #[instrument(skip(self, ctx), fields(user_id = %user_id))]
    pub async fn get_address_by_user(
        &self,
        ctx: &SecurityContext,
        user_id: Uuid,
    ) -> Result<Option<Address>, DomainError> {
        self.get_user_address(ctx, user_id).await
    }

    #[instrument(skip(self, ctx, address), fields(user_id = %user_id))]
    pub async fn put_user_address(
        &self,
        ctx: &SecurityContext,
        user_id: Uuid,
        address: NewAddress,
    ) -> Result<Address, DomainError> {
        info!("Upserting address for user");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Prefetch: load user and existing address without authorization scope.
        // These internal reads extract tenant_id for the PDP request — no data
        // is leaked to the caller. Authorization is enforced on the mutation below.
        let prefetch = AccessScope::allow_all();

        let user = self
            .users_repo
            .get(&conn, &prefetch, user_id)
            .await?
            .ok_or_else(|| DomainError::user_not_found(user_id))?;

        let existing = self.repo.get_by_user_id(&conn, &prefetch, user_id).await?;

        let now = OffsetDateTime::now_utc();

        if let Some(existing_model) = existing {
            // UPDATE path (S08 prefetch) — pass prefetched owner_tenant_id
            // to PDP for narrow eq constraint + TOCTOU protection.
            let scope = self
                .enforcer
                .access_scope_with(
                    ctx,
                    &resources::ADDRESS,
                    actions::UPDATE,
                    Some(existing_model.id),
                    &AccessRequest::new().resource_property(
                        properties::OWNER_TENANT_ID,
                        serde_json::json!(existing_model.tenant_id.to_string()),
                    ),
                )
                .await?;

            let mut updated: Address = existing_model;
            updated.city_id = address.city_id;
            updated.street = address.street;
            updated.postal_code = address.postal_code;
            updated.updated_at = now;

            let _ = self.repo.update(&conn, &scope, updated.clone()).await?;

            info!("Successfully updated address for user");
            Ok(updated)
        } else {
            // CREATE path — PDP returns constraints, scope from PDP.
            let scope = self
                .enforcer
                .access_scope_with(
                    ctx,
                    &resources::ADDRESS,
                    actions::CREATE,
                    None,
                    &AccessRequest::new().resource_property(
                        properties::OWNER_TENANT_ID,
                        serde_json::json!(user.tenant_id.to_string()),
                    ),
                )
                .await?;

            let id = address.id.unwrap_or_else(Uuid::now_v7);

            let new_address = Address {
                id,
                tenant_id: user.tenant_id,
                user_id,
                city_id: address.city_id,
                street: address.street,
                postal_code: address.postal_code,
                created_at: now,
                updated_at: now,
            };

            let _ = self.repo.create(&conn, &scope, new_address.clone()).await?;

            info!("Successfully created address for user");
            Ok(new_address)
        }
    }

    #[instrument(skip(self, ctx), fields(user_id = %user_id))]
    pub async fn delete_user_address(
        &self,
        ctx: &SecurityContext,
        user_id: Uuid,
    ) -> Result<(), DomainError> {
        info!("Deleting address for user");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        // TODO: prefetch owner_tenant_id would narrow scope and improve
        // TOCTOU (AUTHZ_USAGE_SCENARIOS.md).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::DELETE, None)
            .await?;

        let rows_affected = self.repo.delete_by_user_id(&conn, &scope, user_id).await?;

        if rows_affected == 0 {
            return Err(DomainError::not_found("Address", user_id));
        }

        info!("Successfully deleted address for user");
        Ok(())
    }

    #[instrument(skip(self, ctx), fields(user_id = %new_address.user_id))]
    pub async fn create_address(
        &self,
        ctx: &SecurityContext,
        new_address: NewAddress,
    ) -> Result<Address, DomainError> {
        info!("Creating new address");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Prefetch: load user without authorization scope. This internal read
        // extracts tenant_id for the PDP request — no data is leaked to the
        // caller. Authorization is enforced on the CREATE below.
        let prefetch_scope = AccessScope::allow_all();

        let user = self
            .users_repo
            .get(&conn, &prefetch_scope, new_address.user_id)
            .await?
            .ok_or_else(|| DomainError::user_not_found(new_address.user_id))?;

        // Force tenant to match user's tenant (defense in depth)
        let tenant_id = user.tenant_id;

        // PDP returns constraints for CREATE too — scope from PDP, not payload.
        let scope = self
            .enforcer
            .access_scope_with(
                ctx,
                &resources::ADDRESS,
                actions::CREATE,
                None,
                &AccessRequest::new().resource_property(
                    properties::OWNER_TENANT_ID,
                    serde_json::json!(tenant_id.to_string()),
                ),
            )
            .await?;

        let now = OffsetDateTime::now_utc();
        let id = new_address.id.unwrap_or_else(Uuid::now_v7);

        let address = Address {
            id,
            tenant_id,
            user_id: new_address.user_id,
            city_id: new_address.city_id,
            street: new_address.street,
            postal_code: new_address.postal_code,
            created_at: now,
            updated_at: now,
        };

        let _ = self.repo.create(&conn, &scope, address.clone()).await?;

        info!("Successfully created address with id={}", address.id);
        Ok(address)
    }

    #[instrument(skip(self, ctx), fields(address_id = %id))]
    pub async fn update_address(
        &self,
        ctx: &SecurityContext,
        id: Uuid,
        patch: AddressPatch,
    ) -> Result<Address, DomainError> {
        info!("Updating address");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        // TODO: prefetch owner_tenant_id would narrow scope and improve
        // TOCTOU (AUTHZ_USAGE_SCENARIOS.md,).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::UPDATE, Some(id))
            .await?;

        let found = self.repo.get(&conn, &scope, id).await?;

        let mut current: Address = found.ok_or_else(|| DomainError::not_found("Address", id))?;

        if let Some(city_id) = patch.city_id {
            current.city_id = city_id;
        }
        if let Some(street) = patch.street {
            current.street = street;
        }
        if let Some(postal_code) = patch.postal_code {
            current.postal_code = postal_code;
        }
        current.updated_at = OffsetDateTime::now_utc();

        let _ = self.repo.update(&conn, &scope, current.clone()).await?;

        info!("Successfully updated address");
        Ok(current)
    }

    #[instrument(skip(self, ctx), fields(address_id = %id))]
    pub async fn delete_address(&self, ctx: &SecurityContext, id: Uuid) -> Result<(), DomainError> {
        info!("Deleting address");

        let conn = self.db.conn().map_err(DomainError::from)?;

        // Subtree without closure — PDP expands tenant hierarchy (see module doc).
        // TODO: prefetch owner_tenant_id would narrow scope and improve
        // TOCTOU (AUTHZ_USAGE_SCENARIOS.md).
        let scope = self
            .enforcer
            .access_scope(ctx, &resources::ADDRESS, actions::DELETE, Some(id))
            .await?;

        let deleted = self.repo.delete(&conn, &scope, id).await?;

        if !deleted {
            return Err(DomainError::not_found("Address", id));
        }

        info!("Successfully deleted address");
        Ok(())
    }
}
