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

The SDK provides [`PolicyEnforcer`](authz_resolver-sdk/src/pep/enforcer.rs) — a high-level PEP object that encapsulates the full flow: build request → evaluate via PDP → compile constraints to `AccessScope`.

A single enforcer serves all resource types; the resource type is supplied per call via a [`ResourceType`](authz_resolver-sdk/src/pep/enforcer.rs) descriptor. Action names are plain `&str` constants defined by the consuming module (not by the SDK).

```rust
use authz_resolver_sdk::pep::{PolicyEnforcer, ResourceType};

const USER: ResourceType = ResourceType {
    name: "users_info.user",
    supported_properties: &["owner_tenant_id", "id"],
};

// Create once during init (serves all resource types)
let enforcer = PolicyEnforcer::new(authz);

// All CRUD operations return AccessScope (PDP always returns constraints)
let scope = enforcer.access_scope(&ctx, &USER, "get", Some(id)).await?;
let scope = enforcer.access_scope(&ctx, &USER, "create", None).await?;
```

For advanced scenarios (ABAC resource properties, custom tenant mode, barrier bypass), use `access_scope_with` with an `AccessRequest`:

```rust
use authz_resolver_sdk::pep::AccessRequest;

// CREATE with target tenant + resource properties
let scope = enforcer.access_scope_with(
    &ctx, &USER, "create", None,
    &AccessRequest::new()
        .context_tenant_id(target_tenant_id)
        .tenant_mode(TenantMode::RootOnly)
        .resource_property("owner_tenant_id", json!(target_tenant_id.to_string())),
).await?;

// Billing — ignore barriers (constrained scope)
let scope = enforcer.access_scope_with(
    &ctx, &USER, "list", None,
    &AccessRequest::new().barrier_mode(BarrierMode::Ignore),
).await?;
```

**Decision Matrix** (fail-closed):

All resource operations use `access_scope` / `access_scope_with` which sets `require_constraints=true`.

| decision | require_constraints | constraints | Result |
|----------|-------------------|-------------|--------|
| false    | *                 | *           | Denied |
| true     | false             | *           | `allow_all()` (advanced use via `build_request` only) |
| true     | true              | empty       | `ConstraintsRequiredButAbsent` |
| true     | true              | present     | Compile to `AccessScope` |

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

The static plugin requires no configuration. It always returns `decision: true` with
`owner_tenant_id IN [context_tenant_id]` constraint for all operations (including CREATE).

This is suitable for development and single-tenant deployments.

## Usage

### Typical Module Setup

```rust
use authz_resolver_sdk::pep::{PolicyEnforcer, ResourceType};

// Define resource type and action constants (once, in module)
mod resources {
    use super::ResourceType;
    pub const USER: ResourceType = ResourceType {
        name: "users_info.user",
        supported_properties: &["owner_tenant_id", "id"],
    };
}
mod actions {
    pub const LIST: &str = "list";
    pub const CREATE: &str = "create";
    // ...
}

// During init — create one enforcer, share across services
let authz = hub.get::<dyn AuthZResolverGatewayClient>()?;
let enforcer = PolicyEnforcer::new(authz);

// In a service method — constrained (returns AccessScope)
let scope = enforcer
    .access_scope(&ctx, &resources::USER, actions::LIST, None)
    .await?;
let users = secure_conn.find::<user::Entity>(&scope)?.all(conn).await?;
```

## Technical Decisions

### Gateway + Plugin Pattern

Multiple authorization backends are planned (static, OPA, Cedar, custom). The gateway handles cross-cutting concerns while plugins implement specific policy engines.

### Constraint-Based Access

Instead of simple allow/deny, the PDP returns constraints that translate to row-level filters. This enables:
- **Tenant isolation** — Automatic scoping to the caller's tenant hierarchy
- **Resource-level access** — Fine-grained access to specific resource IDs
- **Composable scopes** — Multiple constraints are ORed for flexible access patterns

### Property Resolution

The `supported_properties` in `ResourceType` declares which properties a resource
supports for authorization. The SecureORM maps these property names to actual DB
columns via `ScopableEntity::resolve_property()`.

For `#[derive(Scopable)]` entities, the mapping is auto-generated:
- `tenant_col` → `"owner_tenant_id"`
- `resource_col` → `"id"`
- `owner_col` → `"owner_id"`
- `pep_prop(custom = "column")` → `"custom"` (custom properties)

See `libs/modkit-db-macros` for the Scopable derive macro documentation.

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
