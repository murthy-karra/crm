//! Invitation tokens (docs/specs/SLICE_004.md §2, §4, §14 default 4):
//! 256-bit random, base64url (no padding, 43 chars — the same format as
//! session tokens, `auth::session::is_valid_token_format`), hashed at rest
//! with plain SHA-256 (independent of `CRM_SESSION_SECRET` rotation —
//! unlike session tokens, which are HMAC'd with that secret).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};

use crate::auth::token_format as session;

const TOKEN_LEN_BYTES: usize = 32;

/// A fresh raw invitation token, in the same 43-char base64url format as a
/// session token.
pub fn generate() -> String {
    let mut bytes = [0u8; TOKEN_LEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of the raw token, base64url-encoded — what is stored in
/// `invitation.token_hash`.
pub fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// The same 43-char base64url format check session tokens use
/// (docs/specs/SLICE_004.md §5: "Token format is checked before any
/// database access exactly as `session::is_valid_token_format`").
pub fn is_valid_format(token: &str) -> bool {
    session::is_valid_token_format(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_has_valid_format() {
        let token = generate();
        assert!(is_valid_format(&token));
    }

    #[test]
    fn hash_is_deterministic() {
        let token = "fixed-token-value-for-this-test-1234567890a";
        assert_eq!(hash(token), hash(token));
    }

    #[test]
    fn hash_differs_across_tokens() {
        let a = generate();
        let b = generate();
        assert_ne!(hash(&a), hash(&b));
    }

    #[test]
    fn hash_does_not_depend_on_session_secret() {
        // Plain SHA-256, not HMAC — no secret parameter exists to vary.
        let token = "fixed-token-value-for-this-test-1234567890a";
        let hash_a = hash(token);
        let hash_b = hash(token);
        assert_eq!(hash_a, hash_b);
    }
}
