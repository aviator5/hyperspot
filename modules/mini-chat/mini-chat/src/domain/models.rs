use modkit_macros::domain_model;
use time::OffsetDateTime;
use uuid::Uuid;

// ── Chat ──

/// A chat conversation.
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub model: String,
    pub title: Option<String>,
    pub is_temporary: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Enriched chat response with message count (no `tenant_id/user_id`).
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDetail {
    pub id: Uuid,
    pub model: String,
    pub title: Option<String>,
    pub is_temporary: bool,
    pub message_count: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Data for creating a new chat.
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChat {
    pub model: String,
    pub title: Option<String>,
    pub is_temporary: bool,
}

/// Partial update data for a chat.
///
/// Uses `Option<Option<String>>` for nullable fields to distinguish
/// "not provided" (None) from "set to null" (Some(None)).
///
/// Note: `model` is immutable for the chat lifetime
/// (`cpt-cf-mini-chat-constraint-model-locked-per-chat`).
/// `is_temporary` toggling is a P2 feature (`:temporary` endpoint).
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::option_option)]
pub struct ChatPatch {
    pub title: Option<Option<String>>,
}

// ── Message ──

/// A chat message as returned by the list endpoint.
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Uuid,
    pub request_id: Uuid,
    pub role: String,
    pub content: String,
    pub attachment_ids: Vec<Uuid>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub created_at: OffsetDateTime,
}

// ── Reaction ──

/// A reaction on an assistant message.
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    pub message_id: Uuid,
    pub reaction: String,
    pub created_at: OffsetDateTime,
}

/// Result of a reaction deletion.
#[domain_model]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionDeleted {
    pub message_id: Uuid,
    pub deleted: bool,
}
