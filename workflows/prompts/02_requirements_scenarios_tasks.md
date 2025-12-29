# Prompt: Requirements, Scenarios, and Tasks Generation

## Variables

- `{module_name}` - Module name in snake_case (e.g., `oagw`, `cred_store`)
- `{MODULE}` - Module prefix in UPPERCASE (e.g., `OAGW`, `CREDSTORE`)

## Prompt - Part 1: Generate Requirements and Scenarios

```
Read: modules/{module_name}/docs/1_INTENT_DESIGN.md

Generate:
- modules/{module_name}/docs/2_INTENT_REQUIREMENTS.md
- modules/{module_name}/docs/3_INTENT_SCENARIOS.md

## 2_INTENT_REQUIREMENTS.md Rules

- Module-specific requirements use ID: REQ-{MODULE}-{N} (e.g., REQ-OAGW-42)
- Global requirements (REQ-N) are defined in `guidelines/MODULE_REQUIREMENTS.md` — do NOT duplicate them
- Use **MUST**, **SHALL** for mandatory; **MAY**, **SHOULD** for optional
- Each requirement should be atomic and testable
- Can reference: global requirements `(REQ-N)`, phases from design `(PHASE-{MODULE}-N)`

Example:
```markdown
### REQ-{MODULE}-42: Configurable Timeout
Each endpoint MAY have a custom timeout configuration that overrides the default.
- Default timeout: 30 seconds
- Configurable per endpoint in module config
- References: REQ-1 (tenant isolation must still apply)
- Introduced in: PHASE-{MODULE}-2

### REQ-{MODULE}-43: Upstream Health Check
The gateway MUST verify upstream health before forwarding requests.
- Health check interval: configurable (default 10s)
- Unhealthy upstreams are excluded from routing
```

Note: Global requirements like tenant isolation (REQ-1), tracing (REQ-10), error format (REQ-20)
are already defined in `guidelines/MODULE_REQUIREMENTS.md`. Reference them directly.

## 3_INTENT_SCENARIOS.md Rules

- If phases are used, organize scenarios by phase sections (`## Phase 1`, `## Phase 2`, etc.)
- If no phases, just list scenarios without section headers
- Each scenario has a unique, phase-agnostic ID: SCEN-{MODULE}-{N} (e.g., SCEN-OAGW-01)
- No sub-scenarios — each case gets its own ID
- Use **WHEN**/**THEN**/**AND** bullets
- Can reference: global requirements `(REQ-N)`, module requirements `(REQ-{MODULE}-N)`, phases `(PHASE-{MODULE}-N)`

Example with phases:
```markdown
## Phase 1

### SCEN-{MODULE}-01: Process valid request
- **WHEN** a valid request is received with tenant context
- **THEN** tenant isolation is enforced (REQ-1)
- **AND** the response includes only tenant-scoped data
- **AND** a tracing span is created (REQ-10)

### SCEN-{MODULE}-02: Reject unauthorized request
- **WHEN** a request is received without valid tenant context
- **THEN** HTTP 403 Forbidden is returned (REQ-1, REQ-20)

## Phase 2

### SCEN-{MODULE}-15: Apply custom timeout
- **WHEN** an endpoint has a custom timeout configured (REQ-{MODULE}-42)
- **THEN** the custom timeout is used instead of the default
```

Example without phases (simple module):
```markdown
### SCEN-{MODULE}-01: Process valid request
- **WHEN** a valid request is received with tenant context
- **THEN** tenant isolation is enforced (REQ-1)

### SCEN-{MODULE}-02: Reject unauthorized request
- **WHEN** a request is received without valid tenant context
- **THEN** HTTP 403 Forbidden is returned (REQ-1, REQ-20)
```

## Phasing Rules

- Phases are **optional** — use them for incremental delivery of complex modules
- If used, align with phases defined in 1_INTENT_DESIGN.md
- Each phase must be independently implementable
```

## Prompt - Part 2: Generate Implementation Tasks

```
Now create `modules/{module_name}/docs/TASKS.md` with concrete implementation checklist.

Read:
- modules/{module_name}/docs/1_INTENT_DESIGN.md
- modules/{module_name}/docs/2_INTENT_REQUIREMENTS.md
- modules/{module_name}/docs/3_INTENT_SCENARIOS.md

Generate a hierarchical task list following ModKit patterns:

## Format

```markdown
## Phase 1 (or omit if no phases)

### 1. SDK Implementation
- [ ] 1.1 Define API traits in contract/client.rs (REQ-{MODULE}-N)
- [ ] 1.2 Define domain models in contract/model.rs
- [ ] 1.3 Define error types in contract/error.rs

### 2. Domain Layer
- [ ] 2.1 Implement {service} service (SCEN-{MODULE}-01, SCEN-{MODULE}-02)
- [ ] 2.2 Implement authorization checks (REQ-1, REQ-3)
- [ ] 2.3 Add tracing instrumentation (REQ-10)
- [ ] 2.4 Add structured logging (REQ-11)

### 3. Infrastructure (if database needed)
- [ ] 3.1 Create SeaORM entities in infra/storage/entity.rs
- [ ] 3.2 Implement mapper (entity ↔ contract models)
- [ ] 3.3 Create database migrations
- [ ] 3.4 Implement secure ORM queries with SecurityCtx (REQ-1)

### 4. REST API
- [ ] 4.1 Define DTOs in api/rest/dto.rs with serde/utoipa
- [ ] 4.2 Implement handlers in api/rest/handlers.rs (SCEN-{MODULE}-N)
- [ ] 4.3 Register routes in api/rest/routes.rs with OperationBuilder
- [ ] 4.4 Add error mapping to Problem (REQ-20)
- [ ] 4.5 Add standard errors with .standard_errors()

### 5. Module Wiring
- [ ] 5.1 Implement Module trait in module.rs
- [ ] 5.2 Register client in ClientHub (init)
- [ ] 5.3 Add module to config/quickstart.yaml
- [ ] 5.4 Create GTS schemas/instances (if needed)

### 6. Configuration
- [ ] 6.1 Define config struct in config.rs with serde
- [ ] 6.2 Add validation logic
- [ ] 6.3 Document config options
```

## Rules for Tasks

- Break work into concrete, actionable items
- Reference scenarios (SCEN-{MODULE}-N) and requirements (REQ-N, REQ-{MODULE}-N) inline
- Follow ModKit layer structure: SDK → Domain → Infrastructure → REST → Wiring
- Each task should take < 1 hour for experienced developer
- Use checkboxes: `- [ ]` for incomplete
- Number hierarchically: 1, 2, 3 for groups; 1.1, 1.2 for subtasks
- Be specific about file locations (e.g., "in api/rest/dto.rs")
```

## Prompt - Part 3: Add Cross-References to Design Doc

```
Now update `modules/{module_name}/docs/1_INTENT_DESIGN.md` to add specific ID cross-references.

Replace descriptive text with specific IDs where applicable:

**Before:**
- `forward_request()` - handles request forwarding and error cases

**After:**
- `forward_request()` - see SCEN-{MODULE}-01, SCEN-{MODULE}-02

**Before:**
- Per-endpoint configurable timeouts

**After:**
- Per-endpoint configurable timeouts (REQ-{MODULE}-42)

Add cross-references throughout the design doc:
- In API Surface section: reference scenarios that exercise each method
- In Phases section: list specific requirements and scenarios each phase implements
- Keep existing global requirement references (REQ-1, REQ-10, etc.)
```
