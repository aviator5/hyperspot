# ModKit

Declarative module system and common runtime utilities used across CyberWare.

## Overview

The `cyberware-modkit` crate provides:

- Module registration and lifecycle (inventory-based discovery)
- `ClientHub` for typed in-process clients
- REST/OpenAPI helpers (`OperationBuilder`, `OpenApiRegistry`, RFC-9457 `Problem`)
- Runtime helpers (module registry/manager, lifecycle helpers)

## Features

- **`db` (default)**: Enables DB integration (depends on `cyberware-modkit-db`), including:
  - `DatabaseCapability` (migrations contract)
  - `DbOptions::Manager` (runtime DB manager support)
  - DB handle resolution in `ModuleCtx` / `ModuleContextBuilder`

### Build without DB

To build `cyberware-modkit` without pulling in `cyberware-modkit-db` and its transitive dependencies:

```bash
cargo build -p cyberware-modkit --no-default-features
```

## License

Licensed under Apache-2.0.
