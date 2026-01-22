# Auth Resolver Technical Specification

> **Related ADR:** [0001-pdp-pep-authorization-model.md](../../../docs/adrs/authorization/0001-pdp-pep-authorization-model.md)

## Overview

Auth Resolver is HyperSpot's Policy Decision Point (PDP) integration module. It implements an AuthZEN-based authorization API extended with constraint-based filtering for SQL-level enforcement.

The core problem Auth Resolver solves: HyperSpot modules need to enforce authorization at the **query level** (SQL WHERE clauses), not just perform point-in-time access checks. For LIST operations, we need **constraints** that can filter results, not a boolean decision or enumerated resource IDs.

---

## AuthZEN Overview

AuthZEN Authorization API 1.0 (approved by OpenID Foundation on 2026-01-12) defines:

### Access Evaluation API

Point-in-time check: "Can subject S perform action A on resource R?"

```
POST /access/v1/evaluation

Request:
{
  "subject": { "type": "user", "id": "alice", "properties": {...} },
  "action": { "name": "read", "properties": {...} },
  "resource": { "type": "document", "id": "doc-123", "properties": {...} },
  "context": { ... }
}

Response:
{
  "decision": true,
  "context": { ... }
}
```

### Search APIs

AuthZEN defines three search operations:

| API | Question | Response |
|-----|----------|----------|
| Subject Search | "Who can perform action A on resource R?" | List of subject entities |
| Resource Search | "What resources can subject S perform action A on?" | List of resource entities |
| Action Search | "What actions can subject S perform on resource R?" | List of action entities |

```
POST /access/v1/search/resources

Request:
{
  "subject": { "type": "user", "id": "alice" },
  "action": { "name": "read" },
  "resource": { "type": "document" }  // no "id" - searching
}

Response:
{
  "results": [
    { "type": "document", "id": "doc-123" },
    { "type": "document", "id": "doc-456" },
    ...
  ],
  "page": { "next_token": "...", "result_count": 2 }
}
```

**Why Search APIs don't work for HyperSpot:**

Search APIs assume the PDP has access to resource data. In HyperSpot, resources live in the PEP's database - the PDP cannot enumerate what it doesn't have. This creates an architectural mismatch: the component that knows "who can access what" (PDP) is separate from the component that has "what exists" (PEP).

---

## AuthZEN Gap Analysis

| Aspect | AuthZEN 1.0 | HyperSpot Requirement | Resolution |
|--------|-------------|----------------------|------------|
| **Point access check** | `decision: true/false` | Same | AuthZEN-compliant |
| **List/query operations** | Resource Search returns IDs | Need constraints for SQL WHERE | **Extended evaluation response** |
| **Resource location** | PDP has resource data | Resources in PEP's database | **Constraints instead of enumeration** |
| **Tenant hierarchy** | Not specified | First-class primitive | Extension via `context.tenant_scope` |
| **Resource groups** | Not specified | First-class primitive | Extension via filters |
| **Capability negotiation** | Not specified | PEP declares what it can enforce | Extension via `context.capabilities` |
| **Constraint-based filtering** | Not supported | Core requirement | **Extended evaluation response** |

### The Fundamental Problem: PDP Doesn't Have Resources

In HyperSpot's architecture, resources live in the PEP's database. The PDP knows authorization policies but cannot enumerate resources it doesn't have access to.

**The naive approach without constraints:**

```

+----------+         +----------+         +----------+
|  Client  |         |   PEP    |         |   PDP    |
+----+-----+         +----+-----+         +----+-----+
     | GET /events?limit=10                    |
     +------------------->|                    |
     |                    |                    |
     |                    | SELECT * FROM events (millions!)
     |                    +----------> DB
     |                    |<----------
     |                    |                    |
     |                    | evaluate(evt-1)?   |
     |                    +------------------->|
     |                    | evaluate(evt-2)?   |
     |                    +------------------->|
     |                    | ... millions more  |
     |                    |                    |
     |                    | filter results     |
     |                    | apply limit=10     |  <- wrong, limit applied AFTER filtering
     |      response      |
     |<-------------------+
```

