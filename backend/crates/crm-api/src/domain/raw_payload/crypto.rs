//! Raw-payload encryption and the keyed content hash
//! (docs/specs/SLICE_002.md §7). Never logs plaintext, ciphertext, the key,
//! or the content hash (spec §8).

use chacha20poly1305::aead::{Aead, Payload};
// chacha20poly1305 (via the `aead`/`chacha20` crate family) pulls a
// different `crypto-common` generation than this crate's own `hmac`/`sha2`
// (0.13/0.11, as auth::session already uses) — both `KeyInit` traits are
// needed here (one per cipher), so one is aliased to avoid a name clash.
use chacha20poly1305::KeyInit as AeadKeyInit;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use sha2::Sha256;
use uuid::Uuid;

use crate::config::RawPayloadKey;

type HmacSha256 = Hmac<Sha256>;

const NONCE_LEN: usize = 24;
const HASH_CONTEXT: &[u8] = b"crm-raw-payload-content-hash-v1";

pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

/// Opaque on purpose: never carries the plaintext, key material, or a
/// description of *why* decryption failed (spec §8 — logged with ids only).
#[derive(Debug)]
pub struct CryptoError;

/// `organization_id ‖ raw_payload.id` — binds a ciphertext to the exact row
/// and Organization it was sealed for, so it cannot be re-pointed to
/// another row (docs/specs/SLICE_002.md §7).
fn associated_data(organization_id: Uuid, raw_payload_id: Uuid) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(organization_id.as_bytes());
    out[16..].copy_from_slice(raw_payload_id.as_bytes());
    out
}

fn cipher(key: &RawPayloadKey) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.as_bytes().into())
}

