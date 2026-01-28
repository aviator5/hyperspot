# Authorization Usage Scenarios

This document demonstrates the authorization model through concrete examples.
Each scenario shows the full flow: HTTP request → PDP evaluation → SQL execution.

For the core authorization design, see [AUTH.md](./AUTH.md).

All examples use a Task Management domain:
- **Resource:** `tasks` table with `id`, `owner_tenant_id`, `title`, `status`
- **Resource Groups:** Projects (tasks belong to projects)

---

## Table of Contents

- [When to Use Projection Tables](#when-to-use-projection-tables)
  - [When No Projection Tables Are Needed](#when-no-projection-tables-are-needed)
  - [When to Use `tenant_closure`](#when-to-use-tenant_closure)
  - [When to Use `resource_group_membership`](#when-to-use-resource_group_membership)
  - [When to Use `resource_group_closure`](#when-to-use-resource_group_closure)
  - [Combinations Summary](#combinations-summary)
- [Scenarios](#scenarios)
  - [With `tenant_closure`](#with-tenant_closure)
  - [Without `tenant_closure`](#without-tenant_closure)
  - [Resource Groups](#resource-groups)
  - [Advanced Patterns](#advanced-patterns)
- [TOCTOU Analysis](#toctou-analysis)
- [References](#references)

---

## When to Use Projection Tables

Before diving into scenarios, it's important to understand **why** a module chooses particular projection tables. The choice depends on the application's tenant structure, resource organization, and query patterns.

### Key Principle: Capabilities Determine Flow

| PEP Capability | Closure Table | Prefetch | PDP Response |
|----------------|---------------|----------|--------------|
| `tenant_hierarchy` | tenant_closure ✅ | **No** | `in_tenant_subtree` predicate |
| (none) | ❌ | **Yes** | `eq`/`in` or decision only |
| `group_hierarchy` | resource_group_closure ✅ | **No** | `in_group_subtree` predicate |
| `group_membership` | resource_group_membership ✅ | **No** | `in_group` predicate |
| (none for groups) | ❌ | **Yes** | explicit resource IDs |

### When No Projection Tables Are Needed

| Condition | Why Tables Aren't Required |
|-----------|---------------------------|
| Few tenants per vendor | PDP can return explicit tenant IDs in `in` predicate |
| Flat tenant structure | No hierarchy → `in_tenant_subtree` not needed |
| No resource groups | `in_group*` predicates not used |
| Low frequency LIST requests | Prefetch overhead is acceptable |

**Example:** Internal enterprise tool with 10 tenants, flat structure.

### When to Use `tenant_closure`

| Condition | Why Closure Is Needed |
|-----------|----------------------|
| Tenant hierarchy (parent-child) + many tenants | PDP cannot return all IDs in `in` predicate |
| Frequent LIST requests by subtree | Subtree JOINs more efficient than explicit ID lists |

**Note:** Self-managed tenants (barriers) and tenant status filtering can be checked by PDP on its side — this doesn't require closure on PEP side.

**Example:** Multi-tenant SaaS with organization hierarchy (org → teams → projects) and thousands of tenants.

### When to Use `resource_group_membership`

| Condition | Why Membership Table Is Needed |
|-----------|-------------------------------|
| Resources belong to groups | Projects, workspaces, folders |
| Frequent group-based filters | "Show all tasks in Project X" |
| Access control via groups | Role assignments at group level |

**Example:** Project management tool where tasks belong to projects.

### When to Use `resource_group_closure`

| Condition | Why Group Closure Is Needed |
|-----------|----------------------------|
| Group hierarchy | Nested folders, sub-projects |
| Subtree queries by groups | "Show all in folder and subfolders" |
| Many groups | PDP cannot expand entire hierarchy to explicit IDs |

**Example:** Document management with nested folders.

### Combinations Summary

| Use Case | tenant_closure | group_membership | group_closure |
|----------|----------------|------------------|---------------|
| Simple SaaS (flat tenants, no groups) | ❌ | ❌ | ❌ |
| Enterprise SaaS (tenant hierarchy) | ✅ | ❌ | ❌ |
| Project-based SaaS (flat tenants + projects) | ❌ | ✅ | ❌ |
| Complex SaaS (hierarchy + nested projects) | ✅ | ✅ | ✅ |

---

## Scenarios

> **Note:** SQL examples use subqueries for clarity. Production implementations
> may use JOINs or EXISTS for performance optimization.

### With `tenant_closure`

PEP has local tenant_closure table → can enforce `in_tenant_subtree` predicates.

---

#### Scenario 1: LIST (tenant subtree)

**Context:** User requests all tasks visible in their tenant subtree.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true,
      "respect_barrier": true
    },
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_tenant_subtree",
            "resource_property": "owner_tenant_id",
            "root_tenant_id": "T1",
            "respect_barrier": true
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE owner_tenant_id IN (
  SELECT descendant_id FROM tenant_closure
  WHERE ancestor_id = 'T1'
    AND (barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'T1')
)
```

---

#### Scenario 2: GET (tenant subtree)

**Context:** User requests a specific task; PEP enforces tenant subtree access at query level.

**Request:**
```http
GET /tasks/task-456
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "id": "task-456"
  },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_tenant_subtree",
            "resource_property": "owner_tenant_id",
            "root_tenant_id": "T1"
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE id = 'task-456'
  AND owner_tenant_id IN (
    SELECT descendant_id FROM tenant_closure
    WHERE ancestor_id = 'T1'
  )
```

**Result interpretation:**
- 1 row → return task
- 0 rows → **404 Not Found** (hides resource existence from unauthorized users)

---

#### Scenario 3: UPDATE (tenant subtree)

**Context:** User updates a task; constraint ensures atomic authorization check.

**Request:**
```http
PUT /tasks/task-456
Authorization: Bearer <token>
Content-Type: application/json

{"status": "completed"}
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "update" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "id": "task-456"
  },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_tenant_subtree",
            "resource_property": "owner_tenant_id",
            "root_tenant_id": "T1"
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
UPDATE tasks
SET status = 'completed'
WHERE id = 'task-456'
  AND owner_tenant_id IN (
    SELECT descendant_id FROM tenant_closure
    WHERE ancestor_id = 'T1'
  )
```

**Result interpretation:**
- 1 row affected → success
- 0 rows affected → **404 Not Found** (task doesn't exist or no access)

---

#### Scenario 4: CREATE

**Context:** User creates a new task. No constraints needed — PDP just validates permission.

**Request:**
```http
POST /tasks
Authorization: Bearer <token>
Content-Type: application/json

{"title": "New Task", "owner_tenant_id": "T2"}
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "create" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "properties": {
      "owner_tenant_id": "T2"
    }
  },
  "context": {
    "tenant_id": "T2",
    "require_constraints": false,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true
}
```

**SQL:**
```sql
INSERT INTO tasks (id, owner_tenant_id, title, status)
VALUES ('task-new', 'T2', 'New Task', 'pending')
```

**Note:** No constraints returned — `require_constraints: false` allows PEP to trust the decision for CREATE operations.

---

#### Scenario 5: Subtree with Barrier

**Context:** User in parent tenant T1 requests tasks, but child tenant T2 is self-managed (barrier). Tasks in T2 subtree should be excluded.

**Tenant hierarchy:**
```
T1 (parent)
├── T2 (self_managed=true)  ← barrier
│   └── T3
└── T4
```

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true,
      "respect_barrier": true
    },
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_tenant_subtree",
            "resource_property": "owner_tenant_id",
            "root_tenant_id": "T1",
            "respect_barrier": true
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE owner_tenant_id IN (
  SELECT descendant_id FROM tenant_closure
  WHERE ancestor_id = 'T1'
    AND (barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'T1')
)
```

**Result:** Returns tasks from T1 and T4 only. Tasks from T2 and T3 are excluded because T2 is a barrier.

**tenant_closure data example:**

| ancestor_id | descendant_id | barrier_ancestor_id |
|-------------|---------------|---------------------|
| T1 | T1 | NULL |
| T1 | T2 | T2 |
| T1 | T3 | T2 |
| T1 | T4 | NULL |
| T2 | T2 | NULL |
| T2 | T3 | NULL |

When querying from T1 with `respect_barrier=true`, only rows where `barrier_ancestor_id IS NULL OR barrier_ancestor_id = 'T1'` match → T1, T4.

---

### Without `tenant_closure`

PEP has no tenant_closure table → PDP returns explicit IDs or PEP prefetches attributes.

---

#### Scenario 6: LIST (explicit tenant IDs)

**Context:** PEP doesn't have tenant_closure. PDP resolves the subtree and returns explicit tenant IDs.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": []
  }
}
```

**PDP → PEP Response:**

PDP resolves the subtree internally and returns explicit IDs:

```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in",
            "resource_property": "owner_tenant_id",
            "values": ["T1", "T2", "T3"]
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE owner_tenant_id IN ('T1', 'T2', 'T3')
```

**Trade-off:** PDP must know the tenant hierarchy and resolve it. Works well for small tenant counts; may not scale for thousands of tenants.

---

#### Scenario 7: GET (prefetch)

**Context:** PEP doesn't have tenant_closure. For point operations, PEP prefetches resource attributes to send to PDP.

**Request:**
```http
GET /tasks/task-456
Authorization: Bearer <token>
```

**Step 1 — PEP prefetches resource attributes:**
```sql
SELECT owner_tenant_id FROM tasks WHERE id = 'task-456'
```
Result: `owner_tenant_id = 'T2'`

**Step 2 — PEP → PDP Request (with prefetched properties):**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "id": "task-456",
    "properties": {
      "owner_tenant_id": "T2"
    }
  },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": []
  }
}
```

**PDP → PEP Response:**

PDP validates that T2 is in T1's subtree and returns `eq` constraint for TOCTOU protection:

```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "eq",
            "resource_property": "owner_tenant_id",
            "value": "T2"
          }
        ]
      }
    ]
  }
}
```

**Step 3 — SQL with constraint:**
```sql
SELECT * FROM tasks
WHERE id = 'task-456'
  AND owner_tenant_id = 'T2'
```

**Result interpretation:**
- 1 row → return task
- 0 rows → concurrent modification occurred (tenant changed between prefetch and query) or resource doesn't exist → **404**

---

#### Scenario 8: UPDATE (prefetch + TOCTOU protection)

**Context:** Same as Scenario 7, but for UPDATE. The `eq` constraint protects against TOCTOU race conditions.

**Request:**
```http
PUT /tasks/task-456
Authorization: Bearer <token>
Content-Type: application/json

{"status": "completed"}
```

**Step 1 — PEP prefetches:**
```sql
SELECT owner_tenant_id FROM tasks WHERE id = 'task-456'
```
Result: `owner_tenant_id = 'T2'`

**Step 2 — PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "update" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "id": "task-456",
    "properties": {
      "owner_tenant_id": "T2"
    }
  },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": []
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "eq",
            "resource_property": "owner_tenant_id",
            "value": "T2"
          }
        ]
      }
    ]
  }
}
```

**Step 3 — SQL with constraint:**
```sql
UPDATE tasks
SET status = 'completed'
WHERE id = 'task-456'
  AND owner_tenant_id = 'T2'
