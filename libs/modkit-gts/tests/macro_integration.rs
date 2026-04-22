//! Integration tests for the `modkit-gts-macro` crate.
//!
//! Proc-macros can't be tested from inside the crate that defines them —
//! they must be exercised against real Rust types in a consumer crate.
//! `modkit-gts` is the natural consumer (it re-exports both `gts_schema`
//! and `gts_instance!` and ships the inventory collectors they target),
//! so integration tests live here.
//!
//! Covered:
//! - `#[gts_schema]` on a base (non-generic) type registers a schema into
//!   the global `InventorySchema` collector and sets `SCHEMA_ID`.
//! - `#[gts_schema]` on a derived unit struct (`base = ParentStruct`)
//!   additionally auto-emits `impl Default` so generic helpers like
//!   `PluginV1::<P>::build_registration` can construct the marker
//!   internally.
//! - `gts_instance!` (typed) — `segment = "<tail>"` + `instance = Struct { ... }`:
//!   schema prefix auto-derived from `Struct::SCHEMA_ID` via
//!   `const_format::concatcp!`. Two opt-in extensions:
//!   - `schema = Type` — override prefix source (for level-3+ derived
//!     schemas where the serialised struct is a shallower base).
//!   - `name = IDENT` → emits `pub static NAME: LazyLock<T>` alongside
//!     the inventory submission for typed runtime access.
//! - `gts_instance_raw!` — raw-JSON payload path for instances that do not
//!   correspond to a Rust struct.
//! - `id` field auto-injection (both typed and raw forms).
//!
//! Negative cases (macro rejects bad inputs: `segment + payload`,
//! explicit `id:` field, struct-update `..rest`, trailing `~` in segment,
//! etc.) are compile-time errors and would be exercised via `trybuild`
//! snapshot tests — skipped here to keep the test suite self-contained.

use modkit_gts::{
    GtsInstanceId, GtsSchema, InventoryInstance, InventorySchema, PluginV1, gts_instance,
    gts_instance_raw, gts_schema,
};

// =====================================================================
//                              Test types
// =====================================================================

/// Base non-generic test type. Exercises `#[gts_schema]` on a base and
/// serves as the parent schema for `gts_instance!` segment-form tests.
#[gts_schema(
    schema_id = "gts.test.core.macro.thing.v1~",
    description = "Test base type for modkit-gts-macro integration tests",
    properties = "id,name"
)]
pub struct TestThingV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

/// Derived unit-struct test type. Exercises:
/// - Inventory registration of derived schemas.
/// - Auto-emitted `impl Default` for derived unit structs.
///
/// Uses `PluginV1` (from `modkit-gts` itself) as the parent.
#[gts_schema(
    base = PluginV1,
    schema_id = "gts.cf.modkit.plugins.plugin.v1~test.core.macro.fake_plugin.v1~",
    description = "Fake plugin spec (tests)",
    properties = ""
)]
pub struct FakePluginSpecV1;

// ---------- 3-level schema chain with additional properties ----------
//
// Shape:
//   DeepLevel1V1<P>           (level 1, generic base; `payload: P`)
//     └── DeepLevel2V1<N>     (level 2, generic derived; own fields `name`,
//                              `nested: N` → nested under parent's `payload`)
//           └── DeepLevel3V1  (level 3, concrete derived; own fields
//                              `amount`, `label` → nested under parent's
//                              `nested`)
//
// This exercises: deep `#[gts_schema]` chains (3 levels), derived types
// with their own fields (not just unit-struct markers), and correct
// placement of `additionalProperties: false` at each nesting level.

/// Level-1 generic base for the deep-chain tests.
#[gts_schema(
    schema_id = "gts.test.deep.core.level1.v1~",
    description = "Level-1 base for 3-level chain tests",
    properties = "id,payload"
)]
pub struct DeepLevel1V1<P: gts::GtsSchema> {
    pub id: GtsInstanceId,
    pub payload: P,
}