**Problems:**
1. **O(N) evaluations** - Must check every resource, even to return 10 items
2. **Pagination breaks** - `limit=10` applied after filtering; may return 0-10 items unpredictably
3. **Total count impossible** - Can't know total without evaluating all resources
4. **Dynamic data** - Results change between pagination requests

**What HyperSpot needs:**
```jsonc
// Extended evaluation response with constraints
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        // PDP uses logical field name (DSL), not physical column
        { "type": "field", "field": "resource.owner_tenant_id", "op": "in_closure", "ancestor_id": "tenant-123", "respect_barrier": true }
      ]
    }]
  }
}
// PEP maps resource.owner_tenant_id -> owner_tenant_id column
// PEP compiles to: WHERE owner_tenant_id IN (SELECT descendant_id FROM tenant_closure WHERE ancestor_id = 'tenant-123')
// Now: SELECT * FROM events WHERE (constraints) LIMIT 10 - correct pagination!
```

**Our resolution**: Extend the AuthZEN evaluation response with optional `context.constraints`. The PDP expresses "what the user can access" as predicates; the PEP applies them to SQL before fetching data.

---

## Core Terms

- **Tenant** - Domain of ownership/responsibility and policy (billing, security, data isolation)
- **Subject / Principal** - Actor initiating the request (user or API client)
- **Subject Tenant** - Tenant the subject belongs to
- **Context Tenant** - Tenant scope root for the operation (may differ from subject tenant in cross-tenant scenarios)
- **Resource Owner Tenant** - Actual tenant owning the resource (`owner_tenant_id`)
- **Resource** - Object with owner tenant identifier
- **Resource Group** - Optional container for resources (project/workspace/folder)
- **Permission** - `{ resource_type, action }` - allowed operation identifier
- **Access Constraints** - Structured predicates returned by the PDP for query-time enforcement. NOT policies (which are stored vendor-side), but compiled, time-bound enforcement artifacts.
- **PDP (Policy Decision Point)** - Auth Resolver implementing authorization decisions
- **PEP (Policy Enforcement Point)** - HyperSpot domain modules applying constraints

---

## Integration Architecture

```
+---------------------------------------------------------------------+
|                        Vendor Platform                              |
|  +----------+  +-----------------+  +------------+  +------------+  |
|  |   IdP    |  | Tenant Service  |  |  RG Svc    |  | Authz Svc  |  |
|  +----^-----+  +-------^---------+  +-----^------+  +-----^------+  |
+-------+----------------+------------------+---------------+---------+
        |                |                  |               |
+-------+----------------+------------------+---------------+---------+
|       |         HyperSpot                 |               |         |
|  +----+----+  +--------+--------+  +------+-----+  +------+------+  |
|  |  AuthN  |  | Tenant Resolver |  | RG Resolver|  |Auth Resolver|  |
|  | (JWT/   |  |    (Gateway)    |  |  (Gateway) |  |   (PDP)     |  |
|  | Introsp)|  +--------+--------+  +------+-----+  +------+------+  |
|  +---------+           |                  |               |         |
|                        v                  v               |         |
|              +-------------------------------+            |         |
|              |     Local Projections         |            |         |
|              |  * tenant_closure             |            |         |
|              |  * resource_group_closure     |            |         |
|              |  * resource_group_membership  |            |         |
|              +-------------------------------+            |         |
|                                                           |         |
|  +-------------------------------------------------------+-------+ |
|  |                    Domain Module (PEP)                 |       | |
|  |  +-------------+                                       |       | |
|  |  |   Handler   |---- /access/v1/evaluation ----------->|       | |
|  |  +------+------+     (returns decision + constraints)          | |
|  |         | Compile constraints to SQL                           | |
|  |         v                                                      | |
|  |  +-------------+                                               | |
|  |  |  Database   |  WHERE owner_tenant_id IN (...)               | |
|  |  +-------------+                                               | |
|  +----------------------------------------------------------------+ |
+---------------------------------------------------------------------+
```

---

## API Specifications

### Access Evaluation API (AuthZEN-extended)

Two endpoints for authorization checks, following AuthZEN structure:

- `POST /access/v1/evaluation` - Single evaluation request
- `POST /access/v1/evaluations` - Batch evaluation (array of requests -> array of responses)

