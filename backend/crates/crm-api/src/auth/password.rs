use std::sync::LazyLock;

// password-hash pins its own rand_core version; use its re-export rather
// than this crate's rand (a newer, incompatible major version) to satisfy
// SaltString::generate's trait bound.
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

/// A real, valid Argon2id hash with no meaningful plaintext, computed once
/// at first use. Verifying against it whenever no `local_credential` row
/// exists keeps failure timing indistinguishable from a real wrong-password
/// check (docs/specs/SLICE_001.md §3).
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("dummy-password-for-timing-safety").expect("dummy hash must succeed")
});

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Runs a real Argon2id verification against a fixed dummy hash and
/// discards the result. Callers use this on the "no such credential" path
/// so failure timing does not reveal whether the account exists.
pub fn verify_dummy_password(candidate: &str) {
    let _ = verify_password(candidate, &DUMMY_HASH);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_hash_and_verify() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn hash_is_argon2id_phc_string() {
        let hash = hash_password("whatever").unwrap();
        assert!(
            hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "unexpected hash format: {hash}"
        );
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(!verify_password("anything", "not-a-valid-phc-string"));
    }

    #[test]
    fn dummy_verification_does_not_panic() {
        verify_dummy_password("anything at all");
    }
}
