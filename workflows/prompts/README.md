# Workflow Prompt Templates

This directory contains prompt templates referenced by workflows in the parent directory.

## Prompt Files

| File | Used In | Description |
|------|---------|-------------|
| `01_intent_design.md` | Step 1.3 | Generate `1_INTENT_DESIGN.md` (without specific IDs) |
| `02_requirements_scenarios_tasks.md` | Step 2.1, 2.2, 2.3 | Generate `2_INTENT_REQUIREMENTS.md`, `3_INTENT_SCENARIOS.md`, `TASKS.md`, then update `1_INTENT_DESIGN.md` |
| `03_code_generation.md` | Step 3.1 | Generate module code for a phase following `TASKS.md` |
| `04_compilation_fix.md` | Step 3.4 | Fix compilation/lint errors without changing logic |
| `05_e2e_tests.md` | Step 4.1 | Generate end-to-end tests |
| `06_unit_tests.md` | Step 4.2 | Generate unit tests |
| `07_actual_docs.md` | Step 3.5 | Generate actual docs (`4_ACTUAL_*.md`, `7_INTENT_VS_ACTUAL.md`) |

## Usage

These prompts are templates. Replace `{module_name}`, `{MODULE}`, etc. with actual values before use.

## Adding New Prompts

1. Use sequential numbering: `08_new_prompt.md`
2. Update this README
3. Reference from the workflow document
