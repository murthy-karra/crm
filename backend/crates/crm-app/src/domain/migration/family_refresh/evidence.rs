//! Bounded encrypted evidence. Tenant, bundle, plan, row and purpose are all
//! authenticated; successful decryption never grants business authorization.
use super::model::{Family, ENGINE};
use crate::domain::migration::{crypto, MigrationError};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Purpose {
    Binding,
    Source,
    Mapping,
    Manifest,
    Result,
    HistoryDisplay,
    Receipt,
}
impl Purpose {
    fn label(self) -> &'static str {
        match self {
            Self::Binding => "binding",
            Self::Source => "source",
            Self::Mapping => "mapping",
            Self::Manifest => "manifest",
            Self::Result => "result",
            Self::HistoryDisplay => "history-display",
            Self::Receipt => "receipt",
        }
    }
    fn limit(self) -> usize {
        match self {
            Self::Binding | Self::Mapping | Self::Receipt => 64 * 1024,
            Self::Source => 4 * 1024 * 1024,
            Self::Manifest | Self::Result => 64 * 1024 * 1024,
            Self::HistoryDisplay => 4096,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Scope {
    pub organization: OrganizationId,
    pub bundle: Uuid,
    pub plan: Uuid,
    pub family: Family,
    pub revision: i64,
}
impl Scope {
    fn purpose(self, purpose: Purpose) -> Result<String, MigrationError> {
        if self.revision <= 0
            || (matches!(purpose, Purpose::HistoryDisplay) && self.family != Family::History)
        {
            return Err(MigrationError::Crypto);
        }
        Ok(format!(
            "{ENGINE}:{}:{}:{}:{}",
            self.plan,
            self.family.as_str(),
            self.revision,
            purpose.label()
        ))
    }
    pub fn seal<T: Serialize>(
        self,
        key: &RawPayloadKey,
        row: Uuid,
        purpose: Purpose,
        value: &T,
    ) -> Result<crypto::Sealed, MigrationError> {
        // Stream into a bounded buffer: a hostile retained value must not cause
        // a full unbounded serialization allocation before the size check.
        let mut buffer = BoundedBuffer {
            bytes: Vec::new(),
            limit: purpose.limit(),
        };
        serde_json::to_writer(&mut buffer, value).map_err(|_| MigrationError::StorageLimit)?;
        crypto::seal_history(
            key,
            self.organization,
            self.bundle,
            row,
            &self.purpose(purpose)?,
            &buffer.bytes,
        )
        .map_err(|_| MigrationError::Crypto)
    }
    pub fn open<T: DeserializeOwned>(
        self,
        key: &RawPayloadKey,
        row: Uuid,
        purpose: Purpose,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<T, MigrationError> {
        if nonce.len() != 24 || ciphertext.len() < 16 || ciphertext.len() > purpose.limit() + 16 {
            return Err(MigrationError::Crypto);
        }
        let bytes = crypto::open_history(
            key,
            self.organization,
            self.bundle,
            row,
            &self.purpose(purpose)?,
            nonce,
            ciphertext,
        )
        .map_err(|_| MigrationError::Crypto)?;
        serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)
    }
}
struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("family evidence limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn scope() -> Scope {
        Scope {
            organization: OrganizationId::new(Uuid::new_v4()),
            bundle: Uuid::new_v4(),
            plan: Uuid::new_v4(),
            family: Family::Activity,
            revision: 1,
        }
    }
    #[test]
    fn evidence_cannot_move_between_authority_scopes_or_purposes() {
        let key = RawPayloadKey::new([37; 32]);
        let scope = scope();
        let row = Uuid::new_v4();
        let source = json!({"body":"synthetic retained note"});
        let sealed = scope.seal(&key, row, Purpose::Result, &source).unwrap();
        assert_eq!(
            scope
                .open::<Value>(
                    &key,
                    row,
                    Purpose::Result,
                    &sealed.nonce,
                    &sealed.ciphertext
                )
                .unwrap(),
            source
        );
        let changed = [
            Scope {
                organization: OrganizationId::new(Uuid::new_v4()),
                ..scope
            },
            Scope {
                bundle: Uuid::new_v4(),
                ..scope
            },
            Scope {
                plan: Uuid::new_v4(),
                ..scope
            },
            Scope {
                family: Family::Metadata,
                ..scope
            },
            Scope {
                revision: 2,
                ..scope
            },
        ];
        for other in changed {
            assert!(other
                .open::<Value>(
                    &key,
                    row,
                    Purpose::Result,
                    &sealed.nonce,
                    &sealed.ciphertext
                )
                .is_err());
        }
        assert!(scope
            .open::<Value>(
                &key,
                Uuid::new_v4(),
                Purpose::Result,
                &sealed.nonce,
                &sealed.ciphertext
            )
            .is_err());
        assert!(scope
            .open::<Value>(
                &key,
                row,
                Purpose::Manifest,
                &sealed.nonce,
                &sealed.ciphertext
            )
            .is_err());
        let mut corrupted = sealed.ciphertext;
        corrupted[0] ^= 1;
        assert!(scope
            .open::<Value>(&key, row, Purpose::Result, &sealed.nonce, &corrupted)
            .is_err());
    }
    #[test]
    fn evidence_enforces_limits_before_encrypting_or_decrypting() {
        let key = RawPayloadKey::new([38; 32]);
        let scope = scope();
        let row = Uuid::new_v4();
        assert!(scope
            .seal(&key, row, Purpose::Mapping, &"x".repeat(65536))
            .is_err());
        assert!(scope
            .open::<Value>(&key, row, Purpose::Mapping, &[0; 24], &vec![0; 65553])
            .is_err());
        assert!(Scope {
            revision: 0,
            ..scope
        }
        .seal(&key, row, Purpose::Mapping, &json!({}))
        .is_err());
    }
}
