# PRD — LLM Gateway


<!-- toc -->

- [1. Overview](#1-overview)
  - [1.1 Purpose](#11-purpose)
  - [1.2 Background / Problem Statement](#12-background--problem-statement)
  - [1.3 Goals (Business Outcomes)](#13-goals-business-outcomes)
  - [1.4 Glossary](#14-glossary)
- [2. Actors](#2-actors)
  - [2.1 Human Actors](#21-human-actors)
  - [2.2 System Actors](#22-system-actors)
- [3. Operational Concept & Environment](#3-operational-concept--environment)
- [4. Scope](#4-scope)
  - [4.1 In Scope](#41-in-scope)
  - [4.2 Out of Scope](#42-out-of-scope)
- [5. Functional Requirements](#5-functional-requirements)
  - [P1 — Core](#p1--core)
  - [P2 — Reliability & Governance](#p2--reliability--governance)
  - [P3 — Optimization](#p3--optimization)
  - [P4 — Enterprise](#p4--enterprise)
- [6. Non-Functional Requirements](#6-non-functional-requirements)
  - [Scalability](#scalability)
- [7. Public Library Interfaces](#7-public-library-interfaces)
- [8. Use Cases](#8-use-cases)
- [9. Acceptance Criteria](#9-acceptance-criteria)
- [10. Dependencies](#10-dependencies)
- [11. Assumptions](#11-assumptions)
- [12. Risks](#12-risks)

<!-- /toc -->

## 1. Overview

### 1.1 Purpose

LLM Gateway provides unified access to multiple LLM providers. Consumers interact with a single interface regardless of underlying provider. Gateway normalizes requests and responses but does not execute tools or interpret content — this is consumer responsibility.

LLM Gateway is the central integration point between platform consumers and external AI providers. It abstracts provider differences — request formats, authentication, error handling, rate limits — behind a unified API. Consumers send requests in a normalized format; Gateway translates them to provider-specific calls and normalizes responses back.

The Gateway supports diverse modalities: text generation, embeddings, vision, audio, video, and document processing. It handles both synchronous and asynchronous operations, including streaming responses and long-running jobs. All interactions go through the Outbound API Gateway for reliability and credential management.

Gateway is stateless for request processing — it does not store conversation history or execute tools, and consumers provide full context with each request. Gateway persists async and batch job state (internal-to-provider job ID mappings, job results) to support long-running operations that can take up to 24 hours. Gateway guarantees at-least-once delivery of usage records to the Usage Tracker module.

**Target Users**:
- **Platform Developers** — build AI-powered features using Gateway API
- **External API Consumers** — third-party developers accessing AI capabilities via public API

### 1.2 Background / Problem Statement

Platform consumers need access to AI capabilities from multiple LLM providers — OpenAI, Anthropic, Google, and others. Each provider has its own API format, authentication mechanism, error semantics, and rate limiting behavior. Without a unified abstraction, every consumer must implement provider-specific integration logic, handle failover independently, and manage credentials directly. This fragments the codebase, increases the surface area for security issues, and makes it difficult to enforce consistent governance policies across the platform.

The current state requires each consuming module to negotiate provider differences at the application layer: translating request formats, normalizing error responses, and implementing retry logic per provider. Usage tracking and budget enforcement are ad-hoc or absent, making it impossible to provide tenant-level cost visibility or enforce spending limits. Content moderation and PII filtering require each consumer to integrate interception logic independently, with no guarantee of consistent policy enforcement.

LLM Gateway addresses these problems by providing a single integration point that abstracts provider differences behind a unified API. It centralizes governance — budget enforcement, rate limiting, usage tracking, and audit logging — at the tenant level. Pre-call and post-response interceptors enable consistent content moderation and PII filtering policies without requiring consumer-side implementation.

### 1.3 Goals (Business Outcomes)

**Success Criteria** (required at GA, baseline: new module — no prior data):
- Gateway overhead < 50ms P99 at up to 1 000 concurrent requests (excluding provider latency)
- Availability ≥ 99.9% measured monthly (Gateway infrastructure only — excludes provider outages; fallback-active mode counts as available)
- Expected steady-state throughput: up to 500 requests/second; peak: up to 2 000 requests/second
- Maintenance windows follow platform-defined schedule; Gateway supports rolling restarts with zero-downtime deployments

**Post-GA Growth Targets** (6 months after GA):
- Steady-state throughput: up to 1 000 requests/second; peak: up to 5 000 requests/second
- Maintain < 50ms P99 overhead at increased load
- Support up to 50 000 async/batch jobs/day per tenant

**Capacity Planning Inputs**:
- Expected async/batch job volume: up to 10 000 jobs/day per tenant at steady state
- DB row growth: up to 50 000 rows/day across all tenants (job records + usage delivery); bounded by retention cleanup
- Storage per async result: up to 1 MB (text responses); up to 100 MB (media generation results stored in FileStorage, not DB)

**Capabilities**:
- Text generation (chat completion)
- Multimodal input/output (images, audio, video, documents)
- Embeddings generation
- Tool/function calling
- Structured output with schema validation

### 1.4 Glossary

| Term | Definition |
|------|------------|
| OAGW | Outbound API Gateway — handles external API calls, credential injection, circuit breaking |
| TTFT | Time-to-first-token — latency until first response chunk |
| GTS | Generic Type System — JSON Schema-based type definitions |
| FileStorage | Platform module for storing and retrieving binary files (images, audio, video, documents) |
| Model Registry | Platform module that resolves model identifiers to provider and endpoint information per tenant |

## 2. Actors

### 2.1 Human Actors

No dedicated human actors. All interactions with LLM Gateway are mediated through the Consumer system actor (platform modules, external API clients).

### 2.2 System Actors

#### Consumer

**ID**: `cpt-cf-llm-gateway-actor-consumer`

- **Role**: Sends requests to the Gateway.
- **Needs**: Provider-agnostic API for all AI modalities; normalized responses; async job management; usage visibility.
- **Direction**: inbound
- **Data exchanged**: Chat completion requests (messages, model, parameters), tool definitions, async job commands; receives normalized responses, streaming chunks, job status/results.
- **Availability**: Consumer availability is not a Gateway concern — Gateway responds to incoming requests as they arrive.

#### Provider

**ID**: `cpt-cf-llm-gateway-actor-provider`

- **Role**: External AI service that processes requests. Accessed via Outbound API Gateway.
- **Direction**: outbound
- **Data exchanged**: Provider-formatted requests (prompts, parameters, tool schemas); receives completions, embeddings, generated media, tool calls, usage metrics.
- **Availability**: Provider unavailability triggers fallback (if configured) or returns a provider-unavailable error to the consumer. OAGW circuit breaking protects against cascading failures.

#### Hook Plugin

**ID**: `cpt-cf-llm-gateway-actor-hook-plugin`

- **Role**: Pre-call and post-response interception (moderation, PII, transformation).
- **Direction**: bidirectional
- **Data exchanged**: Outbound: request/response payloads for inspection; inbound: allow/block/modify decisions with optionally transformed payloads.
- **Availability**: If Hook Plugin is unavailable, Gateway fails the request with a hook-unavailable error. Gateway does not bypass hooks silently — the tenant's security policy must be enforced.

#### Usage Tracker

**ID**: `cpt-cf-llm-gateway-actor-usage-tracker`

- **Role**: Budget checks and usage reporting.
- **Direction**: outbound
- **Data exchanged**: Usage records (tokens, cost estimate, latency, attribution: tenant, user, model); budget check requests before execution; receives budget status.
- **Availability**: If Usage Tracker is unavailable for budget checks, Gateway rejects the request with a budget-check-unavailable error. Usage record delivery is guaranteed at-least-once — records are queued and retried until delivered.

#### Audit Module

**ID**: `cpt-cf-llm-gateway-actor-audit-module`

- **Role**: Compliance event logging.
- **Direction**: outbound
- **Data exchanged**: Audit events (request started, completed, failed, blocked, fallback triggered) with tenant/user/model attribution.
- **Availability**: Audit Module unavailability does not block request processing. Audit events are delivered best-effort — Gateway logs a warning if delivery fails but does not reject the consumer request.

#### Model Registry

**ID**: `cpt-cf-llm-gateway-actor-model-registry`

- **Role**: Resolves model identifiers to provider and endpoint information per tenant. Gateway queries Model Registry on every request to determine which provider and endpoint to use.
- **Direction**: outbound
- **Data exchanged**: Model resolution requests (model identifier, tenant context); receives provider details (endpoint URL, provider type, capabilities, configuration).
- **Availability**: If Model Registry is unavailable, Gateway returns a model-resolution-unavailable error. Gateway cannot route requests without model resolution.

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- Requires persistent storage for async/batch job state and guaranteed usage record delivery
- All external provider calls route through Outbound API Gateway (OAGW) — Gateway never calls providers directly
- Media files (images, audio, video, documents) are stored and retrieved via FileStorage module

## 4. Scope

### 4.1 In Scope

- Unified LLM API
- Provider abstraction via adapters (OpenAI, Anthropic, Google, and others)
- Multimodal support: text, images, audio, video, documents
- Synchronous and streaming request/response
- Async job execution with durably persisted job state and results
- Batch processing with durably persisted job ID mappings
- Tool/function calling pass-through (consumer executes tools)
- Structured output with schema validation
- Pre-call and post-response hook plugin interception
- Usage tracking with guaranteed at-least-once delivery
- Provider fallback and timeout enforcement
- Per-tenant budget enforcement and rate limiting

### 4.2 Out of Scope

- Conversation state storage — consumers provide full context per request
- Tool execution — Gateway returns tool calls for consumer to execute
- Prompt engineering or prompt optimization
- Model training, fine-tuning, or model hosting
- Direct provider access bypassing OAGW
- Content caching or response deduplication

## 5. Functional Requirements

### P1 — Core

#### Chat Completion

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-chat-completion-v1`

The system **MUST** accept a chat completion request with messages and model identifier, route it to the resolved provider, and return a normalized response with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Streaming Chat Completion

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-streaming-v1`

The system **MUST** support streaming mode for chat completions, delivering response chunks in a normalized event format as they arrive from the provider.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Embeddings Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-embeddings-v1`

The system **MUST** accept one or more text inputs and return vector embeddings in a normalized format with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Vision (Image Analysis)

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-vision-v1`

The system **MUST** accept messages with image references (FileStorage URLs or external URLs), resolve the media, route to a vision-capable model, and return the analysis with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Image Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-image-generation-v1`

The system **MUST** accept a text prompt, generate an image via the resolved provider, store the result, and return a retrievable URL with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Speech-to-Text

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-speech-to-text-v1`

The system **MUST** accept messages with an audio reference (FileStorage URL or external URL), resolve the media, route to an STT-capable model, and return the transcription with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Text-to-Speech

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-text-to-speech-v1`

The system **MUST** accept a text input, synthesize audio via the resolved provider, store the result, and return a retrievable URL with usage metrics. The system **MUST** support streaming mode for audio delivery.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Video Understanding

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-video-understanding-v1`

The system **MUST** accept messages with a video reference (FileStorage URL or external URL), resolve the media, route to a video-capable model, and return the analysis with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Video Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-video-generation-v1`

The system **MUST** accept a text prompt, generate video via the resolved provider, store the result, and return a retrievable URL with usage metrics. Video generation typically requires async mode due to long processing times.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Tool/Function Calling

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-tool-calling-v1`

The system **MUST** accept requests with tool definitions (reference, inline GTS, or unified format), resolve schema references, and forward to the provider. The system **MUST** return tool calls in a unified format for the consumer to execute. The system **MUST NOT** execute tools — this is consumer responsibility.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Structured Output

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-structured-output-v1`

The system **MUST** accept a JSON schema with the request and validate the provider response against it, returning either the validated response or a validation error with details.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Document Understanding

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-document-understanding-v1`

The system **MUST** accept messages with a document URL, resolve the document, route to a capable model, and return the analysis with usage metrics.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Async Jobs

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-async-jobs-v1`

The system **MUST** support async execution for long-running operations, returning a job ID that the consumer polls for results. The system **MUST** durably persist job state and store job results until the retention period expires.

The system **MUST** provide a uniform async experience regardless of whether the provider supports async natively:
- Consumer always receives a job ID and polls for results
- The system **MUST** guarantee result availability for the retention period after completion

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Realtime Audio

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-realtime-audio-v1`

The system **MUST** support bidirectional audio streaming via a persistent connection for real-time voice conversations, with usage reported on session close.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Usage Tracking

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-fr-usage-tracking-v1`

The system **MUST** report usage after each request via Usage Tracker: tokens, cost estimate, latency, attribution (tenant, user, conversation, model). The system **MUST** guarantee at-least-once delivery of usage records.

Cross-cutting concern — applies to all operations, no dedicated UC.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-usage-tracker`

### P2 — Reliability & Governance

#### Provider Fallback

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-provider-fallback-v1`

The system **MUST** automatically switch to a fallback provider with matching capabilities when the primary provider fails.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Timeout Enforcement

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-timeout-v1`

The system **MUST** enforce the following timeout types:
- Time-to-first-token (TTFT): max wait for initial response chunk
- Total generation timeout: max duration for complete response

On timeout the system **MUST** trigger fallback (if configured) or return a timeout error.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

#### Pre-Call Interceptor

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-pre-call-interceptor-v1`

The system **MUST** invoke Hook Plugin before sending a request to the provider. The plugin can allow, block, or modify the request.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-hook-plugin`

#### Post-Response Interceptor

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-post-response-interceptor-v1`

The system **MUST** invoke Hook Plugin after receiving a response from the provider. The plugin can allow, block, or modify the response.

**Actors**: `cpt-cf-llm-gateway-actor-hook-plugin`, `cpt-cf-llm-gateway-actor-consumer`

#### Per-Tenant Budget Enforcement

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-budget-enforcement-v1`

The system **MUST** check budget before execution via Usage Tracker, reject requests when budget is exhausted, and report actual usage after completion.

Cross-cutting concern — applies to all operations, no dedicated UC.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-usage-tracker`

#### Rate Limiting

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-fr-rate-limiting-v1`

The system **MUST** enforce rate limits at tenant and user levels and reject requests exceeding configured limits.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`

### P3 — Optimization

#### Batch Processing

- [ ] `p3` - **ID**: `cpt-cf-llm-gateway-fr-batch-processing-v1`

The system **MUST** accept a batch of requests for async processing at reduced cost, abstracting provider batch APIs. Batch jobs can take up to 24 hours to complete; the system **MUST** durably persist batch-to-provider job ID mappings and results.

**Actors**: `cpt-cf-llm-gateway-actor-consumer`, `cpt-cf-llm-gateway-actor-provider`

### P4 — Enterprise

#### Audit Events

- [ ] `p4` - **ID**: `cpt-cf-llm-gateway-fr-audit-events-v1`

The system **MUST** emit audit events via Audit Module for compliance: request started, completed, failed, blocked, fallback triggered.

Cross-cutting concern — applies to all operations, no dedicated UC.

**Actors**: `cpt-cf-llm-gateway-actor-audit-module`

## 6. Non-Functional Requirements

### Scalability

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-nfr-scalability-v1`

The system **MUST** support horizontal scaling without instance-local state or inter-instance coordination. Any instance **MUST** be able to serve any request. Job state **MUST** be accessible from all instances.

### Data Retention

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-nfr-data-retention-v1`

Async job records (ID mappings, results) are retained for 10 minutes after completion for LLM completion jobs and 48 hours after completion for batch jobs. Expired records are cleaned up automatically. Usage delivery records are removed after successful delivery.

### Compatibility

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-nfr-compatibility-v1`

Gateway API follows the Open Responses protocol and maintains backward compatibility within the same major version. Breaking changes require a new API version prefix. Co-existence with other platform modules is guaranteed — Gateway communicates exclusively through SDK traits and ClientHub, with no shared mutable state.

### NFR Exclusions

The following quality domains are handled at the platform level and do not require module-specific NFRs:

- **Authentication / Authorization**: Handled by platform AuthN/AuthZ modules via SecurityContext. All Gateway endpoints require valid authentication; authorization is enforced per tenant through platform middleware.
- **Security implementation**: Credential management handled by CredStore; TLS and network security handled by infrastructure. Content security (PII filtering, moderation) is handled by Hook Plugins, not Gateway itself.
- **Safety**: Not applicable — LLM Gateway is a software-only API module with no physical interaction or safety-critical operations.
- **Usability / Accessibility**: Not applicable — API-only module with no user interface.
- **Compliance / Regulatory**: LLM Gateway does not store conversation content or PII. Request/response data is transient. Compliance requirements for data processed by LLM providers are the responsibility of consumers and the providers themselves.
- **Operations (Deployment / Monitoring)**: Handled by platform infrastructure. Gateway follows standard ModKit module deployment and observability patterns.
- **Maintainability / Documentation**: Follows platform-wide documentation standards. API documentation generated from OpenAPI specs in DESIGN.

### Recovery

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-nfr-recovery-v1`

Async and batch job state must survive Gateway instance restarts with zero data loss (RPO: 0). Recovery time is bounded by infrastructure restart time (RTO: platform-defined). Persistent storage availability is an infrastructure concern.

## 7. Public Library Interfaces

Not applicable — LLM Gateway exposes only a REST API (documented in DESIGN.md), not a public Rust crate interface.

## 8. Use Cases

#### UC-001: Chat Completion

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-chat-completion-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant.

**Flow**:
1. Consumer sends chat_completion(model, messages)
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns response
5. Gateway returns normalized response with usage

**Postconditions**: Response returned, usage reported.

**Alternative Flows**:
- **Provider error**: Gateway normalizes provider error to Gateway error format and returns to consumer.
- **Timeout**: Gateway enforces TTFT and total timeouts; on expiry triggers fallback (if configured) or returns timeout error.
- **Invalid model**: Gateway returns model-not-found error if model is unavailable for tenant.

**Acceptance criteria**:
- Response in normalized format regardless of provider
- Usage metrics included (tokens, cost estimate)
- Provider errors normalized to Gateway error format

#### UC-002: Streaming Chat Completion

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-streaming-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports streaming.

**Flow**:
1. Consumer sends chat_completion(stream=true)
2. Gateway resolves provider via Model Registry
3. Gateway establishes streaming connection to provider
4. Gateway normalizes each chunk
5. Gateway streams chunks to Consumer
6. Gateway sends final usage summary

**Postconditions**: Stream completed, usage reported.

**Alternative Flows**:
- **Mid-stream provider failure**: Gateway closes stream with error event and reports partial usage.
- **Consumer disconnects**: Gateway cancels upstream provider request and reports usage for consumed tokens.

**Acceptance criteria**:
- Chunks normalized from provider format
- Final message includes usage metrics
- Connection errors propagated to consumer

#### UC-003: Embeddings Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-embeddings-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Embedding model available for tenant.

**Flow**:
1. Consumer sends embed(model, input[])
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns vectors
5. Gateway returns vectors with usage

**Postconditions**: Vectors returned, usage reported.

**Alternative Flows**:
- **Provider error**: Gateway normalizes error and returns to consumer.
- **Invalid input**: Gateway returns validation error if input is empty or exceeds provider limits.

**Acceptance criteria**:
- Vectors returned in normalized format
- Usage metrics included (tokens)

#### UC-004: Vision (Image Analysis)

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-vision-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports required content type.

**Flow**:
1. Consumer sends chat_completion with image URLs
2. Gateway resolves provider via Model Registry
3. Gateway fetches images from FileStorage
4. Gateway sends request via Outbound API Gateway
5. Provider returns analysis
6. Gateway returns response with usage

**Postconditions**: Response returned, usage reported.

**Alternative Flows**:
- **Media resolution failure**: Gateway returns media-not-found error if FileStorage URL is invalid or inaccessible.
- **Provider error**: Gateway normalizes error and returns to consumer.

**Acceptance criteria**:
- Multiple images supported per request
- Response in normalized format
- Usage metrics included

#### UC-005: Image Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-image-generation-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Image generation model available for tenant.

**Flow**:
1. Consumer sends generation request with prompt
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns generated image
5. Gateway stores image in FileStorage
6. Gateway returns URL with usage

**Postconditions**: Image stored, URL returned, usage reported.

**Alternative Flows**:
- **Provider error**: Gateway normalizes error and returns to consumer.
- **FileStorage upload failure**: Gateway returns storage error; generated image is lost.

**Acceptance criteria**:
- Generated image accessible via returned URL
- Response in normalized format
- Usage metrics included

#### UC-006: Speech-to-Text

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-speech-to-text-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: STT model available for tenant.

**Flow**:
1. Consumer sends message with audio URL
2. Gateway resolves provider via Model Registry
3. Gateway fetches audio from FileStorage
4. Gateway sends request via Outbound API Gateway
5. Provider returns transcription
6. Gateway returns text response with usage

**Postconditions**: Transcription returned, usage reported.

**Alternative Flows**:
- **Media resolution failure**: Gateway returns media-not-found error if audio URL is invalid or inaccessible.
- **Provider error**: Gateway normalizes error and returns to consumer.

**Acceptance criteria**:
- Transcription in normalized format
- Usage metrics included

#### UC-007: Text-to-Speech

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-text-to-speech-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: TTS model available for tenant.

**Flow**:
1. Consumer sends TTS request with text
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns audio
5. Gateway stores audio in FileStorage
6. Gateway returns URL with usage

**Postconditions**: Audio stored, URL returned, usage reported.

**Alternative Flows**:
- **Provider error**: Gateway normalizes error and returns to consumer.
- **FileStorage upload failure**: Gateway returns storage error; generated audio is lost.

**Acceptance criteria**:
- Generated audio accessible via returned URL
- Streaming mode supported (audio chunks returned directly)
- Usage metrics included

#### UC-008: Video Understanding

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-video-understanding-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports required content type.

**Flow**:
1. Consumer sends message with video URL
2. Gateway resolves provider via Model Registry
3. Gateway fetches video from FileStorage
4. Gateway sends request via Outbound API Gateway
5. Provider returns analysis
6. Gateway returns response with usage

**Postconditions**: Response returned, usage reported.

**Alternative Flows**:
- **Media resolution failure**: Gateway returns media-not-found error if video URL is invalid or inaccessible.
- **Provider error**: Gateway normalizes error and returns to consumer.

**Acceptance criteria**:
- Response in normalized format
- Usage metrics included

#### UC-009: Video Generation

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-video-generation-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Video generation model available for tenant.

**Flow**:
1. Consumer sends generation request with prompt
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns generated video
5. Gateway stores video in FileStorage
6. Gateway returns URL with usage

**Postconditions**: Video stored, URL returned, usage reported.

**Alternative Flows**:
- **Provider error**: Gateway normalizes error and returns to consumer.
- **FileStorage upload failure**: Gateway returns storage error; generated video is lost.

**Acceptance criteria**:
- Generated video accessible via returned URL
- Async mode supported (typically required due to long processing)
- Usage metrics included

#### UC-010: Tool/Function Calling

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-tool-calling-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports function calling.

**Flow**:
1. Consumer sends chat_completion with tool definitions
2. Gateway resolves provider via Model Registry
3. Gateway resolves schemas via Type Registry (for reference and inline GTS formats)
4. Gateway converts tools to provider format
5. Gateway sends request via Outbound API Gateway
6. Provider returns tool_calls
7. Gateway returns tool_calls in unified format
8. Consumer executes tools, sends results
9. Gateway forwards tool results to provider
10. Provider returns final response
11. Gateway returns response with usage

**Postconditions**: Response returned, usage reported.

**Alternative Flows**:
- **Schema resolution failure**: Gateway returns schema-not-found error if GTS reference is unresolvable.
- **Provider error**: Gateway normalizes error and returns to consumer.
- **Partial tool execution**: Consumer sends partial tool results; Gateway forwards to provider for continued generation.

**Acceptance criteria**:
- Tool definitions supported: reference, inline GTS, unified format (OpenAI-like)
- Tool calls returned in unified format
- Response in normalized format

#### UC-011: Structured Output

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-structured-output-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant.

**Flow**:
1. Consumer sends chat_completion with response_schema
2. Gateway resolves provider via Model Registry
3. Gateway sends request via Outbound API Gateway
4. Provider returns JSON response
5. Gateway validates response against schema
6. Gateway returns validated response with usage (or validation_error if invalid)

**Postconditions**: Valid JSON returned, usage reported.

**Alternative Flows**:
- **Schema validation failure**: Gateway returns validation_error with details if response does not match schema.
- **Provider error**: Gateway normalizes error and returns to consumer.

**Acceptance criteria**:
- Response validated against provided schema
- Returns validation_error with details if schema validation fails
- Response in normalized format

#### UC-012: Document Understanding

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-document-understanding-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports required content type.

**Flow**:
1. Consumer sends message with document URL
2. Gateway resolves provider via Model Registry
3. Gateway fetches document from FileStorage
4. Gateway sends request via Outbound API Gateway
5. Provider returns analysis
6. Gateway returns response with usage

**Postconditions**: Response returned, usage reported.

**Alternative Flows**:
- **Media resolution failure**: Gateway returns media-not-found error if document URL is invalid or inaccessible.
- **Provider error**: Gateway normalizes error and returns to consumer.

**Acceptance criteria**:
- Response in normalized format
- Usage metrics included

#### UC-013: Async Jobs

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-async-jobs-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant.

**Flow**:
1. Consumer sends request with async=true
2. Gateway resolves provider via Model Registry
3. Gateway initiates async job
4. Gateway returns job_id
5. Consumer polls get_job(job_id)
6. Gateway returns status/result
7. (Optional) Consumer cancels job via cancel_job(job_id)

**Postconditions**: Job completed, cancelled, or result returned.

**Alternative Flows**:
- **Provider error during execution**: Gateway marks job as failed, stores error details in job result.
- **Job not found**: Gateway returns job-not-found error if job ID is invalid or expired.
- **Cancellation of completed job**: Gateway returns job-already-completed status; no-op.

**Acceptance criteria**:
- Sync provider + async request: Gateway simulates job
- Async provider + sync request: Gateway polls internally
- Job status: pending, running, completed, failed, cancelled
- Job cancellation supported

#### UC-014: Realtime Audio

- [ ] `p1` - **ID**: `cpt-cf-llm-gateway-usecase-realtime-audio-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, model supports realtime audio.

**Flow**:
1. Consumer establishes persistent bidirectional connection
2. Gateway resolves provider via Model Registry
3. Gateway connects to provider via bidirectional channel
4. Bidirectional audio/text streaming
5. Consumer closes connection
6. Gateway returns usage summary

**Postconditions**: Session closed, usage reported.

**Alternative Flows**:
- **Connection failure**: Gateway returns connection error if realtime channel cannot be established.
- **Mid-session provider failure**: Gateway closes session with error event and reports partial usage.

**Acceptance criteria**:
- Bidirectional streaming supported
- Usage summary on close

#### UC-015: Provider Fallback

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-usecase-provider-fallback-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant.

**Flow**:
1. Consumer sends request with fallback configuration
2. Gateway resolves provider via Model Registry
3. Gateway sends request to primary provider
4. Primary provider fails
5. Gateway selects fallback from request configuration
6. Gateway sends request to fallback provider
7. Gateway returns response (fallback indicated)

**Postconditions**: Response returned via fallback.

**Acceptance criteria**:
- Fallback configuration provided in request
- Fallback selection based on capability match
- Response includes fallback indicator

#### UC-016: Timeout Enforcement

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-usecase-timeout-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant.

**Flow**:
1. Consumer sends request
2. Gateway starts timeout tracking (TTFT, total)
3. Gateway sends request to provider
4. If TTFT timeout: Gateway triggers fallback or error
5. If total timeout: Gateway triggers fallback or error
6. Gateway returns response or error

**Postconditions**: Response returned or timeout error.

**Acceptance criteria**:
- TTFT (time-to-first-token) timeout enforced
- Total generation timeout enforced
- On timeout: fallback (if configured) or error

#### UC-017: Pre-Call Interceptor

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-usecase-pre-call-interceptor-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Hook Plugin configured for tenant.

**Flow**:
1. Consumer sends request
2. Gateway invokes Hook Plugin pre_call
3. Plugin allows, blocks, or modifies request
4. If blocked: Gateway returns request_blocked error
5. If allowed/modified: Gateway proceeds with request

**Postconditions**: Request processed or blocked.

**Acceptance criteria**:
- Plugin can allow, block, or modify request
- Blocked requests return request_blocked error

#### UC-018: Post-Response Interceptor

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-usecase-post-response-interceptor-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Hook Plugin configured for tenant.

**Flow**:
1. Provider returns response
2. Gateway invokes Hook Plugin post_response
3. Plugin allows, blocks, or modifies response
4. If blocked: Gateway returns response_blocked error
5. If allowed/modified: Gateway returns response to consumer

**Postconditions**: Response returned or blocked.

**Acceptance criteria**:
- Plugin can allow, block, or modify response
- Blocked responses return response_blocked error

#### UC-019: Rate Limiting

- [ ] `p2` - **ID**: `cpt-cf-llm-gateway-usecase-rate-limiting-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Rate limits configured for tenant.

**Flow**:
1. Consumer sends request
2. Gateway checks rate limits
3. If limit exceeded: Gateway returns rate_limited error
4. If within limits: Gateway proceeds with request

**Postconditions**: Request processed or rejected.

**Acceptance criteria**:
- Rate limits enforced at tenant level
- Rate limits enforced at user level
- Exceeded requests return rate_limited error

#### UC-020: Batch Processing

- [ ] `p3` - **ID**: `cpt-cf-llm-gateway-usecase-batch-processing-v1`
**Actor**: `cpt-cf-llm-gateway-actor-consumer`

**Preconditions**: Model available for tenant, provider supports batch API.

**Flow**:
1. Consumer submits batch of requests
2. Gateway resolves provider via Model Registry
3. Gateway submits to provider batch API
4. Gateway returns batch_id
5. Consumer polls for results
6. Gateway returns status and results
7. (Optional) Consumer cancels batch

**Postconditions**: Batch completed, results returned.

**Acceptance criteria**:
- Abstracts OpenAI Batch API, Anthropic Message Batches
- Partial results available as completed
- Batch cancellation supported

## 9. Acceptance Criteria

- [ ] All supported modalities (text, image, audio, video, document) produce normalized responses regardless of provider
- [ ] Gateway overhead < 50ms P99 excluding provider latency
- [ ] Availability ≥ 99.9%
- [ ] Async and batch job state survives Gateway instance restarts
- [ ] Usage records are delivered to Usage Tracker at least once for every completed request
- [ ] Expired job records are cleaned up within the defined retention windows

## 10. Dependencies

| Dependency | Description | Criticality |
|------------|-------------|-------------|
| Outbound API Gateway (OAGW) | Routes all external provider calls, injects credentials, provides circuit breaking | `p1` |
| Model Registry | Resolves model identifiers to provider and endpoint information | `p1` |
| FileStorage | Stores and retrieves media files (images, audio, video, documents) | `p1` |
| CredStore | Provides provider API credentials to OAGW | `p1` |
| Usage Tracker | Receives usage reports for budget enforcement and billing | `p1` |
| Type Registry | Resolves GTS schema references for tool definitions | `p2` |
| Audit Module | Receives compliance audit events | `p4` |

## 11. Assumptions

- LLM providers are accessible via OAGW with valid credentials in CredStore
- Model Registry maintains an up-to-date catalog of available models and their capabilities per tenant
- FileStorage is available for media operations (upload, download)
- Database is available and shared across all Gateway instances

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Provider API breaking changes | Adapter incompatibility, request failures | Version-pinned adapters, integration test suite per provider |
| Provider rate limit exhaustion | Requests rejected by provider | Gateway-level rate limiting, provider fallback chains |
| Database growth from batch jobs | Storage pressure, degraded query performance | Automated retention cleanup, monitoring on table sizes |
| Provider outage during batch processing | Batch jobs stuck in pending state | Timeout-based job expiration, consumer-visible status updates |
| Usage record delivery lag | Budget enforcement decisions based on stale data | Guaranteed delivery mechanism with short polling interval |

## 13. Open Questions

No open questions at this time. All scope, protocol, and integration decisions have been resolved through ADRs (0001–0005) and design discussions.

## 14. Traceability

- **Design**: [DESIGN.md](./DESIGN.md)
- **ADRs**: [ADR/](./ADR/)