/// Level-2 generic derived — carries its own data (`name`) plus a
/// further-nestable `nested: N` generic slot.
#[gts_schema(
    base = DeepLevel1V1,
    schema_id = "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~",
    description = "Level-2 derived (3-level chain tests)",
    properties = "name,nested"
)]
pub struct DeepLevel2V1<N: gts::GtsSchema> {
    pub name: String,
    pub nested: N,
}

/// Level-3 concrete derived — own fields only, no further nesting.
/// Represents the "additional properties" case the mini-chat refactor
/// discussion earmarked (`AmPermissionV1 { audit_category, mfa_required }`
/// style extension).
#[gts_schema(
    base = DeepLevel2V1,
    schema_id = "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~",
    description = "Level-3 leaf with own fields",
    properties = "amount,label"
)]
pub struct DeepLevel3V1 {
    pub amount: i64,
    pub label: String,
}

// =====================================================================
//         gts_instance! (typed) + gts_instance_raw! (JSON)
// =====================================================================

// `gts_instance_raw!` — raw-JSON payload for a non-Rust-struct instance.
gts_instance_raw! {
    instance_id = "gts.test.core.macro.thing.v1~test.macro.instance.raw.v1",
    payload = { "name": "raw" }
}

// `gts_instance!` (typed) — `segment` + `instance`. Schema prefix is
// pulled from `TestThingV1::SCHEMA_ID` at compile time.
gts_instance! {
    segment = "test.macro.instance.segment.v1",
    instance = TestThingV1 {
        name: "segment".to_owned(),
    }
}

// `name = IDENT` — opt-in typed runtime accessor via `LazyLock`.
// Emits `pub static NAMED_INSTANCE: LazyLock<TestThingV1>` alongside the
// normal inventory submission. Callers use `&NAMED_INSTANCE` to get a
// `&TestThingV1` without deserialising back from the JSON payload.
gts_instance! {
    name = NAMED_INSTANCE,
    segment = "test.macro.instance.named.v1",
    instance = TestThingV1 {
        name: "named".to_owned(),
    }
}

// 3-level instance via raw-JSON form. `instance_id` contains two interior
// `~` separators, so `split_instance_id` must correctly identify the
// rightmost `~` as the schema/instance boundary. The prefix
// `gts.test.deep.core.level1.v1~test.deep.core.level2.v1~` is the
// full `DeepLevel2V1::SCHEMA_ID` — i.e. the instance conforms to the
// level-2 derived schema.
//
// Payload shape follows the allOf chain: outer keys are level-1 base
// fields (`payload`), level-2 fields live under that generic slot, and
// any level-3 data would live one level deeper under `nested`.
// Deliberately kept minimal — validation correctness is owned by
// upstream jsonschema; here we only verify our macro's id parsing.
gts_instance_raw! {
    instance_id = "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.sample.v1",
    payload = {
        "payload": {
            "name": "deep-sample",
            "nested": {}
        }
    }
}

// =====================================================================
//                                Tests
// =====================================================================

fn find_instance(full_id: &str) -> &'static InventoryInstance {
    inventory::iter::<InventoryInstance>
        .into_iter()
        .find(|e| e.instance_id == full_id)
        .unwrap_or_else(|| panic!("instance {full_id} missing from inventory"))
}

// ---------- #[gts_schema] ----------

#[test]
fn gts_schema_sets_schema_id_const() {
    assert_eq!(TestThingV1::SCHEMA_ID, "gts.test.core.macro.thing.v1~");
    assert_eq!(
        FakePluginSpecV1::SCHEMA_ID,
        "gts.cf.modkit.plugins.plugin.v1~test.core.macro.fake_plugin.v1~"
    );
}

#[test]
fn gts_schema_registers_base_and_derived_in_inventory() {
    let schema_ids: std::collections::HashSet<&'static str> = inventory::iter::<InventorySchema>
        .into_iter()
        .map(|e| e.schema_id)
        .collect();

    assert!(
        schema_ids.contains("gts.test.core.macro.thing.v1~"),
        "base test schema TestThingV1 missing from inventory; got: {schema_ids:?}"
    );
    assert!(
        schema_ids.contains("gts.cf.modkit.plugins.plugin.v1~test.core.macro.fake_plugin.v1~"),
        "derived test schema FakePluginSpecV1 missing from inventory; got: {schema_ids:?}"
    );
}

