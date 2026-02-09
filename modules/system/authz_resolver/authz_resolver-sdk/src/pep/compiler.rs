//! PEP constraint compiler.
//!
//! Compiles PDP evaluation responses into `AccessScope` for the secure ORM.
//!
//! ## Decision Matrix (fail-closed)
//!
//! | decision | `require_constraints` | constraints | Result |
//! |----------|-------------------|-------------|--------|
//! | false    | *                 | *           | Denied |
//! | true     | false             | *           | `allow_all()` |
//! | true     | true              | empty       | `allow_all()` (unrestricted) |
//! | true     | true              | present     | Compile constraints → `AccessScope` |
//!
//! Unknown predicate types fail that constraint (fail-closed).

use modkit_security::AccessScope;
use uuid::Uuid;

use crate::constraints::{Constraint, Predicate};
use crate::models::EvaluationResponse;

/// Well-known resource properties that map to `AccessScope` fields.
const PROPERTY_OWNER_TENANT_ID: &str = "owner_tenant_id";
const PROPERTY_ID: &str = "id";

/// Error during constraint compilation.
#[derive(Debug, thiserror::Error)]
pub enum ConstraintCompileError {
    /// The PDP explicitly denied access.
    #[error("access denied by PDP")]
    Denied,

    /// All constraints contained unknown predicates (fail-closed).
    #[error("all constraints failed compilation (fail-closed): {reason}")]
    AllConstraintsFailed { reason: String },
}

/// Compile an evaluation response into an `AccessScope`.
///
/// Implements the PEP Decision Matrix:
/// - `decision=false` → `Err(Denied)`
/// - `decision=true, require_constraints=false` → `Ok(allow_all())`
/// - `decision=true, require_constraints=true, constraints=[]` → `Ok(allow_all())`
/// - `decision=true, require_constraints=true, constraints=[..]` → compile predicates
///
/// ## Constraint compilation
///
/// Multiple constraints are `ORed`: tenant/resource IDs from all constraints
/// are merged into a single `AccessScope`.
///
/// Known predicates:
/// - `owner_tenant_id` with `eq`/`in` → `AccessScope::tenants_only(ids)`
/// - `id` with `eq`/`in` → `AccessScope::resources_only(ids)`
///
/// Unknown predicates are skipped (fail-closed for that constraint).
/// If ALL constraints fail compilation, returns `AllConstraintsFailed`.
///
/// # Errors
///
/// - `Denied` if `decision` is `false`
/// - `AllConstraintsFailed` if all constraints have unsupported predicates
pub fn compile_to_access_scope(
    response: &EvaluationResponse,
    require_constraints: bool,
) -> Result<AccessScope, ConstraintCompileError> {
    // Step 1: Check decision
    if !response.decision {
        return Err(ConstraintCompileError::Denied);
    }

    // Step 2: If constraints not required, return allow_all
    if !require_constraints {
        return Ok(AccessScope::allow_all());
    }

    // Step 3: If no constraints provided, treat as unrestricted
    if response.constraints.is_empty() {
        return Ok(AccessScope::allow_all());
    }

    // Step 4: Compile constraints (ORed — merge all tenant/resource IDs)
    let mut tenant_ids: Vec<Uuid> = Vec::new();
    let mut resource_ids: Vec<Uuid> = Vec::new();
    let mut any_compiled = false;
    let mut fail_reasons: Vec<String> = Vec::new();

    for constraint in &response.constraints {
        match compile_constraint(constraint) {
            Ok(compiled) => {
                any_compiled = true;
                tenant_ids.extend_from_slice(&compiled.tenant_ids);
                resource_ids.extend_from_slice(&compiled.resource_ids);
            }
            Err(reason) => {
                fail_reasons.push(reason);
            }
        }
    }

    // If no constraint compiled successfully, fail-closed
    if !any_compiled {
        return Err(ConstraintCompileError::AllConstraintsFailed {
            reason: fail_reasons.join("; "),
        });
    }

    // Build final scope from merged IDs
    if tenant_ids.is_empty() && resource_ids.is_empty() {
        // All compiled constraints produced empty results
        return Ok(AccessScope::allow_all());
    }

    Ok(AccessScope::both(tenant_ids, resource_ids))
}

/// Intermediate result from compiling a single constraint.
struct CompiledConstraint {
    tenant_ids: Vec<Uuid>,
    resource_ids: Vec<Uuid>,
}

