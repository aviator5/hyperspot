# Prompt: Code Generation

## Variables

- `{module_name}` - Module name in snake_case (e.g., `oagw`, `cred_store`)
- `{MODULE}` - Module prefix in UPPERCASE (e.g., `OAGW`, `CREDSTORE`)
- `{PHASE_N}` - Current phase number (e.g., `1`, `2`)

## Prompt

```
Read:
- modules/{module_name}/docs/1_INTENT_DESIGN.md
- modules/{module_name}/docs/2_INTENT_REQUIREMENTS.md
- modules/{module_name}/docs/3_INTENT_SCENARIOS.md
- modules/{module_name}/docs/TASKS.md

Also follow:
- guidelines/NEW_MODULE.md
- docs/MODKIT_UNIFIED_SYSTEM.md
- docs/TRACING_SETUP.md

Implement needed modules (sdk, gateway, plugins):
- modules/{module_name}/{module_name}-sdk
- modules/{module_name}/{module_name}
- Default plugin crate if this is a gateway+plugin architecture

## Implementation Rules

1. **Follow TASKS.md systematically:**
   - Work through Phase {PHASE_N} tasks in order
   - Check off each task as you complete it: `- [x]`
   - Each task should reference scenarios/requirements it implements

2. **Implement all Phase {PHASE_N} scenarios in production quality**

3. **For later phases (Phase N > {PHASE_N}):**
   - Insert clear TODO comments: `// TODO(PHASE-{MODULE}-{N}): <description>`
   - Place TODOs in service/infra layers, not scattered across handlers

4. Implement all interfaces, handlers, functions, structures
5. Create all GTS schemas/types definitions
6. Register module in `config/quickstart.yaml`

## Constraints

- Code must compile using `make build`
- Do NOT run `make all`, `make clippy`, `make dylint`, `make test`
- Do NOT invent new project-wide conventions
- Do NOT change scenario semantics
- Strictly follow 1_INTENT_DESIGN.md, 2_INTENT_REQUIREMENTS.md, and 3_INTENT_SCENARIOS.md

## Code Structure Requirements

- Handlers must be thin: parse/validate → call service → map errors to Problem
- Business logic belongs in domain/service layer
- Access control checks for every scenario
- Tracing instrumentation on handlers/service entry points
```
