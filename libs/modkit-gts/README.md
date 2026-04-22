# cf-modkit-gts

ModKit integration with the [Global Type System](https://github.com/hypernetix/gts-spec) (GTS). Provides a process-wide link-time **inventory** of GTS schemas and instances, proc-macros that populate it, and a small set of shipped platform base types (`PluginV1<P>`, `AuthzPermissionV1`).

## What this crate is for

- **Link-time inventory** — any `#[gts_schema(...)]`-annotated struct or `gts_instance!` declaration, in any crate linked into the process, lands in a shared `inventory`-based collector. `types-registry::init()` reads the collector and registers everything before publishing its client. No per-module registration code is required for entries known at compile time.
- **Platform base types** — this crate also ships the handful of shipped base schemas that many modules need directly: `PluginV1<P>` (base of every modkit plugin instance) and `AuthzPermissionV1` (base of every authorization permission).

Module-specific schemas (e.g. plugin specs or module-local permission extensions) can use either the wrapped `#[gts_schema(...)]` macro from this crate (and join the inventory automatically) or the raw upstream `#[gts_macros::struct_to_gts_schema(...)]` macro plus runtime registration in the module's `init()`. Both paths end up in `types-registry`; the first has zero boilerplate.

## Adding a platform base schema (inside this crate)

1. Create a new file, e.g. `src/role.rs`.
2. Annotate a struct with `#[gts_schema(schema_id = "...", ...)]`:

   ```rust
   use modkit_gts_macro::gts_schema;

   #[gts_schema(
       schema_id = "gts.cf.core.authz.role.v1~",
       description = "Authorization role",
       properties = "name,permissions,display_name"
   )]
   pub struct RoleV1 {
       pub name: String,
       pub permissions: Vec<String>,
       pub display_name: String,
   }
   ```
3. Add `mod role;` (and optional `pub use role::RoleV1;`) to `lib.rs`.

`all_inventory_schemas()` picks it up automatically — no edits to central lists.

## Declaring a well-known GTS instance

Two separate macros by payload shape:

- **`gts_instance!`** — **typed**, Rust struct literal. Compile-time field/type checking. Preferred.
- **`gts_instance_raw!`** — **raw JSON**, string literal for the instance id plus a JSON payload. Use when the instance has no Rust struct counterpart.

### `gts_instance!` — typed

Pass a struct literal; the compiler checks every field name and type against the schema struct.

```rust
use modkit_gts::{AuthzPermissionV1, gts_instance};

gts_instance! {
    segment = "cf.am._.tenant_create.v1",
    instance = AuthzPermissionV1 {
        resource_type: "gts.cf.core.am.tenant.*".to_owned(),
        action: "create".to_owned(),
        display_name: "Tenant creation".to_owned(),
    }
}
```

Full instance id `gts.cf.modkit.authz.permission.v1~cf.am._.tenant_create.v1` is assembled by the macro from `AuthzPermissionV1::SCHEMA_ID + segment`. The `id` field is auto-injected as `GtsInstanceId::new(SCHEMA_ID, segment)` and the struct is serialised via `serde_json::to_value`. Typos, wrong field types, missing required fields — all compile errors.

**`schema = Type` — when the serialised struct and the conforming schema differ**

Most of the time these are the same type (flat base schema like `AuthzPermissionV1`), and `schema =` can be omitted — the macro uses the `instance` struct's own `SCHEMA_ID`.

Set `schema =` when they differ, which happens for derived-schema chains. Derived GTS types (`base = Parent`) don't get `serde::Serialize` — only `GtsSerialize` — so you **cannot** pass them directly as `instance = ...`. Instead you wrap through a shallower base that does implement `Serialize`, and spell the deeper conforming schema in `schema = ...`:

```rust
// Given:
//   BaseEventV1<P>           (level 1, base — Serialize)
//     └── AuditPayloadV1<D>  (level 2, derived — only GtsSerialize)
//           └── PlaceOrderDataV1  (level 3, derived — only GtsSerialize)

gts_instance! {
    segment = "acme.orders.place_order_01.v1",
    schema = PlaceOrderDataV1,                           // level-3 SCHEMA_ID drives the prefix
    instance = BaseEventV1::<AuditPayloadV1<PlaceOrderDataV1>> {
        payload: AuditPayloadV1::<PlaceOrderDataV1> {
            category: "orders".to_owned(),
            data: PlaceOrderDataV1 {
                order_id: "o1".to_owned(),
                amount: 42,
            },
        },
    }
}
// full instance id: gts.A.v1~B.v1~C.v1~acme.orders.place_order_01.v1
```

Rule of thumb: pick `schema =` by **which GTS schema the content fulfils**, not by which Rust generic shape you needed to make it `Serialize`. Passing `schema = AuditPayloadV1<PlaceOrderDataV1>` would yield the level-2 prefix — Rust generic parameters do not change a type's `SCHEMA_ID` (it's a fixed const on the outermost type). See [integration tests](./tests/macro_integration.rs) for the full 3-level chain declaration and assertions.

**Optional — typed runtime accessor via `name = IDENT`:**

```rust
gts_instance! {
    name = CHAT_READ_PERM,
    segment = "cf.mini_chat._.chat_read.v1",
    instance = AuthzPermissionV1 {
        resource_type: /* ... */,
        action: /* ... */,
        display_name: /* ... */,
    }
}

// Elsewhere:
let p: &AuthzPermissionV1 = &CHAT_READ_PERM;
println!("{}", p.resource_type);
```

`pub static CHAT_READ_PERM: LazyLock<AuthzPermissionV1>` is emitted alongside the normal inventory submission — `id` field is populated exactly as it would be in the JSON payload. Opt-in: omit `name` and the static isn't generated.

### `gts_instance_raw!` — raw JSON

Use when the payload does not correspond to a single canonical Rust struct.

```rust
use modkit_gts::gts_instance_raw;

gts_instance_raw! {
    instance_id = "gts.cf.core.events.topic.v1~cf.core._.audit.v1",
    payload = { "name": "audit", "description": "Audit log events" }
}
```

`id` is auto-injected as the full `instance_id` literal. No compile-time field checking — validation happens at `types-registry::switch_to_ready()` via full JSON Schema validation.

### Generic base types

Some base types (e.g. `PluginV1<P>`) are generic over a derived-spec `P: GtsSchema`. Spell such instances with turbofish — `PluginV1::<DerivedSpec> { …, properties: DerivedSpec }`. The derived spec is declared with `#[gts_macros::struct_to_gts_schema(base = PluginV1, ...)]` in the owning module and, when its `properties` depend on config, registered with `types-registry` at module `init()` (in-process: it already has a reference to itself; OoP: via an SDK-level publish helper).

## Boundary with `types-registry`

- `types-registry` is a **content-agnostic** aggregator. It calls `all_inventory_schemas()` / `all_inventory_instances()` and registers whatever it finds. It never names specific types, so adding a new GTS type requires zero edits in `types-registry`.
- The inventory is process-global. In in-process runs, `types-registry::init()` already sees every contributing crate's entries. In the future OoP world, each process publishes its own inventory to the remote registry via the SDK; modules are not involved in that either way.
