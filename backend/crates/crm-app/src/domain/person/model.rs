//! Read-model DTOs shared by the People and Person-detail endpoints
//! (docs/specs/SLICE_002.md §5).

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::ids::UserId;

#[derive(Debug, Clone, Serialize)]
pub struct StageRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRef {
    pub id: UserId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonSummary {
    pub id: Uuid,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub stage: StageRef,
    pub assigned_user: Option<UserRef>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub inquiry_count: i64,
    pub last_inquiry_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// `first_name`/`last_name` joined by a space (whichever are present), else
/// `primary_email`, else `primary_phone` — a Person always has at least one
/// contact method, so this is never empty in practice
/// (docs/specs/SLICE_002.md §5).
pub fn compute_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    primary_email: Option<&str>,
    primary_phone: Option<&str>,
) -> String {
    let name = [first_name, last_name]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        return name;
    }
    if let Some(email) = primary_email {
        return email.to_string();
    }
    if let Some(phone) = primary_phone {
        return phone.to_string();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_full_name() {
        assert_eq!(
            compute_display_name(Some("Ada"), Some("Lovelace"), Some("ada@example.com"), None),
            "Ada Lovelace"
        );
    }

    #[test]
    fn uses_first_name_only_when_last_is_absent() {
        assert_eq!(
            compute_display_name(Some("Ada"), None, Some("ada@example.com"), None),
            "Ada"
        );
    }

    #[test]
    fn falls_back_to_primary_email() {
        assert_eq!(
            compute_display_name(None, None, Some("ada@example.com"), Some("+15555550100")),
            "ada@example.com"
        );
    }

    #[test]
    fn falls_back_to_primary_phone_when_no_email() {
        assert_eq!(
            compute_display_name(None, None, None, Some("+15555550100")),
            "+15555550100"
        );
    }
}
