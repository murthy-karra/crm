//! The Organization intake address (docs/specs/SLICE_007a.md §4): stored
//! as `(slug, token)`, rendered by configuration so the scheme can flip
//! (O-014 decision 1, blocking only at the DNS rung) without touching a
//! row. `parse_recipient` accepts BOTH forms regardless of the configured
//! scheme for the same reason. Neither function logs its input.

use std::fmt;

use crate::config::{IntakeAddressScheme, IntakeMailConfig};

/// The local-part prefix of the subdomain form and the subdomain label of
/// the local-part form.
const LEADS: &str = "leads";
/// Tokens match the DB CHECK alphabet (`[a-z0-9]{8}`), which is wider
/// than the mint alphabet (`[a-z2-7]`) so backfilled hex tokens resolve.
const TOKEN_LEN: usize = 8;
const SLUG_MAX_LEN: usize = 40;

/// The Organization's intake-address anti-forgery secret (hardening chunk
/// S2 — the one flagged-for-sign-off item,
/// docs/design/type-safety-hardening.md "Flagged for sign-off" #2): a
/// tenant credential (`organization.intake_token`), so — unlike
/// `NormalizedEmail`/`NormalizedPhone` (`domain/contact.rs`), which are
/// PII but not secrets — this type goes further than a redacted `Debug`:
///
/// - no `Display`, so it can never flow through a stray `format!`/
///   `{token}` interpolation the way `NormalizedEmail` deliberately
///   allows as its "escape hatch";
/// - no `PartialEq`/`Eq`, so `token_a == token_b` is a compile error —
///   [`verify`](IntakeToken::verify) (constant-time) is the only way to
///   compare one, closing off any future accidental non-constant-time
///   compare of a tenant credential;
/// - `Debug` is redacted, mirroring the config-secret newtypes
///   (`RawPayloadKey` etc., `crm-app/src/config.rs`).
///
/// [`reveal`](IntakeToken::reveal) is the one general accessor (SQL
/// binds, the mint/rotate boundary, `IntakeAddress::render`'s
/// interpolation) — a deliberate, named "you are extracting the secret"
/// call, not a restricted API only two sites may use. Of its call sites,
/// exactly two put the value into an HTTP response: `intake_address` and
/// `rotate_intake_address` in `crm-api/src/routes/organization.rs` (both
/// via `IntakeAddress::render`, never directly).
#[derive(Clone)]
pub struct IntakeToken(String);

impl IntakeToken {
    /// Wraps an already-token-shaped `String` — minted
    /// (`admin::validation::mint_intake_token`), read back from an
    /// `organization.intake_token` row, or parsed from a presented
    /// recipient address (below, gated by `is_token` first). No
    /// validation here — the same trivial-wrap shape as the id newtypes
    /// (`OrganizationId::new` etc.), not `NormalizedEmail`'s validating
    /// parse: by the time this is called, the value's format is already
    /// established by its caller.
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// The one general accessor for the raw secret. See the type's own
    /// doc for exactly which call sites use it and why each is
    /// legitimate. Never store or log the result; never format it beyond
    /// an immediate, deliberate use.
    pub fn reveal(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison — the ONLY equality this type offers (no
    /// `PartialEq`). Wraps the same byte-XOR-accumulate algorithm
    /// `receive.rs`'s dummy-token branch also uses, kept as an
    /// independent copy there rather than imported: this codebase's
    /// established pattern for this exact primitive is a small local
    /// copy per call site (`routes/inbound_email.rs`,
    /// `telephony/webhook.rs`, `domain/intake/receive.rs`), so the
    /// dummy-token timing equalizer stays byte-for-byte untouched by
    /// this chunk rather than being rewired through this method.
    pub fn verify(&self, candidate: &[u8]) -> bool {
        constant_time_eq(self.0.as_bytes(), candidate)
    }
}

impl fmt::Debug for IntakeToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IntakeToken(REDACTED)")
    }
}

/// Same constant-time byte comparison as `receive.rs`'s local copy — see
/// [`IntakeToken::verify`]'s doc for why this is an independent copy
/// rather than a shared import.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

