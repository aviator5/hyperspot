use modkit_db::odata::sea_orm_filter::{FieldToColumn, ODataFieldMapping};
use modkit_odata::filter::{FieldKind, FilterField};

use crate::infra::db::entity::chat::{Column, Entity, Model};

/// Cursor/sort field enum for chat pagination.
///
/// P1 only supports cursor-based pagination with `updated_at DESC` + `id` tiebreaker.
/// No user-facing $filter or $orderby is exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatCursorField {
    UpdatedAt,
    Id,
}

impl FilterField for ChatCursorField {
    const FIELDS: &'static [Self] = &[Self::UpdatedAt, Self::Id];

    fn name(&self) -> &'static str {
        match self {
            Self::UpdatedAt => "updated_at",
            Self::Id => "id",
        }
    }

    fn kind(&self) -> FieldKind {
        match self {
            Self::UpdatedAt => FieldKind::DateTimeUtc,
            Self::Id => FieldKind::Uuid,
        }
    }
}

pub struct ChatODataMapper;

impl FieldToColumn<ChatCursorField> for ChatODataMapper {
    type Column = Column;

    fn map_field(field: ChatCursorField) -> Column {
        match field {
            ChatCursorField::UpdatedAt => Column::UpdatedAt,
            ChatCursorField::Id => Column::Id,
        }
    }
}

impl ODataFieldMapping<ChatCursorField> for ChatODataMapper {
    type Entity = Entity;

    fn extract_cursor_value(model: &Model, field: ChatCursorField) -> sea_orm::Value {
        match field {
            ChatCursorField::UpdatedAt => {
                sea_orm::Value::TimeDateTimeWithTimeZone(Some(Box::new(model.updated_at)))
            }
            ChatCursorField::Id => sea_orm::Value::Uuid(Some(Box::new(model.id))),
        }
    }
}
