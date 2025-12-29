# Module Requirements

Global requirements that apply to all modules. Referenced from module-specific scenarios using `REQ-N` format.

## Security

### REQ-1: Tenant Isolation
All data access MUST validate tenant context via `SecurityCtx` before returning results.

### REQ-2: Authentication Required
All non-public endpoints MUST require valid authentication.

### REQ-3: Authorization Check
All operations MUST verify the caller has appropriate permissions for the requested action.

## Observability

### REQ-10: Tracing Spans
Every handler entry point MUST create a tracing span with operation name and relevant context.

### REQ-11: Structured Logging
All log entries MUST use structured format with `trace_id` field for correlation.

### REQ-12: Error Logging
All errors MUST be logged with appropriate severity level and context before returning to caller.

## Error Handling

### REQ-20: RFC-9457 Problem Details
All error responses MUST use RFC-9457 Problem Details format via `modkit::Problem`.

### REQ-21: No Internal Details in Errors
Error responses MUST NOT expose internal implementation details, stack traces, or sensitive information.

## API Conventions

### REQ-30: Endpoint Versioning
All REST endpoints MUST follow `/{service-name}/v{N}/{resource}` pattern.

### REQ-31: Idempotency
Mutating operations SHOULD support `Idempotency-Key` header for safe retries.
