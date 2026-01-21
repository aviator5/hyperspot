# ADR-0001: Auth Resolver Based on AuthZEN Authorization API 1.0

- **Status**: Draft
- **Date**: 2026-01-21
- **Decision Drivers**: AuthZEN standardization (approved 2026-01-12), vendor-neutral authorization, scalable query-time enforcement, pluggable modular architecture

## Table of Contents

- [Context](#context)
- [Decision](#decision)
- [AuthZEN Overview](#authzen-overview)
- [AuthZEN Gap Analysis](#authzen-gap-analysis)
- [HyperSpot Extension: Access Constraints API](#hyperspot-extension-access-constraints-api)
  - [Design Principles](#design-principles)
  - [Three Authorization Cases](#three-authorization-cases)
- [Core Terms](#core-terms)
- [Non-Core Vendor Concepts](#non-core-vendor-concepts)
- [Global Type System (GTS)](#global-type-system-gts)
- [Integration Architecture](#integration-architecture)
  - [Tenant Resolver (Gateway)](#tenant-resolver-gateway)
  - [Resource Group Resolver (Gateway)](#resource-group-resolver-gateway)
  - [Auth Resolver (Gateway)](#auth-resolver-gateway)
- [API Specifications](#api-specifications)
  - [Access Evaluation API (AuthZEN-compliant)](#access-evaluation-api-authzen-compliant)
  - [Access Constraints API (HyperSpot Extension)](#access-constraints-api-hyperspot-extension)
- [PEP Enforcement](#pep-enforcement)
  - [Semantics (Normative)](#semantics-normative)
  - [Error Handling (Normative)](#error-handling-normative)
  - [Case 2: Read-One with Unknown Tenant](#case-2-read-one-with-unknown-tenant)
  - [SQL Compilation Rules](#sql-compilation-rules)
  - [Self-Managed Barrier Semantics](#self-managed-barrier-semantics)
- [Database Prerequisites](#database-prerequisites)
- [Validation Scenarios](#validation-scenarios)
- [Rationale](#rationale)
- [Consequences](#consequences)
- [Open Questions](#open-questions)

---

## Context

HyperSpot is a pluggable, modular platform intended to be embedded into multi-tenant vendor platforms. Each vendor can have its own identity provider (IdP), authorization model, and an Account/Tenant service that is the system of record for tenant metadata and hierarchy.

HyperSpot must integrate with these vendor-specific systems without assuming a particular policy model (RBAC/ABAC/ReBAC), storage mechanism (ACL tables, policy documents, relationship graphs), or hierarchy representation. At the same time, HyperSpot modules must enforce authorization efficiently at the data layer (e.g., by constructing SQL predicates).

On January 12, 2026, the OpenID Foundation approved [AuthZEN Authorization API 1.0](https://openid.net/specs/authorization-api-1_0.html), establishing a standard for authorization APIs between Policy Decision Points (PDPs) and Policy Enforcement Points (PEPs).

**Key requirement not fully addressed by AuthZEN**: HyperSpot modules need to enforce authorization at the **query level** (e.g., SQL WHERE clauses), not just perform point-in-time access checks. For LIST/query operations, we need **constraints** that can filter results, not a boolean decision or enumerated resource IDs.

---

## Decision

We adopt AuthZEN Authorization API 1.0 as the foundation for Auth Resolver, with a **HyperSpot-specific extension** for constraint-based authorization:

1. **Access Evaluation API** (`/access/v1/evaluation`) — Fully AuthZEN-compliant for point access checks
2. **Access Constraints API** (`/access/v1/constraints`) — HyperSpot extension returning structured predicates for query-time enforcement

We introduce three gateway integration points:

1. **Tenant Resolver** — Integrates with vendor tenant/account system
2. **Resource Group Resolver** — Integrates with vendor resource-group model
3. **Auth Resolver** — PDP implementing AuthZEN + Access Constraints extension

Auth Resolver is the Policy Decision Point (PDP) for HyperSpot. HyperSpot domain modules act as Policy Enforcement Points (PEPs) by applying the returned constraints when querying or mutating data.

---

## AuthZEN Overview

AuthZEN Authorization API 1.0 defines:

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
  "resource": { "type": "document" }  // no "id" — searching
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

---

## AuthZEN Gap Analysis

| Aspect | AuthZEN 1.0 | HyperSpot Requirement | Gap |
|--------|-------------|----------------------|-----|
| **Point access check** | `decision: true/false` | Same | None |
| **List/query operations** | Resource Search returns IDs | Need constraints for SQL WHERE | **Critical gap** |
| **Tenant hierarchy** | Not specified | First-class primitive | Extension needed |
| **Resource groups** | Not specified | First-class primitive | Extension needed |
| **Capability negotiation** | Not specified | PEP declares what it can enforce | Extension needed |
| **Constraint-based filtering** | Not supported | Core requirement | **Critical gap** |

### The Fundamental Problem with Resource Search

Consider listing events across a tenant hierarchy with 10,000 tenants and millions of events:

**AuthZEN approach (Resource Search):**
```json
// Response would enumerate millions of resource IDs
{
  "results": [
    { "type": "event", "id": "evt-1" },
    { "type": "event", "id": "evt-2" },
    // ... millions more
  ]
}
```

This is impractical for:
1. Large result sets (pagination doesn't solve the O(N) problem)
2. Dynamic data (results change between pages)
3. SQL-based enforcement (can't pass millions of IDs to WHERE IN)

**What HyperSpot needs:**
```json
// Response returns constraints that compile to SQL
{
  "alternatives": [{
    "tenant_scope": {
      "mode": "context_tenant_and_descendants",
      "context_tenant_id": "tenant-123"
    }
  }]
}
// Compiles to: WHERE owner_tenant_id IN (SELECT descendant_id FROM tenant_closure WHERE ancestor_id = 'tenant-123')
```

---

## HyperSpot Extension: Access Constraints API

We introduce a new endpoint that extends AuthZEN's model:

```
POST /access/v1/constraints
```

This returns **structured predicates** instead of enumerated resources. The predicates are designed for SQL-first enforcement but are transport-agnostic.

### Design Principles

1. **AuthZEN alignment** — Use same `subject`, `action`, `resource`, `context` structure
2. **Constraint-first** — Return predicates, not enumerated IDs
3. **Capability negotiation** — PEP declares enforcement capabilities
4. **Fail-closed** — Unknown constraints or schemas result in deny
5. **OR/AND semantics** — Multiple access paths via `alternatives[]` (OR), each alternative is conjunctive (AND)

### Three Authorization Cases

Understanding which API to use depends on what the PEP knows at request time:

| Case | PEP knows | PDP returns | PEP action | API |
|------|-----------|-------------|------------|-----|
| **1. Read-one, tenant+resource known** | tenant_id + resource_id | `decision: true/false` | Fetch by ID (authz done by PDP) | Evaluation API |
| **2. Read-one, tenant unknown** | only resource_id | constraints | Build SQL with tenant scope | Constraints API |
| **3. List/query** | neither | constraints | Build SQL with constraints as filter | Constraints API |

**Case 1 — Full evaluation by PDP:**
- PEP knows tenant (from API path, encoded in resource ID, or cached)
- PDP has all information to make a complete authorization decision
- Returns boolean — PEP does not need to add authorization to SQL
- Example: `GET /tenants/{tenant_id}/events/{event_id}` — tenant is explicit in path

**Cases 2 & 3 — Partial evaluation, PEP applies constraints:**
- PDP lacks information to make a complete decision (doesn't know resource properties)
- Returns "what scope is allowed" as structured constraints
- PEP applies constraints as SQL WHERE clause to complete the authorization
- Example Case 2: `GET /events/{event_id}` — resource could belong to any tenant in authorized subtree
- Example Case 3: `GET /events?topic=...` — listing resources across tenant scope

**Why Case 2 must use Constraints API:**

For `GET /events/{event_id}` where tenant is not in the path, the PEP doesn't know `owner_tenant_id` before querying. The resource might belong to:
- The context tenant directly
- Any descendant tenant (if `context_tenant_and_descendants` scope applies)

Calling the Evaluation API would require first fetching the resource to discover its `owner_tenant_id`, which creates two problems:
1. Two database round-trips instead of one
2. Information leakage — returning 403 vs 404 reveals whether the resource exists

The correct pattern: call Constraints API, then query with `WHERE id = :event_id AND owner_tenant_id IN (...)`. If 0 rows returned, return 404 (don't distinguish "not found" from "not authorized").

---

## Core Terms

- **Tenant**
  A domain of ownership/responsibility and policy (billing, security settings, default data isolation). Cross-tenant access is granted via explicit delegations, not by transferring ownership.

- **Subject / Principal**
  The actor initiating the request (user or API client). In cross-tenant scenarios, it is critical to distinguish where the subject belongs vs where the subject is acting.

- **Subject Tenant**
  The tenant the subject/principal belongs to.

- **Context Tenant**
  The tenant in whose context the request is being processed; the scope root for authorization. This is the tenant boundary within which the operation is authorized. In cross-tenant scenarios, the context tenant may differ from the subject tenant (e.g., when a parent tenant user operates within a child tenant's scope).

- **Resource Owner Tenant**
  The actual tenant that owns the resource (`owner_tenant_id` column). May equal the context tenant, or be a descendant of it when `context_tenant_and_descendants` scope is used.

- **Resource**
  An object with an owner tenant identifier (`owner_tenant_id`). Authorization checks follow the pattern: principal performs an operation on a resource.

- **Resource Group**
  An optional core primitive representing a container for resources to simplify access management and lifecycle (e.g., project/workspace/folder). A resource may belong to multiple resource groups simultaneously. Some vendors provide resource-group hierarchy (often implemented using a closure table). If a vendor does not support resource groups, this concept is not used.

- **Permission**
  An identifier of an allowed operation, independent of a specific subject or resource instance. A permission is defined as `{ resource_type, action }`. `resource_type` is a GTS type identifier. `action` MAY be a string in v1, and MAY evolve into a GTS-typed identifier in future versions.

- **Access Constraints**
  Computed applicability constraints for a permission in the context of a specific request. In other words: "Which tenants/resources can this operation apply to right now?" This is the output of the Access Constraints API.

  Access Constraints are expressed as structured predicates/filters (not necessarily enumerated IDs), used by HyperSpot modules to build safe SQL (or other enforcement). Constraints are time-bound (TTL) and valid at evaluation time.

  Access Constraints are NOT Access Policies. Policies are stored vendor-side (RBAC/ABAC/ReBAC, grants, relationship graphs, etc.). Access Constraints are *request-scoped, time-bound enforcement artifacts* produced by evaluating policies for a specific request and compiling them into a form enforceable by the PEP.

- **PDP (Policy Decision Point)**
  Auth Resolver implementing authorization decisions.

- **PEP (Policy Enforcement Point)**
  HyperSpot modules applying constraints.

---

## Non-Core Vendor Concepts

These concepts may exist in vendor systems but HyperSpot must not depend on them directly:

- **Role**
  A named bundle of permissions assigned to users/groups for administration convenience.

- **Authorization Model / Policy Model**
  How access rules are expressed and stored by the vendor (RBAC/ABAC/ReBAC, Zanzibar-style relationship graphs, Cedar/OPA policies, ACL-like grants, or hybrids). HyperSpot must integrate without assuming a specific model or storage mechanism.

---

## Global Type System (GTS)

HyperSpot uses a Global Type System (GTS) and a Type Registry as the authoritative source of truth for all shared type identifiers used across gateways and modules. In particular, `tenant_type`, `resource_type`, `subject_type` (and other type-like identifiers) are GTS type identifiers. This enables vendor-safe extensibility, compatibility checks, and consistent interpretation of typed payloads across integrations.

---

## Integration Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Vendor Platform                              │
│  ┌──────────┐  ┌─────────────────┐  ┌────────────┐  ┌────────────┐  │
│  │   IdP    │  │ Tenant Service  │  │  RG Svc    │  │ Authz Svc  │  │
│  └────▲─────┘  └───────▲─────────┘  └─────▲──────┘  └─────▲──────┘  │
└───────┼────────────────┼──────────────────┼───────────────┼─────────┘
        │                │                  │               │
┌───────┼────────────────┼──────────────────┼───────────────┼─────────┐
│       │         HyperSpot                 │               │         │
│  ┌────┴────┐  ┌────────┴────────┐  ┌──────┴─────┐  ┌──────┴──────┐  │
│  │  AuthN  │  │ Tenant Resolver │  │ RG Resolver│  │Auth Resolver│  │
│  │ (JWT/   │  │    (Gateway)    │  │  (Gateway) │  │   (PDP)     │  │
│  │ Introsp)│  └────────┬────────┘  └──────┬─────┘  └──────┬──────┘  │
│  └─────────┘           │                  │               │         │
│                        ▼                  ▼               │         │
│              ┌─────────────────────────────────┐          │         │
│              │     Local Projections           │          │         │
│              │  • tenant_projection            │          │         │
│              │  • tenant_closure               │          │         │
│              │  • resource_group_projection    │          │         │
│              │  • resource_group_closure       │          │         │
│              │  • resource_group_membership    │          │         │
│              └─────────────────────────────────┘          │         │
│                                                           │         │
│  ┌────────────────────────────────────────────────────────┼───────┐ │
│  │                    Domain Module (PEP)                 │       │ │
│  │  ┌─────────────┐                                       │       │ │
│  │  │   Handler   │──── /access/v1/evaluation ───────────►│       │ │
│  │  │             │──── /access/v1/constraints ──────────►│       │ │
│  │  └──────┬──────┘                                               │ │
│  │         │ Compile constraints to SQL                           │ │
│  │         ▼                                                      │ │
│  │  ┌─────────────┐                                               │ │
│  │  │  Database   │  WHERE owner_tenant_id IN (...)               │ │
│  │  └─────────────┘                                               │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Tenant Resolver (Gateway)

#### Responsibility

Tenant Resolver integrates with the vendor's tenant/account system and provides tenant metadata and tenant hierarchy. Across integrations, tenant topology is a forest.

#### Expected Capabilities

- Read a tenant by ID
- List children for a tenant (with filtering and pagination as needed)
- List parents for a tenant

Each tenant includes at minimum:

- Identifier
- Type
- Status
- Management mode (e.g., managed vs self-managed)
- Name
- Parent identifier

Management mode does not grant access by itself; it is a policy input used to validate delegations and enforce vendor rules.

#### Local Projections in Domain Modules

For efficient authorization and data-layer enforcement in large hierarchies, HyperSpot deployments may maintain a local tenant-closure projection (ancestor/descendant relationships).

- Projection population is expected to be driven primarily by event-driven sync from Tenant Resolver (normalized tenant create/update/move events), with optional periodic reconciliation as a safety net.
- If the vendor cannot provide events, polling-based sync may be used with clearly defined staleness bounds.
- Modules and Auth Resolver integrations that rely on subtree semantics should explicitly document whether they depend on this closure projection and how they behave under eventual consistency.

### Resource Group Resolver (Gateway)

#### Responsibility

Resource Group Resolver integrates with the vendor's resource-group model (projects/workspaces/folders, etc.), including resource-to-group membership (many-to-many) and group hierarchy (if present).

#### Expected Capabilities

- Read a resource group by ID
- Resolve group hierarchy relationships (parents/children/descendants as needed)
- Resolve resource-to-group membership (many-to-many)
- Provide normalized updates (events) so domain modules can keep local projections in sync

#### Local Projections in Domain Modules

For efficient enforcement (especially LIST endpoints), domain modules may maintain local projection tables (or equivalent) such as:

- Resource-to-group membership projection that enables fast JOIN/EXISTS checks
- Optionally, group-closure projection if group hierarchy is required for enforcement

Projection population is expected to be driven primarily by event-driven sync from Resource Group Resolver (with optional write-through on HyperSpot-initiated changes and periodic reconciliation as a safety net).

### Auth Resolver (Gateway)

#### Responsibility

Auth Resolver provides authentication (AuthN) and authorization (AuthZ). For authorization, it is the Policy Decision Point (PDP) returning decisions and enforceable Access Constraints. HyperSpot modules apply these constraints as PEPs.

#### Authentication

HyperSpot authentication can be configured in two ways:

- Always delegate authentication to Auth Resolver (vendor-specific tokens/sessions supported; may use token introspection for opaque tokens)
- Accept only JWT Bearer tokens validated locally (requires vendor IdP to support OpenID Connect discovery and JWKs)

When vendor tokens are opaque (non-JWT) or require near-real-time revocation, Auth Resolver may validate them via token introspection (or equivalent vendor APIs) and return a normalized subject identity and tenant context.

Required normalized authentication output (beyond standard token claims):

- Subject identifier
- Subject type
- Subject tenant identifier
- Subject tenant type

#### Authorization

Auth Resolver provides two APIs:

1. **Access Evaluation API** (`/access/v1/evaluation`) — AuthZEN-compliant boolean decision
2. **Access Constraints API** (`/access/v1/constraints`) — HyperSpot extension returning enforceable constraints

---

## API Specifications

### Access Evaluation API (AuthZEN-compliant)

For authorization checks where the PDP can make a **complete decision**:

- **Case 1 (read-one, tenant+resource known)**: When tenant is in the API path, encoded in the resource ID, or cached — PDP evaluates and returns boolean
- **Non-database permission checks**: E.g., "can this user access the admin panel?"
- **Resource already in memory**: When resource properties are known without a database query

The PDP evaluates the full authorization question and returns a boolean. The PEP does not need to apply additional authorization constraints in SQL.

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
    "name": "read"
  },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "id": "e81307e5-5ee8-4c0a-8d1f-bd98a65c517e",
    "properties": {
      "owner_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
      "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
    }
  },
  "context": {
    // HyperSpot extension: tenant context for cross-tenant scenarios
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
  }
}
```

#### Response

```jsonc
{
  "decision": true,
  "context": {
    // Optional: enforcement hints, obligations, reasons
    "reason_admin": { "policy": "tenant-member-read" }
  }
}
```

### Access Constraints API (HyperSpot Extension)

For authorization checks where the PDP **cannot make a complete decision** because resource properties are unknown:

- **Case 2 (read-one, tenant unknown)**: `GET /events/{event_id}` — PDP returns tenant scope; PEP applies as WHERE clause
- **Case 3 (list/query)**: `GET /events?topic=...` — PDP returns constraints; PEP applies as filter
- **Create**: PDP returns allowed target tenant(s)/group(s); PEP validates before INSERT
- **Update/Delete**: PDP returns constraints; PEP applies as WHERE clause to scope mutation

The PDP performs partial evaluation and returns structured constraints. The PEP completes the authorization by applying constraints at the data layer (SQL WHERE clause).

#### Request

```
POST /access/v1/constraints
Content-Type: application/json
```

```jsonc
{
  // AuthZEN-aligned subject/action/resource structure
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": {
      "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
    }
  },
  "action": {
    "name": "list"  // or "read", "create", "update", "delete"
  },
  "resource": {
    "type": "gts.x.events.event.v1~"
    // Note: no "id" for list operations — this is a query, not a point check
    // For single-resource operations, "id" may be included
  },

  // HyperSpot extension: context with tenant scope and intent
  "context": {
    // Tenant context anchor
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",

    // Intent: what scope the caller is requesting
    "intent": {
      "tenant_scope": {
        "mode": "context_tenant_and_descendants",  // or "context_tenant_only"
        "ignore_self_managed_barrier": false,      // default: false (respects barrier)
        "ids": [...],                              // optional: narrow to specific tenants
        "attributes": {                            // optional: attribute-based filtering
          "status": ["active"]
        }
      },
      "group_scope": {                             // optional
        "root_id": "dept-123",                     // optional: hierarchy root
        "ids": ["group-123", "group-456"]          // optional: specific groups
      },
      "resource_scope": {                          // optional
        "ids": ["res-1", "res-2"],                 // optional: specific resource IDs
        "attributes": {                            // optional: attribute filters
          "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
        }
      }
    },

    // PEP capabilities: what the caller can enforce
    "capabilities": {
      "tenant_closure": true,            // can use tenant_closure table
      "resource_group_membership": true, // can use membership projection
      "resource_group_closure": true     // can use group closure table
    }
  }
}
```

#### Response (Allow)

```jsonc
{
  "decision": "allow",  // "allow" | "deny"

  // Schema version for fail-closed validation
  "schema": "urn:hyperspot:authz:constraints:v1",

  // Time-bound validity
  "issued_at": "2026-01-21T10:00:00Z",
  "ttl_seconds": 60,

  // Echoed request context for audit/debugging
  "subject": { "type": "...", "id": "...", "properties": {...} },
  "action": { "name": "list" },
  "resource": { "type": "..." },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": { ... }  // echoed
  },

  // Alternatives: OR semantics (at least one must be satisfiable)
  // Each alternative: AND semantics (all scopes must be satisfied)
  "alternatives": [
    {
      // Tenant scope constraint
      "tenant_scope": {
        "mode": "context_tenant_and_descendants",  // | "context_tenant_only" | "explicit_ids"
        "context_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
        "ignore_self_managed_barrier": false,
        // If mode="explicit_ids" or PEP lacks closure capability:
        "ids": ["51f18034-...", "93953299-...", "7a8b9c0d-..."],  // optional
        "attributes": {               // optional: attribute-based filtering
          "status": ["active"]
        }
      },

      // Resource group constraint (optional, omit if no group-based policy)
      "group_scope": {
        "mode": "explicit_ids",       // | "descendants_of_root"
        "ids": ["group-123", "group-456"],
        // If mode="descendants_of_root":
        "root_id": "dept-root"        // optional
      },

      // Resource attribute constraints (optional)
      "resource_scope": {
        "attributes": {
          "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1"
        },
        // Explicit resource IDs if policy restricts to specific resources
        "ids": [...]                   // optional, rare
      }
    }
    // Additional alternatives represent OR paths
    // e.g., user has access via role A OR via group B
  ]
}
```

#### Response (Deny)

```jsonc
{
  "decision": "deny",
  "schema": "urn:hyperspot:authz:constraints:v1",
  "issued_at": "2026-01-21T10:00:00Z",
  "subject": { ... },
  "action": { ... },
  "resource": { ... },
  "context": {
    "reason_admin": { "policy": "no-matching-grant" }
  }
}
```

---

## PEP Enforcement

### Semantics (Normative)

- The PEP MUST provide:
  - Subject identity and subject tenant
  - Requested permission (resource type + action)
  - Resource context (tenant/group) when applicable
  - Target selection (single resource vs query)
  - Enforcement capabilities (what the PEP can enforce)

- The PDP returns a decision artifact:
  - `alternatives[]` combined with `combine="OR"`
  - Each alternative is a conjunctive constraint set (AND):
    `tenant_scope AND group_scope AND resource_scope`

- Decision vs alternatives:
  - `decision="deny"` means final deny. The response MAY omit alternatives. If alternatives are present, they MUST be ignored by the PEP.
  - `decision="allow"` means conditionally allow, subject to enforcement:
    - The response MUST include a non-empty `alternatives[]`.
    - The PEP MUST treat each alternative as an enforceable constraint candidate.
    - If the PEP cannot enforce an alternative, it MUST treat that alternative as false.
    - The PEP MUST allow the request only if at least one alternative is enforceable and evaluates to true at the data layer.
  - If `decision="allow"` and alternatives are missing/empty → the PEP MUST deny (fail-closed).

- The PEP MUST:
  1. Validate `schema` via Type Registry; unknown → deny.
  2. Compile each alternative into SQL WHERE predicate (AND inside).
  3. Combine alternatives with OR.
  4. Treat any non-enforceable alternative as `false`.
  5. If all alternatives are `false`, deny (fail-closed).
  6. Enforce TTL (`ttl_seconds`) and never use expired constraints.

### Error Handling (Normative)

- **PDP unavailability**: If the PEP cannot reach Auth Resolver (network failure, timeout, 5xx errors), the PEP MUST treat this as a **deny**. This is fail-closed behavior — no authorization decision means no access.
- **Malformed responses**: If the PDP returns a response that fails schema validation or contains unrecognized `schema`, the PEP MUST deny.
- **Timeouts**: PEPs SHOULD define reasonable timeouts for constraint calls. Timeout → deny.
- **Partial failures**: If `decision="allow"` but `alternatives` is malformed or unparseable, the PEP MUST deny.

> **Note on Actions**: The `action` field (`read`, `list`, `create`, `update`, `delete`) is passed to the PDP and influences policy evaluation, but the constraint response format and PEP enforcement mechanism are identical across all actions.
>
> **Create operations**: Since the resource doesn't exist yet, `resource.id` is omitted in the request. The returned constraints define which tenant(s) and/or resource group(s) the subject is authorized to create resources within.

### Case 2: Read-One with Unknown Tenant

For endpoints like `GET /events/{event_id}` where tenant is not in the API path:

**Problem**: The PEP doesn't know `owner_tenant_id` before querying. The resource might belong to:
- The context tenant directly
- Any descendant tenant (if `context_tenant_and_descendants` scope applies)

**Anti-pattern (two queries, information leakage):**
```
1. Fetch resource to get owner_tenant_id
2. Call Evaluation API with known properties
3. Return resource or 403
```

This approach has two critical flaws:
- Two database round-trips instead of one
- Information leakage: returning 403 vs 404 reveals whether the resource exists to unauthorized users

**Correct pattern (single query with Constraints API):**
```
1. Call Constraints API to get tenant scope
2. Build SQL: WHERE id = :event_id AND owner_tenant_id IN (...)
3. If 0 rows → 404 (don't distinguish "not found" from "not authorized")
```

**Benefits:**
- Single database round-trip
- No information leakage (attacker cannot probe for resource existence)
- Authorization enforced at data layer
- Consistent with list operations (same constraint-based approach)

See [Scenario 1](#scenario-1-same-tenant-access-read-one--case-2) for a complete example.

### SQL Compilation Rules

#### Tenant Scope

| Constraint | SQL |
|------------|-----|
| `tenant_scope.mode = "context_tenant_only"` | `owner_tenant_id = :context_tenant_id` |
| `tenant_scope.mode = "context_tenant_and_descendants"` (with closure) | `owner_tenant_id IN (SELECT descendant_id FROM tenant_closure WHERE ancestor_id = :context_tenant_id)` |
| `tenant_scope.mode = "context_tenant_and_descendants"` + `ignore_self_managed_barrier = false` | `... AND barrier_ancestor_id IS NULL` |
| `tenant_scope.mode = "context_tenant_and_descendants"` + `attributes.status` | `... JOIN tenant_projection tp ON ... WHERE tp.status IN (:statuses)` |
| `tenant_scope.ids` (explicit IDs, fallback when PEP lacks closure) | `owner_tenant_id IN (:id1, :id2, ...)` |

#### Group Scope

| Constraint | SQL |
|------------|-----|
| `group_scope.ids` | `resource_id IN (SELECT resource_id FROM resource_group_membership WHERE group_id IN (:ids))` |
| `group_scope.root_id` (descendants via closure) | `resource_id IN (SELECT m.resource_id FROM resource_group_membership m WHERE m.group_id IN (SELECT descendant_id FROM resource_group_closure WHERE ancestor_id = :root_id))` |
| `group_scope.root_id` + `group_scope.ids` (intersection) | Both constraints combined with AND |
| `null / absent` | No group constraint |

#### Resource Scope

| Constraint | SQL |
|------------|-----|
| `resource_scope.ids` | `AND resource_id IN (:id1, :id2)` |
| `resource_scope.attributes.X` | `AND X = :value` |
| `null / absent` | No additional resource constraint |

#### Multiple Alternatives (OR)

```sql
WHERE (
    -- Alternative 1
    (tenant_scope_1 AND group_scope_1 AND resource_scope_1)
    OR
    -- Alternative 2
    (tenant_scope_2 AND resource_scope_2)
)
```

### Self-Managed Barrier Semantics

`management_mode` is NOT a simple filter. It acts as a **barrier** in hierarchy traversal:

```
Tenant A (managed)
├── Tenant B (self-managed)  ← BARRIER
│   ├── Tenant C (managed)   ← Hidden from A
│   └── Tenant D (managed)   ← Hidden from A
└── Tenant E (managed)       ← Visible to A
    └── Tenant F (managed)   ← Visible to A
```

**Barrier rule**: When traversing from `context_tenant_id`, stop at self-managed tenants. The self-managed tenant AND its entire subtree are hidden by default.

**`ignore_self_managed_barrier` flag**: Callers can explicitly opt-in to cross the self-managed barrier by setting `ignore_self_managed_barrier: true` in the request. However, the PDP has final say based on permission policy — some permissions may never allow crossing the barrier.

**Permission-dependent visibility**: Some permissions (e.g., "view usage billing") may ignore the barrier and see through to self-managed subtrees. The PDP determines this based on permission policy.

**Implementation options for barrier:**
1. **Closure table approach**: Add `barrier_ancestor_id` column that tracks the nearest self-managed ancestor (NULL if none). Query filters on this.
2. **Separate closure tables**: `tenant_closure_full` (all relationships) vs `tenant_closure_managed` (stops at self-managed barriers).
3. **Runtime CTE**: Recursive query that stops at self-managed boundaries.

---

## Database Prerequisites

### Base Resource Columns

Every resource table SHOULD include the following columns:

| Column | Type | Description |
|--------|------|-------------|
| `id` | `UUID PRIMARY KEY` | Resource identifier |
| `owner_tenant_id` | `UUID NOT NULL` | The resource owner tenant; critical for tenant isolation |
| `creator_subject_id` | `UUID` | Audit: subject who created the resource |
| `creator_tenant_id` | `UUID` | Audit: tenant from which the resource was created (supports cross-tenant creation tracking) |

### Tenant Projection Table

Local projection populated by event-driven sync from Tenant Resolver:

```sql
CREATE TABLE tenant_projection (
    tenant_id           UUID PRIMARY KEY,
    tenant_type         TEXT NOT NULL,        -- GTS type identifier
    status              TEXT NOT NULL,        -- active, suspended, blocked, etc.
    management_mode     TEXT NOT NULL,        -- managed, self_managed
    name                TEXT NOT NULL,
    parent_tenant_id    UUID NULL,            -- NULL for root tenants
    synced_at           TIMESTAMP NOT NULL,   -- last sync from Tenant Resolver

    FOREIGN KEY (parent_tenant_id) REFERENCES tenant_projection(tenant_id)
);
```

### Tenant Closure Table

For efficient ancestor/descendant queries (subtree semantics in authorization):

```sql
CREATE TABLE tenant_closure (
    ancestor_id         UUID NOT NULL,
    descendant_id       UUID NOT NULL,
    depth               INT NOT NULL,             -- 0 = self, 1 = direct child, etc.
    barrier_ancestor_id UUID NULL,                -- nearest self-managed ancestor in path (NULL if none)

    PRIMARY KEY (ancestor_id, descendant_id),
    FOREIGN KEY (ancestor_id) REFERENCES tenant_projection(tenant_id),
    FOREIGN KEY (descendant_id) REFERENCES tenant_projection(tenant_id),
    FOREIGN KEY (barrier_ancestor_id) REFERENCES tenant_projection(tenant_id)
);

CREATE INDEX idx_tenant_closure_ancestor ON tenant_closure(ancestor_id);
CREATE INDEX idx_tenant_closure_descendant ON tenant_closure(descendant_id);
CREATE INDEX idx_tenant_closure_barrier ON tenant_closure(barrier_ancestor_id) WHERE barrier_ancestor_id IS NOT NULL;
```

The closure table includes self-referencing entries `(T, T, 0)` for every tenant, enabling "tenant or descendants" queries without special-casing.

### Resource Group Projection Table

Local projection populated by event-driven sync from Resource Group Resolver:

```sql
CREATE TABLE resource_group_projection (
    group_id            UUID PRIMARY KEY,
    group_type          TEXT NOT NULL,        -- GTS type (project/workspace/folder)
    name                TEXT NOT NULL,
    owner_tenant_id     UUID NOT NULL,        -- which tenant owns this group
    parent_group_id     UUID NULL,            -- for hierarchical groups
    synced_at           TIMESTAMP NOT NULL,

    FOREIGN KEY (owner_tenant_id) REFERENCES tenant_projection(tenant_id),
    FOREIGN KEY (parent_group_id) REFERENCES resource_group_projection(group_id)
);
```

### Resource Group Closure Table

For efficient ancestor/descendant queries on group hierarchy (if applicable):

```sql
CREATE TABLE resource_group_closure (
    ancestor_id     UUID NOT NULL,
    descendant_id   UUID NOT NULL,
    depth           INT NOT NULL,

    PRIMARY KEY (ancestor_id, descendant_id),
    FOREIGN KEY (ancestor_id) REFERENCES resource_group_projection(group_id),
    FOREIGN KEY (descendant_id) REFERENCES resource_group_projection(group_id)
);

CREATE INDEX idx_resource_group_closure_ancestor ON resource_group_closure(ancestor_id);
CREATE INDEX idx_resource_group_closure_descendant ON resource_group_closure(descendant_id);
```

### Resource-to-Group Membership Table

Many-to-many relationship between resources and resource groups:

```sql
CREATE TABLE resource_group_membership (
    resource_id         UUID NOT NULL,
    group_id            UUID NOT NULL,
    synced_at           TIMESTAMP NOT NULL,

    PRIMARY KEY (resource_id, group_id),
    FOREIGN KEY (group_id) REFERENCES resource_group_projection(group_id)
);

CREATE INDEX idx_membership_resource ON resource_group_membership(resource_id);
CREATE INDEX idx_membership_group ON resource_group_membership(group_id);
```

### Example Domain Table: Events

For scenario validation, we use the following events table:

```sql
CREATE TABLE events (
    id              UUID PRIMARY KEY,
    owner_tenant_id UUID NOT NULL,
    topic_id        UUID NOT NULL,        -- UUID v5 derived from GTS topic identifier
    payload         JSONB,
    created_at      TIMESTAMP NOT NULL,

    FOREIGN KEY (owner_tenant_id) REFERENCES tenant_projection(tenant_id)
);
```

---

## Validation Scenarios

> **Note on SQL examples:** The SQL queries in these scenarios intentionally use subqueries (`IN (SELECT ...)`) for clarity and readability. In production, these can be rewritten as JOINs or EXISTS clauses for performance optimization depending on the database and query planner.

### Scenario 0: Read-One with Known Tenant — Case 1 (Evaluation API)

**Request:** `GET /tenants/{tenant_id}/events/{event_id}`

Tenant ID is explicit in the API path. The PEP can provide full resource context to the PDP.

**PEP → PDP Request (Evaluation API):**

```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": { "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "id": "e81307e5-5ee8-4c0a-8d1f-bd98a65c517e",
    "properties": {
      "owner_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
    }
  },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": true,
  "context": {
    "reason_admin": { "policy": "tenant-member-read" }
  }
}
```

**PEP Action:**

The PDP already verified authorization. The PEP simply fetches by ID:

```sql
SELECT e.* FROM events e WHERE e.id = 'e81307e5-5ee8-4c0a-8d1f-bd98a65c517e'
```

No tenant constraint in SQL — the PDP confirmed access. If the resource doesn't exist, return 404.

**Key difference from Case 2:** The PEP does not need to apply tenant scope in SQL because the PDP had full context to make the decision. This is appropriate when:
- Tenant is explicit in the API path (`/tenants/{tenant_id}/...`)
- Resource ID encodes tenant information
- Resource metadata was already cached/retrieved

---

### Scenario 1: Same-Tenant Access (Read One) — Case 2 (Constraints API)

**Request:** `GET /events/{event_id}?topic={topic_id}`

Subject reads an event within their own tenant (`subject.properties.tenant_id = context.tenant_id`).

**PEP → PDP Request:**

```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": { "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" }
  },
  "action": { "name": "read" },
  "resource": {
    "type": "gts.x.events.event.v1~",
    "id": "e81307e5-5ee8-4c0a-8d1f-bd98a65c517e",
    "properties": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" }
  },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": {
      "tenant_scope": { "mode": "context_tenant_only" },
      "resource_scope": {
        "ids": ["e81307e5-5ee8-4c0a-8d1f-bd98a65c517e"],
        "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" }
      }
    },
    "capabilities": { "tenant_closure": true, "resource_group_membership": false }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "schema": "urn:hyperspot:authz:constraints:v1",
  "issued_at": "2026-01-21T10:00:00Z",
  "ttl_seconds": 60,
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" },
    "resource_scope": {
      "ids": ["e81307e5-5ee8-4c0a-8d1f-bd98a65c517e"],
      "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" }
    }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.id = 'e81307e5-5ee8-4c0a-8d1f-bd98a65c517e'
  AND e.topic_id = '<uuid5-from-gts-topic-id>'
  AND e.owner_tenant_id = '51f18034-3b2f-4bfa-bb99-22113bddee68'
```

---

### Scenario 2: Same-Tenant Access (List) — Case 3

**Request:** `GET /events?topic={topic_id}`

Subject lists events within their own tenant.

**PEP → PDP Request:**

```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": { "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.events.event.v1~" },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": {
      "tenant_scope": { "mode": "context_tenant_only" },
      "resource_scope": { "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" } }
    },
    "capabilities": { "tenant_closure": true, "resource_group_membership": false }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "schema": "urn:hyperspot:authz:constraints:v1",
  "issued_at": "2026-01-21T10:00:00Z",
  "ttl_seconds": 60,
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" },
    "resource_scope": { "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" } }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.topic_id = '<uuid5-from-gts-topic-id>'
  AND e.owner_tenant_id = '51f18034-3b2f-4bfa-bb99-22113bddee68'
```

---

### Scenario 3: Tenant Subtree Access (List with Closure) — Case 3

**Prerequisites:** Domain module has `tenant_closure` table.

**Request:** `GET /events?topic={topic_id}`

Subject lists events across their tenant and all descendant tenants.

**Tenant Hierarchy:**
```
Subject/Context Tenant (51f18034-...) ← parent
    ├── Child Tenant A (93953299-...)
    └── Child Tenant B (7a8b9c0d-...)
```

**PEP → PDP Request:**

```json
{
  "subject": {
    "type": "gts.x.core.security.subject.user.v1~",
    "id": "a254d252-7129-4240-bae5-847c59008fb6",
    "properties": { "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68" }
  },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.events.event.v1~" },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": {
      "tenant_scope": { "mode": "context_tenant_and_descendants" },
      "resource_scope": { "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" } }
    },
    "capabilities": { "tenant_closure": true, "resource_group_membership": false }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "schema": "urn:hyperspot:authz:constraints:v1",
  "issued_at": "2026-01-21T10:00:00Z",
  "ttl_seconds": 60,
  "alternatives": [{
    "tenant_scope": {
      "mode": "context_tenant_and_descendants",
      "context_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68"
    },
    "resource_scope": { "attributes": { "topic_id": "gts.x.core.events.topic.v1~z.app._.some_topic.v1" } }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.topic_id = '<uuid5-from-gts-topic-id>'
  AND e.owner_tenant_id IN (
      SELECT descendant_id FROM tenant_closure
      WHERE ancestor_id = '51f18034-3b2f-4bfa-bb99-22113bddee68'
  )
```

---

### Scenario 4: Tenant Subtree Access Without Closure (Fallback to IDs) — Case 3

**Prerequisites:** Domain module does **NOT** have `tenant_closure` table.

Same as Scenario 3, but PEP declares `capabilities.tenant_closure = false`.

**PEP → PDP Request:**

```json
{
  "context": {
    "capabilities": { "tenant_closure": false, "resource_group_membership": false }
  }
}
```

**PDP → PEP Response:**

PDP expands the tenant subtree into explicit IDs:

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": {
      "mode": "context_tenant_and_descendants",
      "context_tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
      "ids": [
        "51f18034-3b2f-4bfa-bb99-22113bddee68",
        "93953299-bcf0-4952-bc64-3b90880d6beb",
        "7a8b9c0d-1234-5678-9abc-def012345678"
      ]
    }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.topic_id = '<uuid5-from-gts-topic-id>'
  AND e.owner_tenant_id IN (
      '51f18034-3b2f-4bfa-bb99-22113bddee68',
      '93953299-bcf0-4952-bc64-3b90880d6beb',
      '7a8b9c0d-1234-5678-9abc-def012345678'
  )
```

**Trade-off:** Works for small hierarchies but doesn't scale. For large tenant trees, closure table sync is strongly recommended.

---

### Scenario 5: Resource Group Access (List) — Case 3

**Prerequisites:** Domain module has `resource_group_membership` table.

**Request:** `GET /events?topic={topic_id}`

Subject lists events. Policy grants access to resources in "Project Alpha" group.

**PEP → PDP Request:**

```json
{
  "subject": { "type": "...", "id": "...", "properties": { "tenant_id": "51f18034-..." } },
  "action": { "name": "list" },
  "resource": { "type": "gts.x.events.event.v1~" },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": {
      "tenant_scope": { "mode": "context_tenant_only" },
      "resource_scope": { "attributes": { "topic_id": "..." } }
    },
    "capabilities": { "tenant_closure": true, "resource_group_membership": true }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-..." },
    "group_scope": { "mode": "explicit_ids", "ids": ["d4e5f6a7-1234-5678-9abc-projectalpha1"] },
    "resource_scope": { "attributes": { "topic_id": "..." } }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.topic_id = '<uuid5>'
  AND e.owner_tenant_id = '51f18034-3b2f-4bfa-bb99-22113bddee68'
  AND e.id IN (
      SELECT resource_id FROM resource_group_membership
      WHERE group_id = 'd4e5f6a7-1234-5678-9abc-projectalpha1'
  )
```

---

### Scenario 6: Resource Group Hierarchy (List) — Case 3

**Prerequisites:** Domain module has `resource_group_closure` table.

**Resource Group Hierarchy:**
```
"Department" (aaa11111-...) ← granted access here
    ├── "Team Alpha" (bbb22222-...) → Event A
    └── "Team Beta" (ccc33333-...) → Event B
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-..." },
    "group_scope": { "mode": "descendants_of_root", "root_id": "aaa11111-1111-1111-1111-department111" }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.owner_tenant_id = '51f18034-...'
  AND e.id IN (
      SELECT m.resource_id
      FROM resource_group_membership m
      WHERE m.group_id IN (
          SELECT descendant_id FROM resource_group_closure
          WHERE ancestor_id = 'aaa11111-1111-1111-1111-department111'
      )
  )
```

---

### Scenario 7: Multiple Alternatives (OR Logic) — Case 3

**Context:** Subject has two independent access paths:
1. Group-based access via "Project Alpha"
2. Direct resource grants for specific event IDs

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [
    {
      "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-..." },
      "group_scope": { "mode": "explicit_ids", "ids": ["d4e5f6a7-...projectalpha1"] }
    },
    {
      "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-..." },
      "resource_scope": { "ids": ["ccc33333-...", "ddd44444-..."] }
    }
  ]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.owner_tenant_id = '51f18034-...'
  AND (
      -- Alternative 1: Group-based access
      e.id IN (
          SELECT resource_id FROM resource_group_membership
          WHERE group_id = 'd4e5f6a7-...projectalpha1'
      )
      OR
      -- Alternative 2: Direct resource grants
      e.id IN ('ccc33333-...', 'ddd44444-...')
  )
```

**Key points:**
- Alternatives are OR'ed
- Within each alternative, scopes are AND'ed
- Tenant constraint remains in both alternatives as defense-in-depth

---

### Scenario 8: Create in Same-Tenant — Constraints API

**Request:** `POST /events`

Subject creates a new event. The resource doesn't exist yet.

**PEP → PDP Request:**

```json
{
  "subject": { "type": "...", "id": "...", "properties": { "tenant_id": "51f18034-..." } },
  "action": { "name": "create" },
  "resource": { "type": "gts.x.events.event.v1~" },
  "context": {
    "tenant_id": "51f18034-3b2f-4bfa-bb99-22113bddee68",
    "intent": {
      "tenant_scope": { "mode": "context_tenant_only" },
      "resource_scope": { "attributes": { "topic_id": "..." } }
    }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "51f18034-..." },
    "resource_scope": { "attributes": { "topic_id": "..." } }
  }]
}
```

**PEP Enforcement:**

The PEP validates that the target `owner_tenant_id` matches the constraint before inserting:

```sql
-- PEP validates: target owner_tenant_id must equal context_tenant_id
INSERT INTO events (id, owner_tenant_id, topic_id, payload, created_at, creator_subject_id, creator_tenant_id)
VALUES (
    'new-event-uuid',
    '51f18034-...',  -- enforced by constraint
    '<uuid5>',
    '{"message": "Hello"}',
    NOW(),
    'a254d252-...',  -- subject_id (audit)
    '51f18034-...'   -- subject_tenant_id (audit)
)
```

---

### Scenario 9: Create in Child Tenant (Cross-Tenant) — Constraints API

**Request:** `POST /events` with `owner_tenant_id` pointing to a child tenant.

Subject from parent tenant creates a resource owned by a child tenant.

**Tenant Hierarchy:**
```
Subject Tenant (51f18034-...) ← parent, subject belongs here
    └── Target Tenant (93953299-...) ← resource will be owned here
```

**PEP → PDP Request:**

```json
{
  "subject": { "properties": { "tenant_id": "51f18034-..." } },
  "action": { "name": "create" },
  "resource": { "type": "gts.x.events.event.v1~" },
  "context": {
    "tenant_id": "93953299-bcf0-4952-bc64-3b90880d6beb",  // target tenant
    "intent": { "tenant_scope": { "mode": "context_tenant_only" } }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": { "mode": "context_tenant_only", "context_tenant_id": "93953299-..." }
  }]
}
```

**Generated INSERT:**

```sql
INSERT INTO events (id, owner_tenant_id, ..., creator_subject_id, creator_tenant_id)
VALUES (
    'new-event-uuid',
    '93953299-...',  -- owner: child tenant
    ...,
    'a254d252-...',  -- creator: subject from parent
    '51f18034-...'   -- creator_tenant: parent (audit trail)
)
```

---

### Scenario 10: Create Denied — Constraints API

**Request:** `POST /events` — subject has no `create` permission.

**PDP → PEP Response:**

```json
{
  "decision": "deny",
  "schema": "urn:hyperspot:authz:constraints:v1",
  "context": { "reason_admin": { "policy": "no-matching-grant" } }
}
```

**PEP Response:**

```
HTTP 403 Forbidden
{ "error": "access_denied", "message": "You do not have permission to create this resource type" }
```

---

### Scenario 11: Self-Managed Barrier — Case 3

**Tenant Hierarchy:**
```
Context Tenant (51f18034-..., managed, active)
├── Child A (93953299-..., managed, active)      ← Visible
├── Child B (7a8b9c0d-..., self-managed, active) ← BARRIER (hidden + subtree hidden)
│   └── Grandchild C (aaa11111-..., managed)     ← Hidden (behind barrier)
└── Child D (bbb22222-..., managed, suspended)   ← Hidden (status filter)
```

**PEP → PDP Request:**

```json
{
  "context": {
    "tenant_id": "51f18034-...",
    "intent": {
      "tenant_scope": {
        "mode": "context_tenant_and_descendants",
        "ignore_self_managed_barrier": false,
        "attributes": { "status": ["active"] }
      }
    }
  }
}
```

**PDP → PEP Response:**

```json
{
  "decision": "allow",
  "alternatives": [{
    "tenant_scope": {
      "mode": "context_tenant_and_descendants",
      "context_tenant_id": "51f18034-...",
      "ignore_self_managed_barrier": false,
      "attributes": { "status": ["active"] }
    }
  }]
}
```

**Generated SQL:**

```sql
SELECT e.*
FROM events e
WHERE e.owner_tenant_id IN (
    SELECT tc.descendant_id
    FROM tenant_closure tc
    JOIN tenant_projection tp ON tc.descendant_id = tp.tenant_id
    WHERE tc.ancestor_id = '51f18034-...'
      AND tc.barrier_ancestor_id IS NULL  -- self-managed barrier
      AND tp.status IN ('active')         -- status filter
)
```

**Result:** Returns events from Context Tenant and Child A only.
- Child B excluded (self-managed barrier)
- Grandchild C excluded (behind barrier)
- Child D excluded (status filter: suspended)

---

## Rationale

### Why Extend AuthZEN Rather Than Replace

1. **Standards compliance** — AuthZEN is now an approved OpenID standard (January 12, 2026)
2. **Access Evaluation** — Fully reusable for point checks (read/update/delete specific resource)
3. **Extension mechanism** — AuthZEN explicitly supports extensions via context and properties
4. **Ecosystem alignment** — Future tooling, libraries, and interop

### Why Not Use AuthZEN Resource Search Directly

1. **Scalability** — Cannot enumerate millions of resources
2. **Dynamic data** — Results change between pagination
3. **SQL enforcement** — Need predicates, not ID lists
4. **Hierarchy support** — Need first-class tenant/group tree semantics

### Why Keep Tenant/Resource Group Resolvers

Tenant and Resource Group Resolvers serve different purposes from Auth Resolver:

- **Tenant Resolver** — Metadata, hierarchy, sync events (not authorization)
- **Resource Group Resolver** — Membership, hierarchy, sync events (not authorization)
- **Auth Resolver** — Authorization decisions and constraints

Auth Resolver may call Tenant/RG Resolvers internally to build constraints, but PEPs interact with all three independently.

### Alternatives Considered

| Solution | Suitable for PDP Implementation | Suitable as PEP Contract |
|----------|--------------------------------|--------------------------|
| OPA Partial Evaluation | Yes (policy backend) | No (Rego AST too complex for PEP) |
| OpenID AuthZEN (as-is) | Partial (access checks only) | No (boolean/IDs, not constraints) |
| AWS Cedar | Yes (policy backend) | No (same as OPA PE) |
| AuthZEN + HyperSpot Extension | — | Yes (this ADR) |

---

## Consequences

### Positive

- Standards-based foundation (AuthZEN 1.0)
- Clear separation: AuthZEN for point checks, extension for query enforcement
- Vendor-neutral: no assumption about policy model (RBAC/ABAC/ReBAC)
- SQL-first: constraints designed for efficient database enforcement
- Fail-closed: structural guarantees against authorization bypass

### Negative

- Non-standard extension requires documentation and tooling
- Two API endpoints instead of one (evaluation vs constraints)
- PDP complexity: must understand both AuthZEN and HyperSpot extensions

### Risks

- AuthZEN may add constraint-like features in future versions (alignment opportunity or divergence risk)
- Extension may not be compatible with off-the-shelf AuthZEN PDPs

---

## Open Questions

1. **AuthZEN Search API relationship** — Should `/access/v1/constraints` be positioned as an alternative to Resource Search, or a separate concept entirely?

2. **Batch constraints** — AuthZEN supports batch evaluation. Should we support batch constraints (multiple resource types in one call)?

3. **Constraint caching** — Can constraints be cached at the PEP level beyond TTL? What invalidation signals are needed?

4. **AuthZEN context structure** — Is embedding HyperSpot-specific fields in `context` the right approach, or should we use a dedicated extension namespace?

5. **IANA registration** — Should HyperSpot register its extension parameters with the AuthZEN metadata registry?

6. **Auth Resolver Authentication** — How does Auth Resolver (PDP) authenticate the calling module (PEP)? Options include mTLS, service tokens, or network-level isolation.

---

## References

- [OpenID AuthZEN Authorization API 1.0](https://openid.net/specs/authorization-api-1_0.html) (approved 2026-01-12)
- HyperSpot GTS (Global Type System) — `modules/types-registry/`
