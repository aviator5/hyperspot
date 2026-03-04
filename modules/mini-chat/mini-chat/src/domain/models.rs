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