PDP returns `decision` plus optional `constraints` for each evaluation.

#### Design Principles

1. **AuthZEN alignment** - Use same `subject`, `action`, `resource`, `context` structure
2. **Constraints are optional** - PDP decides when to include based on action type
3. **Constraint-first** - Return predicates, not enumerated IDs
4. **Capability negotiation** - PEP declares enforcement capabilities
5. **Fail-closed** - Unknown constraints or schemas result in deny
6. **OR/AND semantics** - Multiple constraints are OR'd (alternative access paths), filters within constraint are AND'd

#### Request

```
POST /access/v1/evaluation
Content-Type: application/json
```

```jsonc
{
  // AuthZEN standard fields
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": {
      "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
    }
  },
  "action": {
    "name": "list"  // or "read", "update", "delete", "create"
  },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "id": "e81307e5-5ee8-4c0a-8d1f-bd98a65c517e",  // present for point ops, absent for list
    "properties": {
      "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
    }
  },

  // HyperSpot extension: context with tenant scope and PEP capabilities
  "context": {
    // Tenant scope configuration
    "tenant_scope": {
      "root_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
      "include_self": true,
      "depth": "descendants",      // "none" | "children" | "descendants"
      "respect_barrier": true,     // honor self_managed barrier in hierarchy traversal
      "status": ["active", "suspended"]  // optional, filters by tenant status
    },

    // PEP capabilities: what the caller can enforce locally
    "capabilities": {
      "require_constraints": true,              // if true, decision without constraints = deny
      "local_tenant_tables": true,              // can use tenant_closure table
      "local_resource_group_membership": true,  // can use resource_group_membership table
      "local_resource_group_closure": true      // can use resource_group_closure table
    }
  }
}
```

#### Response

The response contains a `decision` and, when `decision: true`, optional `context.constraints`. Each constraint contains a `filters` array of typed filter objects that the PEP compiles to SQL.

```jsonc
{
  "decision": true,
  "context": {
    // Multiple constraints are OR'd together (alternative access paths)
    "constraints": [{
      // Filters within a constraint are AND'd together
      "filters": [
        {
          // Tenant closure filter - uses local tenant_closure table
          "type": "field",
          "field": "resource.owner_tenant_id",
          "op": "in_closure",
          "ancestor_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
          "respect_barrier": true,
          "status": ["active", "suspended"]
        },
        {
          // Simple column equality from resource properties
          "type": "field",
          "field": "resource.topic_id",
          "op": "eq",
          "value": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
        }
      ]
    }]
  }
}
```

#### PEP Decision Matrix

| `decision` | `constraints` | `require_constraints` | PEP Action |
|------------|---------------|----------------------|------------|
| `false` | (any) | (any) | **403 Forbidden** |
| `true` | absent | `false` | Allow (trust PDP decision) |
| `true` | absent | `true` | **403 Forbidden** (constraints required but missing) |
| `true` | present | (any) | Apply constraints to SQL |

**Key insight:** PEP declares via `require_constraints` capability whether it needs constraints for the operation. For LIST operations, this should typically be `true`; for CREATE, it can be `false`.

#### Operation-Specific Behavior

**CREATE** (no constraints needed):
```jsonc
// PEP -> PDP
{
  "action": { "name": "create" },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "properties": { "owner_tenant_id": "tenant-B", "topic_id": "..." }
  }
  // ... subject, context
}

// PDP -> PEP
{ "decision": true }  // no constraints - PEP trusts decision

// PEP: INSERT INTO events ...
```

**LIST** (constraints required):
```jsonc
// PEP -> PDP
{
  "action": { "name": "list" },
  "resource": { "type": "gts.x.events.event.v1~" }  // no id
  // ... subject, context
}

// PDP -> PEP
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        { "type": "field", "field": "resource.owner_tenant_id", "op": "in_closure", "ancestor_id": "tenant-A", "respect_barrier": true }
      ]
    }]
  }
}

// PEP: SELECT * FROM events WHERE (constraints)
```

