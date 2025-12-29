# Prompt: End-to-End Tests

## Variables

- `{module_name}` - Module name in snake_case (e.g., `oagw`, `cred_store`)
- `{MODULE}` - Module prefix in UPPERCASE (e.g., `OAGW`, `CREDSTORE`)
- `{PHASE_N}` - Current phase number (e.g., `1`, `2`)

## Prompt

```
Read:
- modules/{module_name}/docs/3_INTENT_SCENARIOS.md
- modules/{module_name}/docs/2_INTENT_REQUIREMENTS.md
- The implemented code in modules/{module_name}/

Implement end-to-end tests for all PHASE-{MODULE}-{PHASE_N} scenarios.

## Test Requirements

1. Each SCEN-{MODULE}-{N} tagged with PHASE-{MODULE}-{PHASE_N} must have E2E coverage
2. Tests should verify the scenario's Given/When/Then flow
3. Tests should validate referenced requirements are satisfied
4. Use real HTTP calls against the running server
5. Test both success and error paths

## Test Structure

- Place tests in `testing/e2e/` or module's integration test directory
- Name tests to reference scenario IDs: `test_scen_{module}_{N}_description`
- Include setup/teardown for test data
- Mock external dependencies if needed for later phases

## Coverage Target

Aim for 80% code coverage from E2E tests alone.
Combined with unit tests, target 90% total coverage.
```
