# Mini-Chat Module

AI chat module for Cyber Fabric. Provides a REST API for managing chats, messages (with SSE streaming), models, reactions, and attachments.

## Directory Structure

```
modules/mini-chat/
├── mini-chat/          # Main module crate
│   └── src/
│       ├── api/        # REST handlers, routes, DTOs, SSE
│       ├── domain/     # Business logic, services, repository traits
│       └── infra/      # DB entities/repos, LLM providers, model policy
├── mini-chat-sdk/      # SDK crate (contract types, plugin API, GTS IDs)
├── plugins/
│   └── static-model-policy-plugin/  # Dev plugin: static model catalog from config
├── scripts/
│   └── smoke-test-api.py            # API smoke test (stdlib-only Python)
└── docs/               # PRD, DESIGN, ADRs, OpenAPI spec
```

## Running Locally

```bash
make mini-chat
```

This starts the server at `http://127.0.0.1:8087` with SQLite, mock auth, and single-tenant mode.

### Configuration

Config file: **`config/mini-chat.yaml`**

#### Setting the LLM API key

Find the `static-credstore-plugin` section and set your key:

```yaml
static-credstore-plugin:
  config:
    secrets:
      - key: "azure-openai-key"
        value: "<YOUR_API_KEY>"
```

#### Setting the LLM provider endpoint

Find the `mini-chat` -> `config` -> `providers` section:

```yaml
mini-chat:
  config:
    providers:
      azure_openai:
        kind: openai_responses
        host: "<your-resource>.openai.azure.com"
        api_path: "/openai/v1/responses"
        auth_config:
          secret_ref: "cred://azure-openai-key"
```

## API

Base URL: `http://127.0.0.1:8087/mini-chat/v1`

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/mini-chat/v1/models` | List available models |
| GET | `/mini-chat/v1/models/{id}` | Get model details |
| POST | `/mini-chat/v1/chats` | Create a chat |
| GET | `/mini-chat/v1/chats` | List chats |
| GET | `/mini-chat/v1/chats/{id}` | Get a chat |
| PATCH | `/mini-chat/v1/chats/{id}` | Update a chat |
| DELETE | `/mini-chat/v1/chats/{id}` | Delete a chat |
| POST | `/mini-chat/v1/chats/{id}/messages/stream` | Send message (SSE) |
| GET | `/mini-chat/v1/chats/{id}/messages` | List messages |
| PUT | `/mini-chat/v1/chats/{id}/messages/{mid}/reaction` | Set reaction |
| DELETE | `/mini-chat/v1/chats/{id}/messages/{mid}/reaction` | Remove reaction |
| GET | `/mini-chat/v1/chats/{id}/turns/{rid}` | Get turn status |
| POST | `/mini-chat/v1/chats/{id}/turns/{rid}/retry` | Retry a failed turn (SSE) |
| PATCH | `/mini-chat/v1/chats/{id}/turns/{rid}` | Edit turn (SSE) |
| DELETE | `/mini-chat/v1/chats/{id}/turns/{rid}` | Delete a turn |

OpenAPI docs (when server is running): http://127.0.0.1:8087/docs

## Smoke Test

```bash
# All steps (requires a valid API key for SSE streaming)
python3 modules/mini-chat/scripts/smoke-test-api.py

# Skip SSE streaming (no real API key needed)
python3 modules/mini-chat/scripts/smoke-test-api.py --no-sse
```

## Documentation

- [PRD](docs/PRD.md)
- [Design](docs/DESIGN.md)
- [ADRs](docs/ADR/)
- [OpenAPI spec](docs/openapi.json)
