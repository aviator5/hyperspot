# New Module Generation Pipeline

This document defines a repeatable, **human-controlled** workflow for generating new modules with LLMs.
It is designed to be:

- **Human-auditable** (review gates, checklists, clear step boundaries)
- **Project-aligned** (ModKit + DDD-light + SDK pattern, observability, security)

Prompt templates referenced in this workflow are stored in the `workflows/prompts/` directory.

## Terminology

| Term | Format | Description |
|------|--------|-------------|
| **Workflow step** | Step 1, Step 2, ... | A stage in this development workflow |
| **Implementation phase** | `PHASE-{MODULE}-{N}` | Incremental delivery phase (e.g., `PHASE-OAGW-1`, `PHASE-OAGW-2`) |
| **Global requirement** | `REQ-{N}` | Project-wide requirement from `guidelines/MODULE_REQUIREMENTS.md` (e.g., `REQ-1`, `REQ-10`) |
| **Module requirement** | `REQ-{MODULE}-{N}` | Module-specific requirement (e.g., `REQ-OAGW-42`) |
| **Scenario** | `SCEN-{MODULE}-{N}` | A use case or behavior specification, grouped by phase in the document (e.g., `SCEN-OAGW-01`) |
| **Task** | `TASK-{MODULE}-{N}` | Top-level implementation task representing a logical unit of work (e.g., `TASK-OAGW-01`) |
| **Module version** | SemVer (e.g., `0.2.0`) | Rust package version, tracked in `CHANGELOG.md` |

## Roles

- **Human owner**: decides scope, accepts/declines changes, merges PRs.
- **High Reasoning LLM**: creates architecture/code skeletons and higher-level reasoning (e.g., Claude Opus 4.5, GPT-5, Gemini 3 Pro).
- **Low Reasoning LLM**: fixes compilation/lints without changing intended behavior (e.g., Claude Sonnet 4.5, Claude Haiku 4.5, GPT-5-Codex, Gemini 3 Flash).

## Standard artifacts

All documentation is under `modules/{module_name}/docs/`:

**Intent documents** (what we plan to build):

| File | Description |
|------|-------------|
| `INTENT_DESIGN.md` | Planned architecture, dependencies, implementation details by phases |
| `INTENT_REQUIREMENTS.md` | Planned implementation requirements referenced from scenarios |
| `INTENT_SCENARIOS.md` | Planned use cases and behavior specifications tagged with phases |

**Actual documents** (what was actually built):

| File | Description |
|------|-------------|
| `ACTUAL_DESIGN.md` | Actual architecture as implemented |
| `ACTUAL_REQUIREMENTS.md` | Actual implementation requirements |
| `ACTUAL_SCENARIOS.md` | Actual scenarios with implementation status |
| `INTENT_VS_ACTUAL.md` | Gap analysis between intent and actual |

**Implementation tracking:**

| File | Description |
|------|-------------|
| `TASKS.md` | Implementation checklist generated at code generation time, split by phases, referencing scenarios/requirements. Checkboxes track progress as tasks are completed. |

**Version tracking:**

