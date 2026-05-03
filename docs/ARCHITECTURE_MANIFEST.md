# CyberFabric Architecture Manifest

> This document describes the architectural direction and the implemented architectural foundations of the CyberFabric repository. It is intended as a readable blueprint for architects and contributors who want to understand how the platform is structured, why the main technical decisions were made, and which capabilities are already present in the codebase.
>
> Status markers use repository evidence. An item is marked `[x]` only when logic is implemented in this repository today.

## 1. Overview

CyberFabric is a set of libraries and modules for building **XaaS services**. It plays a role of **middleware** as it sits between low-level infrastructure and product-specific logic, providing reusable runtime foundations, platform system modules, and higher-level application modules that teams compose into service platforms.

This repository contains the **Rust implementation**. The CyberFabric ecosystem may include additional repositories in other languages (e.g. C#, Go) sharing the same architecture patterns, API conventions, and security model.

CyberFabric is not a standalone application. Target systems select a subset of these libraries and modules into their own service binaries. This library-oriented approach enables flexible deployments — from lightweight edge appliances to scalable cloud services — from the same codebase, choosing exactly the modules each deployment requires.

**Key Philosophy:**
- **Modular by Design**: Everything is a Module - composable, independent units with plugin patterns for pluggable workers
- **Unified transport-agnostic module interfaces**: Support for both in-process and distributed deployment
- **Secure-by-default execution**: multi-tenancy, access control, scoped data access
- **Extensible at Every Level**: [GTS](https://github.com/globaltypesystem/gts-spec)-powered extension points for custom data types, business logic, and third-party integrations
- **Typed API discipline**: generated OpenAPI and GTS specs, structured errors
- **SaaS Ready**: Multi-tenancy, granular access control, usage tracking, and tenant customization built-in
- **Cloud Operations Excellence**: Production-grade observability, database agnostic design, API best practices, and resilience patterns via ModKit
- **Quality First**: 90%+ test coverage target with unit, integration, E2E, performance, and security testing
- **Universal Deployment**: Single codebase runs on cloud, on-prem Windows/Linux workstation, or edge/mobile
- **Developer Friendly**: AI-assisted code generation, automatic OpenAPI docs, DDD-light structure, and type-safe APIs

The repository separates concerns into three layers:

- **ModKit and platform libraries** (`libs/`) establish the runtime and development model.
- **System modules** (`modules/system/`) provide control-plane and cross-cutting platform capabilities.
- **Business and application modules** (`modules/`) implement end-user-facing service logic on top of the platform foundation.

## 2. Non-goals

1. CyberFabric doesn't optimize for **minimalism** or the lowest barrier to entry

CyberFabric does not aim to be the simplest or smallest framework for building SaaS or AI applications. It intentionally prioritizes explicit structure, governance, composability, and long-term evolvability over quick-start simplicity or minimal configuration.

2. CyberFabric doesn't provide a **rich catalog of end-user services** out of the box

CyberFabric does not aim to ship a comprehensive set of ready-made, end-user SaaS services (e.g. CRM, ticketing, billing products) as part of its core. Its primary focus is the foundational layer — runtime, control plane, GenAI capabilities, workflows, and extensibility — on top of which vendors and product teams build their own complete SaaS offerings.

3. CyberFabric doesn't attempt to replace **cloud infrastructure or PaaS layers**

CyberFabric is not a replacement for cloud providers or infrastructure platforms such as AWS, Azure, GCP, or on-prem orchestration stacks. It does not offer physical infrastructure, networking, container orchestration, or low-level resource scheduling. Instead, CyberFabric intentionally positions itself above IaaS/PaaS and below vendor-developed SaaS, focusing on application-level services, governance, and GenAI enablement.

## 3. Architectural principles

These patterns describe the repository's architectural direction. Most are enforced in the codebase today; forward-looking items explicitly state that status.

### 3.1. Cross-cutting concerns are explicit modules

Authorization, authentication, tenancy, ingress, outbound traffic, type registries, and runtime orchestration are each implemented as a regular module with its own SDK, lifecycle, and API surface — not hidden inside the framework.

**How.** Each concern publishes a public interface in an SDK package. The implementation registers itself in `ClientHub` (the typed service locator). Consumers resolve the interface at runtime and never depend on the implementation package. Example: `authz-resolver-sdk` defines the authorization interface; consumer modules resolve it from `ClientHub` without importing the resolver's internals. The same pattern applies to `authn-resolver`, `tenant-resolver`, `types-registry`, `oagw`, etc.

**Why.** Concerns can be swapped, tested in isolation, or deployed out-of-process without touching callers. The runtime assembles only the modules a given deployment needs.

### 3.2. Contracts are separated from implementation

Every module's public API lives in a dedicated SDK package (`<module>-sdk/`) containing only the interface definition, transport-agnostic models, and error types. The implementation depends on the SDK, never the other way around.

**How.** The compiler enforces this boundary — implementation-private types are not in scope for consumers. REST endpoints use `OperationBuilder`, which requires each route to declare its method, path, auth posture, request/response schemas, and error types at registration time. `OpenApiRegistry` collects these declarations and generates `/openapi.json` automatically. Because metadata lives next to handler wiring (not in a separate spec file), it stays in sync with the code by construction.

**Why.** Refactoring module internals is safe as long as the SDK interface stays compatible. Consumers can develop and test against SDK types alone. The OpenAPI spec is always consistent with the running code.

### 3.3. Security is structural, not opt-in

The architecture makes the insecure path harder than the secure one. Module developers get tenant-scoped, authorized database access by default.

**How.** The security data-path is a linear chain:

1. **Static checks** — Custom lints and CI workflows catch violations at build time (e.g. domain layer importing infra, raw SQL outside migrations).
2. **Authentication** — API Gateway validates tokens and injects `SecurityContext` into every request. Modules never parse tokens.
3. **Authorization** — Handlers call `PolicyEnforcer`, which queries the PDP plugin. The PDP returns a decision plus row-level constraints, compiled into an `AccessScope`.
4. **Database scoping** — Modules access the database through `SecureConn`, which applies `AccessScope` as automatic WHERE clauses to implement tenant-level isolation and ABAC. Raw connections are not exposed.
5. **Credentials storage** — all the credentials are stored in dedicated `credstore` module.
6. **Outbound traffic** — External HTTP goes through `oagw`, which centralizes credential injection and egress policy.

**Why.** There is no "unscoped" shortcut to accidentally use.

### 3.4. Extensibility through addition, not modification

New implementations and data types are added without changing existing modules.

**How.** Two mechanisms:
- **Plugin pattern** — A host module defines a plugin interface in its SDK and registers the plugin schema in the type registry. Plugin modules implement the interface and register as scoped clients in `ClientHub`, keyed by GTS instance ID. The host discovers plugins at runtime and routes to the selected one via a `vendor` config field. Adding a new backend (e.g. a custom auth provider) means writing a new plugin — the host and all its consumers stay unchanged.
- **GTS type extensibility** — The [Global Type System](https://github.com/GlobalTypeSystem/gts-spec) provides versioned, schema-validated type definitions. New data types (event formats, document schemas, serverless workflows and functions, permissions, license types, custom attributes) can appear in the system without modifying existing endpoints or storage. In Rust, CyberFabric derives GTS definitions directly from source code types and then registers the resulting JSON Schemas in the Types Registry. That means event schemas, plugin contracts, and other typed contracts can be generated from Rust code in the same way OpenAPI is generated from route declarations, instead of being maintained as hand-written side artifacts.

**Why.** Third-party integrations are isolated with no dependency on platform internals beyond the SDK. Incompatible schema changes are caught at registration time. Host module tests remain stable when plugins are added or removed.

### 3.5. Module contracts are independent of deployment topology

A module's interface, models, and errors look the same whether it runs in-process or as a separate service over REST API/gRPC.

**How.** For in-process execution, the module registers a local adapter in `ClientHub`. For out-of-process execution, a REST API/gRPC client implementing the same SDK interface is registered instead. Consumers resolve the interface the same way in both cases. A YAML config field (`runtime.type: local | oop`) switches between modes — no code changes required.

**Why.** Teams start with single-process composition for simplicity and move modules to separate processes when needed, without changing calling code. Integration tests use in-process mode for speed; production can use process isolation where required.

### 3.6. Unified API style is part of the architecture, not per-module taste

CyberFabric does not treat HTTP shape, query conventions, and API description as local stylistic choices. Modules follow one API style built around versioned paths, typed route registration, shared middleware, OpenAPI generation, and standard query patterns such as OData for filtering and ordering.

**How.** `OperationBuilder` is the authoritative route-registration mechanism in ModKit. A route declares method, versioned path, auth posture, license posture, request schema, response schema, tags, summary, and registered error responses in one place. `OpenApiRegistry` collects these declarations into the generated `/openapi.json`. For query shape, ModKit exposes OData helpers such as `with_odata_filter`, `with_odata_orderby`, and `with_odata_select`, and workspace Dylints enforce that REST endpoints use the standardized extension methods rather than ad-hoc query conventions.

This produces one recognizable API dialect across modules:

- Versioned endpoints such as `/resource-group/v1/groups`
- Uniform OpenAPI publication through the gateway
- Shared pagination/filter/order conventions
- OData-style filtering for collection resources where applicable
- Consistent auth, rate-limit, timeout, and observability behavior at the gateway
- [ ] Rate limiting — implemented in OAGW domain layer; API Gateway integration pending.
- [~] License posture declaration — OperationBuilder declaration and base-license gate implemented; per-feature entitlement validation against license resolver pending.

**Why.** Consumers, SDK authors, tests, docs, and gateway behavior all stay predictable. A module does not invent its own filtering language, pagination rules, or error envelope, so cross-module tooling and client generation remain feasible.

### 3.7. Failure semantics use unified canonical errors

CyberFabric is converging on one platform-wide error vocabulary instead of each module inventing its own transport-level failure categories. The canonical error model aligns with the 16 standard gRPC categories and maps them into REST via RFC-9457 `Problem` documents while preserving machine-readable type identity through GTS.

**Status.** Foundation implemented; repository-wide migration still in progress.

**How.** `libs/modkit-canonical-errors/` defines `CanonicalError` with the following 16 categories: `Cancelled`, `Unknown`, `InvalidArgument`, `DeadlineExceeded`, `NotFound`, `AlreadyExists`, `PermissionDenied`, `ResourceExhausted`, `FailedPrecondition`, `Aborted`, `OutOfRange`, `Unimplemented`, `Internal`, `ServiceUnavailable`, `DataLoss`, and `Unauthenticated`. Each category has typed context, an HTTP mapping, and a stable GTS type identifier. The canonical error stack then renders these errors as RFC-9457 `Problem` responses for HTTP while keeping the underlying category model suitable for future gRPC and internal SDK alignment.

This gives the platform one error taxonomy across:

- REST wire responses
- module and SDK boundaries
- future gRPC transport
- observability and retry classification
- schema registration and contract validation

**Why.** Standardized error categories reduce drift, make retry behavior machine-readable, and keep API, SDK, and transport layers aligned. The same architectural decision also enables generated documentation and stronger static enforcement of allowed error patterns.

### 3.8. Dylint turns architecture into build-time policy

CyberFabric treats custom static analysis as a core architectural mechanism, not a best-effort coding aid. Architectural boundaries, API conventions, GTS usage rules, and security restrictions are enforced during builds through repository-specific Dylint rules.

**How.** The workspace includes `tools/dylint_lints/`, a dedicated Dylint suite that checks contract-layer purity, DTO placement and schema derives, domain-layer isolation, direct SQL restrictions, versioned REST paths, mandatory `OperationBuilder` metadata, OData extension usage, GTS identifier correctness, and other cross-cutting rules. These lints run alongside the normal Rust toolchain and CI checks, which means architectural violations fail fast before review or runtime.

This is a shift-left quality mechanism: the repository pushes correctness, consistency, and architecture conformance into compile-time and CI-time validation rather than relying only on code review.

**Why.** In a large modular platform, architecture decays quickly if it lives only in markdown. Dylint makes the desired structure executable and keeps both human contributors and AI-assisted changes inside the intended design envelope.

### 3.9. Runtime composition is declarative and discovered

CyberFabric composes systems by declaration and discovery rather than by hand-written assembly code. Modules declare capabilities and dependencies; the runtime discovers them, builds a dependency-ordered registry, and wires the system from those declarations.

**How.** ModKit uses the `inventory` crate to collect module registrators across the workspace and feed them into `ModuleRegistry::discover_and_build()`. The resulting registry is topologically sorted from declared dependencies before the host runtime starts executing phases. This means a module contributes its capabilities once, in its own crate, and then becomes available to any host binary without bespoke composition glue.

**Why.** This keeps composition scalable as the repository grows. Adding a module does not require editing a central switchboard, and dependency ordering becomes a platform guarantee rather than an application-specific convention.

### 3.10. Lifecycle orchestration is part of the architecture

CyberFabric modules do not invent their own startup and shutdown semantics. The platform defines an explicit lifecycle with ordered phases, barrier points, and dependency-aware teardown, and modules integrate into that lifecycle through capabilities.

**How.** `HostRuntime` runs a shared sequence of phases including `pre_init`, DB migration, `init`, `post_init`, REST wiring, gRPC wiring, start/stop, and OoP orchestration. System modules run first where required, `post_init` is a barrier phase that begins only after all `init` hooks complete, and shutdown runs in reverse dependency order with a platform deadline for graceful stop. Cancellation tokens propagate through the runtime so background work cooperates with shutdown rather than outliving the host.

**Why.** This gives all modules one predictable operational model. Contributors can rely on stable ordering guarantees, shared cancellation semantics, and consistent startup/shutdown behavior instead of encoding lifecycle assumptions ad hoc in each module.

### 3.11. Modules own schema; the runtime owns privileged persistence operations

Persistence follows the same separation-of-concerns model as APIs and security: a module owns its schema and migrations, but the runtime owns privileged execution of those migrations and withholds raw privileged DB access from module code.

**How.** Modules expose migrations through `DatabaseCapability::migrations()`. During the DB phase, `HostRuntime` resolves the underlying database handle from the runtime-managed `DbManager`, collects migrations from each module, and executes them through the migration runner. Module code typically works with `DBProvider`, `SecureConn`, or higher-level repository abstractions rather than direct privileged connections, while migration history is tracked per module.

**Why.** Schema evolution remains modular and local to the owning module, but privilege stays centralized in the runtime. That reduces the chance of accidental cross-module interference and keeps persistence governance aligned with the repository's secure-by-default posture.

### 3.12. Domain logic stays pure; adapters carry transport and infrastructure concerns

CyberFabric follows a DDD-light structure in which domain logic is kept free from transport and infrastructure details, while REST/gRPC adapters and infra layers handle boundary-specific concerns.

**How.** The standard module layout separates SDK contracts, module bootstrap, domain logic, API adapters, and infrastructure. Domain types and services live under `domain/`, REST DTOs and route wiring stay in API-facing layers, and persistence/integration logic stays in infra. This boundary is reinforced not only by structure but also by custom Dylints and the `#[domain_model]` macro requirement for domain-layer types.

**Why.** Business logic stays easier to test, reuse, and evolve because it is not entangled with HTTP, database, or framework details. At the same time, adapter code remains explicit about where transport translation and persistence concerns begin.

### 3.13. Clustered coordination is the next platform primitive

The next major architectural addition is a unified cluster coordination capability for distributed CyberFabric deployments. The intent is to make cross-instance coordination a first-class platform concern rather than something each module reinvents with ad-hoc locks, local registries, or deployment-specific glue.

**How.** The cluster module is intended to provide four platform-level primitives behind stable contracts: distributed cache, leader election, distributed locks, and service discovery. Consumer modules will declare what they need and the platform will resolve those primitives against operator-selected backends. The design direction already visible in repository docs is that backends may vary by primitive, capability requirements will be validated at startup, cache-backed defaults will exist for other primitives, and watch/lifecycle semantics will be standardized across the coordination surface.

**Why.** Existing modules already show the need for shared coordination patterns such as node discovery, leader-elected background work, distributed rate limiting, and backend-dependent service location. Elevating these into one platform primitive keeps module contracts stable across deployment shapes and prevents each module from inventing incompatible coordination behavior.

## 4. Why Rust

Rust is a strong fit for CyberFabric core modules because this repository is building a platform layer for long-lived XaaS systems, where concurrency, correctness, and maintainability matter more than short-term implementation speed alone.

- **Compile-time safety**
  - Rust eliminates broad classes of memory and concurrency failures before runtime.

- **Refactoring confidence**
  - Strong typing and compiler diagnostics make large architectural changes safer, especially when contracts span multiple crates.

- **Good fit for reusable platform code**
  - Libraries such as ModKit, security layers, transport layers, and registries benefit from predictable performance and explicit interfaces.

- **Static analysis as part of architecture**
  - Rust's ecosystem, combined with Clippy and custom Dylints, allows many project rules to become enforceable at build time.

- **Operational efficiency**
  - A low-footprint runtime makes it practical to run realistic local/edge systems, end-to-end tests, and service combinations without depending on heavyweight environments.

## 5. Why a monorepo

The monorepo model is a natural fit because CyberFabric is a co-evolving platform rather than a loose collection of unrelated packages.

- **Atomic contract evolution**
  - Core contracts and all consumers can be updated together.

- **Shared quality gates**
  - Lints, CI checks, testing flows, and security scanning stay consistent across the entire platform.

- **Integrated architecture validation**
  - Requirements, design, code, tests and examples can be validated in one workspace.

- **Better support for generated and assisted development**
  - A single repository context improves the feedback loop for architectural changes that cut across many crates.

## 6. Repository structure

The repository has three main architectural strata:

- **`libs/` — ModKit and platform libraries**
  - This is the foundation layer.
  - It contains `modkit` and companion libraries for security, DB access, OData, gRPC transport, HTTP, canonical errors, node info, macros, and shared utilities.
  - This layer defines the engineering rules that higher layers reuse.

- **`modules/system/` — system modules**
  - This is the platform control plane and shared infrastructure layer.
  - It includes API ingress, module orchestration, authn/authz, tenancy, resource groups, types registry, nodes registry, outbound API gateway, and related cross-cutting services.
  - These modules carry much of the architectural weight of the repository because they establish the runtime model and platform guarantees.

- **`modules/` — business and application modules**
  - Outside `modules/system/`, the broader `modules/` tree contains the end-user-facing service modules that deliver actual product functionality.
  - This layer is expected to grow to dozens of modules covering areas such as GenAI, serverless, business logic, and core functionality (see [MODULES.md](MODULES.md) for the full inventory and roadmap).
  - All business modules follow the same ModKit patterns, SDK conventions, and security model established by the platform layers.

Additional assembly lives in `apps/`, where executable applications compose modules into examples of runnable systems.

## 7. ModKit

ModKit is the central framework of this repository. It turns the module architecture into a reusable runtime discipline.

### 7.1 Modkit capabilities

The `cf-modkit` crate and adjacent libraries provide the common substrate on which the rest of the repository is built.

What ModKit provides:

- [x] **Inventory-based module discovery**
  - Modules are discovered via the `inventory` crate and assembled into a registry.

- [x] **Modules lifecycle orchestration**
  - `HostRuntime` executes explicit phases such as `pre_init`, DB migrations, `init`, `post_init`, REST wiring, gRPC wiring, start/stop, and OoP orchestration.

- [x] **Type-safe in-process communication**
  - `ClientHub` registers and resolves typed module clients without leaking transport details.

- [x] **REST and OpenAPI composition**
  - `OperationBuilder` and `OpenApiRegistry` make route metadata part of the architecture rather than an afterthought.

- [x] **Module-owned database migrations executed by the runtime**
  - Modules provide migrations; the runtime executes them without handing out raw privileged DB access to modules.

- [x] **Security primitives**
  - `SecurityContext`, `AccessScope`, secure ORM patterns, and policy-enforcement integration are part of the stack.

- [x] **Observability primitives**
  - Tracing, OpenTelemetry integration, request IDs, and health endpoints are implemented in foundation and system layers.

- [x] **Transport flexibility**
  - In-process, REST, and gRPC all fit the same modular model.

### 7.2 Request Lifecycle

CyberFabric defines how every authenticated request flows through a fixed platform-owned sequence before reaching module business logic. This pattern is typical for XaaS control plane services: the platform resolves cross-cutting concerns such as authentication, authorization, and license validation up front, and provides explicit placeholders where that processing can be customized through plugins or extensions defined with GTS, for example to add a new licenseable feature or a new user role.

```mermaid
sequenceDiagram
    participant C as Client
    participant GW as API Gateway
    participant AuthN as AuthN Resolver
    participant AuthZ as AuthZ Resolver
    participant Lic as License Resolver
    participant Mod as Execution Module
    participant UC as Usage Collector
    participant DB as Database (SecureConn)
    C->>GW: HTTP Request
    GW->>AuthN: Validate token → SecurityContext
    AuthN-->>GW: SecurityContext
    GW->>AuthZ: Resolve AccessScope (PolicyEnforcer)
    AuthZ-->>GW: AccessScope + row-level constraints
    GW->>Lic: Validate license feature for route
    Lic-->>GW: Allowed / Denied
    GW->>Mod: Handler(SecurityContext, AccessScope)
    Mod->>DB: SecureConn.scope_with(AccessScope)
    DB-->>Mod: Scoped query results
    Mod->>UC: Record usage event (async)
    Mod-->>GW: Response
    GW-->>C: HTTP Response
```

> The platform owns steps 1–4. Module code receives `SecurityContext` and `AccessScope` as injected parameters; it never performs token parsing or tenant resolution directly. Usage recording (step 8) is asynchronous and non-blocking on the response path. Step 4 (license validation) is currently implemented at the base-license level; per-feature entitlement is pending.

## 8. Module model

![architecture.drawio.png](img/architecture.drawio.png)

See [MODULES.md](MODULES.md) for the full module inventory.

### 8.1. Module capabilities

In CyberFabric, a module is a logical runtime component with explicit dependencies, capabilities, API surface, and lifecycle.

- [x] Modules are registered and discovered through ModKit.
- [x] Modules can expose typed SDK APIs.
- [x] Modules can expose REST endpoints.
- [x] Modules can own database schema and migrations.
- [x] Modules can participate in background runtime lifecycle.
- [x] Modules can run in-process or out-of-process.

### 8.2. DDD-light module layout

The standard module layout follows a Domain-Driven Design (DDD-light) structure:

- SDK crate for stable contracts
- module crate for bootstrap and capability declaration
- domain layer for core logic
- API adapters for REST/gRPC boundaries
- infra layer for persistence and integration

- [x] SDK pattern is documented and used.
- [x] REST DTOs and transport concerns are separated from domain logic.
- [x] Domain-layer rules are reinforced by architectural lints.

## 9. Execution model

CyberFabric supports both in-process and out-of-process module execution. The logical module model and contracts remain the same regardless of the physical deployment boundary.

### 9.1. In-process execution

The default mode is in-process composition: modules share one runtime, communicate through typed clients, and are wired together by ModKit.

- [x] `ClientHub` implements typed in-process client resolution.
- [x] Module lifecycle and REST/gRPC assembly are handled by the shared runtime.

### 9.2. Out-of-process execution

Modules can also run as separate processes communicating via gRPC.

- [x] `HostRuntime` contains explicit OoP orchestration hooks.
- [x] `modkit-transport-grpc` exists as a transport library.
- [x] `docs/modkit_unified_system/09_oop_grpc_sdk_pattern.md` documents the pattern.
- [x] `examples/oop-modules/` demonstrates the model with calculator examples.

## 10. Security architecture

Security in CyberFabric spans the language choice, module boundaries, DB access rules, policy enforcement, and CI controls. See [docs/security/SECURITY.md](security/SECURITY.md) for the full security architecture.

Security foundations:

- [x] **Rust safety baseline**
  - Memory safety, strong typing, and strict lint posture are part of the default development model.

- [x] **Secure ORM / compile-time scoping**
  - Secure DB access and scoping rules are enforced through ModKit DB patterns and macros.

- [x] **Split AuthN/AuthZ architecture**
  - Authentication resolution and authorization resolution are modeled as separate system services.

- [x] **`SecurityContext` propagation model**
  - Request identity and scope flow through the platform as explicit data rather than thread-local magic.

- [x] **Policy-based tenant and group scoping**
  - Tenant hierarchy and resource groups act as platform inputs to authorization decisions.

- [x] **Outbound API security boundary**
  - OAGW (Outbound API Gateway) centralizes outbound HTTP policy, credential resolution, and egress hardening.

- [x] **Credential handling architecture**
  - `credstore` and secrecy-aware types are present for secret handling.

- [x] **Static and CI security gates**
  - Clippy, custom Dylints, `cargo-deny`, CodeQL, fuzzing, and related scanners are part of the repo.

## 11. API, type, and error contracts

### 11.1. REST and OpenAPI

- [x] API Gateway builds a unified HTTP surface with health endpoints, middleware, auth, request IDs, tracing, timeouts, and docs endpoints.
- [ ] Rate limiting — implemented in OAGW domain layer; API Gateway integration pending.
- [~] License posture declaration — OperationBuilder declaration and base-license gate implemented; per-feature entitlement validation against license resolver pending.
- [x] `OpenApiRegistry` and `OperationBuilder` are implemented in ModKit.
- [x] `/openapi.json` and `/docs` are served by the API gateway.
- [x] OData extensions are implemented in `OperationBuilder` for standardized `$filter`, `$select`, and `$orderby` support.
- [x] Workspace Dylints enforce versioned endpoints and standardized OData extension usage.

The important architectural point is that OpenAPI is generated from the same Rust route declarations that wire the running service. CyberFabric does not maintain a separate hand-authored HTTP contract description.

### 11.2. GTS contracts and schema generation

- [x] GTS schemas can be generated directly from Rust types
- [x] Plugin specifications already use this pattern in SDK crates such as `authn-resolver-sdk` and `mini-chat-sdk`.
- [x] Generated GTS JSON Schemas are intended for registration in the Types Registry.
- [x] GTS-specific Dylints validate identifier correctness and prevent unsupported schema-generation patterns such as `schema_for!` on GTS structs.

This is the non-HTTP counterpart to OpenAPI generation. OpenAPI describes REST endpoints; GTS-generated JSON Schema describes platform contracts and typed data beyond REST, including plugin specs, events, and other globally identified contracts. Together they let CyberFabric derive both API and non-API contracts from Rust source rather than duplicating schemas manually.

### 11.3. RFC-9457 and canonical errors

- [x] RFC-9457 `Problem` handling is implemented in the ModKit/API stack.
- [x] `libs/modkit-canonical-errors/` provides typed canonical categories.
- [x] `libs/modkit-canonical-errors-macro/` provides resource-scoped error helpers.
- [x] The canonical error foundation covers the 16 gRPC-aligned categories used as the platform-wide failure vocabulary.
- [ ] Repository-wide migration to canonical errors is not complete yet.

The canonical error model stabilizes failure semantics, improves machine-readability, reduces ad-hoc error drift, and aligns REST, future gRPC, and internal domain errors. It also connects failure semantics to GTS-backed type identity, so error categories become part of the platform contract surface rather than just human-readable messages.

## 12. Observability and operations

- [x] OpenTelemetry tracing initialization exists in `libs/modkit/src/telemetry/`.
- [x] API Gateway implements request IDs, tracing, timeout layers, body limits, and structured access logging.
- [x] API Gateway exposes `/health` and `/healthz`.
- [ ] Rate limiting — implemented in OAGW domain layer; API Gateway integration pending.
- [x] The repo contains CI workflows and test infrastructure aligned with operational quality.

## 13. Other references

1. [docs/MODULES.md](MODULES.md)
2. [docs/modkit_unified_system/README.md](modkit_unified_system/README.md)
3. [docs/security/SECURITY.md](security/SECURITY.md)
4. [docs/REPO_PLAYBOOK.md](REPO_PLAYBOOK.md)
5. [docs/TESTING.md](TESTING.md)