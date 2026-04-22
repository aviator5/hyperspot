<!-- Updated: 2026-04-22 -->

# Canonical Permission GTS Type

Specification of the canonical base GTS type for Cyber Fabric authorization permissions. This document describes:

- The GTS schema definition and allowed field semantics.
- The well-known instance ID naming convention.
- How modules declare their permissions.
- Scenario examples covering CRUD-style, wildcard, and ABAC-style permissions.

For the overall authorization model (PDP/PEP, SecurityContext, constraints), see [DESIGN.md](./DESIGN.md).

## Purpose

Cyber Fabric modules all need to describe "what can be granted" in a uniform way so admin UIs and the future AuthZ Management module can:

- List every permission any module has declared.
- Filter by owning module, resource type, or action.
- Attach permissions to identities/roles without understanding module internals.

Historically each module hard-coded its `(resource_type, action)` tuples in Rust constants (e.g. `modules/mini-chat/mini-chat/src/domain/service/mod.rs`). There was no canonical GTS type behind them and no discoverable catalog.

This doc defines `gts.cf.modkit.authz.permission.v1~` as that canonical base type. Modules ship their permissions as well-known instances of this type and register them with `types-registry` at startup.

## Base Type Definition

**GTS ID:** `gts.cf.modkit.authz.permission.v1~`

**Rust struct:** `modkit_gts::AuthzPermissionV1`

**Location:** `libs/modkit-gts/src/permission.rs`

**Schema fields (v1):**

| Field           | Type     | Required | Semantics                                                                                                           |
|-----------------|----------|----------|---------------------------------------------------------------------------------------------------------------------|
| `id`            | `string` | yes      | Full GTS instance ID of the permission (injected automatically when constructed as a well-known instance).          |
| `resource_type` | `string` | yes      | GTS expression identifying the set of resources the permission applies to. See **`resource_type` Semantics** below. |
| `action`        | `string` | yes      | Concrete action name. Lowercase snake_case. No wildcard, no list.                                                   |
| `display_name`  | `string` | yes      | Human-readable label for admin UIs.                                                                                 |

The schema has `additionalProperties: false` in v1. Future fields on the base (`description`, `category`, `deprecated`, `implies`, …) will be added via GTS minor version evolution when a concrete consumer needs them — YAGNI governs today's shape.

**Extending with per-permission metadata.** If a module needs ABAC-style per-permission attributes (audit category, MFA requirement, risk class, …), it can declare a derived schema with `#[gts_macros::struct_to_gts_schema(base = AuthzPermissionV1, schema_id = "...", ...)]` and register instances against that derived schema (three-segment instance IDs, analogous to how [`PluginV1`-derived plugin specs](../../MODKIT_PLUGINS.md) work). This path is reserved for concrete consumers with real need; today's `AuthzPermissionV1` is non-generic and module catalogs live at level 2.

## Instance ID Convention

Well-known permission instances use a two-segment GTS chain:

```
gts.cf.modkit.authz.permission.v1~<vendor>.<package>.<namespace>.<permission_name>.v1
```

The right-hand segment encodes the declaring module's ownership (`<vendor>.<package>.<namespace>`) and an internal handle for the permission (`<permission_name>`). Use `_` as a placeholder when a slot has no meaningful value — e.g. when `<package>` already identifies the module uniquely, `<namespace>` is `_`.

Examples:

- `gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_read.v1`
- `gts.cf.modkit.authz.permission.v1~cf.am._.tenant_create.v1`
- `gts.cf.modkit.authz.permission.v1~cf.mini_chat._.retry_turn.v1`

## `resource_type` Semantics

The `resource_type` field accepts a **GTS expression**. Three forms are permitted, in order of specificity:

1. **Concrete GTS type ID** — `gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.v1~`. Matches exactly that type and (per GTS §3.6 implicit derived-type coverage) anything derived from it. Note the trailing `~` marking this as a type identifier, not an instance.
2. **Wildcard pattern (GTS §3.5)** — `gts.cf.core.am.tenant.*`, `gts.cf.modkit.plugins.plugin.v1~cf.*`. Matches any concrete ID within the wildcarded subtree. Evaluation follows the matching semantics documented in GTS §3.6.
3. **Query Language predicates (GTS §3.3)** — `gts.cf.core.ai_chat.chat.v1~[category='support']`. Allows ABAC-style attribute constraints. PEPs must advertise the filtered attribute (e.g. `category`) in their `supported_properties`, otherwise evaluation is fail-closed per [DESIGN.md](./DESIGN.md) rule #9.

