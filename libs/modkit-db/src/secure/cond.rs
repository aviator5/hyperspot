use sea_orm::{ColumnTrait, Condition, EntityTrait, sea_query::Expr};

use crate::secure::{AccessScope, ScopableEntity};
use modkit_security::access_scope::{FilterOp, ScopeConstraint};

/// Build a deny-all condition (`WHERE false`).
fn deny_all() -> Condition {
    Condition::all().add(Expr::value(false))
}

/// Builds a `SeaORM` `Condition` from an `AccessScope` using property resolution.
///
/// # OR/AND Semantics
///
/// - Multiple constraints are OR-ed (alternative access paths)
/// - Filters within a constraint are AND-ed (all must match)
/// - Unknown properties fail that constraint (fail-closed)
/// - If all constraints fail resolution, deny-all
///
/// # Policy Rules
///
/// | Scope | Behavior |
/// |-------|----------|
/// | deny-all (default) | `WHERE false` |
/// | unconstrained (allow-all) | No filtering (`WHERE true`) |
/// | single constraint | AND of resolved filters |
/// | multiple constraints | OR of ANDed filter groups |
pub fn build_scope_condition<E>(scope: &AccessScope) -> Condition
where
    E: ScopableEntity + EntityTrait,
    E::Column: ColumnTrait + Copy,
{
    if scope.is_unconstrained() {
        return Condition::all();
    }
    if scope.is_deny_all() {
        return deny_all();
    }

    let compiled: Vec<Condition> = scope
        .constraints()
        .iter()
        .filter_map(build_constraint_condition::<E>)
        .collect();

    match compiled.len() {
        0 => deny_all(),
        1 => compiled.into_iter().next().unwrap_or_else(deny_all),
        _ => {
            let mut or_cond = Condition::any();
            for c in compiled {
                or_cond = or_cond.add(c);
            }
            or_cond
        }
    }
}

/// Build SQL for a single constraint (AND of filters).
///
/// Returns `None` if any filter references an unknown property (fail-closed).
fn build_constraint_condition<E>(constraint: &ScopeConstraint) -> Option<Condition>
where
    E: ScopableEntity + EntityTrait,
    E::Column: ColumnTrait + Copy,
{
    if constraint.is_empty() {
        return Some(Condition::all());
    }
    let mut and_cond = Condition::all();
    for filter in constraint.filters() {
        let col = E::resolve_property(filter.property())?;
        match filter.op() {
            FilterOp::In => {
                and_cond = and_cond.add(Expr::col(col).is_in(filter.values().to_vec()));
            }
        }
    }
    Some(and_cond)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use modkit_security::access_scope::{FilterOp, ScopeConstraint, ScopeFilter, properties};

    #[test]
    fn test_deny_all_scope() {
        let scope = AccessScope::default();
        assert!(scope.is_deny_all());
    }

    #[test]
    fn test_allow_all_scope() {
        let scope = AccessScope::allow_all();
        assert!(scope.is_unconstrained());
    }

    #[test]
    fn test_tenant_scope_not_empty() {
        let tid = uuid::Uuid::new_v4();
        let scope = AccessScope::for_tenant(tid);
        assert!(!scope.is_deny_all());
        assert!(scope.contains_value(properties::OWNER_TENANT_ID, tid));
    }

    #[test]
    fn test_or_scope_has_multiple_constraints() {
        let t1 = uuid::Uuid::new_v4();
        let t2 = uuid::Uuid::new_v4();
        let r1 = uuid::Uuid::new_v4();

        let scope = AccessScope::from_constraints(vec![
            ScopeConstraint::new(vec![
                ScopeFilter::new(properties::OWNER_TENANT_ID, FilterOp::In, vec![t1]),
                ScopeFilter::new(properties::RESOURCE_ID, FilterOp::In, vec![r1]),
            ]),
            ScopeConstraint::new(vec![ScopeFilter::new(
                properties::OWNER_TENANT_ID,
                FilterOp::In,
                vec![t2],
            )]),
        ]);
        assert_eq!(scope.constraints().len(), 2);
    }
}
