# Prompt: Generate Actual Documentation

## Variables

- `{module_name}` - Module name in snake_case (e.g., `oagw`, `cred_store`)
- `{MODULE}` - Module prefix in UPPERCASE (e.g., `OAGW`, `CREDSTORE`)
- `{PHASE_N}` - Current phase number (e.g., `1`, `2`)

## Prompt

```
Review the implemented code in `modules/{module_name}/` and the intent documents:
- 1_INTENT_DESIGN.md
- 2_INTENT_REQUIREMENTS.md
- 3_INTENT_SCENARIOS.md
- TASKS.md

Generate documentation reflecting the actual implementation:

## 4_ACTUAL_DESIGN.md

Document the architecture as actually implemented:
- Actual module structure and dependencies
- Actual API surface
- Actual data models
- Any deviations from intent design

## 5_ACTUAL_REQUIREMENTS.md

Document the requirements as actually implemented:
- For each REQ-{MODULE}-{N}, document actual implementation
- Note any requirements that were modified or deferred
- Add any new requirements discovered during implementation

## 6_ACTUAL_SCENARIOS.md

Document scenarios with implementation status:
- Mark each SCEN-{MODULE}-{N} as: [IMPLEMENTED], [PARTIAL], [DEFERRED], or [CHANGED]
- For [PARTIAL] or [CHANGED], explain what differs from intent
- Reference the actual code locations

## 7_INTENT_VS_ACTUAL.md

Analyze gaps between intent and actual:
- List all differences between intent and actual
- For each difference:
  - What was intended
  - What was actually implemented
  - Why the difference exists
  - Impact assessment
  - Resolution plan (if needed)

Format example:
### SCEN-{MODULE}-01: Basic Request Forwarding
- **Intent:** Forward requests with 30s timeout
- **Actual:** Implemented with 60s timeout
- **Reason:** Upstream service requires longer timeout
- **Impact:** Low - better reliability
- **Resolution:** Update intent doc to reflect new requirement

## Update TASKS.md

Ensure all Phase {PHASE_N} tasks are checked off:
- Change `- [ ]` to `- [x]` for completed tasks
- If any tasks were not completed, note why in 7_INTENT_VS_ACTUAL.md
- If any additional tasks were needed, add them with `- [x]` (already completed)
```
