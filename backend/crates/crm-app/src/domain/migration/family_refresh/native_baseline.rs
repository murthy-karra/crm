//! Prior-refresh after-state is authenticated in its original result scope.
//! A present but unusable head never falls back to an older first-import result.
use super::{
    evidence::{Purpose, Scope},
    metadata_delta::{Ownership, Snapshot},
    model::{Baseline, Hold, Kind},
    source_policy,
};
use crate::{config::RawPayloadKey, domain::migration::MigrationError, ids::OrganizationId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, Row};
use uuid::Uuid;

/// Produced only by a successful refresh unit, after reading its committed
/// native state under the unit locks. Scope AEAD binds Org/bundle/plan/result.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AfterState {
    pub version: u8,
    pub manifest: Uuid,
    pub source_id: String,
    pub person: Uuid,
    pub target: Uuid,
    pub state: State,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum State {
    Metadata {
        snapshot: Snapshot,
        ownership: Ownership,
    },
    Note {
        native: Value,
        owned: bool,
    },
    Task {
        native: Value,
        owned: bool,
    },
}
impl State {
    fn kind(&self) -> Kind {
        match self {
            Self::Metadata { .. } => Kind::Metadata,
            Self::Note { .. } => Kind::Note,
            Self::Task { .. } => Kind::Task,
        }
    }
}
/// Closed result payload. Held/excluded results do not contain an after-state.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultData {
    pub after_state: Option<AfterState>,
}

pub(super) struct Proven {
    pub result: Uuid,
    pub revision: i64,
    pub snapshot: Uuid,
    pub sequence: i64,
    pub proof: AfterState,
}
pub(super) enum Selection {
    Absent,
    Proven(Box<Proven>),
    Held(Hold),
}
pub(super) struct Request<'a> {
    pub organization: OrganizationId,
    pub bundle: &'a PgRow,
    pub plan: &'a PgRow,
    pub cohort: &'a PgRow,
    pub kind: Kind,
    pub target: Uuid,
    pub source_id: &'a str,
}

pub(super) async fn load(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    request: Request<'_>,
) -> Result<Selection, MigrationError> {
    let Request {
        organization,
        bundle,
        plan,
        cohort,
        kind,
        target,
        source_id,
    } = request;
    let kind_name = match kind {
        Kind::Metadata => "metadata",
        Kind::Note => "note",
        Kind::Task => "task",
        _ => return Err(MigrationError::InvalidInput),
    };
    // Bounded even if damaged evidence points multiple source keys at one target.
    let heads = sqlx::query("SELECT * FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND target_id=$4 LIMIT 2")
        .bind(organization.0).bind(bundle.get::<i64,_>("source_account_id")).bind(kind_name).bind(target).fetch_all(&mut *conn).await?;
    let Some(head) = heads.first() else {
        return Ok(Selection::Absent);
    };
    if heads.len() != 1 || head.get::<Uuid, _>("person_id") != cohort.get::<Uuid, _>("person_id") {
        return Ok(Selection::Held(Hold::IdentityMismatch));
    }
    let r = sqlx::query(include_str!("sql/native_baseline.sql"))
        .bind(head.get::<Uuid, _>("result_id"))
        .bind(organization.0)
        .bind(bundle.get::<Uuid, _>("parent_import_id"))
        .bind(bundle.get::<Uuid, _>("parent_plan_id"))
        .bind(bundle.get::<i64, _>("source_account_id"))
        .bind(kind.family().as_str())
        .bind(kind_name)
        .bind(head.get::<Vec<u8>, _>("source_key_hmac"))
        .bind(cohort.get::<Uuid, _>("person_id"))
        .bind(target)
        .bind(cohort.get::<String, _>("source_person_id"))
        .bind(cohort.get::<Option<Uuid>, _>("original_result_id"))
        .bind(cohort.get::<Option<Uuid>, _>("admission_result_id"))
        .bind(cohort.get::<Uuid, _>("creation_snapshot_id"))
        .bind(bundle.get::<DateTime<Utc>, _>("created_at"))
        .fetch_optional(&mut *conn)
        .await?;
    let Some(r) = r else {
        return Ok(Selection::Held(Hold::BaselineUnproven));
    };
    if let Err(hold) = source_policy::qualify_core_snapshots(
        conn,
        organization,
        bundle.get("source_account_id"),
        plan.get::<Option<Uuid>, _>("source_snapshot_id")
            .ok_or(MigrationError::SourceNotEligible)?,
        r.get("source_snapshot_id"),
    )
    .await?
    {
        return Ok(Selection::Held(hold));
    }
    let scope = Scope {
        organization,
        bundle: r.get("bundle_id"),
        plan: r.get("plan_id"),
        family: kind.family(),
        revision: r.get("plan_revision"),
    };
    let data: ResultData = scope.open(
        key,
        r.get("id"),
        Purpose::Result,
        r.get("nonce"),
        r.get("ciphertext"),
    )?;
    let Some(proof) = data.after_state else {
        return Ok(Selection::Held(Hold::BaselineUnproven));
    };
    if proof.version != 1
        || proof.manifest != r.get::<Uuid, _>("manifest_id")
        || proof.person != cohort.get::<Uuid, _>("person_id")
        || proof.target != target
        || proof.source_id != source_id
        || proof.state.kind() != kind
    {
        return Ok(Selection::Held(Hold::BaselineUnproven));
    }
    Ok(Selection::Proven(Box::new(Proven {
        result: r.get("id"),
        revision: r.get("native_revision"),
        snapshot: r.get("source_snapshot_id"),
        sequence: r.get("capture_sequence"),
        proof,
    })))
}

