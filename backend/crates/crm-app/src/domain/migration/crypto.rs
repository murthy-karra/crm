//! Separate AEAD/HMAC purposes for credentials and bounded probe evidence.
//! Both use the existing development key, but cannot cross-decrypt or be
//! rebound to another tenant, row, revision, or purpose.
use crate::config::RawPayloadKey;
use crate::ids::OrganizationId;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit as AeadKeyInit, XChaCha20Poly1305, XNonce};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub struct CryptoError;
const NONCE_LEN: usize = 24;
pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

fn aad(purpose: &[u8], org: OrganizationId, row: uuid::Uuid, revision: i32) -> Vec<u8> {
    let mut value = Vec::with_capacity(purpose.len() + 1 + 16 + 16 + 4);
    value.extend_from_slice(purpose);
    value.push(0);
    value.extend_from_slice(org.as_uuid().as_bytes());
    value.extend_from_slice(row.as_bytes());
    value.extend_from_slice(&revision.to_be_bytes());
    value
}
fn seal_purpose(
    key: &RawPayloadKey,
    purpose: &[u8],
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    let mut nonce = [0; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad(purpose, org, row, revision),
            },
        )
        .map_err(|_| CryptoError)?;
    Ok(Sealed { nonce, ciphertext })
}
fn open_purpose(
    key: &RawPayloadKey,
    purpose: &[u8],
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if nonce.len() != NONCE_LEN {
        return Err(CryptoError);
    }
    XChaCha20Poly1305::new(key.as_bytes().into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad(purpose, org, row, revision),
            },
        )
        .map_err(|_| CryptoError)
}
pub fn seal_credential(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    seal_purpose(
        key,
        b"crm-migration-fub-credential-v1",
        org,
        row,
        revision,
        plaintext,
    )
}
pub fn open_credential(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open_purpose(
        key,
        b"crm-migration-fub-credential-v1",
        org,
        row,
        revision,
        nonce,
        ciphertext,
    )
}
pub fn seal_evidence(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    seal_purpose(
        key,
        b"crm-migration-fub-evidence-v1",
        org,
        row,
        1,
        plaintext,
    )
}
pub fn seal_identity(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    seal_purpose(
        key,
        b"crm-migration-fub-identity-v1",
        org,
        row,
        revision,
        plaintext,
    )
}
pub fn open_identity(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    revision: i32,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open_purpose(
        key,
        b"crm-migration-fub-identity-v1",
        org,
        row,
        revision,
        nonce,
        ciphertext,
    )
}
pub fn open_evidence(
    key: &RawPayloadKey,
    org: OrganizationId,
    row: uuid::Uuid,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open_purpose(
        key,
        b"crm-migration-fub-evidence-v1",
        org,
        row,
        1,
        nonce,
        ciphertext,
    )
}
pub fn content_hmac(key: &RawPayloadKey, plaintext: &[u8]) -> [u8; 32] {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("valid key");
    m.update(b"crm-migration-fub-evidence-hash-v1");
    m.update(plaintext);
    m.finalize().into_bytes().into()
}

/// Request idempotency digest. The caller supplies an operation-specific
/// canonical byte sequence; credentials are HMAC input only and never enter
/// the receipt row or response JSON.
pub fn request_digest(key: &RawPayloadKey, operation: &str, input: &[u8]) -> [u8; 32] {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("valid key");
    m.update(b"crm-migration-fub-request-v1\0");
    m.update(operation.as_bytes());
    m.update(b"\0");
    m.update(input);
    m.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn purpose_and_aad_are_bound() {
        let key = RawPayloadKey::new([3; 32]);
        let org = OrganizationId::new(uuid::Uuid::new_v4());
        let row = uuid::Uuid::new_v4();
        let sealed = seal_credential(&key, org, row, 1, b"secret").unwrap();
        assert_eq!(
            open_credential(&key, org, row, 1, &sealed.nonce, &sealed.ciphertext).unwrap(),
            b"secret"
        );
        assert!(open_credential(&key, org, row, 2, &sealed.nonce, &sealed.ciphertext).is_err());
        assert!(open_purpose(
            &key,
            b"crm-migration-fub-evidence-v1",
            org,
            row,
            1,
            &sealed.nonce,
            &sealed.ciphertext
        )
        .is_err());
    }
}

