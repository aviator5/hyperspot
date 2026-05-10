# Tenant Resolver

Main module for tenant resolution in Cyber Ware. Discovers plugins via GTS types-registry and routes tenant operations to the selected plugin.

## Overview

The `cyberware-tenant-resolver` module provides:

- **Plugin discovery** — Finds tenant-resolver plugins via GTS types-registry
- **Vendor-based selection** — Selects plugin by vendor and priority
- **Self-access enforcement** — Source == target tenant is always allowed
- **ClientHub integration** — Registers `TenantResolverClient` for inter-module use

This is a **main module** — it contains no tenant data itself. All operations are delegated to the active plugin (e.g., `cyberware-static-tr-plugin`, `cf-single-tenant-tr-plugin`, or a custom implementation).

## Architecture

```
Consumer Module
    │
    ▼
TenantResolverClient  (SDK trait, registered in ClientHub)
    │
    ▼
tenant-resolver gateway  (this crate — discovers & routes)
    │
    ▼
TenantResolverPluginClient  (SDK trait, scoped by GTS instance ID)
    │
    ▼
Plugin implementation  (provides tenant data)
```

## Usage

Consumers interact with the resolver through the `TenantResolverClient` trait from the SDK:

```rust
use tenant_resolver_sdk::TenantResolverClient;

let resolver = hub.get::<dyn TenantResolverClient>()?;

// Get single tenant
let tenant = resolver.get_tenant(&ctx, tenant_id).await?;

// Batch get
let tenants = resolver.get_tenants(&ctx, &[id1, id2], &GetTenantsOptions::default()).await?;

// Hierarchy traversal
let ancestors = resolver.get_ancestors(&ctx, tenant_id, &GetAncestorsOptions::default()).await?;
let descendants = resolver.get_descendants(&ctx, tenant_id, &GetDescendantsOptions::default()).await?;

// Ancestry check
let is_anc = resolver.is_ancestor(&ctx, parent_id, child_id, &IsAncestorOptions::default()).await?;
```

## Configuration

The module is configured via the server's YAML config. Plugin selection is automatic based on GTS registration.

## Writing a Plugin

Implement the `TenantResolverPluginClient` trait from `cyberware-tenant-resolver-sdk` and register it with a GTS instance ID derived from the `TenantResolverPluginSpecV1` schema.

## Testing

```bash
cargo test -p cyberware-tenant-resolver
```

## License

Apache-2.0
