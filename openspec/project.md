# Project Context

## Purpose
HyperSpot Server is a modular, high-performance platform for building AI services in Rust. Built on the **ModKit** framework, it provides:

- **Modular architecture** — Everything is a Module with composable, independent units
- **Type safety** — Compile-time guarantees via typestate builders and trait-based APIs
- **Multi-tenancy** — Built-in tenant isolation with secure ORM layer
- **Database agnostic** — PostgreSQL, MySQL, SQLite via unified API
- **GTS extensibility** — Global Type System for versioned, pluggable extensions

## Tech Stack
- **Language:** Rust (stable)
- **Web Framework:** Axum (HTTP), tonic (gRPC)
- **ORM:** SeaORM with SQLx
- **Databases:** PostgreSQL, MySQL, SQLite
- **API Spec:** OpenAPI 3.1 via utoipa
- **Testing:** cargo test, pytest (E2E), testcontainers
- **Linting:** clippy (pedantic), custom dylint lints
- **Build:** cargo, make

## Project Conventions

### Code Style
- **Line length:** 100 characters max
- **Indentation:** 4 spaces
- **Trailing commas:** Required in multi-line expressions
- **Formatting:** `cargo fmt` (rustfmt)
- **No unwrap/expect:** Use proper Result types (clippy denies this)
- **Structured logging:** `tracing::info!(field = value, "message")`

### Architecture Patterns

**DDD-Light Layer Architecture** (per module):
```
modules/<module-name>/src/
├── lib.rs              # Public exports
├── module.rs           # Module trait implementations
├── config.rs           # Typed configuration
├── contract/           # PUBLIC API (inter-module communication)
│   ├── client.rs       # Trait definitions for ClientHub
│   ├── model.rs        # Transport-agnostic domain models
│   └── error.rs        # Domain errors
├── api/                # TRANSPORT ADAPTERS
│   └── rest/           # HTTP layer
│       ├── dto.rs      # DTOs with serde/utoipa (REST-specific)
│       ├── handlers.rs # Axum handlers
│       └── routes.rs   # OperationBuilder registration
├── domain/             # BUSINESS LOGIC
│   ├── service.rs      # Orchestration and business rules
│   └── model.rs        # Rich domain models
└── infra/              # INFRASTRUCTURE
    └── storage/        # Database layer
        ├── entity.rs   # SeaORM entities
        ├── mapper.rs   # Entity <-> Contract conversions
        └── migrations/ # Database migrations
```

**Critical separation rules (enforced by linters):**
1. **Contract layer** — NO serde, NO utoipa, NO HTTP types (pure domain)
2. **API/REST layer** — DTOs MUST have serde + utoipa, MUST be in `api/rest/`
3. **REST endpoints** — MUST follow `/{service-name}/v{N}/{resource}` pattern
4. **DTO isolation** — DTOs only referenced within `api/rest/`, not from domain/contract

**Inter-Module Communication:**
- Type-safe ClientHub pattern: `hub.get::<dyn MyApi>()?`
- Module registration via `inventory` crate (compile-time)
- Topological initialization based on dependencies

### Testing Strategy
- **Target:** 90%+ code coverage
- **Unit tests:** Domain logic, mappers, utilities
- **Integration tests:** Database interactions, module wiring (testcontainers)
- **E2E tests:** Full request flows via Python/pytest
- **Commands:**
  - `cargo test` — All unit tests
  - `make test-sqlite`, `make test-pg`, `make test-mysql` — DB integration
  - `make e2e-local` / `make e2e-docker` — E2E tests
  - `make coverage` — Coverage report

### Git Workflow
**Commit Convention:** `<type>(<scope>): <description>`

Types: `feat`, `fix`, `tech`, `cleanup`, `refactor`, `test`, `docs`, `style`, `chore`, `perf`, `ci`, `build`, `revert`, `security`, `breaking`

**DCO Required:**
```bash
git commit -s -m "feat(api): add user authentication"
```