```

**TOCTOU protection:** If another request changed `owner_tenant_id` between prefetch and UPDATE, the WHERE clause won't match → 0 rows affected → **404**. This prevents unauthorized modification even in a race condition.

---

#### Scenario 9: Same-tenant only (eq predicate)

**Context:** Simplest case — user can only access resources in their own tenant. No subtree, no hierarchy.

**Request:**
```http
GET /tasks/task-456
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.tasks.task.v1~",
    "id": "task-456"
  },
  "context": {
    "tenant_id": "T1",
    "require_constraints": true,
    "capabilities": []
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "eq",
            "resource_property": "owner_tenant_id",
            "value": "T1"
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE id = 'task-456'
  AND owner_tenant_id = 'T1'
```

**Note:** No prefetch needed — PDP returns direct `eq` constraint based on the subject's tenant.

---

### Resource Groups

---

#### Scenario 10: LIST with group_membership

**Context:** User has access to specific projects (flat group membership, no hierarchy).

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_id": "T1",
    "require_constraints": true,
    "capabilities": ["group_membership"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_group",
            "resource_property": "id",
            "group_ids": ["ProjectA", "ProjectB"]
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE id IN (
  SELECT resource_id FROM resource_group_membership
  WHERE group_id IN ('ProjectA', 'ProjectB')
)
```

---

#### Scenario 11: LIST with group_hierarchy

**Context:** User has access to a project folder and all its subfolders.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_id": "T1",
    "require_constraints": true,
    "capabilities": ["group_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_group_subtree",
            "resource_property": "id",
            "root_group_id": "FolderA"
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE id IN (
  SELECT resource_id FROM resource_group_membership
  WHERE group_id IN (
    SELECT descendant_id FROM resource_group_closure
    WHERE ancestor_id = 'FolderA'
  )
)
```

---

### Advanced Patterns

---

#### Scenario 12: Combined Tenant + Group (AND)

**Context:** User has access to tasks in their tenant subtree AND in specific projects. Both conditions must be satisfied.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_subtree": {
      "root_id": "T1",
      "include_root": true
    },
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy", "group_membership"]
  }
}
```

**PDP → PEP Response:**

Single constraint with multiple predicates (AND semantics):

```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_tenant_subtree",
            "resource_property": "owner_tenant_id",
            "root_tenant_id": "T1"
          },
          {
            "type": "in_group",
            "resource_property": "id",
            "group_ids": ["ProjectA"]
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE owner_tenant_id IN (
    SELECT descendant_id FROM tenant_closure
    WHERE ancestor_id = 'T1'
  )
  AND id IN (
    SELECT resource_id FROM resource_group_membership
    WHERE group_id = 'ProjectA'
  )
```

---

#### Scenario 13: Multiple Access Paths (OR)

**Context:** User has multiple ways to access tasks: (1) via project membership, (2) via explicitly shared tasks.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_id": "T1",
    "require_constraints": true,
    "capabilities": ["group_membership"]
  }
}
```

**PDP → PEP Response:**

Multiple constraints (OR semantics):

```json
{
  "decision": true,
  "context": {
    "constraints": [
      {
        "predicates": [
          {
            "type": "in_group",
            "resource_property": "id",
            "group_ids": ["ProjectA"]
          }
        ]
      },
      {
        "predicates": [
          {
            "type": "in",
            "resource_property": "id",
            "values": ["task-shared-1", "task-shared-2"]
          }
        ]
      }
    ]
  }
}
```

**SQL:**
```sql
SELECT * FROM tasks
WHERE (
    id IN (
      SELECT resource_id FROM resource_group_membership
      WHERE group_id = 'ProjectA'
    )
  )
  OR (
    id IN ('task-shared-1', 'task-shared-2')
  )
