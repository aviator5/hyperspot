//! HTTP DTOs (serde/utoipa) — REST-only request and response types.
//!
//! All REST DTOs live here; SDK `models.rs` stays transport-agnostic.
//! Provide `From` conversions between SDK models and DTOs in this file.

use crate::domain::models::ChatDetail;
use axum::response::sse::Event;
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::infra::llm::{Citation, ToolPhase, Usage};

// ════════════════════════════════════════════════════════════════════════════
// Chat CRUD DTOs
// ════════════════════════════════════════════════════════════════════════════

/// Request DTO for creating a new chat.
#[derive(Debug, Clone)]
#[modkit_macros::api_dto(request)]
pub struct CreateChatReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Request DTO for updating a chat title.
#[derive(Debug, Clone)]
#[modkit_macros::api_dto(request)]
pub struct UpdateChatReq {
    pub title: String,
}

/// Response DTO for chat details.
#[derive(Debug, Clone)]
#[modkit_macros::api_dto(response)]
pub struct ChatDetailDto {
    pub id: Uuid,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub is_temporary: bool,
    pub message_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<ChatDetail> for ChatDetailDto {
    fn from(d: ChatDetail) -> Self {
        Self {
            id: d.id,
            model: d.model,
            title: d.title,
            is_temporary: d.is_temporary,
            message_count: d.message_count,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// StreamEvent — the SSE wire type
// ════════════════════════════════════════════════════════════════════════════

/// Server-sent event envelope for the `messages:stream` endpoint.
///
/// Each variant maps to a distinct `event:` name and `data:` JSON payload.
/// Ordering grammar: `ping* delta* tool* citations? (done | error)`.
#[derive(Debug, Clone, ToSchema)]
pub enum StreamEvent {
    Ping,
    Delta(DeltaData),
    Tool(ToolData),
    Citations(CitationsData),
    Done(Box<DoneData>),
    Error(ErrorData),
}

/// Delta text chunk.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeltaData {
    pub r#type: &'static str,
    pub content: String,
}

/// Tool lifecycle event.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ToolData {
    pub phase: ToolPhase,
    pub name: String,
    pub details: serde_json::Value,
}

/// Citations from provider annotations.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CitationsData {
    pub items: Vec<Citation>,
}

/// Successful stream completion.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DoneData {
    pub message_id: Option<String>,
    pub usage: Option<Usage>,
    pub effective_model: String,
    pub selected_model: String,
    pub quota_decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downgrade_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downgrade_reason: Option<String>,
}

/// Stream error (terminal).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorData {
    pub code: String,
    pub message: String,
}

impl StreamEvent {
    /// Convert to an Axum SSE [`Event`] with the correct `event:` name
    /// and `data:` JSON payload.
    pub fn into_sse_event(self) -> Result<Event, axum::Error> {
        match self {
            StreamEvent::Ping => Ok(Event::default().event("ping").data("{}")),
            StreamEvent::Delta(d) => Event::default().event("delta").json_data(&d),
            StreamEvent::Tool(t) => Event::default().event("tool").json_data(&t),
            StreamEvent::Citations(c) => Event::default().event("citations").json_data(&c),
            StreamEvent::Done(d) => Event::default().event("done").json_data(&*d),
            StreamEvent::Error(e) => Event::default().event("error").json_data(&e),
        }
    }

    /// Classify this event for the [`StreamPhase`] state machine.
    #[must_use]
    pub fn event_kind(&self) -> StreamEventKind {
        match self {
            StreamEvent::Ping => StreamEventKind::Ping,
            StreamEvent::Delta(_) => StreamEventKind::Delta,
            StreamEvent::Tool(_) => StreamEventKind::Tool,
            StreamEvent::Citations(_) => StreamEventKind::Citations,
            StreamEvent::Done(_) | StreamEvent::Error(_) => StreamEventKind::Terminal,
        }
    }

    /// Whether this is a terminal event (done or error).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, StreamEvent::Done(_) | StreamEvent::Error(_))
    }
}