**CI Checks (all PRs must pass):**
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `make dylint`

Run locally: `make ci` or `make check`

## Domain Context

**ModKit Framework Core Concepts:**
- **Module** — Self-contained unit with lifecycle, dependencies, and capabilities
- **ClientHub** — Type-safe inter-module communication via trait resolution
- **SecureORM** — Request-scoped security context with automatic tenant isolation
- **GTS** — Global Type System for versioned, pluggable extensions
- **OperationBuilder** — Type-state builder for compile-time REST route safety

**Module Lifecycle:**
```
Stopped → init() → migrate() → register_rest() → start() → Running → stop() → Stopped
```

**REST API Conventions:**
- Endpoint pattern: `/{service-name}/v{N}/{resource}`
- Error handling: RFC-9457 Problem Details
- Pagination: OData-style with cursor-based support
- Filter syntax: `field.op=value` (e.g., `status.in=open,urgent`)

## OpenSpec Conventions

### ID Formats

| Type | Format | Example |
|------|--------|---------|
| Phase | `{MODULE}-P{N}` | OAGW-P1 |
| Requirement | `{MODULE}-REQ{N}` | OAGW-REQ01 |
| Global Requirement | `REQ{N}` | REQ1 |

Tasks use OpenSpec native format (no custom IDs).

### Two-Tier Requirement System

```
docs/MODULE_REQUIREMENTS.md             Global (REQ1, REQ2, REQ3...)
         ↓ referenced by
modules/{m}/docs/REQUIREMENTS.md        Module-specific (OAGW-REQ01, OAGW-REQ02...)
```

### Module Naming

Specs use `module-{name}` folder naming:
- `module-oagw` (Outbound API Gateway)
- `module-api-ingress` (API Ingress)

### Cross-References

Requirements can reference:
- Global requirements: `(REQ1)`
- Other module requirements: `(OAGW-REQ02)`
- Phases: `(OAGW-P1)`

### Status Indicators

- ✅ Implemented
- 🚧 In Progress
- ⏳ Planned
- ❌ Deprecated

### Module Documentation Structure

Each module has three docs in `modules/{module}/docs/`:
- **DESIGN.md** — Architecture + big phases (milestones)
- **IMPLEMENTATION_PLAN.md** — Trackable features/stories (checkboxes)
- **REQUIREMENTS.md** — Module-specific requirements ({MODULE}-REQ{N})

## Important Constraints

**Architecture Enforcement (via dylint lints):**
- **DE01xx** — Contract layer purity (no serde, no utoipa, no HTTP types)
- **DE02xx** — API layer conventions (DTOs in api/rest/, must have serde+utoipa)
- **DE08xx** — REST endpoint versioning required

**Security:**
- Request-scoped SecurityCtx (never store in services)
- Implicit deny-all for database queries (empty scope = `WHERE 1=0`)
- No `unwrap_used` or `expect_used` (proper error handling required)

**Configuration Precedence:**
1. YAML config file (`--config`)
2. Environment variables (`HYPERSPOT_*` prefix)
3. Default values in code

## External Dependencies

**Key Libraries:**
- `axum` — HTTP framework
- `tonic` — gRPC framework
- `sea-orm` / `sqlx` — Database ORM and driver
- `utoipa` — OpenAPI 3.1 generation
- `inventory` — Compile-time module registration
- `arc-swap` — Lock-free read-heavy shared state

**Development Tools:**
- `cargo-llvm-cov` — Code coverage
- `cargo-deny` — License/dependency checks
- `cargo-audit` — Security vulnerabilities
- `dylint` — Custom architecture lints
- `testcontainers` — Integration test databases

**Documentation:**
- Architecture: `docs/ARCHITECTURE_MANIFEST.md`
- ModKit Guide: `docs/MODKIT_UNIFIED_SYSTEM.md`
- Plugin System: `docs/MODKIT_PLUGINS.md`
- Secure ORM: `docs/SECURE-ORM.md`
- New Module: `guidelines/NEW_MODULE.md`