**GET/UPDATE/DELETE** (constraints for SQL-level enforcement):
```jsonc
// PEP -> PDP
{
  "action": { "name": "read" },
  "resource": { "type": "gts.x.events.event.v1~", "id": "evt-123" }
  // ... subject, context
}

// PDP -> PEP
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        { "type": "field", "field": "resource.owner_tenant_id", "op": "in_closure", "ancestor_id": "tenant-A", "respect_barrier": true }
      ]
    }]
  }
}

// PEP: SELECT * FROM events WHERE id = :id AND (constraints)
// 0 rows -> 404 (hides resource existence)
```

#### Response with Resource Group Filter

```jsonc
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        {
          "type": "field",
          "field": "resource.owner_tenant_id",
          "op": "in_closure",
          "ancestor_id": "tenant-A",
          "respect_barrier": true
        },
        {
          // Resource group membership with closure - uses resource_group_membership + resource_group_closure tables
          "type": "group_membership",
          "op": "in_closure",
          "ancestor_id": "project-root-group"
        }
      ]
    }]
  }
}
```

#### Deny Response

```jsonc
{
  "decision": false
}
```

---

## Filter Types Reference

The following filter types can appear in the `filters` array. Filter fields use **logical names** (DSL), not physical column names. PEP maps logical fields to actual database columns.

| Filter | Type | Op | Parameters | Example Field | SQL Mapping |
|--------|------|-----|------------|---------------|-------------|
| Field equality | `field` | `eq` | `field`, `value` | `resource.owner_tenant_id` | `owner_tenant_id = :value` |
| Field IN | `field` | `in` | `field`, `values[]` | `resource.status` | `status IN (:values)` |
| Tenant closure | `field` | `in_closure` | `field`, `ancestor_id`, `respect_barrier?`, `status?` | `resource.owner_tenant_id` | Subquery to `tenant_closure` |
| Group membership IN | `group_membership` | `in` | `values[]` | - | Subquery to `resource_group_membership` |
| Group membership closure | `group_membership` | `in_closure` | `ancestor_id` | - | Nested subquery to membership + closure |

### Field Filter (`type: "field"`)

```jsonc
// Equality
{ "type": "field", "field": "resource.topic_id", "op": "eq", "value": "uuid-123" }
// PEP maps resource.topic_id -> topic_id column
// SQL: topic_id = 'uuid-123'

// IN list
{ "type": "field", "field": "resource.status", "op": "in", "values": ["active", "pending"] }
// SQL: status IN ('active', 'pending')

// Tenant closure (requires local_tenant_tables capability)
{
  "type": "field",
  "field": "resource.owner_tenant_id",
  "op": "in_closure",
  "ancestor_id": "tenant-A",
  "respect_barrier": true,
  "status": ["active", "suspended"]
}
// PEP maps resource.owner_tenant_id -> owner_tenant_id column
// SQL: owner_tenant_id IN (
//   SELECT descendant_id FROM tenant_closure
//   WHERE ancestor_id = 'tenant-A'
//     AND (barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'tenant-A')
//     AND status IN ('active', 'suspended')
// )
```

**Field naming convention (DSL):**

| Prefix | Maps To | Example |
|--------|---------|---------|
| `resource.<property>` | Resource table column | `resource.owner_tenant_id` -> `owner_tenant_id` |

PEP maintains mapping from logical field names to physical schema. PDP uses only logical names — it doesn't know the database schema.

### Group Membership Filter (`type: "group_membership"`)

```jsonc
// Direct membership IN (requires local_resource_group_membership capability)
{ "type": "group_membership", "op": "in", "values": ["group-1", "group-2"] }
// SQL: id IN (
//   SELECT resource_id FROM resource_group_membership
//   WHERE group_id IN ('group-1', 'group-2')
// )

// Group closure membership (requires local_resource_group_closure capability)
{ "type": "group_membership", "op": "in_closure", "ancestor_id": "root-group" }
// SQL: id IN (
//   SELECT resource_id FROM resource_group_membership
//   WHERE group_id IN (
//     SELECT descendant_id FROM resource_group_closure
//     WHERE ancestor_id = 'root-group'
//   )
// )
```

---

## Capabilities -> Filter Matrix

The PEP declares its capabilities in the request. This determines what filter operations the PDP can return:

