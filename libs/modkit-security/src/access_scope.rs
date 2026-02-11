use uuid::Uuid;

/// Well-known authorization property names.
///
/// These constants are shared between the PEP compiler and the ORM condition
/// builder (`ScopableEntity::resolve_property()`), ensuring a single source of
/// truth for property names.
pub mod properties {
    /// Tenant-ownership property. Typically maps to the `tenant_id` column.
    pub const OWNER_TENANT_ID: &str = "owner_tenant_id";

    /// Resource identity property. Typically maps to the primary key column.
    pub const RESOURCE_ID: &str = "id";

    /// Owner (user) identity property. Typically maps to an `owner_id` column.
    pub const OWNER_ID: &str = "owner_id";
}

/// Predicate operation type for scope filters.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FilterOp {
    /// `property IN (values)` — flat set membership.
    In,
    // Future: InSubtree, InGroup, InGroupSubtree, ...
}

/// A single scope filter — a condition on a named resource property.
///
/// The property name (e.g., `"owner_tenant_id"`, `"id"`) is an authorization
/// concept. Mapping to DB columns is done by `ScopableEntity::resolve_property()`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeFilter {
    property: String,
    op: FilterOp,
    values: Vec<Uuid>,
}

impl ScopeFilter {
    /// Create a new scope filter.
    #[must_use]
    pub fn new(property: impl Into<String>, op: FilterOp, values: Vec<Uuid>) -> Self {
        Self {
            property: property.into(),
            op,
            values,
        }
    }

    /// The authorization property name.
    #[inline]
    #[must_use]
    pub fn property(&self) -> &str {
        &self.property
    }

    /// The filter operation.
    #[inline]
    #[must_use]
    pub fn op(&self) -> &FilterOp {
        &self.op
    }

    /// The filter values.
    #[inline]
    #[must_use]
    pub fn values(&self) -> &[Uuid] {
        &self.values
    }
}

/// A conjunction (AND) of scope filters — one access path.
///
/// All filters within a constraint must match simultaneously for a row
/// to be accessible via this path.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeConstraint {
    filters: Vec<ScopeFilter>,
}

impl ScopeConstraint {
    /// Create a new scope constraint from a list of filters.
    #[must_use]
    pub fn new(filters: Vec<ScopeFilter>) -> Self {
        Self { filters }
    }

    /// The filters in this constraint (AND-ed together).
    #[inline]
    #[must_use]
    pub fn filters(&self) -> &[ScopeFilter] {
        &self.filters
    }

    /// Returns `true` if this constraint has no filters.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

/// A disjunction (OR) of scope constraints defining what data is accessible.
///
/// Each constraint is an independent access path (OR-ed). Filters within a
/// constraint are AND-ed. An unconstrained scope bypasses row-level filtering.
///
/// # Examples
///
/// ```
/// use modkit_security::access_scope::{AccessScope, ScopeConstraint, ScopeFilter, FilterOp, properties};
/// use uuid::Uuid;
///
/// // deny-all (default)
/// let scope = AccessScope::deny_all();
/// assert!(scope.is_deny_all());
///
/// // single tenant
/// let tid = Uuid::new_v4();
/// let scope = AccessScope::for_tenant(tid);
/// assert!(!scope.is_deny_all());
/// assert!(scope.contains_value(properties::OWNER_TENANT_ID, tid));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccessScope {
    constraints: Vec<ScopeConstraint>,
    unconstrained: bool,
}

impl Default for AccessScope {
    /// Default is deny-all: no constraints and not unconstrained.
    fn default() -> Self {
        Self::deny_all()
    }
}

impl AccessScope {
    // ── Constructors ────────────────────────────────────────────────

    /// Create an access scope from a list of constraints (OR-ed).
    #[must_use]
    pub fn from_constraints(constraints: Vec<ScopeConstraint>) -> Self {
        Self {
            constraints,
            unconstrained: false,
        }
    }

    /// Create an access scope with a single constraint.
    #[must_use]
    pub fn single(constraint: ScopeConstraint) -> Self {
        Self::from_constraints(vec![constraint])
    }

