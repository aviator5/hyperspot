# Global Requirements

This document defines cross-cutting requirements that apply to all modules in the HyperSpot Server platform. Module-specific requirements are defined in `modules/{module}/docs/REQUIREMENTS.md` and reference these global requirements.

## REQ-1: Tenant Isolation

All database queries MUST be scoped to the requesting tenant.

**Details:**
- The system SHALL enforce tenant isolation at the SecureORM layer
- Queries without explicit security context SHALL return empty results (`WHERE 1=0`)
- Tenant ID MUST be extracted from the authenticated request context
- Cross-tenant data access MUST be explicitly denied unless authorized

**Reference:** `docs/SECURE-ORM.md`

---

## REQ-2: Request Tracing

All HTTP requests MUST include trace context for observability.

**Details:**
- The system SHALL propagate W3C Trace Context headers
- All log entries MUST include trace_id and span_id
- Structured logging format: `tracing::info!(field = value, "message")`
- Inter-module calls MUST preserve trace context

**Reference:** `docs/TRACING_SETUP.md`

---

## REQ-3: Authorization

All API endpoints MUST enforce authorization checks.

**Details:**
- The system SHALL use request-scoped SecurityCtx
- SecurityCtx MUST NOT be stored in long-lived services
- Authorization failures SHALL return 403 Forbidden with Problem Details
- Default policy: implicit deny-all

---

## REQ-4: Error Handling

All API errors MUST conform to RFC-9457 Problem Details.

**Details:**
- Error responses SHALL include: type, title, status, detail
- The system SHALL use `modkit::api::problem` utilities
- Internal errors MUST be logged but not exposed to clients
- Standard error responses: 400, 401, 403, 404, 409, 422, 429, 500

---

## REQ-5: REST API Versioning

All REST endpoints MUST include version in the path.

**Details:**
- Endpoint pattern: `/{service-name}/v{N}/{resource}`
- Breaking changes MUST increment the version number
- Multiple versions MAY be supported simultaneously
- Enforced by: dylint lint DE08xx

---

## REQ-6: Layer Separation

Module code MUST follow DDD-Light layer architecture.

**Details:**
- Contract layer: NO serde, NO utoipa, NO HTTP types (enforced by DE01xx)
- API/REST layer: DTOs MUST have serde + utoipa, MUST be in `api/rest/` (enforced by DE02xx)
- Domain layer: Business logic, no infrastructure dependencies
- Infrastructure layer: Database, external services

**Reference:** `docs/ARCHITECTURE_MANIFEST.md`

---

## REQ-7: Database Operations

Database operations MUST use SecureORM with proper scoping.

**Details:**
- All entities with tenant data MUST derive `Scopable`
- Queries MUST use `secure_conn.find::<Entity>(&ctx)?`
- Migrations MUST be defined in `infra/storage/migrations/`
- Support: PostgreSQL, MySQL, SQLite

**Reference:** `docs/SECURE-ORM.md`

---

## REQ-8: Configuration

Configuration MUST follow the precedence hierarchy.

**Details:**
- Precedence: YAML config → Environment variables → Code defaults
- Environment prefix: `HYPERSPOT_*`
- Secrets MUST NOT be logged or exposed in error messages
- Module configs in `modules.{module}.*` namespace

---

## REQ-9: Testing

All modules MUST maintain adequate test coverage.

**Details:**
- Target: 90%+ code coverage
- Unit tests: Domain logic, mappers, utilities
- Integration tests: Database interactions via testcontainers
- E2E tests: Full request flows via Python/pytest

---

## REQ-10: Code Quality

Code MUST pass all CI quality checks.

**Details:**
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo deny check`
- `make dylint`
- No `unwrap_used` or `expect_used`

---

## How to Reference

In module requirements (`modules/{module}/docs/REQUIREMENTS.md`):
```markdown
## REQ-OAGW-01: Request Forwarding
The gateway SHALL forward requests to configured upstream services.

**References:** (REQ-1), (REQ-2), (REQ-4)
```

In OpenSpec scenarios (`openspec/specs/module-{module}/spec.md`):
```markdown
#### SCEN-OAGW-01: Successful forwarding
- **WHEN** valid request with tenant context
- **THEN** forward to upstream (REQ-OAGW-01)
- **AND** enforce tenant isolation (REQ-1)
```