| Capability | When `false` | When `true` |
|------------|--------------|-------------|
| `require_constraints` | `decision: true` without constraints = allow | `decision: true` without constraints = **deny** |
| `local_tenant_tables` | PDP returns `field, op: in` with explicit tenant IDs | PDP can return `op: in_closure` for tenant hierarchy |
| `local_resource_group_membership` | PDP returns `field, op: in` with explicit resource IDs | PDP can return `type: group_membership` filters |
| `local_resource_group_closure` | PDP returns `group_membership, op: in` with explicit group IDs | PDP can return `group_membership, op: in_closure` |

**`require_constraints` usage:**
- For LIST operations: typically `true` (constraints needed for SQL WHERE)
- For CREATE operations: typically `false` (no query, just permission check)
- For GET/UPDATE/DELETE: depends on whether PEP wants SQL-level enforcement or trusts PDP decision

**Capability degradation**: If a PEP lacks a capability, the PDP must either:
1. Expand the filter to explicit IDs (may be large)
2. Return `decision: false` if expansion is not feasible

---

## PEP Enforcement

### Unified PEP Flow

All operations (LIST, GET, UPDATE, DELETE) follow the same flow:

```
+----------+         +----------+         +----------+
|  Client  |         |   PEP    |         |   PDP    |
+----+-----+         +----+-----+         +----+-----+
     | GET /events        |                    |
     +------------------->|                    |
     |                    | evaluation request |
     |                    +------------------->|
     |                    |                    |
     |                    | decision+constraints
     |                    |<-------------------+
     |                    |                    |
     |                    | SQL with constraints
     |                    +----------> DB
     |                    |<----------
     |      response      |
     |<-------------------+
```

The only difference between LIST and point operations (GET/UPDATE/DELETE) is whether `resource.id` is present.

### Constraint Compilation to SQL

When constraints are present, the PEP compiles the `filters` array to SQL WHERE clauses:

1. **Filters within a constraint** are AND'd together
2. **Multiple constraints** (alternatives) are OR'd together
3. **Unknown filter types** cause that constraint to be treated as false (fail-closed)

### Example: List Events Across Tenant Hierarchy

**HTTP Request:**
```
GET /events/v1/events?topic_id=gts.x.core.events.topic.v1~z.app._.some_topic.v1
X-Tenant-Context: tenant-A
```

**PEP -> Auth Resolver Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "alice-uuid",
    "properties": { "tenant_id": "tenant-A" }
  },
  "action": { "name": "list" },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "properties": {
      "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
    }
  },
  "context": {
    "tenant_scope": {
      "root_id": "tenant-A",
      "include_self": true,
      "depth": "descendants",
      "respect_barrier": true,
      "status": ["active", "suspended"]
    },
    "capabilities": {
      "require_constraints": true,
      "local_tenant_tables": true,
      "local_resource_group_membership": true,
      "local_resource_group_closure": true
    }
  }
}
```

**Auth Resolver -> PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        {
          "type": "field",
          "field": "resource.owner_tenant_id",
          "op": "in_closure",
          "ancestor_id": "tenant-A",
          "respect_barrier": true,
          "status": ["active", "suspended"]
        },
        {
          "type": "field",
          "field": "resource.topic_id",
          "op": "eq",
          "value": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
        }
      ]
    }]
  }
}
```

**PEP Generated SQL:**
```sql
SELECT e.*
FROM events e
WHERE e.owner_tenant_id IN (
    SELECT descendant_id FROM tenant_closure
    WHERE ancestor_id = 'tenant-A'
      AND (barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'tenant-A')
      AND status IN ('active', 'suspended')
  )
  AND e.topic_id = 'gts.x.core.events.topic.v1~z.app._.some_topic.v1'
```

### Example: Multiple Access Paths (OR)

When a user has access through multiple independent paths, the PDP returns multiple constraints:

**Response with alternatives:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "filters": [
          { "type": "field", "field": "resource.owner_tenant_id", "op": "eq", "value": "tenant-A" }
        ]
      },
      {
        "filters": [
          { "type": "group_membership", "op": "in_closure", "ancestor_id": "shared-project-group" }
        ]
      }
    ]
  }
}
```

**Generated SQL:**
```sql
SELECT e.*
FROM events e
WHERE (
    e.owner_tenant_id = 'tenant-A'
  )
  OR (
    e.id IN (
      SELECT resource_id FROM resource_group_membership
      WHERE group_id IN (
        SELECT descendant_id FROM resource_group_closure
        WHERE ancestor_id = 'shared-project-group'
      )
    )
  )
