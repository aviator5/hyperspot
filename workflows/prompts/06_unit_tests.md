# Prompt: Unit Tests

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

Implement unit tests for PHASE-{MODULE}-{PHASE_N} functionality.

## Test Requirements

1. Test domain/service layer logic in isolation
2. Test requirement implementations (REQ-{MODULE}-{N})
3. Mock infrastructure dependencies (database, external services)
4. Test edge cases and error conditions
5. Test input validation

## Test Structure

- Place tests in `#[cfg(test)]` modules within source files or in `tests/` directory
- Name tests descriptively: `test_{function}_{condition}_{expected_result}`
- Use test fixtures for common setup
- Keep tests focused and fast

## What to Test

- Domain service methods
- Mapper/converter functions
- Validation logic
- Error handling paths
- Business rule enforcement

## What NOT to Test

- Framework code (Axum handlers, SeaORM queries)
- External libraries
- Simple getters/setters

## Coverage Target

Combined with E2E tests, achieve 90% total code coverage.
```
