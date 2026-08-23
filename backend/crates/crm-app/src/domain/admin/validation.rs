//! Shared validation for the Slice 004 admin commands
//! (docs/specs/SLICE_004.md §4, §7, §14 default 9): email normalization,
//! display-name bounds, and password bounds.

use super::commands::AdminCommandError;

const MAX_EMAIL_LEN: usize = 254;
const MIN_PASSWORD_LEN: usize = 12;
const MAX_PASSWORD_LEN: usize = 256;
const MIN_DISPLAY_NAME_LEN: usize = 1;
const MAX_DISPLAY_NAME_LEN: usize = 120;
const MIN_ORGANIZATION_NAME_LEN: usize = 1;
const MAX_ORGANIZATION_NAME_LEN: usize = 120;

/// Trim + lowercase, then a syntactic check: exactly one `@`, non-empty
/// local and domain parts, ≤254 chars overall
/// (docs/specs/SLICE_004.md §4 `IssueInvitation`).
pub fn normalize_email(raw: &str) -> Result<String, AdminCommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_EMAIL_LEN {
        return Err(AdminCommandError::InvalidEmail);
    }
    let normalized = trimmed.to_lowercase();

    let mut parts = normalized.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(AdminCommandError::InvalidEmail);
    };
    if local.is_empty() || domain.is_empty() {
        return Err(AdminCommandError::InvalidEmail);
    }

    Ok(normalized)
}

/// Trim, 1–120 chars (docs/specs/SLICE_004.md §7, §14 default 9).
pub fn validate_display_name(raw: &str) -> Result<String, AdminCommandError> {
    let trimmed = raw.trim();
    if trimmed.chars().count() < MIN_DISPLAY_NAME_LEN
        || trimmed.chars().count() > MAX_DISPLAY_NAME_LEN
    {
        return Err(AdminCommandError::MalformedRequest);
    }
    Ok(trimmed.to_string())
}

/// 12–256 chars, no other shape requirement (docs/specs/SLICE_004.md §7,
/// §14 default 9). `< 12` and `> 256` both map to `weak_password`.
pub fn validate_password(raw: &str) -> Result<(), AdminCommandError> {
    let len = raw.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&len) {
        return Err(AdminCommandError::WeakPassword);
    }
    Ok(())
}

/// Trim, 1–120 chars (docs/specs/SLICE_004.md §4 `CreateOrganization`).
pub fn validate_organization_name(raw: &str) -> Result<String, AdminCommandError> {
    let trimmed = raw.trim();
    if trimmed.chars().count() < MIN_ORGANIZATION_NAME_LEN
        || trimmed.chars().count() > MAX_ORGANIZATION_NAME_LEN
    {
        return Err(AdminCommandError::MalformedRequest);
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_trims_and_lowercases() {
        assert_eq!(
            normalize_email("  Alice@Example.COM  ").unwrap(),
            "alice@example.com"
        );
    }

    #[test]
    fn rejects_missing_at() {
        assert!(matches!(
            normalize_email("not-an-email"),
            Err(AdminCommandError::InvalidEmail)
        ));
    }

    #[test]
    fn rejects_multiple_at() {
        assert!(matches!(
            normalize_email("a@b@c.com"),
            Err(AdminCommandError::InvalidEmail)
        ));
    }

    #[test]
    fn rejects_empty_local_or_domain() {
        assert!(matches!(
            normalize_email("@example.com"),
            Err(AdminCommandError::InvalidEmail)
        ));
        assert!(matches!(
            normalize_email("alice@"),
            Err(AdminCommandError::InvalidEmail)
        ));
    }

    #[test]
    fn rejects_over_254_chars() {
        let long = format!("{}@example.com", "a".repeat(250));
        assert!(matches!(
            normalize_email(&long),
            Err(AdminCommandError::InvalidEmail)
        ));
    }

    #[test]
    fn rejects_empty_email() {
        assert!(matches!(
            normalize_email("   "),
            Err(AdminCommandError::InvalidEmail)
        ));
    }

    #[test]
    fn display_name_trims_and_accepts_in_bounds() {
        assert_eq!(validate_display_name("  Alice  ").unwrap(), "Alice");
    }

    #[test]
    fn display_name_rejects_empty_after_trim() {
        assert!(matches!(
            validate_display_name("   "),
            Err(AdminCommandError::MalformedRequest)
        ));
    }

    #[test]
    fn display_name_rejects_over_120() {
        assert!(matches!(
            validate_display_name(&"a".repeat(121)),
            Err(AdminCommandError::MalformedRequest)
        ));
    }

    #[test]
    fn display_name_accepts_exactly_120() {
        assert!(validate_display_name(&"a".repeat(120)).is_ok());
    }

    #[test]
    fn password_rejects_under_12() {
        assert!(matches!(
            validate_password(&"a".repeat(11)),
            Err(AdminCommandError::WeakPassword)
        ));
    }

    #[test]
    fn password_accepts_exactly_12() {
        assert!(validate_password(&"a".repeat(12)).is_ok());
    }

    #[test]
    fn password_accepts_exactly_256() {
        assert!(validate_password(&"a".repeat(256)).is_ok());
    }

    #[test]
    fn password_rejects_over_256() {
        assert!(matches!(
            validate_password(&"a".repeat(257)),
            Err(AdminCommandError::WeakPassword)
        ));
    }

    #[test]
    fn organization_name_trims_and_accepts_in_bounds() {
        assert_eq!(
            validate_organization_name("  Acme Realty  ").unwrap(),
            "Acme Realty"
        );
    }

    #[test]
    fn organization_name_rejects_empty_after_trim() {
        assert!(matches!(
            validate_organization_name("   "),
            Err(AdminCommandError::MalformedRequest)
        ));
    }

    #[test]
    fn organization_name_rejects_over_120() {
        assert!(matches!(
            validate_organization_name(&"a".repeat(121)),
            Err(AdminCommandError::MalformedRequest)
        ));
    }
}