```

### Example: Point Operation (GET with SQL Enforcement)

For GET/UPDATE/DELETE, constraints provide SQL-level enforcement to hide unauthorized resource existence:

**HTTP Request:**
```
GET /events/v1/events/evt-123
X-Tenant-Context: tenant-A
```

**PEP -> Auth Resolver Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "alice-uuid",
    "properties": { "tenant_id": "tenant-A" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "id": "evt-123"
  },
  "context": {
    "tenant_scope": {
      "root_id": "tenant-A",
      "include_self": true,
      "depth": "descendants",
      "respect_barrier": true
    },
    "capabilities": {
      "require_constraints": true,
      "local_tenant_tables": true,
      "local_resource_group_membership": true,
      "local_resource_group_closure": true
    }
  }
}
```

**Auth Resolver -> PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [{
      "filters": [
        {
          "type": "field",
          "field": "resource.owner_tenant_id",
          "op": "in_closure",
          "ancestor_id": "tenant-A",
          "respect_barrier": true
        }
      ]
    }]
  }
}
```

**PEP Generated SQL:**
```sql
SELECT e.*
FROM events e
WHERE e.id = 'evt-123'
  AND e.owner_tenant_id IN (
    SELECT descendant_id FROM tenant_closure
    WHERE ancestor_id = 'tenant-A'
      AND (barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'tenant-A')
  )
-- 0 rows -> 404 Not Found (hides resource existence)
-- 1 row -> return resource
```

### Fail-Closed Rules

The PEP MUST:

1. **Validate decision** - `decision: false` or missing -> deny all (403 Forbidden)
2. **Enforce require_constraints** - If `require_constraints: true` and `decision: true` but no `constraints` -> deny all (403 Forbidden)
3. **Apply constraints when present** - If `constraints` array is present, apply to SQL; if all constraints evaluate to false -> deny all
4. **Trust decision when constraints not required** - `decision: true` without `constraints` AND `require_constraints: false` -> allow (e.g., CREATE operations)
5. **Handle unreachable PDP** - Network failure, timeout -> deny all
6. **Handle unknown filter types** - Treat containing constraint as false; if all constraints false -> deny all
7. **Handle unknown filter ops** - Treat containing constraint as false
8. **Handle missing required fields** - Treat containing constraint as false
9. **Handle unknown field names** - Treat containing constraint as false (PEP doesn't know how to map)

---

## Open Questions

1. **"Allow all" semantics** - Should there be a way for PDP to express "allow all resources of this type" (e.g., for platform support roles)? Currently, constraints must have concrete filters. Future consideration: `filters: []` with explicit "allow all" semantics.

2. **Empty `filters` interpretation** - If a constraint has an empty `filters: []` array, should it mean "match all" or "match none"? Currently undefined.

3. **Batch evaluation optimization** - We support `/access/v1/evaluations` for batch requests. Should PDP optimize constraint generation when multiple evaluations share the same subject/context? Use cases: bulk operations, permission checks for UI rendering.

4. **Constraint caching** - Can constraints be cached at the PEP level beyond TTL? What invalidation signals are needed?

5. **AuthZEN context structure** - Is embedding HyperSpot-specific fields in `context` the right approach, or should we use a dedicated extension namespace?

6. **IANA registration** - Should HyperSpot register its extension parameters with the AuthZEN metadata registry?

7. **AuthZEN Search API relationship** - Our extended evaluation response serves similar purposes to Resource Search. Should we document this as a constraint-based alternative, or position it separately?

---

## References

- [OpenID AuthZEN Authorization API 1.0](https://openid.net/specs/authorization-api-1_0.html) (approved 2026-01-12)
- [Related ADR: PDP/PEP Authorization Model](../../../docs/adrs/authorization/0001-pdp-pep-authorization-model.md)
- [HyperSpot GTS (Global Type System)](../../types-registry/)
