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

/// Slug base for an Organization's intake address (docs/specs/
/// SLICE_007a.md §4): lowercase, non-`[a-z0-9]` runs → `-`, trimmed,
/// clipped to 38 so a `-N` collision suffix keeps the DB CHECK's 40-char
/// ceiling; empty → `org`.
pub fn slugify_organization_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    const SLUG_BASE_MAX: usize = 38;
    if out.len() > SLUG_BASE_MAX {
        out.truncate(SLUG_BASE_MAX);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "org".to_string()
    } else {
        out
    }
}

/// The candidate slugs tried in order: the base, `-2` … `-9`, then three
/// random `-xxxx` suffixes — so a lossy base (every non-Latin name slugifies
/// to `org`) can never exhaust; the slug format is unchanged.
pub fn intake_slug_candidates(name: &str) -> Vec<String> {
    let base = slugify_organization_name(name);
    let mut out = vec![base.clone()];
    out.extend((2..=9).map(|n| format!("{base}-{n}")));
    // `-xxxx` is 5 chars: clip the base to 35 so the CHECK's 40 holds.
    let short: String = base.chars().take(35).collect();
    let short = short.trim_end_matches('-').to_string();
    out.extend((0..3).map(|_| format!("{short}-{}", random_suffix(4))));
    out
}

fn random_suffix(len: usize) -> String {
    use rand::RngExt;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// An unguessable 8-char token from `[a-z2-7]` (~40 bits), the anti-
/// forgery secret in the intake address. Never logged.
pub fn mint_intake_token() -> String {
    use rand::RngExt;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_collapses_and_trims() {
        assert_eq!(
            slugify_organization_name("Cypress Bay Realty"),
            "cypress-bay-realty"
        );
        assert_eq!(
            slugify_organization_name("  Acme -- Realty! "),
            "acme-realty"
        );
        assert_eq!(slugify_organization_name("Ünïcödé Homes"), "n-c-d-homes");
        assert_eq!(slugify_organization_name("---"), "org");
        assert_eq!(slugify_organization_name(""), "org");
        assert_eq!(slugify_organization_name("A1"), "a1");
    }

    #[test]
    fn slugify_clips_to_38_and_never_ends_with_a_dash() {
        let long = format!("{} tail", "a".repeat(37));
        let slug = slugify_organization_name(&long);
        assert_eq!(slug.len(), 37, "{slug}");
        assert!(!slug.ends_with('-'));
        let exact = "b".repeat(60);
        assert_eq!(slugify_organization_name(&exact).len(), 38);
        for c in intake_slug_candidates(&exact) {
            assert!(c.len() <= 40, "{c}");
            assert!(crate::domain::intake::address::is_slug(&c), "{c}");
        }
    }

    #[test]
    fn candidates_are_base_then_2_to_9_then_random() {
        let c = intake_slug_candidates("Acme Realty");
        assert_eq!(c.len(), 12);
        assert_eq!(c[0], "acme-realty");
        assert_eq!(c[1], "acme-realty-2");
        assert_eq!(c[8], "acme-realty-9");
        for extra in &c[9..] {
            assert!(
                extra.starts_with("acme-realty-") && extra.len() == "acme-realty-".len() + 4,
                "{extra}"
            );
            assert!(crate::domain::intake::address::is_slug(extra), "{extra}");
        }
        // The random candidates clip a long base so the 40-char CHECK holds.
        let long = intake_slug_candidates(&"b".repeat(60));
        for c in &long {
            assert!(
                c.len() <= 40 && crate::domain::intake::address::is_slug(c),
                "{c}"
            );
        }
    }

    #[test]
    fn token_is_eight_chars_from_the_mint_alphabet_and_varies() {
        let a = mint_intake_token();
        let b = mint_intake_token();
        for t in [&a, &b] {
            assert_eq!(t.len(), 8);
            assert!(
                t.bytes()
                    .all(|c| c.is_ascii_lowercase() || (b'2'..=b'7').contains(&c)),
                "{t}"
            );
        }
        assert_ne!(a, b);
    }

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
