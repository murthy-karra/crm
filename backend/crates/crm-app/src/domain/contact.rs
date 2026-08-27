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

/// A trimmed, lowercased email address that has passed [`normalize_email`]'s
/// validity check (docs/specs/SLICE_002.md §2). PII — like `ParsedMail` and
/// the config-secret newtypes (`RawPayloadKey` etc.), `Debug` is redacted
/// so an accidental `{:?}` (a span field, a log line, a derived `Debug` on
/// a containing struct) never prints it. `Display` still renders the value
/// for callers that deliberately need it (e.g. a SQL bind). Constructible
/// only via `normalize_email` — the inner field is private and there is no
/// validation-skipping constructor.
#[derive(Clone, PartialEq, Eq)]
pub struct NormalizedEmail(String);

impl NormalizedEmail {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NormalizedEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for NormalizedEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NormalizedEmail(REDACTED)")
    }
}

/// Trim + lowercase. Not normalizable (`None`) if, after trimming, it is
/// empty or contains no `@` (docs/specs/SLICE_002.md §2).
pub fn normalize_email(raw: &str) -> Option<NormalizedEmail> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return None;
    }
    Some(NormalizedEmail(trimmed.to_lowercase()))
}

/// A phone number in [`normalize_phone`]'s canonical `+<digits>` form. PII
/// — same Debug-redaction discipline as [`NormalizedEmail`]; constructible
/// only via `normalize_phone`.
#[derive(Clone, PartialEq, Eq)]
pub struct NormalizedPhone(String);

impl NormalizedPhone {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NormalizedPhone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for NormalizedPhone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NormalizedPhone(REDACTED)")
    }
}

/// Digits only; a 10-digit US number gets a `+1` prefix, an 11-digit number
/// starting with `1` gets a `+` prefix, otherwise `+` + digits. Not
/// normalizable (`None`) with fewer than 10 digits
/// (docs/specs/SLICE_002.md §2).
pub fn normalize_phone(raw: &str) -> Option<NormalizedPhone> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 10 {
        return None;
    }
    if digits.len() == 10 {
        return Some(NormalizedPhone(format!("+1{digits}")));
    }
    if digits.len() == 11 && digits.starts_with('1') {
        return Some(NormalizedPhone(format!("+{digits}")));
    }
    Some(NormalizedPhone(format!("+{digits}")))
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
///
/// The two params are the typed normalized values (hardening chunk V1,
/// completed at lane integration): both are constructible only via
/// `normalize_email`/`normalize_phone`, so transposing them — the swap
/// that would silently break dedup and mint duplicate Persons — no
/// longer compiles. By-value: each is one small `String`, once per
/// inquiry (reviewer-recommended shape).
pub async fn identify(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    email: Option<NormalizedEmail>,
    phone: Option<NormalizedPhone>,
) -> Result<Option<IdentifyMatch>, sqlx::Error> {
    if let Some(email) = email {
        if let Some(person_id) =
            find_earliest_person(conn, organization_id, ContactKind::Email, email.as_str()).await?
        {
            return Ok(Some(IdentifyMatch {
                person_id,
                matched_by: ContactKind::Email,
            }));
        }
    }
    if let Some(phone) = phone {
        if let Some(person_id) =
            find_earliest_person(conn, organization_id, ContactKind::Phone, phone.as_str()).await?
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
    // The accessor design (ParsedLead::normalized_email/phone re-derive
    // instead of caching) and identify()'s typed params both silently
    // depend on normalization being IDEMPOTENT: a re-derived value must
    // equal the stored one, or dedup splits Persons with no compile or
    // test failure (reviewer V1 MINOR-1). Pin it across representative
    // inputs.
    use super::*;

    #[test]
    fn normalizes_email_case_and_whitespace() {
        assert_eq!(
            normalize_email("  Ada@Example.COM  ").unwrap().as_str(),
            "ada@example.com"
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
            normalize_phone("(555) 555-0100").unwrap().as_str(),
            "+15555550100"
        );
    }

    #[test]
    fn normalizes_11_digit_us_phone_with_leading_one() {
        assert_eq!(
            normalize_phone("1-555-555-0100").unwrap().as_str(),
            "+15555550100"
        );
    }

    #[test]
    fn normalizes_international_digit_count_with_plus_prefix() {
        // 11 digits not starting with 1: not the US 11-digit case, so it
        // falls through to the generic "+" + digits rule.
        assert_eq!(
            normalize_phone("44 20 7946 0958").unwrap().as_str(),
            "+442079460958"
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

    // --- Debug redaction (PII: mirrors ParsedMail / the config-secret
    // newtypes) --------------------------------------------------------

    #[test]
    fn normalized_email_debug_is_redacted() {
        let email = normalize_email("Ada@Example.com").unwrap();
        let debug = format!("{email:?}");
        assert_eq!(debug, "NormalizedEmail(REDACTED)");
        assert!(!debug.contains("ada"));
        assert!(!debug.contains("Ada"));
        assert!(!debug.contains("example"));
        // Display, unlike Debug, is the deliberate escape hatch.
        assert_eq!(email.to_string(), "ada@example.com");
    }

    #[test]
    fn normalized_phone_debug_is_redacted() {
        let phone = normalize_phone("(555) 555-0100").unwrap();
        let debug = format!("{phone:?}");
        assert_eq!(debug, "NormalizedPhone(REDACTED)");
        assert!(!debug.contains("555"));
        assert_eq!(phone.to_string(), "+15555550100");
    }

    #[test]
    fn normalization_is_idempotent_for_every_representative_input() {
        for raw in [
            "Ada.Lovelace@Example.COM",
            "  spaced@example.com ",
            "İstanbul@example.com", // dotted capital I: multi-byte lowercase
        ] {
            let once = normalize_email(raw).expect("normalizes");
            let twice = normalize_email(once.as_str()).expect("re-normalizes");
            assert_eq!(twice, once, "email {raw:?}");
        }
        for raw in [
            "(555) 555-0100",   // 10 digits -> +1 prefix
            "1-555-555-0100",   // 11 with leading 1
            "+44 20 7946 0958", // international, >11
            "555.555.0100 x",   // separators stripped
        ] {
            let once = normalize_phone(raw).expect("normalizes");
            let twice = normalize_phone(once.as_str()).expect("re-normalizes");
            assert_eq!(twice, once, "phone {raw:?}");
        }
    }
}
