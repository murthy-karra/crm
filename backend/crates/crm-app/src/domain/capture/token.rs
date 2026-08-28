//! The per-agent capture address (docs/specs/SLICE_009.md §3): `save-
//! <token12>@leads.<domain>`. `CaptureToken` mirrors `IntakeToken`
//! (`domain/intake/address.rs`) in every hardening respect — no `Display`/
//! `PartialEq`, redacted `Debug`, constant-time `verify` — but the lookup
//! path is stronger: intake's B-tree probe keys on the PUBLIC slug, never
//! token bytes, so a non-constant-time index scan there leaks nothing.
//! Capture's address IS the token — there is no separate public label to
//! index on — so this module also exposes `token_lookup_digest`, an
//! UNKEYED SHA-256 of the token: the DB indexes and probes THAT digest
//! (`capture_address.token_lookup`), never the token's own bytes, which
//! restores intake-grade lookup-timing uniformity. Unkeyed suffices
//! because the property being protected is "no B-tree-timing information
//! about the token's own byte prefixes", not preimage resistance in a
//! keyed-MAC sense (docs/specs/SLICE_009.md §3, reviewer finding).

use std::fmt;

use sha2::{Digest, Sha256};

use crate::config::IntakeMailConfig;

/// 12 chars (docs/specs/SLICE_009.md §3): thread-visible addresses warrant
/// more entropy than intake's 8-char token (~60 bits vs ~40).
pub const CAPTURE_TOKEN_LEN: usize = 12;
/// The SAME hyphen-free mint alphabet as intake's (`[a-z2-7]`,
/// `domain/admin/validation.rs::mint_intake_token`) — hyphen-freedom is
/// STRUCTURAL to the grammar-disjointness proof (see `parse_recipient`'s
/// doc): every character of the 12-char token segment is validated
/// against this alphabet, not just the segment's length, so a hyphenated
/// intake org slug like `save-abc` can never be misread as a capture
/// token (the length-only check that WOULD collide — spec §3 reviewer
/// finding).
const MINT_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
/// The fixed local-part prefix (cosmetic; disjointness is by token length
/// and alphabet, not this string — spec §13 safe default (a)).
const SAVE_PREFIX: &str = "save";
/// The fixed subdomain label — the SAME `leads.<domain>` intake's
/// LocalPart scheme uses, independent of the Organization's configured
/// `IntakeAddressScheme` (capture has exactly one grammar, always).
const LEADS: &str = "leads";

/// The capture address token: a per-agent secret credential (the address
/// itself is the credential — see the module doc). Same hardening shape
/// as `IntakeToken` (`domain/intake/address.rs`), whose own doc explains
/// each choice in full; not repeated here.
#[derive(Clone)]
pub struct CaptureToken(String);

impl CaptureToken {
    /// Wraps an already-token-shaped `String` — minted, read back from a
    /// `capture_address.token` row, or parsed from a presented recipient
    /// (gated by `is_capture_token` first). No validation here, mirroring
    /// `IntakeToken::new`.
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// The one general accessor for the raw secret — SQL binds, the
    /// mint/rotate boundary, `render`'s interpolation, and exactly the two
    /// HTTP-response sites in `routes/capture.rs` (mirrors
    /// `IntakeToken::reveal`'s doc in full).
    pub fn reveal(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison — the ONLY equality this type offers.
    pub fn verify(&self, candidate: &[u8]) -> bool {
        constant_time_eq(self.0.as_bytes(), candidate)
    }

    /// `save-<token>@leads.<domain>` (spec §3). Independent of the
    /// Organization's `IntakeAddressScheme` — capture has one grammar,
    /// always local-part form on the `leads.` subdomain.
    pub fn render(&self, cfg: &IntakeMailConfig) -> String {
        format!("{SAVE_PREFIX}-{}@{LEADS}.{}", self.0, cfg.domain)
    }

    /// Parses ONLY the address grammar — no DB lookup (mirrors
    /// `IntakeAddress::parse_recipient`; organization/agent resolution is
    /// a separate DB step, `domain/capture/address.rs::resolve`).
    ///
    /// STRUCTURAL disjointness from intake (spec §3): intake's local-part
    /// form is `<slug>-<8-char-token>@leads.<domain>`
    /// (`domain/intake/address.rs`'s `TOKEN_LEN = 8`); this requires
    /// EXACTLY `save-` + a 12-char token from `MINT_ALPHABET`. Validating
    /// every character of the 12-char segment (not just its length) is
    /// required for the proof: a length-only check would wrongly accept
    /// an intake address for the hyphenated org slug `save-abc` — local
    /// part `save-abc-k7f3q2wd`, whose 12-char remainder after stripping
    /// `save-` (`abc-k7f3q2wd`) is exactly 12 bytes but contains a hyphen,
    /// which `MINT_ALPHABET` excludes — pinned by the disjointness tests
    /// in both directions.
    pub fn parse_recipient(addr: &str, cfg: &IntakeMailConfig) -> Option<CaptureToken> {
        let addr = addr.trim().to_ascii_lowercase();
        let (local, host) = addr.split_once('@')?;
        if local.is_empty() {
            return None;
        }
        let domain = cfg.domain.to_ascii_lowercase();
        let sub = host.strip_suffix(&domain)?.strip_suffix('.')?;
        if sub != LEADS {
            return None;
        }
        let token_part = local.strip_prefix(&format!("{SAVE_PREFIX}-"))?;
        if is_capture_token(token_part) {
            Some(CaptureToken::new(token_part.to_string()))
        } else {
            None
        }
    }
}

impl fmt::Debug for CaptureToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CaptureToken(REDACTED)")
    }
}

