#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
//! `ModKit` GTS integration.
//!
//! This crate bridges Rust types to the [Global Type System] (GTS) used by
//! Cyber Fabric. It provides three things:
//!
//! 1. **Link-time inventory** of GTS schemas and instances — collectors
//!    populated at link time via the `inventory` crate. Any crate in the
//!    process that uses the macros below contributes to the same global
//!    inventory. `types-registry` consumes the inventory at startup, so
//!    there is no per-module registration code for entries known at
//!    compile time.
//! 2. **Proc-macros** that hide the `inventory::submit!` +
//!    `#[gts_macros::struct_to_gts_schema]` boilerplate:
//!    - `#[gts_schema(...)]` — register a GTS schema.
//!    - `gts_instance!` — register a well-known GTS instance.
//! 3. **Platform base types** — a small shipped set of GTS base schemas
//!    ([`PluginV1`], [`AuthzPermissionV1`]) used across the platform.
//!
//! ## Adding a new entry
//!
//! For a new platform base schema, put a `#[gts_schema(...)]`-annotated
//! struct into this crate and add `mod your_module;` below. For a schema
//! or instance owned by a specific module, use the same macros from that
//! module's crate — the inventory is process-global, so entries land in
//! `types-registry` regardless of which crate declares them.
//!
//! [Global Type System]: https://github.com/hypernetix/gts-spec

pub mod permission;
pub mod plugin;

pub use permission::AuthzPermissionV1;
pub use plugin::PluginV1;

// Re-export GTS primitives used by the `gts_instance!` macro's typed form.
// The macro expands `id: <crate>::GtsInstanceId::new(<type_id>, <segment>)`
// and `<crate>::GtsSchema::SCHEMA_ID` for the `segment = "..."` short form;
// consumers don't need to depend on the `gts` crate directly.
pub use gts::{GtsInstanceId, GtsSchema};

// Re-export `const_format` so the `gts_instance!` macro can emit
// `<crate>::const_format::concatcp!(SCHEMA_ID, <segment>)` for compile-time
// instance-id construction without requiring consumers to add a direct dep.
#[doc(hidden)]
pub use const_format;

// Re-export `inventory` so the `#[gts_schema(...)]` and `gts_instance!`
// macros can emit `<crate>::inventory::submit!` without requiring consumer
// crates to add `inventory` as a direct dep.
#[doc(hidden)]
pub use inventory;

// Re-export the companion proc-macros so consumers need only one crate dep.
pub use modkit_gts_macro::{gts_instance, gts_instance_raw, gts_schema};

/// Registration record for a GTS schema contributed to the process-wide
/// inventory.
///
/// Each `#[gts_schema(...)]`-annotated type submits one of these via
/// `inventory::submit!` at macro-expansion time. The `schema_fn` lazily
/// invokes the macro-generated accessor (`gts_schema_with_refs_as_string`)
/// to produce the JSON schema on demand.
#[derive(Clone)]
pub struct InventorySchema {
    /// GTS schema identifier (e.g. `gts.cf.modkit.authz.permission.v1~`).
    pub schema_id: &'static str,
    /// Lazy accessor returning the schema as a JSON string.
    pub schema_fn: fn() -> String,
}

/// Registration record for a well-known GTS instance contributed to the
/// process-wide inventory.
///
/// Submitted by the `gts_instance! { ... }` macro. `type_id` is derived at
/// macro-expansion time from the last `~` in the full instance id.
#[derive(Clone)]
pub struct InventoryInstance {
    /// GTS type identifier the instance conforms to (prefix of the full
    /// instance id up to and including the last `~`).
    pub type_id: &'static str,
    /// Full GTS instance identifier.
    pub instance_id: &'static str,
    /// Lazy accessor returning the instance payload as JSON (with `id`
    /// auto-injected by the macro).
    pub payload_fn: fn() -> serde_json::Value,
}

inventory::collect!(InventorySchema);
inventory::collect!(InventoryInstance);

/// Returns every GTS schema declared via `#[gts_schema(...)]` in any crate
/// linked into the current process.
///
/// Source of truth for each schema is its Rust struct via the
/// macro-generated `gts_schema_with_refs_as_string` accessor (no
/// hand-written JSON, no ZIP).
///
/// # Errors
///
/// Returns an error if any registered schema accessor produces invalid
/// JSON. This should be impossible with a correctly-applied `#[gts_schema]`
/// macro and signals a macro regression.
pub fn all_inventory_schemas() -> anyhow::Result<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for entry in inventory::iter::<InventorySchema> {
        let schema_str = (entry.schema_fn)();
        let value: serde_json::Value = serde_json::from_str(&schema_str).map_err(|e| {
            anyhow::anyhow!(
                "invalid JSON schema emitted by GTS type {}: {e}",
                entry.schema_id
            )
        })?;
        out.push(value);
    }
    Ok(out)
}

/// Returns every well-known GTS instance declared via `gts_instance!` in
/// any crate linked into the current process.
///
/// # Errors
///
/// Currently never fails (each `payload_fn` is a macro-generated
/// `serde_json::json!` invocation that cannot panic at runtime). The
/// `Result` return type is kept for symmetry with [`all_inventory_schemas`]
/// and for future flexibility.
pub fn all_inventory_instances() -> anyhow::Result<Vec<serde_json::Value>> {
    Ok(inventory::iter::<InventoryInstance>
        .into_iter()
        .map(|entry| (entry.payload_fn)())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{
        InventoryInstance, InventorySchema, all_inventory_instances, all_inventory_schemas,
    };

    #[test]
    fn platform_base_schemas_are_registered_and_valid() {
        let schemas = all_inventory_schemas().expect("schemas collect cleanly");

        // Both platform base types shipped by this crate must be present.
        let ids: Vec<&str> = inventory::iter::<InventorySchema>
            .into_iter()
            .map(|e| e.schema_id)
            .collect();
        assert!(
            ids.contains(&"gts.cf.modkit.plugins.plugin.v1~"),
            "PluginV1 not registered; got ids: {ids:?}"
        );
        assert!(
            ids.contains(&"gts.cf.modkit.authz.permission.v1~"),
            "AuthzPermissionV1 not registered; got ids: {ids:?}"
        );
        assert_eq!(
            schemas.len(),
            ids.len(),
            "iter vs aggregated count mismatch (did all entries collect cleanly?)"
        );

        for (idx, s) in schemas.iter().enumerate() {
            assert!(s.is_object(), "schema #{idx} is not a JSON object: {s}");
            assert!(s.get("$id").is_some(), "schema #{idx} missing $id: {s}");
            assert!(
                s.get("type").is_some(),
                "schema #{idx} missing top-level type: {s}"
            );
        }
    }

    #[test]
    fn inventory_instances_registry_is_consistent() {
        // No instances ship from this crate, but the collector path must
        // still run. (External crates may contribute instances; this crate
        // only checks self-consistency.)
        let instances = all_inventory_instances().expect("instances collect cleanly");
        let ids: Vec<&str> = inventory::iter::<InventoryInstance>
            .into_iter()
            .map(|e| e.instance_id)
            .collect();
        assert_eq!(
            instances.len(),
            ids.len(),
            "iter vs aggregated count mismatch"
        );
    }
}
