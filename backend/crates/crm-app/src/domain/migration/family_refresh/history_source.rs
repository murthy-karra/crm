//! Reuse the established global identity and complete canonical comparison.
//! Display interpretation stays metadata-only, even for body-only corrections.
use super::{
    evidence::{Purpose, Scope},
    model::Hold,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{
        crypto,
        history_capture_source::{SourceRecord, Stream},
        history_import_source, MigrationError,
    },
    ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

// Deliberately no Debug: source IDs and metadata are not log fields.
pub struct HistoryEvidence {
    pub identity_hmac: [u8; 32],
    pub semantic_hmac: [u8; 32],
    pub source_person: String,
    pub created: Option<DateTime<Utc>>,
    pub display: HistoryDisplay,
}
#[derive(Serialize)]
#[serde(transparent)]
pub struct HistoryDisplay(Value);
impl HistoryDisplay {
    pub fn metadata(&self) -> &Value {
        &self.0
    }
    pub fn seal(
        &self,
        scope: Scope,
        key: &RawPayloadKey,
        version_id: Uuid,
    ) -> Result<crypto::Sealed, MigrationError> {
        scope.seal(key, version_id, Purpose::HistoryDisplay, self)
    }
}

/// Called only after authenticating a retained page and parsing it with the
/// existing history capture parser. This does not establish cohort eligibility,
/// ordering, ownership or source uniqueness; those checks remain independent.
pub fn interpret(
    key: &RawPayloadKey,
    org: OrganizationId,
    account: i64,
    access_user: i64,
    stream: Stream,
    record: &SourceRecord,
) -> Result<HistoryEvidence, Hold> {
    if account <= 0 || access_user <= 0 {
        return Err(Hold::UnsupportedSource);
    }
    let source_id = record.source_id.as_ref().ok_or(Hold::UnsupportedSource)?;
    let person = record
        .primary_person_id
        .as_ref()
        .filter(|id| {
            !record.relationship_uncertain
                && record.person_refs.len() == 1
                && record.person_refs.first() == Some(*id)
        })
        .ok_or(Hold::IdentityMismatch)?;
    let family = stream.as_str();
    let identity_bytes = serde_json::to_vec(&(account, family, stream.representation(), source_id))
        .map_err(|_| Hold::UnsupportedSource)?;
    let identity_hmac =
        crypto::snapshot_hmac(key, org, "timeline-import-identity-v1", &identity_bytes);
    let semantic_hmac = crypto::snapshot_hmac(
        key,
        org,
        &format!(
            "timeline-import-canonical-v1:{account}:{family}:{}",
            stream.representation()
        ),
        &record.canonical,
    );
    let interpreted = history_import_source::interpret(stream, &record.canonical, access_user)
        .map_err(|e| match e {
            MigrationError::StorageLimit => Hold::UnitTooLarge,
            _ => Hold::UnsupportedSource,
        })?;
    Ok(HistoryEvidence {
        identity_hmac,
        semantic_hmac,
        source_person: person.clone(),
        created: interpreted.created,
        display: HistoryDisplay(interpreted.metadata),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::migration::{
        family_refresh::model::Family,
        history_capture_source::{parse, Cursor, Request},
    };
    use serde_json::json;

    fn record(stream: Stream, value: Value) -> SourceRecord {
        let page = json!({"_metadata":{"collection":stream.collection(),"limit":100,"offset":0,"total":1},stream.collection():[value]});
        parse(
            &Request {
                stream,
                cursor: Cursor::default(),
            },
            &serde_json::to_vec(&page).unwrap(),
            None,
        )
        .unwrap()
        .records
        .remove(0)
    }
    #[test]
    fn body_only_correction_changes_semantics_without_displaying_body() {
        let key = RawPayloadKey::new([74; 32]);
        let org = OrganizationId::new(Uuid::new_v4());
        for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
            let first = record(
                stream,
                json!({"id":1,"personId":101,"body":"PRIVATE_FIRST"}),
            );
            let second = record(
                stream,
                json!({"id":1,"personId":101,"body":"PRIVATE_SECOND"}),
            );
            let a = interpret(&key, org, 17, 3, stream, &first).unwrap();
            let b = interpret(&key, org, 17, 3, stream, &second).unwrap();
            assert_eq!(a.identity_hmac, b.identity_hmac);
            assert_ne!(a.semantic_hmac, b.semantic_hmac);
            assert_eq!(a.display.metadata(), b.display.metadata());
            assert!(!a.display.metadata().to_string().contains("PRIVATE"));
            assert!(a.created.is_none());
            let scope = Scope {
                organization: org,
                bundle: Uuid::new_v4(),
                plan: Uuid::new_v4(),
                family: Family::History,
                revision: 1,
            };
            let version = Uuid::new_v4();
            let sealed = a.display.seal(scope, &key, version).unwrap();
            assert!(sealed.ciphertext.len() <= 4112);
            let display: Value = scope
                .open(
                    &key,
                    version,
                    Purpose::HistoryDisplay,
                    &sealed.nonce,
                    &sealed.ciphertext,
                )
                .unwrap();
            assert_eq!(&display, a.display.metadata());
            assert!(scope
                .open::<Value>(
                    &key,
                    Uuid::new_v4(),
                    Purpose::HistoryDisplay,
                    &sealed.nonce,
                    &sealed.ciphertext
                )
                .is_err());
            assert!(scope
                .open::<Value>(
                    &key,
                    version,
                    Purpose::Result,
                    &sealed.nonce,
                    &sealed.ciphertext
                )
                .is_err());
            assert!(a
                .display
                .seal(
                    Scope {
                        family: Family::Activity,
                        ..scope
                    },
                    &key,
                    version
                )
                .is_err());
        }
    }
    #[test]
    fn identity_keeps_original_namespace_and_rejects_ambiguous_people() {
        let key = RawPayloadKey::new([75; 32]);
        let org = OrganizationId::new(Uuid::new_v4());
        let stream = Stream::Events;
        let valid = record(
            stream,
            json!({"id":"0001","personId":101,"unknown":{"a":1}}),
        );
        let evidence = interpret(&key, org, 17, 3, stream, &valid).unwrap();
        assert_eq!(
            evidence.identity_hmac,
            crypto::snapshot_hmac(
                &key,
                org,
                "timeline-import-identity-v1",
                &serde_json::to_vec(&(17_i64, "events", stream.representation(), "1")).unwrap()
            )
        );
        let other_account = interpret(&key, org, 18, 3, stream, &valid).unwrap();
        assert_ne!(evidence.identity_hmac, other_account.identity_hmac);
        let other_org = interpret(
            &key,
            OrganizationId::new(Uuid::new_v4()),
            17,
            3,
            stream,
            &valid,
        )
        .unwrap();
        assert_ne!(evidence.identity_hmac, other_org.identity_hmac);
        for value in [
            json!({"id":1,"personId":101,"personIds":[101,102]}),
            json!({"id":1}),
        ] {
            assert!(matches!(
                interpret(&key, org, 17, 3, stream, &record(stream, value)),
                Err(Hold::IdentityMismatch)
            ));
        }
        assert!(matches!(
            interpret(&key, org, 0, 3, stream, &valid),
            Err(Hold::UnsupportedSource)
        ));
    }
}
