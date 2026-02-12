# AuthN Resolver

Authentication resolution for CyberFabric — validates bearer tokens and produces `SecurityContext`.

> **Architecture Reference:** This module implements the authentication patterns described in [docs/arch/authorization/DESIGN.md](../../../docs/arch/authorization/DESIGN.md).

## Overview

The **authn_resolver** module provides token-to-identity resolution:

1. **Token validation** — Verify bearer tokens and extract identity information
2. **SecurityContext creation** — Populate `subject_id`, `subject_tenant_id`, `token_scopes`

The API Gateway calls the AuthN Resolver for every authenticated route. The resolver delegates to a plugin selected via GTS vendor matching.

## Public API

The gateway registers [`AuthNResolverGatewayClient`](authn_resolver-sdk/src/api.rs) in ClientHub:

- `authenticate(bearer_token)` — Validate a bearer token and return `AuthenticationResult`

### Models

See [`models.rs`](authn_resolver-sdk/src/models.rs):

**`AuthenticationResult`** — Successful authentication output:
- `security_context` — A `SecurityContext` with identity fields populated (`subject_id`, `subject_tenant_id`, `token_scopes`, `bearer_token`)

### Errors

See [`error.rs`](authn_resolver-sdk/src/error.rs): `Unauthorized`, `NoPluginAvailable`, `ServiceUnavailable`, `Internal`

## Plugin API

Plugins implement [`AuthNResolverPluginClient`](authn_resolver-sdk/src/plugin_api.rs) and register via GTS.

CyberFabric includes one plugin out of the box:
- [`static_authn_plugin`](plugins/static_authn_plugin/) — Config-based plugin for development and testing

## Configuration

### Gateway

See [`config.rs`](authn_resolver-gw/src/config.rs)

```yaml
modules:
  authn_resolver:
    vendor: "hyperspot"  # Selects plugin by matching vendor
```

### Static Plugin

See [`config.rs`](plugins/static_authn_plugin/src/config.rs)

```yaml
modules:
  static_authn_plugin:
    vendor: "hyperspot"
    priority: 100
    mode: accept_all          # accept_all | static_tokens
    default_identity:
      subject_id: "00000000-0000-0000-0000-000000000001"
      subject_tenant_id: "00000000-0000-0000-0000-000000000099"
      token_scopes: ["*"]
    tokens:                   # Only used in static_tokens mode
      - token: "my-secret-token"
        identity:
          subject_id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
          subject_tenant_id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"
          token_scopes: ["read:data"]
```

### API Gateway Integration

The API Gateway uses the AuthN Resolver when `auth_disabled: false`:

```yaml
modules:
  api_gateway:
    auth_disabled: false             # Enable authentication
    require_auth_by_default: true    # Unannotated routes require auth
```

When `auth_disabled: true`, the gateway injects a default `SecurityContext` without calling the resolver.

## Usage

```rust
let authn = hub.get::<dyn AuthNResolverGatewayClient>()?;

match authn.authenticate("Bearer eyJhbG...").await {
    Ok(result) => {
        let ctx = result.security_context;
        println!("Subject: {}", ctx.subject_id());
        println!("Tenant: {:?}", ctx.subject_tenant_id());
        println!("Scopes: {:?}", ctx.token_scopes());
    }
    Err(AuthNResolverError::Unauthorized(msg)) => {
        println!("Invalid token: {msg}");
    }
    Err(err) => {
        println!("Service error: {err:?}");
    }
}
```

## Technical Decisions

### Gateway + Plugin Pattern

The gateway delegates to a single plugin resolved via GTS vendor matching. This allows different authentication backends (JWT, OAuth2, API keys, LDAP) to be swapped without changing consumers.

### SecurityContext Fields

Per the authorization design, `SecurityContext` contains:
- `subject_id` — The authenticated user/service identity
- `subject_type` — Optional subject classification (e.g., "user", "service")
- `subject_tenant_id` — The subject's home tenant
- `token_scopes` — Token permission scopes (`["*"]` = first-party/unrestricted)
- `bearer_token` — The raw bearer token for downstream forwarding

### Static Plugin Modes

- **`accept_all`** — Any non-empty token maps to the default identity (development convenience)
- **`static_tokens`** — Specific tokens map to specific identities (testing with multiple users)

## Implementation Phases

### Phase 1: Core (Current)

- `authenticate` API
- Static plugin with `accept_all` and `static_tokens` modes
- API Gateway integration via middleware
- ClientHub registration for in-process consumption

### Phase 2: JWT Plugin (Planned)

- JWKS-based token validation
- Issuer/audience verification
- Claims-to-SecurityContext mapping

### Phase 3: gRPC (Planned)

- gRPC API for out-of-process consumers