/// Seals `plaintext` with a fresh random 24-byte nonce (safe without
/// counters or rotation at this volume — docs/specs/SLICE_002.md §7).
pub fn seal(
    key: &RawPayloadKey,
    organization_id: Uuid,
    raw_payload_id: Uuid,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let aad = associated_data(organization_id, raw_payload_id);

    let ciphertext = cipher(key)
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| CryptoError)?;

    Ok(Sealed {
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Opens a stored `(nonce, ciphertext)` pair. Fails on a wrong key, a
/// tampered ciphertext, or an AAD mismatch (row re-pointed) — all
/// indistinguishable to the caller by design.
pub fn open(
    key: &RawPayloadKey,
    organization_id: Uuid,
    raw_payload_id: Uuid,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if nonce.len() != NONCE_LEN {
        return Err(CryptoError);
    }
    let nonce = XNonce::from_slice(nonce);
    let aad = associated_data(organization_id, raw_payload_id);

    cipher(key)
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| CryptoError)
}

/// `content_hmac = HMAC-SHA256(k_hash, plaintext)`, `k_hash =
/// HMAC-SHA256(CRM_RAW_PAYLOAD_KEY, "crm-raw-payload-content-hash-v1")`
/// (docs/specs/SLICE_002.md §7). Deterministic (the idempotency unique key
/// depends on it) and key-dependent (a plain hash of small, guessable
/// content would let anyone holding the history table confirm a candidate
/// email after erasure).
pub fn content_hmac(key: &RawPayloadKey, plaintext: &[u8]) -> [u8; 32] {
    let mut derive =
        HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    derive.update(HASH_CONTEXT);
    let k_hash = derive.finalize().into_bytes();

    let mut mac = HmacSha256::new_from_slice(&k_hash).expect("HMAC accepts any key length");
    mac.update(plaintext);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(byte: u8) -> RawPayloadKey {
        // `RawPayloadKey::new` carries the 32-byte invariant in its type
        // (docs/specs/SLICE_006a.md §4).
        RawPayloadKey::new([byte; 32])
    }

    #[test]
    fn seal_open_roundtrip() {
        let key = test_key(0x11);
        let org_id = Uuid::new_v4();
        let payload_id = Uuid::new_v4();
        let plaintext = b"{\"email\":\"ada@example.com\"}";

        let sealed = seal(&key, org_id, payload_id, plaintext).unwrap();
        let opened = open(&key, org_id, payload_id, &sealed.nonce, &sealed.ciphertext).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn tampered_ciphertext_fails_to_open() {
        let key = test_key(0x22);
        let org_id = Uuid::new_v4();
        let payload_id = Uuid::new_v4();
        let mut sealed = seal(&key, org_id, payload_id, b"hello world").unwrap();
        let last = sealed.ciphertext.len() - 1;
        sealed.ciphertext[last] ^= 0xFF;

        let result = open(&key, org_id, payload_id, &sealed.nonce, &sealed.ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_key_fails_to_open() {
        let key_a = test_key(0x33);
        let key_b = test_key(0x44);
        let org_id = Uuid::new_v4();
        let payload_id = Uuid::new_v4();
        let sealed = seal(&key_a, org_id, payload_id, b"hello world").unwrap();

        let result = open(
            &key_b,
            org_id,
            payload_id,
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert!(result.is_err());
    }

    #[test]
    fn wrong_aad_fails_to_open() {
        let key = test_key(0x55);
        let org_id = Uuid::new_v4();
        let payload_id = Uuid::new_v4();
        let other_payload_id = Uuid::new_v4();
        let sealed = seal(&key, org_id, payload_id, b"hello world").unwrap();

        // Same nonce/ciphertext, but opened under a different raw_payload
        // id: the AAD mismatch must fail, proving a ciphertext cannot be
        // re-pointed to another row (docs/specs/SLICE_002.md §7).
        let result = open(
            &key,
            org_id,
            other_payload_id,
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert!(result.is_err());

        let result = open(
            &key,
            Uuid::new_v4(),
            payload_id,
            &sealed.nonce,
            &sealed.ciphertext,
        );
        assert!(result.is_err());
    }

    #[test]
    fn distinct_calls_use_distinct_nonces() {
        let key = test_key(0x66);
        let org_id = Uuid::new_v4();
        let payload_id = Uuid::new_v4();
        let first = seal(&key, org_id, payload_id, b"hello world").unwrap();
        let second = seal(&key, org_id, payload_id, b"hello world").unwrap();
        assert_ne!(first.nonce, second.nonce);
        // Same plaintext, same key, different nonce -> different ciphertext.
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn content_hmac_is_deterministic() {
        let key = test_key(0x77);
        let plaintext = b"same content";
        assert_eq!(content_hmac(&key, plaintext), content_hmac(&key, plaintext));
    }

    #[test]
    fn content_hmac_depends_on_the_key() {
        let key_a = test_key(0x88);
        let key_b = test_key(0x99);
        let plaintext = b"same content";
        assert_ne!(
            content_hmac(&key_a, plaintext),
            content_hmac(&key_b, plaintext)
        );
    }

    #[test]
    fn content_hmac_depends_on_the_content() {
        let key = test_key(0xaa);
        assert_ne!(
            content_hmac(&key, b"content one"),
            content_hmac(&key, b"content two")
        );
    }

    /// Guards the hashing contract (docs/specs/SLICE_002.md §7): plaintext
    /// bytes are `serde_json::to_vec(&payload)`, which is canonical (and
    /// therefore a stable idempotency key) only as long as `serde_json::Map`
    /// stays a `BTreeMap` (key-sorted) rather than an `IndexMap`
    /// (insertion-order) — i.e. the `preserve_order` feature stays off. This
    /// test fails the moment that stops being true, regardless of *why*.
    #[test]
    fn serde_json_value_serializes_object_keys_in_sorted_order() {
        let value = serde_json::json!({
            "zebra": 1,
            "apple": 2,
            "mango": 3,
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text, r#"{"apple":2,"mango":3,"zebra":1}"#);
    }
}
