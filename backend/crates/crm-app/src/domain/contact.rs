//! Contact-method normalization and the identify (dedup) lookup
//! (docs/specs/SLICE_002.md §2, §3).

use sqlx::PgConnection;

use crate::ids::{OrganizationId, PersonId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactKind {
    Email,
    Phone,
}

impl ContactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ContactKind::Email => "email",
            ContactKind::Phone => "phone",
        }
    }
}

/// Trim + lowercase. Not normalizable (`None`) if, after trimming, it is
/// empty or contains no `@` (docs/specs/SLICE_002.md §2).
pub fn normalize_email(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return None;
    }
    Some(trimmed.to_lowercase())
}

/// Digits only; a 10-digit US number gets a `+1` prefix, an 11-digit number
/// starting with `1` gets a `+` prefix, otherwise `+` + digits. Not
/// normalizable (`None`) with fewer than 10 digits
/// (docs/specs/SLICE_002.md §2).
pub fn normalize_phone(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 10 {
        return None;
    }
    if digits.len() == 10 {
        return Some(format!("+1{digits}"));
    }
    if digits.len() == 11 && digits.starts_with('1') {
        return Some(format!("+{digits}"));
    }
    Some(format!("+{digits}"))
}

#[derive(Debug, Clone, Copy)]
pub struct IdentifyMatch {
    pub person_id: PersonId,
    pub matched_by: ContactKind,
}

/// Organization-scoped dedup lookup (docs/specs/SLICE_002.md §3): matches by
/// normalized email first (earliest-created Person on ambiguity), then by
/// normalized phone. Email wins when email and phone match different
/// Persons. Callers must hold the per-Organization intake advisory lock
/// before calling this.
pub async fn identify(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    email: Option<&str>,
    phone: Option<&str>,
) -> Result<Option<IdentifyMatch>, sqlx::Error> {
    if let Some(email) = email {
        if let Some(person_id) =
            find_earliest_person(conn, organization_id, ContactKind::Email, email).await?
        {
            return Ok(Some(IdentifyMatch {
                person_id,
                matched_by: ContactKind::Email,
            }));
        }
    }
    if let Some(phone) = phone {
        if let Some(person_id) =
            find_earliest_person(conn, organization_id, ContactKind::Phone, phone).await?
        {
            return Ok(Some(IdentifyMatch {
                person_id,
                matched_by: ContactKind::Phone,
            }));
        }
    }
    Ok(None)
}

async fn find_earliest_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    kind: ContactKind,
    normalized_value: &str,
) -> Result<Option<PersonId>, sqlx::Error> {
    let kind_str = kind.as_str();
    let row = sqlx::query!(
        r#"SELECT p.id
           FROM contact_method cm
           JOIN person p ON p.id = cm.person_id AND p.organization_id = cm.organization_id
           WHERE cm.organization_id = $1 AND cm.kind = $2 AND cm.normalized_value = $3
           ORDER BY p.created_at ASC, p.id ASC
           LIMIT 1"#,
        organization_id.0,
        kind_str,
        normalized_value,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| PersonId::new(r.id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_email_case_and_whitespace() {
        assert_eq!(
            normalize_email("  Ada@Example.COM  "),
            Some("ada@example.com".to_string())
        );
    }

    #[test]
    fn rejects_email_without_at_sign() {
        assert_eq!(normalize_email("not-an-email"), None);
    }

    #[test]
    fn rejects_empty_or_whitespace_only_email() {
        assert_eq!(normalize_email(""), None);
        assert_eq!(normalize_email("   "), None);
    }

    #[test]
    fn normalizes_10_digit_us_phone() {
        assert_eq!(
            normalize_phone("(555) 555-0100"),
            Some("+15555550100".to_string())
        );
    }

    #[test]
    fn normalizes_11_digit_us_phone_with_leading_one() {
        assert_eq!(
            normalize_phone("1-555-555-0100"),
            Some("+15555550100".to_string())
        );
    }

    #[test]
    fn normalizes_international_digit_count_with_plus_prefix() {
        // 11 digits not starting with 1: not the US 11-digit case, so it
        // falls through to the generic "+" + digits rule.
        assert_eq!(
            normalize_phone("44 20 7946 0958"),
            Some("+442079460958".to_string())
        );
    }

    #[test]
    fn rejects_phone_with_fewer_than_10_digits() {
        assert_eq!(normalize_phone("555-0100"), None);
    }

    #[test]
    fn rejects_garbage_phone() {
        assert_eq!(normalize_phone("not a phone number"), None);
    }
}