pub struct IntakeAddress {
    pub slug: String,
    pub token: IntakeToken,
}

impl IntakeAddress {
    /// Subdomain: `leads-<token>@<slug>.<domain>`;
    /// LocalPart: `<slug>-<token>@leads.<domain>`.
    pub fn render(&self, cfg: &IntakeMailConfig) -> String {
        match cfg.scheme {
            IntakeAddressScheme::Subdomain => {
                format!(
                    "{LEADS}-{}@{}.{}",
                    self.token.reveal(),
                    self.slug,
                    cfg.domain
                )
            }
            IntakeAddressScheme::LocalPart => {
                format!(
                    "{}-{}@{LEADS}.{}",
                    self.slug,
                    self.token.reveal(),
                    cfg.domain
                )
            }
        }
    }

    /// Case-insensitive; a bare address only (no display name, no `+tag`,
    /// no extra labels, exact configured domain). `None` on anything else.
    pub fn parse_recipient(addr: &str, cfg: &IntakeMailConfig) -> Option<IntakeAddress> {
        let addr = addr.trim().to_ascii_lowercase();
        let (local, host) = addr.split_once('@')?;
        if local.is_empty() || host.is_empty() {
            return None;
        }
        let domain = cfg.domain.to_ascii_lowercase();
        let sub = host.strip_suffix(&domain)?.strip_suffix('.')?;
        if sub.is_empty() || sub.contains('.') {
            return None;
        }
        // Subdomain form: local = leads-<token>, sub = <slug>.
        if let Some(token) = local.strip_prefix(&format!("{LEADS}-")) {
            if is_token(token) && is_slug(sub) {
                return Some(IntakeAddress {
                    slug: sub.to_string(),
                    token: IntakeToken::new(token.to_string()),
                });
            }
        }
        // Local-part form: local = <slug>-<token>, sub = leads.
        if sub == LEADS {
            let (slug, token) = local.rsplit_once('-')?;
            if is_token(token) && is_slug(slug) {
                return Some(IntakeAddress {
                    slug: slug.to_string(),
                    token: IntakeToken::new(token.to_string()),
                });
            }
        }
        None
    }
}