**Not accepted for `action`:** wildcards or lists. Each permission carries a single concrete action string. Bundling multiple actions is a future `role`-type concern; keeping the permission atom scalar keeps evaluation straightforward.

## Scenario Examples

### Scenario A — Coarse action on a whole module's resource

```json
{
  "id": "gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_read.v1",
  "resource_type": "gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.v1~",
  "action": "read",
  "display_name": "Read chat"
}
```

Matches every mini-chat chat. The PDP returns `decision: true` and tenant/owner scoping constraints from other policy axes (tenant hierarchy, resource-group membership, etc. — see [DESIGN.md](./DESIGN.md)).

### Scenario B — Wildcard across a vendor/package (GTS §3.5)

```json
{
  "id": "gts.cf.modkit.authz.permission.v1~cf.am._.tenant_create.v1",
  "resource_type": "gts.cf.core.am.tenant.*",
  "action": "create",
  "display_name": "Tenant creation"
}
```

Matches any tenant type under `gts.cf.core.am.tenant.*`. Good for coarse admin permissions where multiple derived tenant kinds share a single "create" gate.

### Scenario C — ABAC-style narrow permission (GTS §3.3 Query Language)

```json
{
  "id": "gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_support_read.v1",
  "resource_type": "gts.cf.core.ai_chat.chat.v1~[category='support']",
  "action": "read",
  "display_name": "Read support chats"
}
```

Built-in AuthZ plugin compiles the `[category='support']` predicate into a PEP constraint (`{ eq: category='support' }`). PEP must advertise `category` in `supported_properties`; otherwise fail-closed per DESIGN.md rule #9.

### Scenario D — Action specific to a module

```json
{
  "id": "gts.cf.modkit.authz.permission.v1~cf.mini_chat._.retry_turn.v1",
  "resource_type": "gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.v1~",
  "action": "retry_turn",
  "display_name": "Retry chat turn"
}
```

Fine-grained domain action. The `action` field is free-form, so each module maps its own verbs (`retry_turn`, `upload_attachment`, `set_reaction`, …) naturally.

## Registration

### Base schema — registered by the platform at startup

The base schema `gts.cf.modkit.authz.permission.v1~` is shipped by the `libs/modkit-gts` crate and self-registers via the `inventory` crate. `types-registry::init()` seeds its own in-memory registry with every inventory schema + well-known instance before publishing the client:

```rust
// modules/system/types-registry/types-registry/src/module.rs (init)
use modkit_gts::{all_inventory_instances, all_inventory_schemas};

let schemas = all_inventory_schemas()?;
let instances = all_inventory_instances()?;
let mut entries = schemas;
entries.extend(instances);
let results = service.register(entries); // internal service call, no ClientHub hop
RegisterResult::ensure_all_ok(&results)?;
// ...then publish client to ClientHub
```

No edit to a central list is ever needed — adding a new `#[gts_schema(...)]` struct anywhere in `modkit-gts` (or in any crate that uses the macro) picks it up automatically. `types-registry` code stays content-agnostic: it only calls aggregator functions and never references specific type names like `PluginV1` or `AuthzPermissionV1`.

### Per-module permission instances — declared at compile time via `gts_instance!`

