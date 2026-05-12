# Canonical Permission GTS Type

Specification of the canonical base GTS Type for Cyber Ware authorization permissions. This document describes:

- The GTS Type Schema definition and allowed field semantics.
- The well-known Instance Identifier naming convention.
- How modules declare their permissions.
- Scenario examples covering CRUD-style, wildcard, and ABAC-style permissions.

For the overall authorization model (PDP/PEP, SecurityContext, constraints), see [DESIGN.md](./DESIGN.md).

## Purpose

Cyber Ware modules all need to describe "what can be granted" in a uniform way so admin UIs and the future AuthZ Management module can:

- List every permission any module has declared.
- Filter by owning module, resource type, or action.
- Attach permissions to identities/roles without understanding module internals.

This doc defines `gts.cf.modkit.authz.permission.v1~` as that canonical base GTS Type. Modules ship their permissions as well-known GTS Instances of this type and register them with `types-registry` at startup.

## Base Type Definition

**GTS Type Identifier:** `gts.cf.modkit.authz.permission.v1~`

**Rust struct:** `modkit_gts::AuthzPermissionV1`

**Location:** `libs/modkit-gts/src/permission.rs`

**Type Schema fields (v1):**

| Field           | Type     | Required | Semantics                                                                                                                  |
|-----------------|----------|----------|----------------------------------------------------------------------------------------------------------------------------|
| `id`            | `string` | yes      | Full GTS Instance Identifier of the permission (injected automatically when constructed as a well-known Instance).         |
| `resource_type` | `string` | yes      | GTS expression identifying the set of resources the permission applies to. See **`resource_type` Semantics** below.        |
| `action`        | `string` | yes      | Concrete action name. Lowercase snake_case. No wildcard, no list.                                                          |
| `display_name`  | `string` | yes      | Human-readable label for admin UIs.                                                                                        |

The Type Schema has `additionalProperties: false` in v1. Future fields on the base (`description`, `category`, `deprecated`, `implies`, …) will be added via GTS minor version evolution when a concrete consumer needs them — YAGNI governs today's shape.

**Extending with per-permission metadata.** If a module needs ABAC-style per-permission attributes (audit category, MFA requirement, risk class, …), it can declare a derived Type Schema with `#[modkit_gts::gts_type_schema(base = AuthzPermissionV1, schema_id = "...", ...)]` and register Instances against that derived Type Schema (three-segment instance IDs, analogous to how [`PluginV1`-derived plugin specs](../../MODKIT_PLUGINS.md) work). The wrapper joins the link-time inventory automatically, so the derived Type Schema lands in `types-registry` on the same path as the base. This is reserved for concrete consumers with real need; today's `AuthzPermissionV1` is non-generic and module catalogs live at level 2.

## Instance Identifier Convention

Well-known permission Instances use a two-segment GTS chain:

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

1. **Concrete GTS Type Identifier** — `gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.v1~`. Matches exactly that type and (per GTS §3.6 implicit derived-type coverage) anything derived from it. Note the trailing `~` marking this as a GTS Type Identifier, not a GTS Instance Identifier.
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

### Base Type Schema — registered by the platform at startup

The base Type Schema `gts.cf.modkit.authz.permission.v1~` is shipped by the `cyberware-modkit-gts` crate (located at `libs/modkit-gts/`) and self-registers via the `inventory` crate. `types-registry::init()` seeds its own in-memory registry with every inventory Type Schema + well-known Instance before publishing the client:

```rust
// modules/system/types-registry/types-registry/src/module.rs (init)
use modkit_gts::{all_inventory_instances, all_inventory_type_schemas};

let type_schemas = all_inventory_type_schemas()?;
let instances = all_inventory_instances()?;
let mut entries = type_schemas;
entries.extend(instances);
let results = service.register(entries); // internal service call, no ClientHub hop
RegisterResult::ensure_all_ok(&results)?;
// ...then publish client to ClientHub
```

No edit to a central list is ever needed — adding a new `#[gts_type_schema(...)]` struct anywhere in `modkit-gts` (or in any crate that uses the macro) picks it up automatically. `types-registry` code stays content-agnostic: it only calls aggregator functions and never references specific type names like `PluginV1` or `AuthzPermissionV1`.

### Per-module permission Instances — declared at compile time via `gts_instance!`