fn is_token(s: &str) -> bool {
    s.len() == TOKEN_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// The DB CHECK: `^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$`.
pub fn is_slug(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > SLUG_MAX_LEN {
        return false;
    }
    let ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    ok(bytes[0]) && ok(bytes[bytes.len() - 1]) && bytes.iter().all(|&b| ok(b) || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(scheme: IntakeAddressScheme) -> IntakeMailConfig {
        IntakeMailConfig {
            domain: "elysianfeld.com".to_string(),
            scheme,
        }
    }

    fn addr() -> IntakeAddress {
        IntakeAddress {
            slug: "cypress-bay-realty".into(),
            token: IntakeToken::new("k7f3q2wd".to_string()),
        }
    }

    /// `IntakeAddress` has no `PartialEq` (hardening chunk S2): `token` is
    /// an `IntakeToken`, which deliberately offers no equality besides
    /// constant-time `verify` — see the type's own doc. Test-only
    /// comparison by (slug, revealed token) instead, so every assertion
    /// this module made before this chunk still checks exactly the same
    /// two fields.
    fn assert_parsed(actual: Option<IntakeAddress>, expected: Option<(&str, &str)>) {
        match (actual, expected) {
            (Some(a), Some((slug, token))) => {
                assert_eq!(a.slug, slug);
                assert_eq!(a.token.reveal(), token);
            }
            (None, None) => {}
            (a, e) => panic!(
                "parsed mismatch: got {:?}, expected {:?}",
                a.map(|x| x.slug),
                e
            ),
        }
    }

    #[test]
    fn renders_both_schemes() {
        assert_eq!(
            addr().render(&cfg(IntakeAddressScheme::Subdomain)),
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com"
        );
        assert_eq!(
            addr().render(&cfg(IntakeAddressScheme::LocalPart)),
            "cypress-bay-realty-k7f3q2wd@leads.elysianfeld.com"
        );
    }

    #[test]
    fn parses_both_forms_regardless_of_configured_scheme() {
        for scheme in [
            IntakeAddressScheme::Subdomain,
            IntakeAddressScheme::LocalPart,
        ] {
            let c = cfg(scheme);
            assert_parsed(
                IntakeAddress::parse_recipient(
                    "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com",
                    &c,
                ),
                Some(("cypress-bay-realty", "k7f3q2wd")),
            );
            assert_parsed(
                IntakeAddress::parse_recipient(
                    "cypress-bay-realty-k7f3q2wd@leads.elysianfeld.com",
                    &c,
                ),
                Some(("cypress-bay-realty", "k7f3q2wd")),
            );
        }
    }

    #[test]
    fn accepts_uppercase_whitespace_and_hex_tokens() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        assert_parsed(
            IntakeAddress::parse_recipient(
                "  LEADS-K7F3Q2WD@Cypress-Bay-Realty.ElysianFeld.com ",
                &c,
            ),
            Some(("cypress-bay-realty", "k7f3q2wd")),
        );
        // Backfilled orgs carry md5-hex tokens: in the CHECK alphabet.
        assert!(
            IntakeAddress::parse_recipient("leads-9f86d081@org-12345678.elysianfeld.com", &c)
                .is_some()
        );
    }

    #[test]
    fn rejects_wrong_domain_extra_labels_tags_and_malformed_parts() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        for bad in [
            "leads-k7f3q2wd@evil.com",
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com.evil.com",
            "leads-k7f3q2wd@a.cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@elysianfeld.com",
            "leads-k7f3q2wd+tag@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2w@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wdx@cypress-bay-realty.elysianfeld.com",
            "leads-K7F3-2WD@cypress-bay-realty.elysianfeld.com",
            "Grace <leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com>",
            "cypress-bay-realty-k7f3q2wd@leads.evil.com",
            "-bad-k7f3q2wd@leads.elysianfeld.com",
            "@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@",
            "",
            "leads-k7f3q2wd@x@cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@cypress-bay-realty.elysianfeld.com.",
            "leads-k7f3q2wd @cypress-bay-realty.elysianfeld.com",
            "leads-k7f3q2wd@cypress-bay-realty.xelysianfeld.com",
        ] {
            assert!(IntakeAddress::parse_recipient(bad, &c).is_none(), "{bad}");
        }
    }

    #[test]
    fn an_org_whose_slug_is_leads_is_unambiguous_across_both_forms() {
        let c = cfg(IntakeAddressScheme::Subdomain);
        assert_parsed(
            IntakeAddress::parse_recipient("leads-abcdefgh@leads.elysianfeld.com", &c),
            Some(("leads", "abcdefgh")),
        );
    }

    // --- IntakeToken (hardening chunk S2) ---------------------------------

    #[test]
    fn intake_token_debug_is_redacted() {
        let token = IntakeToken::new("k7f3q2wd".to_string());
        let debug = format!("{token:?}");
        assert_eq!(debug, "IntakeToken(REDACTED)");
        assert!(!debug.contains("k7f3q2wd"));
    }

    #[test]
    fn intake_token_verify_matches_identical_and_rejects_length_mismatch_or_wrong_bytes() {
        let token = IntakeToken::new("k7f3q2wd".to_string());
        assert!(token.verify(b"k7f3q2wd"));
        assert!(!token.verify(b"k7f3q2we"));
        assert!(!token.verify(b"short"));
        assert!(!token.verify(b""));
        // Single-bit-flip near-misses: every position must be checked, not
        // just short-circuited on the first differing byte.
        for i in 0..8 {
            let mut near_miss = *b"k7f3q2wd";
            near_miss[i] ^= 0x01;
            assert!(!token.verify(&near_miss), "byte {i}");
        }
    }

    #[test]
    fn slug_check_mirrors_the_db_constraint() {
        assert!(is_slug("a"));
        assert!(is_slug("acme-realty"));
        assert!(is_slug(&"a".repeat(40)));
        assert!(!is_slug(&"a".repeat(41)));
        assert!(!is_slug("-acme"));
        assert!(!is_slug("acme-"));
        assert!(!is_slug("Acme"));
        assert!(!is_slug("acme realty"));
        assert!(!is_slug(""));
    }
}