impl modkit::api::api_dto::ResponseApiDto for StreamEvent {}

// ════════════════════════════════════════════════════════════════════════════
// StreamEventKind — for phase state machine
// ════════════════════════════════════════════════════════════════════════════

/// Coarse event classification for ordering enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEventKind {
    Ping,
    Delta,
    Tool,
    Citations,
    Terminal,
}

// ════════════════════════════════════════════════════════════════════════════
// StreamPhase — event ordering state machine
// ════════════════════════════════════════════════════════════════════════════

/// Enforces the SSE ordering grammar: `ping* delta* tool* citations? (done | error)`.
///
/// Only forward transitions are allowed. Out-of-order events produce an
/// [`OrderingViolation`] error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPhase {
    /// Before any events. Accepts ping, delta, tool, citations, terminal.
    Idle,
    /// After one or more pings. Accepts ping, delta, tool, citations, terminal.
    Pinging,
    /// After first delta. Accepts delta, tool, citations, terminal.
    Deltas,
    /// After first tool event. Accepts tool, citations, terminal.
    Tools,
    /// After citations. Accepts terminal only.
    Citations,
    /// Terminal event emitted. No further events accepted.
    Terminal,
}

/// An event that violates the ordering grammar.
#[derive(Debug)]
pub struct OrderingViolation {
    pub phase: StreamPhase,
    pub event: StreamEventKind,
}

impl std::fmt::Display for OrderingViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SSE ordering violation: {} event in {} phase",
            self.event, self.phase
        )
    }
}

impl std::fmt::Display for StreamEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ping => f.write_str("Ping"),
            Self::Delta => f.write_str("Delta"),
            Self::Tool => f.write_str("Tool"),
            Self::Citations => f.write_str("Citations"),
            Self::Terminal => f.write_str("Terminal"),
        }
    }
}

impl std::fmt::Display for StreamPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("Idle"),
            Self::Pinging => f.write_str("Pinging"),
            Self::Deltas => f.write_str("Deltas"),
            Self::Tools => f.write_str("Tools"),
            Self::Citations => f.write_str("Citations"),
            Self::Terminal => f.write_str("Terminal"),
        }
    }
}

