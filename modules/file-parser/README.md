# File Parser Module

File parsing module for CyberWare / ModKit.

## Overview

The `cyberware-file-parser` crate implements the `file-parser` module and registers REST routes.

All document extraction is handled by a single unified backend — [`kreuzberg =4.9.4`](https://github.com/kreuzberg-dev/kreuzberg) — which replaces the previous per-format library set (`tl`, `pdf-extract`, `calamine`, `pptx-to-md`).

Supported formats:

| Extension(s)              | Format                          |
|---------------------------|---------------------------------|
| `pdf`                     | PDF                             |
| `html`, `htm`             | HTML                            |
| `xlsx`, `xls`, `xlsm`, `xlsb` | Excel spreadsheets         |
| `pptx`                    | PowerPoint presentations        |

## Configuration

```yaml
modules:
  file-parser:
    config:
      max_file_size_mb: 100
      # Required. Only files under this directory are accessible via parse-local.
      # Symlinks that resolve outside this directory are also blocked.
      allowed_local_base_dir: /data/documents
```

### Security: Local Path Restrictions

The `parse-local` endpoints validate requested file paths before any filesystem access:

1. Paths containing `..` components are always rejected.
2. The requested path is canonicalized (symlinks resolved) and must fall under `allowed_local_base_dir`.
3. `allowed_local_base_dir` is **required** — the module will fail to start if it is missing or the path cannot be resolved.

## License

This module is licensed under **Apache-2.0**.

### Third-party dependency: kreuzberg

This module depends on [`kreuzberg`](https://github.com/kreuzberg-dev/kreuzberg), pinned at **`=4.9.4`** ([Elastic License 2.0](https://www.elastic.co/licensing/elastic-license)).

| Version range | License |
|---------------|---------|
| `≤ 4.7.4` | MIT |
| `≥ 4.8.0` (including `=4.9.4` used here) | [Elastic License 2.0 (EL-2.0)](https://www.elastic.co/licensing/elastic-license) |

> ℹ️ **EL-2.0 is permitted for this use case.** The `deny.toml` license policy includes
> an explicit exception for `kreuzberg =4.9.4` with documented rationale:
> CyberWare's document parsing is incidental to the platform — it is not sold as a
> standalone document-parsing product competing with kreuzberg.
>
> **EL-2.0 key restrictions to be aware of:**
>
> - You may **not** provide the software (or a product whose primary functionality is
>   substantially the same as kreuzberg) to third parties as a hosted or managed service.
> - You may **not** build a product sold *primarily* as a document-parsing service that
>   competes with kreuzberg.
>
> The dependency is pinned with `=4.9.4` in `Cargo.toml` to prevent silent upgrades.
> Any version bump **must** be reviewed for license changes and approved by the
> maintainers before merging.

