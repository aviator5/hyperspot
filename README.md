# Cyber Ware
![Badge](./.github/badgeHN.svg)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/cyberfabric/cyberware-rust/badge)](https://scorecard.dev/viewer/?uri=github.com/cyberfabric/cyberware-rust)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/12050/badge)](https://www.bestpractices.dev/projects/12050)

**Cyber Ware** is a secure, modular XaaS development framework and middleware written primarily in Rust. It provides ready-to-use building blocks, domain model elements, and APIs with security-in-depth enforcement, multi-tenancy, and granular access control built into every layer.

Cyber Ware is not a ready-to-use service — it is a set of well-integrated libraries (modules) that XaaS vendors compose into their own products. Vendors decide which modules to include, how to combine them into services, and on what infrastructure to run — from edge devices to Kubernetes clusters.

**Five defining characteristics:**

1. **Secure XaaS framework with defense-in-depth** — Every API handler enforces authentication, authorization, tenant isolation, and scoped DB access by default. Security is structural, not opt-in, validated at build time using integrated dynamic lints.

2. **Three-tier module hierarchy** — *Modkit* (`libs/` — ModKit, DB access, error model, API middleware), *System modules* (`modules/system/` — API gateway, authn/authz, tenancy, resource groups, type registry), and *Service modules* (`modules/` — serverless runtime, GenAI subsystems, event system, and domain modules).

3. **Composable libraries, vendor-controlled deployment** — Each module owns its API surface and database, communicates via a Rust-native SDK that facades local vs. remote calls, and is fully infrastructure-agnostic. Vendors choose which modules to bundle and whether to deploy single-process (edge/on-prem), multi-node (bare metal), or on Kubernetes.

4. **Pre-integrated XaaS backbone** — Deep integration with multi-tenancy, licensing and quota management, usage collection, and event systems. Cyber Ware provides its own backbone modules, but each can be replaced or integrated with existing vendor infrastructure via plugins (e.g. subscription management, product catalog, provisioning, or license enforcement).

5. **Extensible domain model via Global Type System** — Modules expose extensible domain objects whose metadata and types are customizable through [GTS](https://github.com/globaltypesystem/gts-spec) — define new event types, user settings, LLM model attributes, etc. CRUD API handlers support customization via hooks and callbacks as serverless functions and workflows.

**Engineering principles:**
- **Spec-Driven Development**: [Specification templates](docs/spec-templates/README.md) (PRD, Design, ADR, Feature) define what gets built *before* code is written. Every module is well documented.
- **Shift Left**: Custom [dylint](tools/dylint_lints/) architectural lints enforce design rules at compile time, alongside Clippy, [tests](#testing), fuzzing, and security audits in CI
- **Quality First**: 90%+ test coverage target with unit, integration, E2E, performance, and security testing
- **Core in Rust**: Compile-time safety, deep static analysis including project-specific lints, so more issues are prevented before review/runtime
- **Monorepo**: Core modules and contracts in one place for atomic refactors, consistent tooling/CI, and realistic local build + E2E testing

See the full architecture [MANIFEST](docs/ARCHITECTURE_MANIFEST.md) for more details, including rationales behind Rust and Monorepo choice.

See also [REPO_PLAYBOOK](docs/REPO_PLAYBOOK.md) with the registry of repository-wide artifacts (guidelines, rules, conventions, etc).

## Quick Start

### Prerequisites

- Rust stable with Cargo ([Install via rustup](https://rustup.rs/))
- Protocol Buffers compiler (`protoc`):
  - macOS: `brew install protobuf`
  - Linux: `apt-get install protobuf-compiler`
  - Windows: Download from https://github.com/protocolbuffers/protobuf/releases
- MariaDB/PostgreSQL/SQLite or in-memory database

### CI/Development Commands

```bash
# Clone the repository
git clone --recurse-submodules <repository-url>
cd cyberware-rust

make ci         # Run full CI pipeline
make fmt        # Check formatting (no changes). Use 'make dev-fmt' to auto-format
make clippy     # Lint (deny warnings). Use 'make dev-clippy' to attempt auto-fix
make test       # Run tests
make example    # Run modkit example module
make check      # Full check suite
make safety     # Extended safety checks (includes dylint/kani)
make deny       # License and dependency checks
```

### Running the Server

```bash
# Quick helper
make quickstart

# Option 1: Run with SQLite database (recommended for development)
cargo run --bin cyberware-example-server -- --config config/quickstart.yaml run

# Option 2: Run without database (no-db mode)
cargo run --bin cyberware-example-server -- --config config/no-db.yaml run

# Option 3: Run with mock in-memory database for testing
cargo run --bin cyberware-example-server -- --config config/quickstart.yaml --mock run

# Check if server is ready (detailed JSON response)
curl http://127.0.0.1:8087/health

# Kubernetes-style liveness probe (simple "ok" response)
curl http://127.0.0.1:8087/healthz

# See API documentation:
# $ make quickstart
# visit: http://127.0.0.1:8087/docs
```

### Example Configuration (config/quickstart.yaml)

```yaml
# Cyber Ware Configuration

# Core server configuration (global section)
server:
  home_dir: "~/.cyberware

# Database configuration (global section)
database:
  url: "sqlite://database/database.db"
  max_conns: 10
  busy_timeout_ms: 5000

# Logging configuration (global section)
logging:
  default:
    console_level: info
    file: "logs/cyberware.og"
    file_level: warn
    max_age_days: 28
    max_backups: 3
    max_size_mb: 1000

# Per-module configurations moved under modules section
modules:
  api_gateway:
    bind_addr: "127.0.0.1:8087"
    enable_docs: true
    cors_enabled: false
```

### Creating Your First Module

See [MODKIT UNIFIED SYSTEM](docs/modkit_unified_system/README.md) and [MODKIT_PLUGINS.md](docs/MODKIT_PLUGINS.md) for details.

## Documentation

- **[Architecture manifest](docs/ARCHITECTURE_MANIFEST.md)** - High-level overview of the architecture
- **[Modules](docs/MODULES.md)** - List of all modules and their roles
- **[MODKIT UNIFIED SYSTEM](docs/modkit_unified_system/README.md) and [MODKIT_PLUGINS.md](docs/MODKIT_PLUGINS.md)** - how to add new modules.
- **[Contributing](CONTRIBUTING.md)** - Development workflow and coding standards

## Security

Cyber Ware applies defense-in-depth security across the entire development lifecycle — from Rust's compile-time safety guarantees and custom architectural lints, through compile-time tenant isolation and PDP/PEP authorization enforcement, to continuous fuzzing, dependency auditing, and automated security scanning in CI.

See **[Security Overview](docs/security/SECURITY.md)** for the full breakdown, including: Secure ORM with compile-time tenant scoping, authentication/authorization architecture (NIST SP 800-162 PDP/PEP model), 90+ Clippy deny-level rules, custom dylint architectural lints, cargo-deny advisory checks, ClusterFuzzLite continuous fuzzing, CodeQL/Scorecard/Snyk/Aikido scanners, and AI-powered PR review bots.

## Specification Templates

Cyber Ware uses industry-standard specification templates (IEEE, ISO, MADR) to drive development. Specs are written *before* implementation and live alongside the code in version control.

- **[Overview & Guide](docs/spec-templates/README.md)** — Template system overview, governance, FDD ID conventions, and document placement rules
- **[PRD.md](docs/spec-templates/PRD.md)** — Product Requirements Document: vision, actors, capabilities, use cases, FR/NFR
- **[DESIGN.md](docs/spec-templates/DESIGN.md)** — Technical Design: architecture, principles, constraints, domain model, API contracts
- **[ADR.md](docs/spec-templates/ADR.md)** — Architecture Decision Record: decisions, options, trade-offs, consequences
- **[FEATURE.md](docs/spec-templates/FEATURE.md)** — Feature Specification: flows, algorithms, states, requirements
- **[UPSTREAM_REQS.md](docs/spec-templates/UPSTREAM_REQS.md)** — Upstream Requirements: technical requirements from other modules to this module

## Configuration

### YAML Configuration Structure

```yaml
# config/server.yaml

# Global server configuration
server:
  home_dir: "~/.cyberware"

# Database configuration
database:
  servers:
    sqlite_users:
      params:
        WAL: "true"
        synchronous: "NORMAL"
        busy_timeout: "5000"
      pool:
        max_conns: 5
        acquire_timeout: "30s"

# Logging configuration
logging:
  default:
    console_level: info
    file: "logs/cyberware.og"
    file_level: warn
    max_age_days: 28
    max_backups: 3
    max_size_mb: 1000

# Per-module configuration
modules:
  api_gateway:
    config:
      bind_addr: "127.0.0.1:8087"
      enable_docs: true
      cors_enabled: true
  users_info:
    database:
      server: "sqlite_users"
      file: "users_info.db"
    config:
      default_page_size: 5
      max_page_size: 100
```

### Environment Variable Overrides

Configuration supports environment variable overrides with `CYBERFABRIC_` prefix:

```bash
export CYBERFABRIC_DATABASE_URL="postgres://user:pass@localhost/db"
export CYBERFABRIC_MODULES_api_gateway_BIND_ADDR="0.0.0.0:8080"
export CYBERFABRIC_LOGGING_DEFAULT_CONSOLE_LEVEL="debug"
```

## Testing

```bash
make check           # full quality gate (fmt + clippy + test + security)
```

Other tests:

```bash
make test            # unit tests (workspace)
make test-sqlite     # integration tests (SQLite, no external DB required)
make e2e-local       # end-to-end tests (builds + starts server automatically)
make e2e-docker      # end-to-end tests (builds + starts server in Docker)
make coverage-unit   # unit test code coverage
make fuzz            # fuzz smoke tests (30 s per target)
```

On **Windows** (no `make`), use the cross-platform CI script directly:

```bash
python tools/scripts/ci.py check          # full CI suite
python tools/scripts/ci.py e2e-local      # end-to-end tests
python tools/scripts/ci.py fuzz --seconds 60  # fuzz smoke run
```

For the complete test strategy, coverage policy, CI pipeline details, and all
available commands see **[docs/TESTING.md](docs/TESTING.md)**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## License

This project is licensed under the Apache 2.0 License - see the [LICENSE](LICENSE) file for details.