impl StreamPhase {
    /// Whether this phase represents a terminal state.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, StreamPhase::Terminal)
    }

    /// Attempt to advance the phase based on the incoming event kind.
    ///
    /// Returns the new phase on success, or an [`OrderingViolation`] if the
    /// event would break the grammar.
    pub fn try_advance(self, kind: StreamEventKind) -> Result<StreamPhase, OrderingViolation> {
        match (self, kind) {
            // Terminal phase rejects everything
            (StreamPhase::Terminal, _) => Err(OrderingViolation {
                phase: self,
                event: kind,
            }),

            // Terminal events are always accepted from non-terminal phases
            (_, StreamEventKind::Terminal) => Ok(StreamPhase::Terminal),

            // Ping: only from Idle or Pinging
            (StreamPhase::Idle | StreamPhase::Pinging, StreamEventKind::Ping) => {
                Ok(StreamPhase::Pinging)
            }

            // Delta: from Idle, Pinging, or Deltas
            (
                StreamPhase::Idle | StreamPhase::Pinging | StreamPhase::Deltas,
                StreamEventKind::Delta,
            ) => Ok(StreamPhase::Deltas),

            // Tool: from Idle, Pinging, Deltas, or Tools
            (
                StreamPhase::Idle | StreamPhase::Pinging | StreamPhase::Deltas | StreamPhase::Tools,
                StreamEventKind::Tool,
            ) => Ok(StreamPhase::Tools),

            // Citations: from Idle, Pinging, Deltas, or Tools (at most once)
            (
                StreamPhase::Idle | StreamPhase::Pinging | StreamPhase::Deltas | StreamPhase::Tools,
                StreamEventKind::Citations,
            ) => Ok(StreamPhase::Citations),

            // Everything else is a violation
            _ => Err(OrderingViolation {
                phase: self,
                event: kind,
            }),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Conversions from provider types
// ════════════════════════════════════════════════════════════════════════════

use crate::infra::llm::ClientSseEvent;

impl From<ClientSseEvent> for StreamEvent {
    fn from(event: ClientSseEvent) -> Self {
        match event {
            ClientSseEvent::Delta { r#type, content } => {
                StreamEvent::Delta(DeltaData { r#type, content })
            }
            ClientSseEvent::Tool {
                phase,
                name,
                details,
            } => StreamEvent::Tool(ToolData {
                phase,
                name: name.to_owned(),
                details,
            }),
            ClientSseEvent::Citations { items } => StreamEvent::Citations(CitationsData { items }),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Request DTO
// ════════════════════════════════════════════════════════════════════════════

/// Request body for `POST /v1/chats/{id}/messages/stream`.
#[derive(Debug, Clone, serde::Deserialize, ToSchema)]
pub struct StreamMessageRequest {
    /// Message content (must be non-empty).
    pub content: String,
    /// Client-generated idempotency key (UUID v4). Optional in P1.
    #[serde(default)]
    pub request_id: Option<uuid::Uuid>,
    /// Attachment IDs to include.
    #[serde(default)]
    pub attachment_ids: Vec<uuid::Uuid>,
    /// Web search configuration.
    #[serde(default)]
    pub web_search: Option<WebSearchConfig>,
}

impl modkit::api::api_dto::RequestApiDto for StreamMessageRequest {}

/// Web search toggle.
#[derive(Debug, Clone, serde::Deserialize, ToSchema)]
pub struct WebSearchConfig {
    pub enabled: bool,
}

// ════════════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_converts_to_sse_event() {
        assert!(StreamEvent::Ping.into_sse_event().is_ok());
    }

    #[test]
    fn delta_converts_to_sse_event() {
        assert!(
            StreamEvent::Delta(DeltaData {
                r#type: "text",
                content: "hello".into(),
            })
            .into_sse_event()
            .is_ok()
        );
    }

    #[test]
    fn delta_data_serializes_correctly() {
        let data = DeltaData {
            r#type: "text",
            content: "hello".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"content\":\"hello\""));
    }

    #[test]
    fn done_serializes_without_optional_fields() {
        let data = DoneData {
            message_id: None,
            usage: None,
            effective_model: "gpt-4o".into(),
            selected_model: "gpt-4o".into(),
            quota_decision: "allow".into(),
            downgrade_from: None,
            downgrade_reason: None,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"effective_model\":\"gpt-4o\""));
        assert!(!json.contains("downgrade_from"));
        assert!(!json.contains("downgrade_reason"));
    }

    #[test]
    fn done_serializes_with_downgrade() {
        let data = DoneData {
            message_id: Some("msg-123".into()),
            usage: Some(Usage {
                input_tokens: 100,
                output_tokens: 50,
            }),
            effective_model: "gpt-4o-mini".into(),
            selected_model: "gpt-4o".into(),
            quota_decision: "downgrade".into(),
            downgrade_from: Some("gpt-4o".into()),
            downgrade_reason: Some("premium_quota_exhausted".into()),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"downgrade_reason\":\"premium_quota_exhausted\""));
        assert!(json.contains("\"downgrade_from\":\"gpt-4o\""));
    }

    #[test]
    fn done_converts_to_sse_event() {
        assert!(
            StreamEvent::Done(Box::new(DoneData {
                message_id: None,
                usage: None,
                effective_model: "gpt-4o".into(),
                selected_model: "gpt-4o".into(),
                quota_decision: "allow".into(),
                downgrade_from: None,
                downgrade_reason: None,
            }))
            .into_sse_event()
            .is_ok()
        );
    }

    #[test]
    fn error_data_serializes_correctly() {
        let data = ErrorData {
            code: "provider_error".into(),
            message: "Something went wrong".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"code\":\"provider_error\""));
        assert!(json.contains("\"message\":\"Something went wrong\""));
    }

    #[test]
    fn error_converts_to_sse_event() {
        assert!(
            StreamEvent::Error(ErrorData {
                code: "provider_error".into(),
                message: "Something went wrong".into(),
            })
            .into_sse_event()
            .is_ok()
        );
    }

    // ── StreamPhase tests ──

    #[test]
    fn phase_idle_accepts_all_kinds() {
        assert_eq!(
            StreamPhase::Idle
                .try_advance(StreamEventKind::Ping)
                .unwrap(),
            StreamPhase::Pinging
        );
        assert_eq!(
            StreamPhase::Idle
                .try_advance(StreamEventKind::Delta)
                .unwrap(),
            StreamPhase::Deltas
        );
        assert_eq!(
            StreamPhase::Idle
                .try_advance(StreamEventKind::Tool)
                .unwrap(),
            StreamPhase::Tools
        );
        assert_eq!(
            StreamPhase::Idle
                .try_advance(StreamEventKind::Citations)
                .unwrap(),
            StreamPhase::Citations
        );
        assert_eq!(
            StreamPhase::Idle
                .try_advance(StreamEventKind::Terminal)
                .unwrap(),
            StreamPhase::Terminal
        );
    }

    #[test]
    fn phase_deltas_rejects_ping() {
        assert!(
            StreamPhase::Deltas
                .try_advance(StreamEventKind::Ping)
                .is_err()
        );
    }

    #[test]
    fn phase_tools_rejects_delta() {
        assert!(
            StreamPhase::Tools
                .try_advance(StreamEventKind::Delta)
                .is_err()
        );
    }

    #[test]
    fn phase_citations_rejects_non_terminal() {
        assert!(
            StreamPhase::Citations
                .try_advance(StreamEventKind::Ping)
                .is_err()
        );
        assert!(
            StreamPhase::Citations
                .try_advance(StreamEventKind::Delta)
                .is_err()
        );
        assert!(
            StreamPhase::Citations
                .try_advance(StreamEventKind::Tool)
                .is_err()
        );
        assert!(
            StreamPhase::Citations
                .try_advance(StreamEventKind::Citations)
                .is_err()
        );
    }

    #[test]
    fn phase_terminal_rejects_everything() {
        assert!(
            StreamPhase::Terminal
                .try_advance(StreamEventKind::Ping)
                .is_err()
        );
        assert!(
            StreamPhase::Terminal
                .try_advance(StreamEventKind::Terminal)
                .is_err()
        );
    }

    #[test]
    fn phase_citations_accepts_terminal() {
        assert_eq!(
            StreamPhase::Citations
                .try_advance(StreamEventKind::Terminal)
                .unwrap(),
            StreamPhase::Terminal
        );
    }

    #[test]
    fn normal_stream_sequence() {
        let mut phase = StreamPhase::Idle;
        phase = phase.try_advance(StreamEventKind::Ping).unwrap();
        assert_eq!(phase, StreamPhase::Pinging);
        phase = phase.try_advance(StreamEventKind::Delta).unwrap();
        assert_eq!(phase, StreamPhase::Deltas);
        phase = phase.try_advance(StreamEventKind::Delta).unwrap();
        assert_eq!(phase, StreamPhase::Deltas);
        phase = phase.try_advance(StreamEventKind::Terminal).unwrap();
        assert_eq!(phase, StreamPhase::Terminal);
    }

    #[test]
    fn tool_stream_sequence() {
        let mut phase = StreamPhase::Idle;
        phase = phase.try_advance(StreamEventKind::Delta).unwrap();
        phase = phase.try_advance(StreamEventKind::Tool).unwrap();
        assert_eq!(phase, StreamPhase::Tools);
        phase = phase.try_advance(StreamEventKind::Tool).unwrap();
        assert_eq!(phase, StreamPhase::Tools);
        phase = phase.try_advance(StreamEventKind::Citations).unwrap();
        assert_eq!(phase, StreamPhase::Citations);
        phase = phase.try_advance(StreamEventKind::Terminal).unwrap();
        assert_eq!(phase, StreamPhase::Terminal);
    }
}
