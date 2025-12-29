# Prompt: Intent Design Generation

## Variables

- `{module_name}` - Module name in snake_case (e.g., `oagw`, `cred_store`)
- `{MODULE}` - Module prefix in UPPERCASE (e.g., `OAGW`, `CREDSTORE`)

## Prompt

```
Create `modules/{module_name}/docs/1_INTENT_DESIGN.md`.

Input: [Describe the module purpose and use cases here]

Must follow:
- guidelines/NEW_MODULE.md
- docs/MODKIT_UNIFIED_SYSTEM.md

**IMPORTANT:** Use descriptive text, NOT specific requirement/scenario IDs (those will be assigned in the next step).

Output must include:

1. **Module Overview**
   - Module category (generic/gateway/worker)
   - Purpose and responsibilities

2. **Public API Surface**
   - SDK trait methods - describe functionality in plain text
   - Data models
   - Example: "`forward_request()` - handles request forwarding and error cases"

3. **Implementation Phases** (optional)
   Organize features into phases: PHASE-{MODULE}-1, PHASE-{MODULE}-2, etc.
   Each phase must be independently implementable.

   For each phase:
   - List features descriptively
   - Example: "Covers: success cases, authorization failures"
   - Include phase-to-phase migration notes

4. **Data Model**
   - Entities and relationships
   - Persistence needs

5. **Security & Tenancy**
   - SecurityCtx usage
   - Access control requirements
   - Can reference global requirements: REQ-1, REQ-2, etc. from guidelines/MODULE_REQUIREMENTS.md

6. **Observability**
   - Tracing plan (can reference REQ-10)
   - Logging requirements (can reference REQ-11)

7. **GTS Integration**
   - Schema definitions
   - Instance definitions

Note: Specific requirement/scenario IDs will be added in Step 2 after requirements and scenarios are created.
```