#[test]
fn gts_schema_auto_emits_default_for_derived_unit_struct() {
    // If `#[gts_schema(base = PluginV1)]` on a unit struct did NOT emit
    // `impl Default`, the fully-qualified trait call below wouldn't
    // compile. That is the whole test. (We use the qualified form
    // specifically to avoid the clippy `default_constructed_unit_structs`
    // rewrite, which would bypass the Default impl and defeat the check.)
    #[allow(clippy::default_constructed_unit_structs)]
    let _via_trait = <FakePluginSpecV1 as Default>::default();
}

#[test]
fn gts_schema_schema_fn_produces_valid_json_with_expected_id() {
    let entry = inventory::iter::<InventorySchema>
        .into_iter()
        .find(|e| e.schema_id == "gts.test.core.macro.thing.v1~")
        .expect("base test schema present");
    let s = (entry.schema_fn)();
    let v: serde_json::Value = serde_json::from_str(&s).expect("schema_fn output parses as JSON");
    assert_eq!(v["$id"], "gts://gts.test.core.macro.thing.v1~");
    assert_eq!(v["type"], "object");
    // Base types emit additionalProperties: false at root.
    assert_eq!(v["additionalProperties"], false);
}

// ---------- gts_instance! ----------

#[test]
fn gts_instance_raw_json_form_emits_inventory_entry_with_auto_id() {
    let entry = find_instance("gts.test.core.macro.thing.v1~test.macro.instance.raw.v1");
    assert_eq!(entry.type_id, "gts.test.core.macro.thing.v1~");
    let payload = (entry.payload_fn)();
    assert_eq!(
        payload["id"], "gts.test.core.macro.thing.v1~test.macro.instance.raw.v1",
        "raw-JSON form must auto-inject `id` equal to instance_id"
    );
    assert_eq!(payload["name"], "raw");
}

#[test]
fn gts_instance_segment_form_concats_schema_id_at_compile_time() {
    // Schema prefix comes from `TestThingV1::SCHEMA_ID`, segment is the
    // literal in the macro. Full id is assembled at compile time via
    // `const_format::concatcp!`.
    let expected_full_id = "gts.test.core.macro.thing.v1~test.macro.instance.segment.v1";
    let entry = find_instance(expected_full_id);
    assert_eq!(
        entry.type_id, "gts.test.core.macro.thing.v1~",
        "segment form must set type_id to the struct's SCHEMA_ID"
    );
    let payload = (entry.payload_fn)();
    assert_eq!(
        payload["id"], expected_full_id,
        "segment form must auto-inject `id` equal to SCHEMA_ID + segment"
    );
    assert_eq!(payload["name"], "segment");
}

#[test]
fn gts_instance_type_id_equals_struct_schema_id_for_segment_form() {
    // Invariant: `type_id` in the inventory entry must match
    // `TestThingV1::SCHEMA_ID` — the source from which the macro built it.
    let entry = find_instance("gts.test.core.macro.thing.v1~test.macro.instance.segment.v1");
    assert_eq!(entry.type_id, TestThingV1::SCHEMA_ID);
}

// ---------- 3-level chain + additional properties ----------

#[test]
fn gts_schema_registers_full_three_level_chain() {
    let schema_ids: std::collections::HashSet<&'static str> = inventory::iter::<InventorySchema>
        .into_iter()
        .map(|e| e.schema_id)
        .collect();

    for id in [
        "gts.test.deep.core.level1.v1~",
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~",
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~",
    ] {
        assert!(
            schema_ids.contains(id),
            "level schema {id} missing from inventory; got: {schema_ids:?}"
        );
    }
}

