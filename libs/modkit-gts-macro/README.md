# cf-modkit-gts-macro

Proc-macros backing the [`cf-modkit-gts`](../modkit-gts/README.md) crate. Not intended for direct use — depend on `cf-modkit-gts` instead; it re-exports everything below and carries the `inventory` collectors the macros target.

## What's here

- **`#[gts_schema(schema_id = "...", …)]`** — attribute macro. Applies
  `#[gts_macros::struct_to_gts_schema(...)]` to the struct, emits an
  `InventorySchema` entry into the process-wide inventory, and — for
  derived unit structs (`base = ParentStruct`) — auto-emits `impl Default`
  so generic helpers can construct the marker without the caller
  re-spelling the type.
- **`gts_instance! { … }`** — function-like macro for **typed** instance
  declarations. Required: `segment = "<tail>"` + `instance = Struct { … }`.
  Prefix comes from `<Struct as GtsSchema>::SCHEMA_ID` at compile time.
  Opt-in extras:
  - `schema = Type` — override prefix source (for level-3+ derived
    schemas where the serialised struct is a shallower base).
  - `name = IDENT` → also emits `pub static NAME: LazyLock<T>` for
    typed runtime access.
- **`gts_instance_raw! { … }`** — function-like macro for **raw-JSON**
  declarations. Params: `instance_id = "<full>"` + `payload = { … }`.
  Use when no Rust struct corresponds to the instance.

Both macros resolve the `cf-modkit-gts` crate path at expansion time via
`proc_macro_crate`, so callers only need `cf-modkit-gts` as a dependency
— no separate dep on this crate.

## Full docs & examples

See **[`cf-modkit-gts` README](../modkit-gts/README.md)**:

- [Adding a platform base schema](../modkit-gts/README.md#adding-a-platform-base-schema-inside-this-crate)
- [Declaring a well-known GTS instance](../modkit-gts/README.md#declaring-a-well-known-gts-instance) — preferred `segment` form, `instance_id` fallback, generic base types
- [Boundary with `types-registry`](../modkit-gts/README.md#boundary-with-types-registry)

Integration tests covering both macros live in
[`libs/modkit-gts/tests/macro_integration.rs`](../modkit-gts/tests/macro_integration.rs).
