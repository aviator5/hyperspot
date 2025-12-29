# Prompt: Compilation Stabilization

## Purpose

Fix compilation, formatting, and lint errors without changing intended behavior.

## Prompt

```
Run `make all` and fix all compilation, format, clippy, and dylint errors and warnings.

## Hard Constraints

- Do NOT change any logic or parameters
- Do NOT modify business behavior
- Do NOT alter API contracts

## Allowed Changes

- Rename unused variables (prefix with `_`)
- Remove unused imports
- Fix doc lint warnings
- Adjust types for API compatibility
- Fix formatting issues
- Add missing derives required by lints

## If Blocked

If something cannot be fixed without changing behavior:
1. Explain why the fix requires behavior change
2. List the specific error/warning
3. Stop and ask for human guidance

Do NOT guess or make assumptions about intended behavior.
```
