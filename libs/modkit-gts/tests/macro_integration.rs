//! Integration tests for the `cyberware-modkit-gts-macro` wrappers.
//!
//! Proc-macros can't be tested from inside the crate that defines them,
//! so the tests live in the consumer crate. The wrappers are thin
//! pass-throughs to upstream `gts-macros` plus exactly one extra
//! emission — an `inventory::submit!` block. These tests only verify
//! that the inventory submission lands; everything else (id validation,
//! prefix const-asserts, schema JSON shape, `pub static` binding
//! emission) is upstream's contract and is covered by upstream's own
//! tests.

use modkit_gts::{
    GtsInstanceId, GtsSchema, InventoryInstance, InventorySchema, gts_instance, gts_instance_raw,
    gts_type_schema,
};

// =====================================================================
//                              Test types
// =====================================================================

/// Base test schema. The wrapper's only job here is to:
/// 1. Forward the attribute to `gts_macros::struct_to_gts_schema`
///    (so `TestThingV1::SCHEMA_ID` and the schema accessor exist).
/// 2. Submit an `InventorySchema` entry with the same `schema_id`.
#[gts_type_schema(
    dir_path = "schemas",
    schema_id = "gts.test.cf.modkit_gts.thing.v1~",
    description = "Test base type for modkit-gts wrapper integration tests",
    properties = "id,name",
    base = true
)]
pub struct TestThingV1 {
    pub id: GtsInstanceId,
    pub name: String,
}

// `gts_instance_raw!` — submits one inventory entry; the value itself
// is built lazily by the closure inside `payload_fn`.
gts_instance_raw!({
    "id": "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.raw.v1",
    "name": "raw",
});

// `gts_instance!` (typed) — same: one inventory entry, value built lazily.
gts_instance! {
    TestThingV1 {
        id: "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.typed.v1",
        name: "typed".to_owned(),
    }
}

// `gts_instance!` with `#[gts_static(NAME)]` — additionally emits a
// `pub static NAME: LazyLock<TestThingV1>` via upstream alongside the
// inventory submission. The wrapper's own job is just the inventory
// part; the static binding is upstream's emission.
gts_instance! {
    #[gts_static(NAMED_INSTANCE)]
    TestThingV1 {
        id: "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.named.v1",
        name: "named".to_owned(),
    }
}

// =====================================================================
//                                Tests
// =====================================================================

const TYPE_ID: &str = "gts.test.cf.modkit_gts.thing.v1~";
const RAW_ID: &str = "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.raw.v1";
const TYPED_ID: &str = "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.typed.v1";
const NAMED_ID: &str = "gts.test.cf.modkit_gts.thing.v1~test.cf.modkit_gts.named.v1";

fn schema_ids() -> Vec<&'static str> {
    inventory::iter::<InventorySchema>
        .into_iter()
        .map(|e| e.schema_id)
        .collect()
}

fn instance_ids() -> Vec<&'static str> {
    inventory::iter::<InventoryInstance>
        .into_iter()
        .map(|e| e.instance_id)
        .collect()
}

fn find_instance(id: &str) -> &'static InventoryInstance {
    inventory::iter::<InventoryInstance>
        .into_iter()
        .find(|e| e.instance_id == id)
        .unwrap_or_else(|| panic!("instance {id} not in inventory; got: {:?}", instance_ids()))
}

#[test]
fn gts_type_schema_wrapper_registers_inventory_schema() {
    // Wrapper contract: `#[gts_type_schema(...)]` adds an `InventorySchema`
    // entry whose `schema_id` matches the attribute literal. Upstream gives
    // us `TestThingV1::SCHEMA_ID` for free — we just check the wrapper's
    // contribution lined up with it.
    let ids = schema_ids();
    assert!(
        ids.contains(&TYPE_ID),
        "TestThingV1's schema not registered; got: {ids:?}"
    );
    assert_eq!(
        TestThingV1::SCHEMA_ID,
        TYPE_ID,
        "wrapper's schema_id literal must match upstream's SCHEMA_ID const",
    );
}

#[test]
fn gts_type_schema_wrapper_schema_fn_returns_well_formed_json() {
    // The wrapper plugs `gts_schema_with_refs_as_string` (upstream's
    // generated accessor) into `InventorySchema::schema_fn`. The string
    // must parse as JSON.
    let entry = inventory::iter::<InventorySchema>
        .into_iter()
        .find(|e| e.schema_id == TYPE_ID)
        .expect("test schema present");
    let s = (entry.schema_fn)();
    let v: serde_json::Value =
        serde_json::from_str(&s).expect("schema_fn output must parse as JSON");
    assert!(v.is_object(), "schema must be a JSON object");
}

#[test]
fn gts_instance_raw_wrapper_registers_inventory_instance() {
    let entry = find_instance(RAW_ID);
    assert_eq!(
        entry.type_id, TYPE_ID,
        "raw wrapper must derive type_id from the instance_id prefix"
    );
    let payload = (entry.payload_fn)();
    assert_eq!(
        payload["id"], RAW_ID,
        "upstream auto-injects `id` into the JSON payload"
    );
    assert_eq!(payload["name"], "raw");
}

#[test]
fn gts_instance_typed_wrapper_registers_inventory_instance() {
    let entry = find_instance(TYPED_ID);
    assert_eq!(entry.type_id, TYPE_ID);
    let payload = (entry.payload_fn)();
    assert_eq!(payload["id"], TYPED_ID);
    assert_eq!(payload["name"], "typed");
}

#[test]
fn gts_instance_with_gts_static_emits_both_inventory_and_typed_static() {
    // The wrapper's own contribution is the inventory entry; the static
    // binding `NAMED_INSTANCE` is upstream's emission. Verify both showed
    // up so the wrapper isn't accidentally suppressing one.
    let entry = find_instance(NAMED_ID);
    assert_eq!(entry.type_id, TYPE_ID);
    let payload = (entry.payload_fn)();
    assert_eq!(payload["id"], NAMED_ID);

    // Typed runtime accessor: the macro-emitted static carries the same id.
    let inst: &TestThingV1 = &NAMED_INSTANCE;
    assert_eq!(inst.id.as_ref(), NAMED_ID);
    assert_eq!(inst.name, "named");
}