pub(super) fn verify_activity(
    proven: Proven,
    organization: OrganizationId,
    account: i64,
    current: Option<&Value>,
) -> Result<Baseline, Hold> {
    let native = match &proven.proof.state {
        State::Note {
            native,
            owned: true,
        }
        | State::Task {
            native,
            owned: true,
        } => native,
        _ => return Err(Hold::BaselineUnproven),
    };
    if native["id"] != json!(proven.proof.target)
        || native["person_id"] != json!(proven.proof.person)
        || native["organization_id"] != json!(organization.0)
        || native["revision"].as_i64() != Some(proven.revision)
        || proven.revision <= 0
        || native["origin"] != "migration"
        || native["source"] != "fub"
        || native["source_external_id"] != format!("v1:{account}:{}", proven.proof.source_id)
        || native.get("deleted_at") != Some(&Value::Null)
        || native.get("deleted_by_user_id") != Some(&Value::Null)
    {
        return Err(Hold::BaselineUnproven);
    }
    let current = current.ok_or(Hold::TargetErased)?;
    if !current["deleted_at"].is_null() {
        return Err(Hold::TargetErased);
    }
    if current["person_id"] != native["person_id"] || current["id"] != native["id"] {
        return Err(Hold::IdentityMismatch);
    }
    if current != native {
        return Err(Hold::LocalChange);
    }
    Ok(Baseline {
        result_id: proven.result,
        person_id: proven.proof.person,
        target_id: proven.proof.target,
        revision: proven.revision,
        native: native.clone(),
        owned: true,
        head_id: Some(proven.result),
    })
}

