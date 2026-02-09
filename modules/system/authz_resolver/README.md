# AuthZ Resolver

Authorization evaluation for CyberFabric — determines access decisions with row-level constraints.

## Overview

The **authz_resolver** module provides policy-based access control:

1. **Authorization evaluation** — Evaluate Subject + Action + Resource against a policy
2. **Constraint-based access** — Return row-level constraints (tenant scoping, resource filtering)
3. **PEP integration** — SDK helpers to build requests and compile constraints into `AccessScope`

The module follows the PEP/PDP pattern: modules act as Policy Enforcement Points (PEP) that build evaluation requests, and the AuthZ Resolver acts as the Policy Decision Point (PDP) that returns decisions with constraints.

## Public API

The gateway registers [`AuthZResolverGatewayClient`](authz_resolver-sdk/src/api.rs) in ClientHub:

- `evaluate(request)` — Evaluate an authorization request, returning decision + constraints

### Models

See [`models.rs`](authz_resolver-sdk/src/models.rs):

**`EvaluationRequest`**:
- `subject` — Who: `id`, `tenant_id`, `subject_type`, `properties`
- `action` — What: `name` (e.g., `"list"`, `"create"`, `"update"`, `"delete"`)
- `resource` — Which: `resource_type` (e.g., `"users_info.user"`), `id`, `require_constraints`
- `context` — Context: `tenant` (with `root_id`), `token_scopes`, `properties`

**`EvaluationResponse`**:
- `decision` — `true` (allow) or `false` (deny)
- `constraints` — Vec of `Constraint` (ORed); each constraint contains ANDed predicates

### Constraints

See [`constraints.rs`](authz_resolver-sdk/src/constraints.rs):

```rust
// Example: "allow access to rows where owner_tenant_id = T1 OR owner_tenant_id IN [T1, T2]"
EvaluationResponse {
    decision: true,
    constraints: vec![
        Constraint { predicates: vec![Predicate::Eq(EqPredicate { property: "owner_tenant_id", value: T1 })] },
        Constraint { predicates: vec![Predicate::In(InPredicate { property: "owner_tenant_id", values: vec![T1, T2] })] },
    ],
}
```

### PEP Pattern

The SDK provides helpers for the common PEP flow:

```rust
use authz_resolver_sdk::pep::{build_evaluation_request, compile_to_access_scope};

// 1. Build request from SecurityContext
let request = build_evaluation_request(
    &ctx,
    "list",                    // action
    "users_info.user",         // resource_type
    None,                      // resource_id (None for collection operations)
    true,                      // require_constraints (true for LIST/GET/UPDATE/DELETE)
    ctx.subject_tenant_id(),   // context_tenant_id
);

// 2. Evaluate via PDP
let response = authz.evaluate(request).await?;

// 3. Compile constraints to AccessScope for SecureORM
let scope = compile_to_access_scope(&response, true)?;
```

**Decision Matrix** (fail-closed):

| decision | require_constraints | constraints | Result |
|----------|-------------------|-------------|--------|
| false    | *                 | *           | Denied |
| true     | false             | *           | `allow_all()` |
| true     | true              | empty       | `allow_all()` |
| true     | true              | present     | Compile to `AccessScope` |

**Note:** CREATE operations need `require_constraints=true` because the SecureORM always needs tenant scope for INSERTs.

### Errors

See [`error.rs`](authz_resolver-sdk/src/error.rs): `Denied`, `NoPluginAvailable`, `ServiceUnavailable`, `Internal`

## Plugin API

Plugins implement [`AuthZResolverPluginClient`](authz_resolver-sdk/src/plugin_api.rs) and register via GTS.

CyberFabric includes one plugin out of the box:
- [`static_authz_plugin`](plugins/static_authz_plugin/) — Always-allow plugin with tenant-scoped constraints

## Configuration

### Gateway

See [`config.rs`](authz_resolver-gw/src/config.rs)

```yaml
modules:
  authz_resolver:
    vendor: "hyperspot"  # Selects plugin by matching vendor
```

### Static Plugin

The static plugin requires no configuration. It always returns `decision: true` with:
- **CREATE** (`require_constraints=false`): No constraints (allow all)
- **LIST/GET/UPDATE/DELETE** (`require_constraints=true`): Returns `owner_tenant_id IN [context_tenant_id]` constraint

This is suitable for development and single-tenant deployments.

## Usage

### Direct API

```rust
let authz = hub.get::<dyn AuthZResolverGatewayClient>()?;

let request = build_evaluation_request(
    &ctx, "list", "users_info.user", None, true, ctx.subject_tenant_id(),
);
let response = authz.evaluate(request).await?;

if !response.decision {
    return Err(DomainError::Forbidden);
}

let scope = compile_to_access_scope(&response, true)?;
let users = secure_conn.find::<user::Entity>(&scope)?.all(conn).await?;
```

### Helper Pattern

Modules typically wrap the PEP flow in a helper:

```rust
async fn authz_scope(
    authz: &dyn AuthZResolverGatewayClient,
    ctx: &SecurityContext,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    require_constraints: bool,
    context_tenant_id: Option<Uuid>,
) -> Result<AccessScope, DomainError> {
    let request = build_evaluation_request(
        ctx, action, resource_type, resource_id,
        require_constraints, context_tenant_id,
    );
    let response = authz.evaluate(request).await
        .map_err(|_| DomainError::Forbidden)?;
    compile_to_access_scope(&response, require_constraints)
        .map_err(|_| DomainError::Forbidden)
}
```

## Technical Decisions

### Gateway + Plugin Pattern

Multiple authorization backends are planned (static, OPA, Cedar, custom). The gateway handles cross-cutting concerns while plugins implement specific policy engines.

### Constraint-Based Access

Instead of simple allow/deny, the PDP returns constraints that translate to row-level filters. This enables:
- **Tenant isolation** — Automatic scoping to the caller's tenant hierarchy
- **Resource-level access** — Fine-grained access to specific resource IDs
- **Composable scopes** — Multiple constraints are ORed for flexible access patterns

### Fail-Closed Design

The compiler uses a fail-closed approach:
- Empty constraints with `require_constraints=true` → `allow_all()` (trusted PDP)
- `decision=false` → always denied, regardless of constraints
- Compilation errors → denied (malformed constraints are not silently allowed)

## Implementation Phases

### Phase 1: Core (Current)

- `evaluate` API
- PEP request builder and constraint compiler
- Static plugin (always-allow with tenant scoping)
- `AccessScope` integration with SecureORM
- ClientHub registration for in-process consumption

### Phase 2: Policy Engine (Planned)

- OPA or Cedar integration
- Policy authoring and management
- Role-based access control (RBAC)

### Phase 3: gRPC (Planned)

- gRPC API for out-of-process consumers