pub fn seal_receipt(
    key: &RawPayloadKey,
    org: OrganizationId,
    request: uuid::Uuid,
    operation: &str,
    bytes: &[u8],
) -> Result<Sealed, CryptoError> {
    seal_purpose(
        key,
        format!("crm-migration-fub-receipt-v1:{operation}").as_bytes(),
        org,
        request,
        1,
        bytes,
    )
}
pub fn open_receipt(
    key: &RawPayloadKey,
    org: OrganizationId,
    request: uuid::Uuid,
    operation: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open_purpose(
        key,
        format!("crm-migration-fub-receipt-v1:{operation}").as_bytes(),
        org,
        request,
        1,
        nonce,
        ciphertext,
    )
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    #[test]
    fn tenant_row_purpose_and_tamper_are_rejected() {
        let key = RawPayloadKey::new([9; 32]);
        let org = OrganizationId::new(uuid::Uuid::new_v4());
        let other = OrganizationId::new(uuid::Uuid::new_v4());
        let row = uuid::Uuid::new_v4();
        let sealed = seal_evidence(&key, org, row, b"synthetic private evidence").unwrap();
        assert_eq!(
            open_evidence(&key, org, row, &sealed.nonce, &sealed.ciphertext).unwrap(),
            b"synthetic private evidence"
        );
        assert!(open_evidence(&key, other, row, &sealed.nonce, &sealed.ciphertext).is_err());
        assert!(open_evidence(
            &key,
            org,
            uuid::Uuid::new_v4(),
            &sealed.nonce,
            &sealed.ciphertext
        )
        .is_err());
        assert!(open_identity(&key, org, row, 1, &sealed.nonce, &sealed.ciphertext).is_err());
        assert!(open_credential(&key, org, row, 1, &sealed.nonce, &sealed.ciphertext).is_err());
        let mut bytes = sealed.ciphertext;
        bytes[0] ^= 1;
        assert!(open_evidence(&key, org, row, &sealed.nonce, &bytes).is_err());
        assert_ne!(
            request_digest(&key, "create", b"key"),
            request_digest(&key, "replace", b"key")
        );
    }
}

/// Snapshot payloads bind the Organization, snapshot, row and closed purpose.
/// Even two rows in one snapshot cannot exchange ciphertext.
pub fn seal_snapshot(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: uuid::Uuid,
    row: uuid::Uuid,
    purpose: &str,
    bytes: &[u8],
) -> Result<Sealed, CryptoError> {
    seal_purpose(
        key,
        format!("crm-fub-core-v1:{run}:{purpose}").as_bytes(),
        org,
        row,
        1,
        bytes,
    )
}
pub fn open_snapshot(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: uuid::Uuid,
    row: uuid::Uuid,
    purpose: &str,
    nonce: &[u8],
    bytes: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open_purpose(
        key,
        format!("crm-fub-core-v1:{run}:{purpose}").as_bytes(),
        org,
        row,
        1,
        nonce,
        bytes,
    )
}
pub fn snapshot_hmac(
    key: &RawPayloadKey,
    org: OrganizationId,
    purpose: &str,
    bytes: &[u8],
) -> [u8; 32] {
    request_digest(key, &format!("snapshot:{org:?}:{purpose}"), bytes)
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn snapshot_payloads_and_lookup_keys_bind_tenant_run_row_and_purpose() {
        let key = RawPayloadKey::new([7; 32]);
        let org = OrganizationId::new(uuid::Uuid::new_v4());
        let other = OrganizationId::new(uuid::Uuid::new_v4());
        let run = uuid::Uuid::new_v4();
        let row = uuid::Uuid::new_v4();
        let sealed = seal_snapshot(&key, org, run, row, "capture", b"synthetic payload").unwrap();
        assert_eq!(
            open_snapshot(
                &key,
                org,
                run,
                row,
                "capture",
                &sealed.nonce,
                &sealed.ciphertext
            )
            .unwrap(),
            b"synthetic payload"
        );
        for (o, r, id, p) in [
            (other, run, row, "capture"),
            (org, uuid::Uuid::new_v4(), row, "capture"),
            (org, run, uuid::Uuid::new_v4(), "capture"),
            (org, run, row, "record"),
        ] {
            assert!(open_snapshot(&key, o, r, id, p, &sealed.nonce, &sealed.ciphertext).is_err());
        }
        assert!(open_snapshot(
            &RawPayloadKey::new([8; 32]),
            org,
            run,
            row,
            "capture",
            &sealed.nonce,
            &sealed.ciphertext
        )
        .is_err());
        assert_ne!(
            snapshot_hmac(&key, org, "contact:email", b"synthetic@test"),
            snapshot_hmac(&key, other, "contact:email", b"synthetic@test")
        );
        assert_ne!(
            snapshot_hmac(&key, org, "contact:email", b"synthetic@test"),
            snapshot_hmac(&key, org, "semantic", b"synthetic@test")
        );
    }
}