pub(super) fn verify_metadata(
    proven: &Proven,
    current: &Snapshot,
) -> Result<(Snapshot, Ownership), Hold> {
    let State::Metadata {
        snapshot,
        ownership,
    } = &proven.proof.state
    else {
        return Err(Hold::BaselineUnproven);
    };
    if proven.proof.target != proven.proof.person
        || snapshot.person != proven.proof.person
        || snapshot.revision != proven.revision
        || proven.revision <= 0
        || snapshot.head != Some(proven.result)
    {
        return Err(Hold::BaselineUnproven);
    }
    if current.person != snapshot.person {
        return Err(Hold::IdentityMismatch);
    }
    if current.head != snapshot.head {
        return Err(Hold::StaleHead);
    }
    if current.revision != snapshot.revision || !current.state.matches(&snapshot.state) {
        return Err(Hold::LocalChange);
    }
    super::metadata_baseline::verify_ownership(snapshot, ownership)?;
    Ok((snapshot.clone(), ownership.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn activity() -> (Proven, OrganizationId, Value) {
        let org = OrganizationId::new(Uuid::new_v4());
        let person = Uuid::new_v4();
        let target = Uuid::new_v4();
        let native = json!({"id":target,"person_id":person,"organization_id":org.0,
            "revision":3,"origin":"migration","source":"fub","source_external_id":"v1:100:21",
            "deleted_at":null,"deleted_by_user_id":null,"title":"refreshed","snoozed_until":null});
        (
            Proven {
                result: Uuid::new_v4(),
                revision: 3,
                snapshot: Uuid::new_v4(),
                sequence: 10,
                proof: AfterState {
                    version: 1,
                    manifest: Uuid::new_v4(),
                    source_id: "21".into(),
                    person,
                    target,
                    state: State::Task {
                        native: native.clone(),
                        owned: true,
                    },
                },
            },
            org,
            native,
        )
    }
    #[test]
    fn prior_activity_keeps_successful_head_and_rejects_local_edits_or_erasure() {
        let (p, org, native) = activity();
        let result = p.result;
        let baseline =
            verify_activity(p, org, 100, Some(&native)).unwrap_or_else(|h| panic!("{h:?}"));
        assert_eq!(baseline.revision, 3);
        assert_eq!(baseline.head_id, Some(result));
        for field in [
            "revision",
            "title",
            "snoozed_until",
            "completed_at",
            "correlation_id",
        ] {
            let (p, org, mut native) = activity();
            native[field] = json!("changed");
            assert!(matches!(
                verify_activity(p, org, 100, Some(&native)),
                Err(Hold::LocalChange)
            ));
        }
        let (p, org, _) = activity();
        assert!(matches!(
            verify_activity(p, org, 100, None),
            Err(Hold::TargetErased)
        ));
        let (p, org, mut native) = activity();
        native["deleted_at"] = json!("2026-09-16T00:00:00Z");
        assert!(matches!(
            verify_activity(p, org, 100, Some(&native)),
            Err(Hold::TargetErased)
        ));
    }
    #[test]
    fn prior_activity_proof_cannot_move_between_accounts_or_targets() {
        let (p, org, native) = activity();
        assert!(matches!(
            verify_activity(p, org, 101, Some(&native)),
            Err(Hold::BaselineUnproven)
        ));
        let (mut p, org, native) = activity();
        p.proof.target = Uuid::new_v4();
        assert!(matches!(
            verify_activity(p, org, 100, Some(&native)),
            Err(Hold::BaselineUnproven)
        ));
        let (mut p, org, native) = activity();
        if let State::Task { owned, .. } = &mut p.proof.state {
            *owned = false;
        }
        assert!(matches!(
            verify_activity(p, org, 100, Some(&native)),
            Err(Hold::BaselineUnproven)
        ));
        let (mut p, org, native) = activity();
        p.revision = 2;
        assert!(matches!(
            verify_activity(p, org, 100, Some(&native)),
            Err(Hold::BaselineUnproven)
        ));
    }
    #[test]
    fn metadata_retains_only_proven_ownership_and_current_head() {
        let result = Uuid::new_v4();
        let person = Uuid::new_v4();
        let owned = Uuid::new_v4();
        let local = Uuid::new_v4();
        let snapshot = Snapshot {
            person,
            revision: 7,
            head: Some(result),
            state: super::super::metadata_delta::State {
                tags: [owned, local].into(),
                ..Default::default()
            },
        };
        let mut p = Proven {
            result,
            revision: 7,
            snapshot: Uuid::new_v4(),
            sequence: 9,
            proof: AfterState {
                version: 1,
                manifest: Uuid::new_v4(),
                source_id: "101".into(),
                person,
                target: person,
                state: State::Metadata {
                    snapshot: snapshot.clone(),
                    ownership: Ownership {
                        tags: [(owned, [[1; 32], [2; 32]].into())].into(),
                        ..Default::default()
                    },
                },
            },
        };
        let (_, ownership) = verify_metadata(&p, &snapshot).unwrap_or_else(|h| panic!("{h:?}"));
        assert_eq!(ownership.tags.len(), 1);
        assert_eq!(ownership.tags[&owned].len(), 2);
        let mut changed = snapshot.clone();
        changed.revision += 2;
        assert!(matches!(
            verify_metadata(&p, &changed),
            Err(Hold::LocalChange)
        ));
        changed = snapshot.clone();
        changed.head = Some(Uuid::new_v4());
        assert!(matches!(
            verify_metadata(&p, &changed),
            Err(Hold::StaleHead)
        ));
        let State::Metadata { ownership, .. } = &mut p.proof.state else {
            unreachable!()
        };
        ownership.tags.insert(local, [[1; 32]].into());
        assert!(matches!(
            verify_metadata(&p, &snapshot),
            Err(Hold::BaselineUnproven)
        ));
    }
}