| File | Description |
|------|-------------|
| `CHANGELOG.md` | Version history following [keepachangelog.com](https://keepachangelog.com/en/1.0.0/) format |

### Design doc format

`INTENT_DESIGN.md` describes module architecture. Phases are **optional** — use them for incremental delivery/development milestones.

**Initial version (Step 1) - descriptive references:**

```markdown
## Overview
Outbound gateway module to route requests to upstream services.

## API Surface
- `forward_request()` - handles request forwarding and error cases
- `configure_timeout()` - allows per-endpoint timeout configuration

## Configuration

### Basic Forwarding (PHASE-OAGW-1)
- Basic request forwarding with static timeout
- Covers: success cases, authorization failures

### Advanced Configuration (PHASE-OAGW-2)
- Per-endpoint configurable timeouts
- Upstream health checks

## Security
- All requests require tenant context (REQ-1)
- Authorization checked per request (REQ-3)
```

**Updated version (Step 2.2) - with cross-references:**

```markdown
## Overview
Outbound gateway module to route requests to upstream services.

## API Surface
- `forward_request()` - see SCEN-OAGW-01, SCEN-OAGW-02
- `configure_timeout()` - see REQ-OAGW-42

## Configuration

### Basic Forwarding (PHASE-OAGW-1)
- Basic request forwarding with static timeout
- Implements: SCEN-OAGW-01, SCEN-OAGW-02

### Advanced Configuration (PHASE-OAGW-2)
- Per-endpoint configurable timeouts (REQ-OAGW-42)
- Upstream health checks (REQ-OAGW-43)
- Implements: SCEN-OAGW-15

## Security
- All requests require tenant context (REQ-1)
- Authorization checked per request (REQ-3)
```

**Notes:**
- Start with descriptive text in Step 1
- Add specific ID cross-references in Step 2.2
- Phases are optional — simple modules may not need them
- When used, phases define incremental delivery milestones

### Requirement format

Requirements in `INTENT_REQUIREMENTS.md` define module-specific implementation details:

```markdown
### REQ-OAGW-42: Configurable Timeout
Each endpoint MAY have a custom timeout configuration that overrides the default.
- Default timeout: 30 seconds
- Configurable per endpoint in module config

### REQ-OAGW-43: Upstream Health Check
The gateway MUST verify upstream health before forwarding requests.
- Health check interval: configurable (default 10s)
- Unhealthy upstreams are excluded from routing
```

- Each requirement has a unique ID: `REQ-{MODULE}-{N}`
- Use **MUST**, **SHALL** for mandatory; **MAY**, **SHOULD** for optional
- Can reference global requirements: `(REQ-N)`
- Can reference phases from design doc: `(PHASE-{MODULE}-N)`

### Scenario format

Scenarios in `INTENT_SCENARIOS.md` have unique IDs with **WHEN**/**THEN**/**AND** bullets. If phases are used, organize by phase sections:

```markdown
## PHASE-MODULE-01

### SCEN-MODULE-01: Descriptive name
- **WHEN** precondition or action
- **THEN** expected result (REQ-N)
- **AND** additional result (optional)

## PHASE-MODULE-02

### SCEN-MODULE-15: Phase 2 scenario
- **WHEN** ...
```

For simple modules without phases or on the early stages when phases are not yet defined, just list scenarios directly:

```markdown
### SCEN-MODULE-01: Descriptive name
- **WHEN** precondition or action
- **THEN** expected result (REQ-N)
```

- Each scenario has a unique ID — no sub-scenarios
- Can reference: `(REQ-N)` global, `(REQ-MODULE-N)` module, `(PHASE-MODULE-N)` phases
- Create separate scenarios for different cases (success, error, edge cases)

### Tasks format

`TASKS.md` contains a checklist of concrete implementation tasks, organized by phases:

```markdown
## Phase 1

### TASK-OAGW-01: SDK Implementation
- [ ] 1.1 Define API traits (REQ-OAGW-42, REQ-OAGW-43)
- [ ] 1.2 Define domain models
- [ ] 1.3 Define error types

### TASK-OAGW-02: Domain Layer
- [ ] 2.1 Implement request forwarding service (SCEN-OAGW-01)
- [ ] 2.2 Implement authorization checks (SCEN-OAGW-02, REQ-1, REQ-3)
- [ ] 2.3 Add tracing instrumentation (REQ-10)

### TASK-OAGW-03: Infrastructure
- [ ] 3.1 Create database entities (if needed)
- [ ] 3.2 Implement repository layer
- [ ] 3.3 Database migrations

### TASK-OAGW-04: REST API
- [ ] 4.1 Define DTOs with serde/utoipa
- [ ] 4.2 Implement handlers for forward_request (SCEN-OAGW-01, SCEN-OAGW-02)
- [ ] 4.3 Register routes with OperationBuilder
- [ ] 4.4 Add error mapping to Problem (REQ-20)

### TASK-OAGW-05: Module Wiring
- [ ] 5.1 Register client in ClientHub
- [ ] 5.2 Add module config in quickstart.yaml
- [ ] 5.3 Add GTS schemas/types

### TASK-OAGW-06: Unit Tests
- [ ] 6.1 Test domain service logic (SCEN-OAGW-01, SCEN-OAGW-02)
- [ ] 6.2 Test entity/DTO mappers
- [ ] 6.3 Test error handling paths
- [ ] 6.4 Test security context validation

## Phase 2

### TASK-OAGW-07: Timeout Configuration
- [ ] 7.1 Add timeout config to module config (REQ-OAGW-42)
- [ ] 7.2 Implement timeout override logic (SCEN-OAGW-15)
- [ ] 7.3 Update handlers to use custom timeouts
- [ ] 7.4 Unit tests for timeout configuration

### TASK-OAGW-08: Health Checks
- [ ] 8.1 Implement upstream health check (REQ-OAGW-43)
- [ ] 8.2 Add health check scheduler
- [ ] 8.3 Exclude unhealthy upstreams from routing
- [ ] 8.4 Unit tests for health check logic
```

**Format rules:**
- Top-level tasks use IDs: `TASK-{MODULE}-{N}` (sequential across all phases)
- Subtasks use hierarchical numbering: 1.1, 1.2, 1.3 (no IDs needed)
- Checkboxes for tracking progress: `- [ ]` (incomplete), `- [x]` (complete)
- Reference requirements and scenarios inline where applicable
- Split by phases if module uses phased delivery
- Tasks should be concrete and actionable (not "think about X" but "implement X")
- Top-level tasks represent logical work units: layers (SDK, Domain, API), features, or test suites

**Why top-level task IDs only:**
- Provides stable references for commits, PRs, and coordination
- Natural granularity for work assignment and progress tracking
- Subtasks are implementation details that don't need permanent IDs
- Reduces ID overhead (5-8 IDs per phase vs 20-30 if all subtasks had IDs)
- Each top-level task is typically a complete layer, feature area, or logical PR scope

### Changelog format

**CHANGELOG.md** — Version history ([keepachangelog.com](https://keepachangelog.com/en/1.0.0/)):

```markdown
## [0.2.0] - 2024-01-15
### Added
- SCEN-OAGW-05: Timeout configuration support
- SCEN-OAGW-12: Retry with exponential backoff
### Fixed
- SCEN-OAGW-01: Handle empty response bodies correctly
```

### Other artifact formats

**INTENT_VS_ACTUAL.md** — Gap analysis:
```markdown
### SCEN-OAGW-01: Forward request to upstream
- **Intent:** Forward requests with 30s timeout
- **Actual:** Implemented with 60s timeout due to upstream requirements
- **Gap:** Timeout value differs, documented in ADR-003
```

### Cross-reference relationships

Documents can reference each other using IDs:

| From | Can reference |
|------|---------------|
| `INTENT_DESIGN.md` | `REQ-N` (global), `REQ-{MODULE}-N` (module), `SCEN-{MODULE}-N` (scenarios) |
| `INTENT_REQUIREMENTS.md` | `REQ-N` (global), `PHASE-{MODULE}-N` (phases from design) |
| `INTENT_SCENARIOS.md` | `REQ-N` (global), `REQ-{MODULE}-N` (module), `PHASE-{MODULE}-N` (phases from design) |
| `TASKS.md` | `REQ-N` (global), `REQ-{MODULE}-N` (module), `SCEN-{MODULE}-N` (scenarios), `PHASE-{MODULE}-N` (phases), `TASK-{MODULE}-N` (other tasks for dependencies) |

## Workflow overview

| Step | Name | Delivery |
|------|------|----------|
| 1 | Intent design | `INTENT_DESIGN.md` (without specific requirement/scenario IDs) |
| 2 | Intent requirements + scenarios | `INTENT_REQUIREMENTS.md`, `INTENT_SCENARIOS.md`, updated `INTENT_DESIGN.md` (with cross-references) |
| 3 | Code + unit tests for Phase 1 | `TASKS.md`, production code + unit tests following `TASKS.md`, stubs for Phase 2+, actual docs (`ACTUAL_DESIGN.md`, `ACTUAL_REQUIREMENTS.md`, `ACTUAL_SCENARIOS.md`, `INTENT_VS_ACTUAL.md`) |
| 4 | E2E tests for Phase 1 | E2E tests with 90%+ total coverage, `CHANGELOG.md` |
| 5 | Repeat for Phase 2+ | Incremental implementation, update actual docs and `TASKS.md` |

**Note:** Step 1 writes design with descriptive text. Step 2 creates requirements/scenarios with IDs and updates design with cross-references. Step 3 generates implementation tasks first, then implements production code and unit tests together following the checklist.

Stop after every step, submit a PR and get approvals from stakeholders.

---

## Step 1: Intent design

**Outcome:** `INTENT_DESIGN.md` - generated, reviewed, committed

**Important:** At this stage, write high-level design **without specific requirement/scenario IDs**. Use descriptive references instead (e.g., "request forwarding scenarios", "timeout configuration capability"). IDs will be assigned in Step 2.

### 1.1 - Prepare environment

1. Install needed tools (e.g., Windsurf, Cursor, GitHub Copilot)
2. Pick models for this project:
   - Reasoning: Claude Opus, GPT-5.2, etc.
   - Coding: Claude Sonnet, Codex, etc.

### 1.2 - Scaffold module layout

1. Create module folder: `modules/{module_name}/`
2. Create docs folder: `modules/{module_name}/docs/`
3. Follow the canonical layout and SDK pattern from `guidelines/NEW_MODULE.md`

### 1.3 - Write intent design

Use direct prompt or your favorite SDD tool to generate `INTENT_DESIGN.md`.

**Prompt template:** See `workflows/prompts/0_intent_design.md`

Key outputs:
- Module category (generic/gateway/worker)
- Public API surface (SDK trait methods) - describe functionality, not IDs
- Data model overview
- Persistence needs
- Security/tenancy assumptions (SecurityCtx usage)
- Observability plan (tracing + logs)
- GTS schema and instance definitions
- **Implementation phases** (`PHASE-{MODULE}-1`, `PHASE-{MODULE}-2`, etc.) if used

**Do NOT reference specific requirement/scenario IDs yet.** Use descriptive text:
- "Need scenarios for request forwarding and error handling"
- "Requires timeout configuration capability"
- "Must support tenant isolation per REQ-1"

### 1.4 - Pre-PR review

- Review scope and missing invariants
- Cross-review with independent models (ChatGPT, Gemini, Claude)

### 1.5 - Submit PR

Submit PR, get stakeholder approvals, commit `INTENT_DESIGN.md`.

---

## Step 2: Intent requirements + scenarios

**Outcome:** `INTENT_REQUIREMENTS.md`, `INTENT_SCENARIOS.md`, and updated `INTENT_DESIGN.md` - generated, reviewed, committed

### 2.1 - Generate requirements and scenarios

**Prompt template:** See `workflows/prompts/02_requirements_scenarios.md`

**Rules:**
- Requirements use IDs: `REQ-{MODULE}-{N}` (e.g., `REQ-OAGW-42`)
- Scenarios use IDs: `SCEN-{MODULE}-{N}` (e.g., `SCEN-OAGW-01`)
- Organize scenarios by phase sections if phases are used
- Reference requirement IDs inline in scenario steps
- Each phase must be independently implementable

### 2.2 - Add cross-references to design doc

**Prompt template:** See `workflows/prompts/02_requirements_scenarios.md` (Part 2)

Update `INTENT_DESIGN.md` to add specific ID references:

**Before (Step 1):**
```markdown
## API Surface
- `forward_request()` - handles request forwarding and error cases
```

**After (Step 2.3):**
```markdown
## API Surface
- `forward_request()` - see SCEN-OAGW-01, SCEN-OAGW-02
```

Add references to requirements and scenarios throughout the design doc where applicable.

### 2.3 - Pre-PR review

- Review for completeness
- Verify cross-references are correct
- Cross-review with independent models

### 2.4 - Submit PR

Submit PR, get stakeholder approvals, commit all three docs: `INTENT_DESIGN.md`, `INTENT_REQUIREMENTS.md`, and `INTENT_SCENARIOS.md`.

---

## Step 3: Generate code + unit tests for Phase 1

**Outcome:** `TASKS.md`, production code + unit tests for Phase 1, stubs for Phase 2+, actual docs

**Important:** Generate `TASKS.md` first, then implement production code and unit tests together following the checklist systematically.

### 3.1 - Generate implementation tasks

**Prompt template:** See `workflows/prompts/03_tasks_generation.md`

Create `TASKS.md` with concrete implementation checklist:
- Top-level tasks use IDs: `TASK-{MODULE}-{N}` (sequential across all phases)
- Subtasks use hierarchical numbering (1.1, 1.2, 1.3)
- Organize by phases if used
- Include unit tests section in each phase
- Reference scenarios and requirements inline
- Tasks should be actionable: "Implement X", "Add Y", "Test Z"

**This is a planning step** - review the generated tasks before proceeding to code generation.

### 3.2 - Code and unit tests generation

**Prompt template:** See `workflows/prompts/03_code_generation.md`

**Constraints:**
- Implement all Phase 1 tasks from `TASKS.md` (production code + unit tests)
- Write unit tests alongside each component (domain, infrastructure, mappers)
- Check off tasks as you complete them: `- [x]`
- Phase 2+ scenarios must have clear `TODO(PHASE-{MODULE}-2)` stubs in code
- Code and tests must compile using `make build`
- Tests should follow the patterns in existing modules
- Do NOT invent new project-wide conventions
- Register module in `config/quickstart.yaml`

### 3.3 - Acceptance criteria (human-controlled)

- `make build` passes (production code + unit tests compile)
- All Phase 1 tasks in `TASKS.md` are checked off: `- [x]`
- All Phase 1 routes registered via ModKit `OperationBuilder`
- Handlers are thin (parse → call service → map errors)
- Phase 1 behavior implemented in domain/service + infra
- Unit tests cover domain logic, mappers, error handling, security validation
- Phase 2+ represented as TODO stubs (not scattered across handlers)
- Access control checks implemented for every Phase 1 scenario
- Tracing instrumentation on handlers/service entry points
- All GTS schemas/types created
- Unit tests follow project patterns (tracing-test, testcontainers if needed)

### 3.4 - Cross-check

Review code against `INTENT_DESIGN.md`, `INTENT_REQUIREMENTS.md`, and `INTENT_SCENARIOS.md`.

### 3.5 - Compilation stabilization

**Prompt template:** See `workflows/prompts/04_compilation_fix.md`

**Hard constraints:**
- Do not change any logic or parameters
- Only: rename unused variables, remove unused imports, fix doc lints, adjust types
- If something cannot be fixed without changing behavior, explain and stop

### 3.6 - Generate actual documentation

Generate docs reflecting actual implementation:
- `ACTUAL_DESIGN.md` - Actual architecture as implemented
- `ACTUAL_REQUIREMENTS.md` - Actual requirements as implemented
- `ACTUAL_SCENARIOS.md` - Actual scenarios with implementation status
- `INTENT_VS_ACTUAL.md` - Gap analysis between intent and actual

**Prompt template:** See `workflows/prompts/07_actual_docs.md`

**Note:** Verify all Phase 1 tasks in `TASKS.md` are checked off `- [x]` before proceeding.

### 3.7 - Submit PR

Open PR with: `TASKS.md`, production code, unit tests, and actual docs. Iterate on review comments and CI until clean.

**Note:** Unit tests should provide good coverage of domain logic, but E2E tests (Step 4) will complete the 90%+ coverage target.

---

## Step 4: Implement E2E tests for Phase 1

### 4.1 - End-to-end tests

**Prompt template:** See `workflows/prompts/05_e2e_tests.md`

Implement E2E tests for all `PHASE-{MODULE}-1` scenarios.

**Coverage expectations:**
- E2E tests validate complete request flows
- Combined with unit tests from Step 3, achieve 90%+ total code coverage
- E2E tests cover integration points and API contracts

### 4.2 - Coverage gate

Verify 90%+ code coverage by combined E2E and unit tests:
```bash
make coverage
```

### 4.3 - Create CHANGELOG.md

Create initial `CHANGELOG.md`:

```markdown
# Changelog

All notable changes to this module will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - YYYY-MM-DD
### Added
- SCEN-{MODULE}-01: <description>
- SCEN-{MODULE}-02: <description>
```

### 4.4 - Submit PR

Open PR with E2E tests and `CHANGELOG.md`. Iterate on review comments and CI until clean.

---

## Step 5: Repeat for Phase 2+

For each subsequent phase:

1. Implement production code + unit tests for Phase N following `TASKS.md`
2. Check off completed Phase N tasks in `TASKS.md`: `- [x]`
3. Add E2E tests for Phase N scenarios
4. Verify coverage remains at 90%+
5. Update actual docs (`ACTUAL_DESIGN.md`, `ACTUAL_REQUIREMENTS.md`, `ACTUAL_SCENARIOS.md`, `INTENT_VS_ACTUAL.md`)
6. Update `CHANGELOG.md` with new scenarios
7. Submit PR and get approvals

Continue until all phases are complete and all tasks in `TASKS.md` are checked off.