    /// Create an "allow all" (unconstrained) scope.
    ///
    /// This represents a legitimate PDP decision with no row-level filtering.
    /// Not a bypass — it's a valid authorization outcome.
    #[must_use]
    pub fn allow_all() -> Self {
        Self {
            constraints: Vec::new(),
            unconstrained: true,
        }
    }

    /// Create a "deny all" scope (no access).
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            constraints: Vec::new(),
            unconstrained: false,
        }
    }

    // ── Convenience constructors ────────────────────────────────────

    /// Create a scope for a set of tenant IDs.
    #[must_use]
    pub fn for_tenants(ids: Vec<Uuid>) -> Self {
        Self::single(ScopeConstraint::new(vec![ScopeFilter::new(
            properties::OWNER_TENANT_ID,
            FilterOp::In,
            ids,
        )]))
    }

    /// Create a scope for a single tenant ID.
    #[must_use]
    pub fn for_tenant(id: Uuid) -> Self {
        Self::for_tenants(vec![id])
    }

    /// Create a scope for a set of resource IDs.
    #[must_use]
    pub fn for_resources(ids: Vec<Uuid>) -> Self {
        Self::single(ScopeConstraint::new(vec![ScopeFilter::new(
            properties::RESOURCE_ID,
            FilterOp::In,
            ids,
        )]))
    }

    /// Create a scope for a single resource ID.
    #[must_use]
    pub fn for_resource(id: Uuid) -> Self {
        Self::for_resources(vec![id])
    }

    /// Create a scope with both tenant AND resource constraints (single path).
    #[must_use]
    pub fn for_tenants_and_resources(tenant_ids: Vec<Uuid>, resource_ids: Vec<Uuid>) -> Self {
        let mut filters = Vec::new();
        if !tenant_ids.is_empty() {
            filters.push(ScopeFilter::new(
                properties::OWNER_TENANT_ID,
                FilterOp::In,
                tenant_ids,
            ));
        }
        if !resource_ids.is_empty() {
            filters.push(ScopeFilter::new(
                properties::RESOURCE_ID,
                FilterOp::In,
                resource_ids,
            ));
        }
        if filters.is_empty() {
            return Self::deny_all();
        }
        Self::single(ScopeConstraint::new(filters))
    }

    // ── Accessors ───────────────────────────────────────────────────

    /// The constraints in this scope (OR-ed).
    #[inline]
    #[must_use]
    pub fn constraints(&self) -> &[ScopeConstraint] {
        &self.constraints
    }

    /// Returns `true` if this scope is unconstrained (allow-all).
    #[inline]
    #[must_use]
    pub fn is_unconstrained(&self) -> bool {
        self.unconstrained
    }

    /// Returns `true` if this scope denies all access.
    ///
    /// A scope is deny-all when it is not unconstrained and has no constraints.
    #[must_use]
    pub fn is_deny_all(&self) -> bool {
        !self.unconstrained && self.constraints.is_empty()
    }

    /// Collect all values for a given property across all constraints.
    ///
    /// Useful for extracting tenant IDs when you know the scope has
    /// only simple tenant-based constraints.
    #[must_use]
    pub fn all_values_for(&self, property: &str) -> Vec<Uuid> {
        let mut result = Vec::new();
        for constraint in &self.constraints {
            for filter in constraint.filters() {
                if filter.property() == property && *filter.op() == FilterOp::In {
                    result.extend_from_slice(filter.values());
                }
            }
        }
        result
    }

    /// Check if any constraint has a filter matching the given property and value.
    #[must_use]
    pub fn contains_value(&self, property: &str, id: Uuid) -> bool {
        self.constraints.iter().any(|c| {
            c.filters().iter().any(|f| {
                f.property() == property && *f.op() == FilterOp::In && f.values().contains(&id)
            })
        })
    }

    /// Check if any constraint references the given property.
    #[must_use]
    pub fn has_property(&self, property: &str) -> bool {
        self.constraints
            .iter()
            .any(|c| c.filters().iter().any(|f| f.property() == property))
    }

}