fn is_capture_token(s: &str) -> bool {
    s.len() == CAPTURE_TOKEN_LEN && s.bytes().all(|b| MINT_ALPHABET.contains(&b))
}

/// Mints a fresh random token from `MINT_ALPHABET`. Collision handling
/// (both the global `token_lookup` UNIQUE and the self-collision-with-old
/// check on rotation) lives with each caller
/// (`domain/capture/address.rs`), mirroring `mint_intake_token`'s split.
pub fn mint_capture_token() -> CaptureToken {
    use rand::RngExt;
    let mut rng = rand::rng();
    let token: String = (0..CAPTURE_TOKEN_LEN)
        .map(|_| MINT_ALPHABET[rng.random_range(0..MINT_ALPHABET.len())] as char)
        .collect();
    CaptureToken::new(token)
}

/// The deterministic, UNKEYED SHA-256 digest of a token's bytes — the
/// `capture_address.token_lookup` lookup key (see module doc for why
/// unkeyed suffices here). Not a `CaptureToken` method: it operates on
/// both freshly-minted tokens (mint/rotate) and presented ones (the
/// receive-path lookup), and keeping it a free function makes both call
/// sites equally visible.
pub fn token_lookup_digest(token: &CaptureToken) -> Vec<u8> {
    Sha256::digest(token.0.as_bytes()).to_vec()
}

