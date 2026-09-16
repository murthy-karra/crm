//! Explicit after-state evidence for new first-family activity results. Legacy
//! results without this proof remain unproven; mutable rows never supply B.
use super::model::{compare_native, Baseline, Current, Hold, Outcome};
use crate::{domain::migration::MigrationError, ids::OrganizationId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct AfterState {
    version: u8,
    native: Value,
}
/// Called by the first-import executor only after its INSERT succeeded, while
/// the native row/Person locks and original import permit are still held.
pub(crate) async fn capture(
    conn: &mut PgConnection,
    org: OrganizationId,
    kind: &str,
    target: Uuid,
) -> Result<AfterState, MigrationError> {
    let sql = match kind {
        "note" => "SELECT to_jsonb(n) FROM note n WHERE id=$1 AND organization_id=$2",
        "task" => "SELECT to_jsonb(t) FROM task t WHERE id=$1 AND organization_id=$2",
        _ => return Err(MigrationError::InvalidInput),
    };
    let native: Value = sqlx::query_scalar(sql)
        .bind(target)
        .bind(org.0)
        .fetch_one(conn)
        .await?;
    if native["revision"] != 1
        || native["origin"] != "migration"
        || native["source"] != "fub"
        || !native["deleted_at"].is_null()
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(AfterState { version: 1, native })
}
/// Trusted relational binding from the immutable global identity, successful
/// result and frozen manifest. Never deserialize it from a request or model.
pub struct Binding<'a> {
    pub organization: OrganizationId,
    pub person: Uuid,
    pub target: Uuid,
    pub result: Uuid,
    pub first_import: Uuid,
    pub native_source_key: &'a str,
}
/// The authenticated after-state is independent of C. Revision comparison
/// rejects edit-and-revert (ABA); equality cannot create migration ownership.
pub fn verify(
    proof: Option<&AfterState>,
    binding: Binding<'_>,
    current: Option<&Value>,
) -> Result<Baseline, Hold> {
    let proof = proof.ok_or(Hold::BaselineUnproven)?;
    let row = &proof.native;
    if proof.version != 1
        || !row.is_object()
        || row["revision"] != 1
        || row["id"] != json!(binding.target)
        || row["organization_id"] != json!(binding.organization.0)
        || row["person_id"] != json!(binding.person)
        || row["correlation_id"] != json!(binding.first_import)
        || row["source"] != "fub"
        || row["source_external_id"] != binding.native_source_key
        || row["origin"] != "migration"
        || row.get("deleted_at") != Some(&Value::Null)
        || row.get("deleted_by_user_id") != Some(&Value::Null)
    {
        return Err(Hold::BaselineUnproven);
    }
    let baseline = Baseline {
        result_id: binding.result,
        person_id: binding.person,
        target_id: binding.target,
        revision: 1,
        native: row.clone(),
        owned: true,
        head_id: None,
    };
    let current = current.ok_or(Hold::TargetErased)?;
    let observation = Current {
        person_id: current
            .get("person_id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or(Hold::IdentityMismatch)?,
        target_id: current
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or(Hold::IdentityMismatch)?,
        revision: current["revision"].as_i64().ok_or(Hold::BaselineUnproven)?,
        native: current.clone(),
        deleted: !current["deleted_at"].is_null(),
        head_id: None,
    };
    match compare_native(Some(&baseline), Some(&observation), current, true, true) {
        Outcome::AlreadyCurrent => Ok(baseline),
        Outcome::Held(hold) => Err(hold),
        _ => Err(Hold::BaselineUnproven),
    }
}

/// Discovery does not advance a baseline or write a head. A classifier must
/// prefer an existing refresh head and revalidate the selected proof at commit.
pub enum Discovery {
    Proven(Baseline),
    Held(Hold),
}
/// Find positive first-import ownership for a frozen Person. The same worker
/// transaction admission as preparation enforces the current admin and lease.
/// Missing legacy after-state is held; it is never backfilled from current data.
pub async fn discover(
    pool: &sqlx::PgPool,
    key: &crate::config::RawPayloadKey,
    claim: &super::cohort::Claim,
    cohort: Uuid,
    kind: super::model::Kind,
    source_id: &str,
) -> Result<Discovery, MigrationError> {
    use crate::domain::migration::{
        activity_model, activity_store, admitted_activity_model, admitted_activity_store,
    };
    use sqlx::Row;
    let kind = match kind {
        super::model::Kind::Note => "note",
        super::model::Kind::Task => "task",
        _ => return Err(MigrationError::InvalidInput),
    };
    let (mut tx, b, p) = super::preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "activity"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let c=sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3")
        .bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let identity=sqlx::query("SELECT * FROM migration_activity_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_id=$4")
        .bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(kind).bind(source_id).fetch_optional(&mut *tx).await?;
    let Some(identity) = identity else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    let target: Uuid = identity.get("target_id");
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND target_id=$4)")
        .bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(kind).bind(target).fetch_one(&mut *tx).await? {return Ok(Discovery::Held(Hold::StaleHead));}
    let admitted = identity
        .get::<Option<Uuid>, _>("admitted_import_id")
        .is_some();
    let (prefix, owner_column, cohort_column) = if admitted {
        (
            "migration_admitted_activity",
            "admitted_",
            "admission_result_id",
        )
    } else {
        ("migration_activity", "", "original_result_id")
    };
    let Some(person_result) = c.get::<Option<Uuid>, _>(cohort_column) else {
        return Ok(Discovery::Held(Hold::IdentityMismatch));
    };
    let root: Uuid = identity
        .get::<Option<Uuid>, _>(format!("{owner_column}import_id").as_str())
        .ok_or(MigrationError::Crypto)?;
    let plan: Uuid = identity
        .get::<Option<Uuid>, _>(format!("{owner_column}plan_id").as_str())
        .ok_or(MigrationError::Crypto)?;
    let manifest: Uuid = identity
        .get::<Option<Uuid>, _>(format!("{owner_column}manifest_id").as_str())
        .ok_or(MigrationError::Crypto)?;
    // Identifiers below are chosen exclusively from the two fixed owner shapes.
    let parent_column = if admitted {
        "admission_result_id"
    } else {
        "parent_result_id"
    };
    let query=format!("SELECT r.*,a.snapshot_id,a.state AS owner_state,m.native_source_key FROM {prefix}_result r JOIN {prefix}_import a ON a.id=r.import_id AND a.organization_id=r.organization_id AND a.confirmed_plan_id=r.plan_id JOIN {prefix}_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.import_id=r.import_id AND m.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.import_id=$2 AND r.plan_id=$3 AND r.manifest_id=$4 AND a.parent_import_id=$5 AND a.parent_plan_id=$6 AND a.source_account_id=$7 AND m.{parent_column}=$8 AND m.source_person_id=$9 AND m.person_id=$10 AND r.person_id=$10 AND m.kind=$11 AND r.kind=$11 AND m.source_id=$12 AND r.source_id=$12 AND r.target_id=$13");
    let r = sqlx::query(&query)
        .bind(claim.organization.0)
        .bind(root)
        .bind(plan)
        .bind(manifest)
        .bind(b.get::<Uuid, _>("parent_import_id"))
        .bind(b.get::<Uuid, _>("parent_plan_id"))
        .bind(b.get::<i64, _>("source_account_id"))
        .bind(person_result)
        .bind(c.get::<String, _>("source_person_id"))
        .bind(c.get::<Uuid, _>("person_id"))
        .bind(kind)
        .bind(source_id)
        .bind(target)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(r) = r else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    if !matches!(
        r.get::<String, _>("owner_state").as_str(),
        "completed" | "cancelled"
    ) {
        return Ok(Discovery::Held(Hold::FirstCoverageRequired));
    }
    if r.get::<String, _>("disposition") != "applied" {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    }
    let proof = if admitted {
        let payload: admitted_activity_model::ResultData = admitted_activity_store::open(
            key,
            claim.organization,
            r.get("snapshot_id"),
            plan,
            r.get("id"),
            "result",
            r.get("nonce"),
            r.get("ciphertext"),
        )?;
        payload.after_state
    } else {
        let payload: activity_model::ResultData = activity_store::open(
            key,
            claim.organization,
            r.get("snapshot_id"),
            plan,
            r.get("id"),
            "result",
            r.get("nonce"),
            r.get("ciphertext"),
        )?;
        payload.after_state
    };
    let sql = if kind == "note" {
        "SELECT to_jsonb(n) FROM note n WHERE id=$1 AND organization_id=$2"
    } else {
        "SELECT to_jsonb(t) FROM task t WHERE id=$1 AND organization_id=$2"
    };
    let current: Option<Value> = sqlx::query_scalar(sql)
        .bind(target)
        .bind(claim.organization.0)
        .fetch_optional(&mut *tx)
        .await?;
    Ok(
        match verify(
            proof.as_ref(),
            Binding {
                organization: claim.organization,
                person: c.get("person_id"),
                target,
                result: r.get("id"),
                first_import: root,
                native_source_key: &r.get::<String, _>("native_source_key"),
            },
            current.as_ref(),
        ) {
            Ok(b) => Discovery::Proven(b),
            Err(reason) => Discovery::Held(reason),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn after_state_rejects_missing_proof_wrong_owner_local_edits_and_aba() {
        let org = OrganizationId::new(Uuid::new_v4());
        let person = Uuid::new_v4();
        let target = Uuid::new_v4();
        let first = Uuid::new_v4();
        let result = Uuid::new_v4();
        let row = json!({"id":target,"organization_id":org.0,"person_id":person,"correlation_id":first,"source":"fub","source_external_id":"100:note:11","origin":"migration","revision":1,"body":"original","deleted_at":null,"deleted_by_user_id":null});
        let proof = AfterState {
            version: 1,
            native: row.clone(),
        };
        let bind = || Binding {
            organization: org,
            person,
            target,
            result,
            first_import: first,
            native_source_key: "100:note:11",
        };
        assert!(verify(Some(&proof), bind(), Some(&row)).unwrap().owned);
        assert_eq!(
            verify(None, bind(), Some(&row)).err(),
            Some(Hold::BaselineUnproven)
        );
        assert_eq!(
            verify(
                Some(&proof),
                Binding {
                    first_import: Uuid::new_v4(),
                    ..bind()
                },
                Some(&row)
            )
            .err(),
            Some(Hold::BaselineUnproven)
        );
        let mut edited = row.clone();
        edited["body"] = json!("changed");
        assert_eq!(
            verify(Some(&proof), bind(), Some(&edited)).err(),
            Some(Hold::LocalChange)
        );
        edited = row.clone();
        edited["revision"] = json!(3);
        assert_eq!(
            verify(Some(&proof), bind(), Some(&edited)).err(),
            Some(Hold::LocalChange)
        );
        assert_eq!(
            verify(Some(&proof), bind(), None).err(),
            Some(Hold::TargetErased)
        );
    }
}