Modules that define permissions depend on `cf-modkit-gts` directly and declare each permission with the typed form of the `gts_instance!` macro. Pass a `segment = "..."` (the tail past the base schema's `~`) plus an `AuthzPermissionV1` struct literal; the schema prefix is taken from `AuthzPermissionV1::SCHEMA_ID` at compile time (via `const_format::concatcp!`) and the compiler type-checks every field. The macro emits an `inventory::submit!` block that lands in the process-wide `InventoryInstance` collector consumed by `types-registry::init()` — no module-side registration code, no `types-registry-sdk` dependency, and no ordering coupling with the declaring module's own `init()`.

```rust
// modules/mini-chat/mini-chat/src/permissions.rs
use crate::domain::service::{actions, resources};
use modkit_gts::{AuthzPermissionV1, gts_instance};

gts_instance! {
    segment = "cf.mini_chat._.chat_read.v1",
    instance = AuthzPermissionV1 {
        resource_type: resources::CHAT.name.to_owned(),
        action: actions::READ.to_owned(),
        display_name: "Read chat".to_owned(),
    }
}

// ...one invocation per (resource_type, action) the module surfaces.
```

Full instance id `gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_read.v1` is assembled by the macro from `<AuthzPermissionV1 as GtsSchema>::SCHEMA_ID + segment`. The `id` field is auto-injected as `GtsInstanceId::new(SCHEMA_ID, segment)` — do **not** pass an explicit `id` field (the macro rejects it). The emitted `payload_fn` serializes the typed struct via `serde_json::to_value`. Typos in field names, wrong field types, and missing required fields surface as compile errors; by pulling `resource_type` / `action` values from `crate::domain::service::{resources, actions}`, the permission catalog and the runtime PEP arguments share a single source of truth.

> **`schema = DerivedType` override.** For level-3+ instances (instance conforms to a derived schema deeper than the base struct being serialised), pass `schema = DerivedType` alongside `segment` + `instance = Base { ... }`. `DerivedType::SCHEMA_ID` becomes the prefix.

> **Runtime registration fallback.** Permissions that cannot be declared at compile time (e.g. synthesized from config) can still be registered via `TypesRegistryClient::register(Vec<serde_json::Value>)` during `init()`. Reach for this only when the `gts_instance!` path is not feasible — the compile-time path is the default.

> **Raw-JSON fallback.** A companion macro `gts_instance_raw!` (same crate) takes `instance_id = "<full>"` + `payload = { "field": value, ... }` for instances that do not correspond to a canonical Rust struct (e.g. future topic/role instances whose schemas ship without bespoke Rust types). No compile-time field checking in that form — validation runs at `types-registry::switch_to_ready()`.

## Ownership Rationale

- **`libs/modkit-gts` (not `libs/modkit`, not `modkit-security`).** The permission base type is OoP-friendly (any process linking modkit transitively gets this crate), keeps `modkit-security` lean (no `gts` / `gts-macros` deps in a security-primitives library), and doesn't mix authz domain content into the framework crate.
- **`types-registry` stays content-agnostic at the code level** (spirit of [issue #156](https://github.com/hypernetix/hyperspot/issues/156)). It imports `modkit-gts` for bootstrap but never references specific type names — only calls `all_inventory_schemas()` / `all_inventory_instances()` aggregators. Adding a new GTS type requires zero edits in `types-registry`.
- **Rust struct is the single source of truth** for the schema. No hand-written JSON, no ZIP packaging — the macro-generated `gts_schema_with_refs_as_string()` accessor is invoked at startup to produce the JSON schema on demand. Zero drift possible.

## Out of Scope

- **AuthZ Management Module.** Full data model for storing grants (identity → permission bindings), role types, role hierarchies, and binding APIs. Covered by a future design.
- **Built-in AuthZ plugin.** The PDP implementation that evaluates permission instances against subject/action/resource requests. Out of scope for the base-type spec.
- **Module migration.** Walking every existing module (mini-chat, users-info, etc.) and converting its hard-coded `resources::*` / `actions::*` constants into registered permission instances is a separate per-module task.
- **`x-gts-traits`** for per-permission evaluation metadata (risk level, MFA-required, audit category). Added when a concrete consumer needs it.
- **Additional schema fields** (`description`, `category`, `implies`, `deprecated`) deferred until driven by a concrete use case.
- **GTS §3.4 Attribute Selector** in `resource_type`. Semantically wrong for describing a *set* of resources; kept for single-value reads from bound instances.

## References

- [DESIGN.md](./DESIGN.md) — overall authn/authz architecture, PDP/PEP contract, constraint semantics.
- [GTS Specification](../../../../gts-spec/README.md) — specifically §2.2 (chain semantics), §3.3 (Query Language), §3.5 (wildcard access control), §3.7 (well-known vs anonymous instances).
- `libs/modkit-gts/src/permission.rs` — Rust definition of `AuthzPermissionV1`.
- `libs/modkit-gts-macro/` — proc-macros `#[gts_schema]`, `gts_instance!`, and `gts_instance_raw!` used to declare GTS types and well-known instances.