/// Never `==` on secrets (repo-wide rule). An independent copy, per this
/// codebase's established per-call-site pattern for this exact primitive
/// (`domain/intake/address.rs`'s own doc explains the precedent in full).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> IntakeMailConfig {
        IntakeMailConfig {
            domain: "elysianfeld.com".to_string(),
            scheme: crate::config::IntakeAddressScheme::LocalPart,
        }
    }

    #[test]
    fn renders_the_save_prefixed_address() {
        let token = CaptureToken::new("abcdefghijkl".to_string());
        assert_eq!(
            token.render(&cfg()),
            "save-abcdefghijkl@leads.elysianfeld.com"
        );
    }

    #[test]
    fn parses_a_well_formed_capture_address_case_and_whitespace_insensitively() {
        let c = cfg();
        let token = CaptureToken::parse_recipient("  SAVE-Abcdefghijkl@Leads.ElysianFeld.com ", &c)
            .expect("parses");
        assert!(token.verify(b"abcdefghijkl"));
    }

    #[test]
    fn mint_produces_a_token_of_the_right_length_and_alphabet() {
        for _ in 0..50 {
            let token = mint_capture_token();
            assert_eq!(token.reveal().len(), CAPTURE_TOKEN_LEN);
            assert!(token.reveal().bytes().all(|b| MINT_ALPHABET.contains(&b)));
        }
    }

    #[test]
    fn token_lookup_digest_is_deterministic_and_depends_on_the_token() {
        let a = CaptureToken::new("abcdefghijkl".to_string());
        let b = CaptureToken::new("abcdefghijkl".to_string());
        let c = CaptureToken::new("mnopqrstuvwx".to_string());
        assert_eq!(token_lookup_digest(&a), token_lookup_digest(&b));
        assert_ne!(token_lookup_digest(&a), token_lookup_digest(&c));
        assert_eq!(token_lookup_digest(&a).len(), 32);
    }

    // --- Grammar disjointness (spec §3, §6 criterion) -------------------

    #[test]
    fn rejects_every_shape_of_intake_local_part_address() {
        let c = cfg();
        // A normal intake local-part address (8-char token): the 12-char
        // check alone already rejects it (length mismatch after
        // stripping "save-").
        assert!(CaptureToken::parse_recipient(
            "cypress-bay-realty-k7f3q2wd@leads.elysianfeld.com",
            &c
        )
        .is_none());
        // The hyphenated-`save-*`-slug edge (spec §3 reviewer finding): an
        // intake org slug literally "save-abc" with an 8-char token
        // produces a local part whose post-"save-" remainder
        // ("abc-k7f3q2wd") is EXACTLY 12 bytes — a length-only check
        // would wrongly accept this as a capture token. The per-character
        // alphabet check (no hyphens in MINT_ALPHABET) rejects it.
        assert!(
            CaptureToken::parse_recipient("save-abc-k7f3q2wd@leads.elysianfeld.com", &c).is_none(),
            "hyphenated save-* slug edge must not parse as a capture address"
        );
        // An intake org literally named "save" (8-char token): the
        // post-"save-" remainder is 8 bytes, not 12.
        assert!(CaptureToken::parse_recipient("save-k7f3q2wd@leads.elysianfeld.com", &c).is_none());
    }

    #[test]
    fn a_capture_address_never_parses_as_an_intake_local_part_address() {
        use crate::domain::intake::IntakeAddress;
        let c = cfg();
        let token = mint_capture_token();
        let addr = token.render(&c);
        // Capture's own parser accepts it...
        assert!(CaptureToken::parse_recipient(&addr, &c).is_some());
        // ...but intake's independent parser must not (org slug "save",
        // and intake's TOKEN_LEN=8 rejects the 12-char remainder).
        assert!(IntakeAddress::parse_recipient(&addr, &c).is_none());
    }

    #[test]
    fn rejects_wrong_domain_wrong_subdomain_and_malformed_shapes() {
        let c = cfg();
        for bad in [
            "save-abcdefghijkl@leads.evil.com",
            "save-abcdefghijkl@elysianfeld.com",
            "save-abcdefghijkl@sub.leads.elysianfeld.com",
            "notsave-abcdefghijkl@leads.elysianfeld.com",
            "save-abcdefghij@leads.elysianfeld.com", // 10 chars, too short
            "save-abcdefghijklm@leads.elysianfeld.com", // 13 chars, too long
            "save-ABCDEFGHIJK1@leads.elysianfeld.com", // digit 1/uppercase outside alphabet
            "save-@leads.elysianfeld.com",
            "@leads.elysianfeld.com",
            "",
            "save-abcdefghijkl@leads.elysianfeld.com.evil.com",
        ] {
            assert!(CaptureToken::parse_recipient(bad, &c).is_none(), "{bad}");
        }
    }

    #[test]
    fn debug_is_redacted() {
        let token = CaptureToken::new("abcdefghijkl".to_string());
        let debug = format!("{token:?}");
        assert_eq!(debug, "CaptureToken(REDACTED)");
        assert!(!debug.contains("abcdefghijkl"));
    }

    #[test]
    fn verify_matches_identical_and_rejects_length_mismatch_or_wrong_bytes() {
        let token = CaptureToken::new("abcdefghijkl".to_string());
        assert!(token.verify(b"abcdefghijkl"));
        assert!(!token.verify(b"abcdefghijkm"));
        assert!(!token.verify(b"short"));
        assert!(!token.verify(b""));
        for i in 0..CAPTURE_TOKEN_LEN {
            let mut near_miss = *b"abcdefghijkl";
            near_miss[i] ^= 0x01;
            assert!(!token.verify(&near_miss), "byte {i}");
        }
    }
}