```

---

#### Scenario 14: Access Denied

**Context:** User doesn't have permission to access the requested resources.

**Request:**
```http
GET /tasks
Authorization: Bearer <token>
```

**PEP → PDP Request:**
```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "user-123",
    "properties": { "tenant_id": "T1" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.tasks.task.v1~" },
  "context": {
    "tenant_id": "T1",
    "require_constraints": true,
    "capabilities": ["tenant_hierarchy"]
  }
}
```

**PDP → PEP Response:**
```json
{
  "decision": false
}
```

**PEP Action:**
- No SQL query is executed
- Return **403 Forbidden** immediately

**Fail-closed principle:** The PEP never executes a database query when `decision: false`. This prevents any data leakage and ensures authorization is enforced before resource access.

---

## TOCTOU Analysis

[Time-of-check to time-of-use (TOCTOU)](https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use) is a class of race condition where a security check is performed at one point in time, but the protected action occurs later when conditions may have changed.

### How Each Scenario Handles TOCTOU

| Scenario | Closure | Prefetch | Constraint | TOCTOU Protection |
|----------|---------|----------|------------|-------------------|
| With closure | ✅ | No | `in_tenant_subtree` | ✅ Atomic SQL check |
| Without closure | ❌ | Yes | `eq` (prefetched value) | ✅ Atomic SQL check |
| CREATE | N/A | No | None | N/A (no existing resource) |

### Key Insight

Without closure tables, PDP still returns a constraint (`eq` or `in`) that is applied in SQL. This ensures TOCTOU protection even without closure tables:

1. **Prefetch:** PEP reads `owner_tenant_id = 'T2'` from database
2. **PDP check:** PDP validates T2 is accessible, returns `eq: owner_tenant_id = 'T2'`
3. **SQL execution:** `UPDATE tasks SET ... WHERE id = 'X' AND owner_tenant_id = 'T2'`
4. **If tenant changed:** WHERE clause won't match → 0 rows affected → 404

The constraint acts as a [compare-and-swap](https://en.wikipedia.org/wiki/Compare-and-swap) mechanism — if the value changed between check and use, the operation atomically fails.

---

## References

- [AUTH.md](./AUTH.md) — Core authorization design
- [TOCTOU - Wikipedia](https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use)
- [Race Conditions - PortSwigger](https://portswigger.net/web-security/race-conditions)
- [AWS Multi-tenant Authorization](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-api-access-authorization/introduction.html)