/// Compile a single constraint's predicates into tenant/resource ID sets.
///
/// All predicates within a constraint are `ANDed`, but for our first iteration
/// we handle single-property constraints by collecting IDs.
/// If any predicate targets an unknown property, the constraint fails.
fn compile_constraint(constraint: &Constraint) -> Result<CompiledConstraint, String> {
    let mut tenant_ids = Vec::new();
    let mut resource_ids = Vec::new();
    let mut has_unknown = false;

    for predicate in &constraint.predicates {
        match predicate {
            Predicate::Eq(eq) => {
                if eq.property == PROPERTY_OWNER_TENANT_ID {
                    tenant_ids.push(eq.value);
                } else if eq.property == PROPERTY_ID {
                    resource_ids.push(eq.value);
                } else {
                    has_unknown = true;
                }
            }
            Predicate::In(in_pred) => {
                if in_pred.property == PROPERTY_OWNER_TENANT_ID {
                    tenant_ids.extend_from_slice(&in_pred.values);
                } else if in_pred.property == PROPERTY_ID {
                    resource_ids.extend_from_slice(&in_pred.values);
                } else {
                    has_unknown = true;
                }
            }
        }
    }

    // If any predicate was unknown, fail this constraint (fail-closed)
    if has_unknown {
        return Err(
            "constraint has unsupported predicates (only owner_tenant_id and id are supported)"
                .to_owned(),
        );
    }

    Ok(CompiledConstraint {
        tenant_ids,
        resource_ids,
    })
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::constraints::{EqPredicate, InPredicate};

    fn uuid(s: &str) -> Uuid {
        Uuid::parse_str(s).unwrap()
    }

    const T1: &str = "11111111-1111-1111-1111-111111111111";
    const T2: &str = "22222222-2222-2222-2222-222222222222";
    const R1: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    // === Decision Matrix Tests ===

    #[test]
    fn decision_false_returns_denied() {
        let response = EvaluationResponse {
            decision: false,
            constraints: vec![],
        };

        let result = compile_to_access_scope(&response, true);
        assert!(matches!(result, Err(ConstraintCompileError::Denied)));
    }

    #[test]
    fn decision_true_no_require_constraints_returns_allow_all() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![],
        };

        let scope = compile_to_access_scope(&response, false).unwrap();
        assert!(scope.is_unconstrained());
    }

    #[test]
    fn decision_true_require_constraints_empty_returns_allow_all() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        assert!(scope.is_unconstrained());
    }

    // === Constraint Compilation Tests ===

    #[test]
    fn single_tenant_eq_constraint() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![Constraint {
                predicates: vec![Predicate::Eq(EqPredicate {
                    property: "owner_tenant_id".to_owned(),
                    value: uuid(T1),
                })],
            }],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        assert_eq!(scope.tenant_ids(), &[uuid(T1)]);
        assert!(scope.resource_ids().is_empty());
    }

    #[test]
    fn multiple_tenants_in_constraint() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![Constraint {
                predicates: vec![Predicate::In(InPredicate {
                    property: "owner_tenant_id".to_owned(),
                    values: vec![uuid(T1), uuid(T2)],
                })],
            }],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        assert_eq!(scope.tenant_ids(), &[uuid(T1), uuid(T2)]);
    }

    #[test]
    fn resource_id_eq_constraint() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![Constraint {
                predicates: vec![Predicate::Eq(EqPredicate {
                    property: "id".to_owned(),
                    value: uuid(R1),
                })],
            }],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        assert!(scope.tenant_ids().is_empty());
        assert_eq!(scope.resource_ids(), &[uuid(R1)]);
    }

    #[test]
    fn multiple_constraints_ored() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![
                Constraint {
                    predicates: vec![Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![uuid(T1)],
                    })],
                },
                Constraint {
                    predicates: vec![Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![uuid(T2)],
                    })],
                },
            ],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        // IDs from both constraints are merged
        assert_eq!(scope.tenant_ids(), &[uuid(T1), uuid(T2)]);
    }

    #[test]
    fn unknown_predicate_fails_constraint() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![Constraint {
                predicates: vec![Predicate::Eq(EqPredicate {
                    property: "unknown_property".to_owned(),
                    value: uuid(T1),
                })],
            }],
        };

        let result = compile_to_access_scope(&response, true);
        assert!(matches!(
            result,
            Err(ConstraintCompileError::AllConstraintsFailed { .. })
        ));
    }

    #[test]
    fn mixed_known_and_unknown_constraints() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![
                // This constraint has an unknown property → fails
                Constraint {
                    predicates: vec![Predicate::Eq(EqPredicate {
                        property: "group_id".to_owned(),
                        value: uuid(T1),
                    })],
                },
                // This constraint is valid → succeeds
                Constraint {
                    predicates: vec![Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![uuid(T2)],
                    })],
                },
            ],
        };

        // Should succeed — the second constraint compiled
        let scope = compile_to_access_scope(&response, true).unwrap();
        assert_eq!(scope.tenant_ids(), &[uuid(T2)]);
    }

    #[test]
    fn both_tenant_and_resource_ids() {
        let response = EvaluationResponse {
            decision: true,
            constraints: vec![Constraint {
                predicates: vec![
                    Predicate::In(InPredicate {
                        property: "owner_tenant_id".to_owned(),
                        values: vec![uuid(T1)],
                    }),
                    Predicate::Eq(EqPredicate {
                        property: "id".to_owned(),
                        value: uuid(R1),
                    }),
                ],
            }],
        };

        let scope = compile_to_access_scope(&response, true).unwrap();
        assert_eq!(scope.tenant_ids(), &[uuid(T1)]);
        assert_eq!(scope.resource_ids(), &[uuid(R1)]);
    }
}