#[test]
fn gts_schema_sets_schema_id_const_at_every_depth() {
    // Note the turbofish on generic types — `SCHEMA_ID` resolves per the
    // impl block generated for `PluginV1<()>`-style invocations by
    // upstream `struct_to_gts_schema`.
    assert_eq!(
        DeepLevel1V1::<()>::SCHEMA_ID,
        "gts.test.deep.core.level1.v1~"
    );
    assert_eq!(
        DeepLevel2V1::<()>::SCHEMA_ID,
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~"
    );
    assert_eq!(
        DeepLevel3V1::SCHEMA_ID,
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~"
    );
}

#[test]
fn gts_schema_emits_additional_properties_for_level_3_derived_with_own_fields() {
    // Fetch the level-3 schema and parse it.
    let entry = inventory::iter::<InventorySchema>
        .into_iter()
        .find(|e| {
            e.schema_id
                == "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~"
        })
        .expect("level-3 schema must be registered");
    let v: serde_json::Value =
        serde_json::from_str(&(entry.schema_fn)()).expect("level-3 schema parses as JSON");

    // Root invariants: derived schemas still carry `additionalProperties: false`
    // at the top level (the regression the upstream bug was about).
    assert_eq!(
        v["additionalProperties"], false,
        "top-level additionalProperties: false must survive 3-level derivation"
    );
    assert_eq!(
        v["$id"],
        "gts://gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~"
    );

    // Level-3's own fields (`amount`, `label`) are nested under parent's
    // generic field (`nested`) in `allOf[1].properties.nested.properties`,
    // and `nested` itself has `additionalProperties: false`.
    let nested = &v["allOf"][1]["properties"]["nested"];
    assert_eq!(
        nested["additionalProperties"], false,
        "nested level-3 payload must reject unknown fields"
    );
    let nested_props = &nested["properties"];
    assert!(
        nested_props.get("amount").is_some(),
        "level-3 own field `amount` missing from emitted schema: {}",
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert!(
        nested_props.get("label").is_some(),
        "level-3 own field `label` missing from emitted schema: {}",
        serde_json::to_string_pretty(&v).unwrap()
    );
}

#[test]
fn gts_instance_split_instance_id_handles_three_level_chain() {
    // Instance id has TWO interior `~` separators; the macro must pick
    // the rightmost one as the schema/instance boundary, yielding the
    // full level-2 derived schema as `type_id`.
    let entry = find_instance(
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.sample.v1",
    );
    assert_eq!(
        entry.type_id, "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~",
        "3-level instance_id must split at the LAST `~` (level-2 derived schema is the type)"
    );
    // And `type_id` tracks `DeepLevel2V1::SCHEMA_ID` via the turbofish.
    assert_eq!(entry.type_id, DeepLevel2V1::<()>::SCHEMA_ID);

    let payload = (entry.payload_fn)();
    assert_eq!(
        payload["id"],
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.sample.v1",
        "auto-injected `id` equals the full instance_id literal"
    );
    // Sanity on the raw-JSON payload shape.
    assert_eq!(payload["payload"]["name"], "deep-sample");
}

// ---------- Instance-of-level-3-schema via typed form ----------
//
// The instance carries `amount` and `label` — fields owned by
// `DeepLevel3V1`. So the instance *conforms to `DeepLevel3V1` schema*,
// which sits at level-3 in the chain; the resulting instance_id is
// `DeepLevel3V1::SCHEMA_ID + segment` = 4 total `~`-separated segments.
//
// Writing `instance = DeepLevel3V1 { ... }` directly doesn't compile:
// derived types don't get `serde::Serialize` (only `GtsSerialize`) and
// they don't carry an `id` field (id lives on the base). The macro can't
// inject `id:` into them, and `serde_json::to_value(&DerivedType)` won't
// typecheck.
//
// `schema = Type` decouples two roles:
//   - **what the instance conforms to** (schema prefix)   → `schema = ...`
//   - **what the macro actually serialises**              → `instance = Base { ... }`
//
// So we pass `schema = DeepLevel3V1` (for SCHEMA_ID) and wrap the runtime
// value through `DeepLevel1V1::<DeepLevel2V1<DeepLevel3V1>>` (the base
// type that actually implements `Serialize` and has the `id` field).

gts_instance! {
    segment = "test.deep.core.typed_lvl4.v1",
    schema = DeepLevel3V1,
    instance = DeepLevel1V1::<DeepLevel2V1<DeepLevel3V1>> {
        payload: DeepLevel2V1::<DeepLevel3V1> {
            name: "typed-level4".to_owned(),
            nested: DeepLevel3V1 {
                amount: 99,
                label: "via_schema_param".to_owned(),
            },
        },
    }
}

#[test]
fn gts_instance_typed_level3_schema_via_schema_override() {
    // Expected: type_id = DeepLevel3V1::SCHEMA_ID (level-3 derived schema —
    // the full chain `gts.A.v1~B.v1~C.v1~`). Full instance_id = prefix +
    // segment → 4 `~`-separated segments total.
    let expected_type_id =
        "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~";
    let expected_full_id = "gts.test.deep.core.level1.v1~test.deep.core.level2.v1~test.deep.core.level3.v1~test.deep.core.typed_lvl4.v1";
    let entry = find_instance(expected_full_id);

    assert_eq!(
        entry.type_id, expected_type_id,
        "type_id must come from `schema = DeepLevel3V1` (level-3 SCHEMA_ID), not from the serialised struct DeepLevel1V1"
    );
    // Verify the invariant against the trait const directly.
    assert_eq!(entry.type_id, DeepLevel3V1::SCHEMA_ID);

    // The serialised payload went through DeepLevel1V1 (the base) — its
    // Serialize impl produced a full base-shaped JSON with `id` and
    // `payload` at the top level. Auto-injection put the level-4 id on
    // the outer struct's `id` field.
    let payload = (entry.payload_fn)();
    assert_eq!(
        payload["id"], expected_full_id,
        "auto-injected `id` = level-3 SCHEMA_ID (via schema override) + segment"
    );

    // Nested shape comes from the allOf chain:
    // - DeepLevel1V1's own field: `payload` (generic slot)
    // - DeepLevel2V1's own fields nested inside that: `name`, `nested`
    // - DeepLevel3V1's own fields nested one deeper: `amount`, `label`
    assert_eq!(payload["payload"]["name"], "typed-level4");
    assert_eq!(payload["payload"]["nested"]["amount"], 99);
    assert_eq!(payload["payload"]["nested"]["label"], "via_schema_param");
}

// ---------- `name = IDENT` — typed runtime accessor ----------

#[test]
fn gts_instance_with_name_emits_typed_static_with_populated_id() {
    // The static NAMED_INSTANCE must exist (compile-proof of macro emit),
    // be of type `LazyLock<TestThingV1>`, and carry the correct auto-injected
    // `id` plus user-provided fields.
    let inst: &TestThingV1 = &NAMED_INSTANCE;

    let expected_id_str = "gts.test.core.macro.thing.v1~test.macro.instance.named.v1";
    assert_eq!(inst.id.as_ref(), expected_id_str);
    assert_eq!(inst.name, "named");

    // Inventory submission must still happen alongside the static — the
    // `name = ...` parameter is additive, not substitutive.
    let entry = find_instance(expected_id_str);
    assert_eq!(entry.type_id, TestThingV1::SCHEMA_ID);
    let payload = (entry.payload_fn)();
    assert_eq!(payload["id"], expected_id_str);
    assert_eq!(payload["name"], "named");
}

#[test]
fn gts_instance_named_static_is_lazy_and_stable_across_calls() {
    // `LazyLock` guarantees single initialisation; subsequent accesses
    // must return references to the same storage. Good for callers that
    // want to pattern-match or cache pointers.
    let a: *const TestThingV1 = &raw const *NAMED_INSTANCE;
    let b: *const TestThingV1 = &raw const *NAMED_INSTANCE;
    assert_eq!(
        a, b,
        "`LazyLock`-backed static must be stable across accesses"
    );
}