Modules that define permissions depend on `cyberware-modkit-gts` directly and declare each permission with the typed form of the `gts_instance!` macro. The macro takes a single `AuthzPermissionV1` struct literal with the full Instance Identifier as the `id` field's string literal; the upstream macro emits a compile-time assertion that the literal's prefix matches `<AuthzPermissionV1 as GtsSchema>::SCHEMA_ID` exactly, so a typo in the prefix is a build error rather than a silent runtime mismatch. The wrapper additionally emits an `inventory::submit!` block that lands in the process-wide `InventoryInstance` collector consumed by `types-registry::init()` — no module-side registration code, no `types-registry-sdk` dependency, and no ordering coupling with the declaring module's own `init()`.

```rust
// modules/mini-chat/mini-chat/src/gts/permissions.rs
use crate::domain::service::actions;
use modkit_gts::{AuthzPermissionV1, gts_instance};

const CHAT_RESOURCE_TYPE_WILDCARD: &str =
    "gts.cf.core.ai_chat.chat.v1~cf.core.mini_chat.chat.*";

gts_instance! {
    AuthzPermissionV1 {
        id: "gts.cf.modkit.authz.permission.v1~cf.mini_chat._.chat_read.v1",
        resource_type: CHAT_RESOURCE_TYPE_WILDCARD.to_owned(),
        action: actions::READ.to_owned(),
        display_name: "Read chat".to_owned(),
    }
}

// ...one invocation per (resource_type, action) the module surfaces.
```

The struct literal must contain exactly one of `id` / `gts_id` / `gtsId` as a string literal — upstream rewrites it into a `GtsInstanceId` value before constructing the typed struct. The emitted `payload_fn` serializes the typed struct via `serde_json::to_value`. Typos in field names, wrong field types, and missing required fields surface as compile errors; by pulling `action` values from `crate::domain::service::actions`, the permission catalog and the runtime PEP arguments share a single source of truth.

> **Optional typed runtime accessor.** Prefix the struct literal with `#[gts_static(NAME)]` to additionally emit `pub static NAME: LazyLock<AuthzPermissionV1>` alongside the inventory submission, giving call-sites a direct typed handle (`&AuthzPermissionV1`) without going through the registry.

> **Runtime registration fallback.** Permissions that cannot be declared at compile time (e.g. synthesized from config) can still be registered via `TypesRegistryClient::register(Vec<serde_json::Value>)` during `init()`. Reach for this only when the `gts_instance!` path is not feasible — the compile-time path is the default.

## Ownership Rationale

- **`libs/modkit-gts` (not `libs/modkit`, not `modkit-security`).** The permission base type is OoP-friendly (any process linking modkit transitively gets this crate), keeps `modkit-security` lean (no `gts` / `gts-macros` deps in a security-primitives library), and doesn't mix authz domain content into the framework crate.
- **`types-registry` stays content-agnostic at the code level** (spirit of [issue #156](https://github.com/cyberfabric/cyberware-rust/issues/156)). It imports `modkit-gts` for bootstrap but never references specific type names — only calls `all_inventory_type_schemas()` / `all_inventory_instances()` aggregators. Adding a new GTS Type requires zero edits in `types-registry`.
- **Rust struct is the single source of truth** for the Type Schema. No hand-written JSON — the macro-generated `gts_schema_with_refs_as_string()` accessor is invoked at startup to produce the JSON Schema document on demand. Zero drift possible.

## Out of Scope

- **AuthZ Management Module.** Full data model for storing grants (identity → permission bindings), role types, role hierarchies, and binding APIs. Covered by a future design.
- **Built-in AuthZ plugin.** The PDP implementation that evaluates permission Instances against subject/action/resource requests. Out of scope for the base-type spec.
- **Module migration.** Walking every existing module (mini-chat, users-info, etc.) and converting its hard-coded `resources::*` / `actions::*` constants into registered permission Instances is a separate per-module task.
- **`x-gts-traits`** for per-permission evaluation metadata (risk level, MFA-required, audit category). Added when a concrete consumer needs it.
- **Additional Type Schema fields** (`description`, `category`, `implies`, `deprecated`) deferred until driven by a concrete use case.
- **GTS §3.4 Attribute selector** in `resource_type`. Semantically wrong for describing a *set* of resources; kept for single-value reads from bound Instances.

## References

- [DESIGN.md](./DESIGN.md) — overall authn/authz architecture, PDP/PEP contract, constraint semantics.
- [GTS Specification](https://github.com/GlobalTypeSystem/gts-spec/blob/main/README.md) — specifically §2.2 (chain semantics), §3.3 (Query Language), §3.5 (wildcard access control), §3.7 (well-known vs anonymous instances).
- `libs/modkit-gts/src/permission.rs` — Rust definition of `AuthzPermissionV1`.
- `libs/modkit-gts-macro/` — proc-macros `#[gts_type_schema]`, `gts_instance!`, and `gts_instance_raw!` used to declare GTS Type Schemas and well-known Instances.
