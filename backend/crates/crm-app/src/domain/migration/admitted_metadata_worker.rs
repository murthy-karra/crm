//! One fenced catalog or Person unit per transaction. Native writes, immutable
//! outcomes, the checkpoint and the selected snapshot's bytes settle together.
use super::{
    admitted_metadata::{self as a, FrozenMapping, FrozenOperation},
    crypto,
    metadata_source::NativeValue,
    metadata_worker, MigrationError,
};
use crate::{auth::workspace, config::RawPayloadKey, ids::OrganizationId};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

const UNIT: i64 = 64 * 1024 * 1024;
#[allow(clippy::too_many_arguments)] // Keep exact tenant/evidence/unit boundaries explicit.
pub(crate) fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, MigrationError> {
    let bytes = crypto::open_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("admitted-metadata-v1:{plan}:{purpose}"),
        nonce,
        ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    serde_json::from_slice(&bytes).map_err(|_| MigrationError::Crypto)
}
pub(crate) async fn target_state(
    c: &mut PgConnection,
    org: OrganizationId,
    kind: &str,
    target: Uuid,
) -> Result<Value, MigrationError> {
    let query=match kind {
        "tag"=>"SELECT jsonb_build_object('id',id,'label',name) FROM tag WHERE organization_id=$1 AND id=$2",
        "field"=>"SELECT jsonb_build_object('id',id,'label',label,'field_type',field_type,'source',source,'external_key',external_key,'archived',archived_at IS NOT NULL) FROM custom_field WHERE organization_id=$1 AND id=$2",
        "option"=>"SELECT jsonb_build_object('id',o.id,'label',o.label,'field_id',o.field_id,'archived',o.archived_at IS NOT NULL OR f.archived_at IS NOT NULL) FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.organization_id=$1 AND o.id=$2",
        _=>return Err(MigrationError::InvalidImportChoice),
    };
    Ok(sqlx::query_scalar(query)
        .bind(org.0)
        .bind(target)
        .fetch_optional(c)
        .await?
        .unwrap_or(Value::Null))
}
pub(crate) async fn claim_state(
    c: &mut PgConnection,
    org: OrganizationId,
    account: i64,
    kind: &str,
    key: &[u8],
) -> Result<Value, MigrationError> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('target_id',target_id,'original_mapping_id',original_mapping_id,'admitted_mapping_id',admitted_mapping_id) FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4").bind(org.0).bind(account).bind(kind).bind(key).fetch_optional(c).await?.unwrap_or(Value::Null))
}
fn same_source(left: &FrozenMapping, right: &FrozenMapping) -> Result<bool, MigrationError> {
    Ok(left.source_id == right.source_id
        && left.raw_choice == right.raw_choice
        && left.machine_name == right.machine_name
        && left.label == right.label
        && left.field_type == right.field_type
        && serde_json::to_value(&left.definition).map_err(|_| MigrationError::Crypto)?
            == serde_json::to_value(&right.definition).map_err(|_| MigrationError::Crypto)?)
}
#[allow(clippy::too_many_arguments)] // Keep exact tenant/evidence/unit boundaries explicit.
pub(crate) async fn claim_equal(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    account: i64,
    kind: &str,
    source_key: &[u8],
    f: &FrozenMapping,
    target: Uuid,
) -> Result<bool, MigrationError> {
    let Some(r)=sqlx::query("SELECT * FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4").bind(org.0).bind(account).bind(kind).bind(source_key).fetch_optional(&mut *c).await? else{return Ok(false)};
    if r.get::<Uuid, _>("target_id") != target {
        return Ok(false);
    }
    if let Some(id) = r.get::<Option<Uuid>, _>("admitted_mapping_id") {
        let owner=sqlx::query("SELECT i.snapshot_id,m.plan_id FROM migration_admitted_metadata_mapping m JOIN migration_admitted_metadata_import i ON i.id=m.import_id AND i.organization_id=m.organization_id WHERE m.id=$1 AND m.organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *c).await?;
        let prior: FrozenMapping = open(
            key,
            org,
            owner.get("snapshot_id"),
            owner.get("plan_id"),
            id,
            "mapping",
            &r.get::<Vec<u8>, _>("evidence_nonce"),
            &r.get::<Vec<u8>, _>("evidence_ciphertext"),
        )?;
        if kind == "tag" {
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT lower($1::text) IS NOT DISTINCT FROM lower($2::text)",
            )
            .bind(&f.label)
            .bind(&prior.label)
            .fetch_one(c)
            .await?
                && (f.label.is_some() || f.raw_choice == prior.raw_choice))
        } else {
            same_source(f, &prior)
        }
    } else {
        let id: Uuid = r.get("original_mapping_id");
        let owner=sqlx::query("SELECT i.snapshot_id,m.plan_id,m.nonce,m.ciphertext FROM migration_metadata_mapping m JOIN migration_metadata_import i ON i.id=m.import_id AND i.organization_id=m.organization_id WHERE m.id=$1 AND m.organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *c).await?;
        let prior: super::metadata_model::Mapping = super::metadata_store::open(
            key,
            org,
            owner.get("snapshot_id"),
            owner.get("plan_id"),
            id,
            "mapping",
            &owner.get::<Vec<u8>, _>("nonce"),
            &owner.get::<Vec<u8>, _>("ciphertext"),
        )?;
        if kind == "tag" {
            return Ok(sqlx::query_scalar::<_, bool>(
                "SELECT lower($1::text) IS NOT DISTINCT FROM lower($2::text)",
            )
            .bind(&f.label)
            .bind(&prior.label)
            .fetch_one(c)
            .await?
                && (f.label.is_some() || f.raw_choice == prior.raw_choice));
        }
        Ok(prior.raw_choice == f.raw_choice
            && prior.label == f.label
            && prior.source_name == f.machine_name
            && serde_json::to_value(prior.field).map_err(|_| MigrationError::Crypto)?
                == serde_json::to_value(&f.definition).map_err(|_| MigrationError::Crypto)?)
    }
}

/// Count only newly retained child evidence; UUID/enums/native rows are excluded.
/// Existing admission/source evidence is referenced, never recharged.
pub(crate) async fn retained(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
) -> Result<i64, MigrationError> {
    Ok(sqlx::query_scalar("SELECT (COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)+octet_length(counts::text)) FROM migration_admitted_metadata_plan WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_person_id)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)) FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_key)+octet_length(source_id)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_id)+octet_length(semantic_hmac)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_source WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_key)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_operation WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_result WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(semantic_hmac)) FROM migration_admitted_metadata_observation WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(digest)+octet_length(nonce)+octet_length(ciphertext)) FROM migration_admitted_metadata_receipt WHERE import_id=$1 AND organization_id=$2),0)+COALESCE((SELECT sum(octet_length(source_key)+octet_length(evidence_nonce)+octet_length(evidence_ciphertext)) FROM migration_metadata_catalog_claim WHERE admitted_import_id=$1 AND organization_id=$2),0))::bigint").bind(root).bind(org.0).fetch_one(c).await?)
}
pub(crate) async fn reserve(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
    plan: Uuid,
    snapshot: Uuid,
    purpose: &str,
    amount: i64,
) -> Result<Uuid, MigrationError> {
    if !(1..=UNIT).contains(&amount) {
        return Err(MigrationError::StorageLimit);
    }
    let r=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(snapshot).bind(org.0).fetch_one(&mut *c).await?;
    let policy = super::snapshot::SnapshotPolicy::default();
    if amount
        > r.get::<i64, _>("run_byte_limit")
            .min(policy.run_ceiling_bytes)
            .saturating_sub(r.get("retained_bytes"))
            .saturating_sub(r.get("reserved_bytes"))
        || amount
            > r.get::<i64, _>("byte_limit")
                .min(policy.org_ceiling_bytes)
                .saturating_sub(r.get("org_retained"))
                .saturating_sub(r.get("org_reserved"))
    {
        return Err(MigrationError::StorageLimit);
    }
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_metadata_reservation(token,import_id,plan_id,snapshot_id,organization_id,purpose,byte_count) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(token).bind(root).bind(plan).bind(snapshot).bind(org.0).bind(purpose).bind(amount).execute(&mut *c).await?;
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(amount).execute(&mut *c).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes+$2 WHERE organization_id=$1").bind(org.0).bind(amount).execute(&mut *c).await?;
    sqlx::query("UPDATE migration_admitted_metadata_import SET reserved_bytes=reserved_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(amount).execute(c).await?;
    Ok(token)
}
pub(crate) async fn settle(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    let r=sqlx::query("DELETE FROM migration_admitted_metadata_reservation WHERE token=$1 AND import_id=$2 AND organization_id=$3 RETURNING byte_count,snapshot_id").bind(token).bind(root).bind(org.0).fetch_optional(&mut *c).await?.ok_or(MigrationError::Conflict)?;
    let reserved: i64 = r.get("byte_count");
    let snapshot: Uuid = r.get("snapshot_id");
    if actual < 0 || actual > reserved {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_snapshot SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(snapshot).bind(org.0).bind(reserved).bind(actual).execute(&mut *c).await?;
    sqlx::query("UPDATE migration_snapshot_storage SET reserved_bytes=reserved_bytes-$2,retained_bytes=retained_bytes+$3 WHERE organization_id=$1").bind(org.0).bind(reserved).bind(actual).execute(&mut *c).await?;
    sqlx::query("UPDATE migration_admitted_metadata_import SET reserved_bytes=reserved_bytes-$3,retained_bytes=retained_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(reserved).bind(actual).execute(c).await?;
    Ok(())
}
pub(crate) async fn charge(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT snapshot_id,latest_plan_id,retained_bytes FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).fetch_one(&mut *c).await?;
    let amount = retained(c, org, root).await? - r.get::<i64, _>("retained_bytes");
    if amount > 0 {
        let t = reserve(
            c,
            org,
            root,
            r.get("latest_plan_id"),
            r.get("snapshot_id"),
            "prepare",
            amount,
        )
        .await?;
        settle(c, org, root, t, amount).await?;
    } else if amount < 0 {
        return Err(MigrationError::StorageLimit);
    }
    Ok(())
}
pub(crate) async fn release(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
) -> Result<(), MigrationError> {
    let tokens=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_admitted_metadata_reservation WHERE import_id=$1 AND organization_id=$2 ORDER BY token").bind(root).bind(org.0).fetch_all(&mut *c).await?;
    for t in tokens {
        settle(c, org, root, t, 0).await?
    }
    Ok(())
}

struct Job {
    root: Uuid,
    plan: Uuid,
    org: OrganizationId,
    snapshot: Uuid,
    account: i64,
    actor: Uuid,
    lease: Uuid,
}
impl Job {
    fn decode<T: DeserializeOwned>(
        &self,
        key: &RawPayloadKey,
        row: &PgRow,
        purpose: &str,
    ) -> Result<T, MigrationError> {
        open(
            key,
            self.org,
            self.snapshot,
            self.plan,
            row.get("id"),
            purpose,
            &row.get::<Vec<u8>, _>("nonce"),
            &row.get::<Vec<u8>, _>("ciphertext"),
        )
    }
    async fn permit(
        &self,
        c: &mut PgConnection,
        unit: Uuid,
        target: Uuid,
        kind: &str,
        source_key: &[u8],
        table: &str,
    ) -> Result<(), MigrationError> {
        let hex = source_key
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let proof = json!({"lease":self.lease,"unit_id":unit,"root_id":self.root,"plan_id":self.plan,"target_id":target,"claim_kind":kind,"claim_key":hex,"native_table":table});
        sqlx::query("SELECT set_config('crm.admitted_metadata_permit',$1,true)")
            .bind(proof.to_string())
            .execute(c)
            .await?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)] // Keep exact tenant/evidence/unit boundaries explicit.
    async fn result(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        unit: Uuid,
        kind: &str,
        outcome: &str,
        person: Option<Uuid>,
        data: Value,
    ) -> Result<i64, MigrationError> {
        let id = Uuid::new_v4();
        let sealed = a::seal(key, self.org, self.snapshot, self.plan, id, "result", &data)?;
        let bytes = (sealed.nonce.len() + sealed.ciphertext.len()) as i64;
        sqlx::query("INSERT INTO migration_admitted_metadata_result(id,import_id,plan_id,unit_id,manifest_id,organization_id,kind,disposition,person_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(id).bind(self.root).bind(self.plan).bind(unit).bind(if kind=="people"{Some(unit)}else{None}).bind(self.org.0).bind(kind).bind(outcome).bind(person).bind(sealed.nonce).bind(sealed.ciphertext).execute(c).await?;
        Ok(bytes)
    }
}
async fn mapping_ready(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    m: &PgRow,
) -> Result<bool, MigrationError> {
    let f: FrozenMapping = j.decode(key, m, "mapping")?;
    let Some(target) = m.get::<Option<Uuid>, _>("target_id") else {
        return Ok(false);
    };
    let kind: String = m.get("kind");
    let result:Option<String>=sqlx::query_scalar("SELECT disposition FROM migration_admitted_metadata_result WHERE import_id=$1 AND organization_id=$2 AND unit_id=$3").bind(j.root).bind(j.org.0).bind(m.get::<Uuid,_>("id")).fetch_optional(&mut *c).await?;
    if !matches!(result.as_deref(), Some("created" | "already_present")) {
        return Ok(false);
    }
    let now = target_state(c, j.org, &kind, target).await?;
    if now.is_null() || now["archived"] == true {
        return Ok(false);
    }
    if m.get::<String, _>("disposition") == "map_existing" && now != f.target_baseline {
        return Ok(false);
    }
    if m.get::<String, _>("disposition") == "create_matching"
        && (now["label"] != json!(f.label)
            || kind == "field"
                && (now["field_type"] != json!(f.field_type)
                    || now["external_key"] != json!(f.machine_name)))
    {
        return Ok(false);
    }
    claim_equal(
        c,
        key,
        j.org,
        j.account,
        &kind,
        &m.get::<Vec<u8>, _>("source_key"),
        &f,
        target,
    )
    .await
}
async fn catalog(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    m: &PgRow,
) -> Result<i64, MigrationError> {
    let f: FrozenMapping = j.decode(key, m, "mapping")?;
    let id: Uuid = m.get("id");
    let kind: String = m.get("kind");
    let disposition: String = m.get("disposition");
    let mut outcome = "held";
    let mut added = 0;
    if matches!(disposition.as_str(), "create_matching" | "map_existing") && f.reasons.is_empty() {
        if let (Some(target), Some(label)) =
            (m.get::<Option<Uuid>, _>("target_id"), f.label.as_deref())
        {
            let sk: Vec<u8> = m.get("source_key");
            let now = target_state(c, j.org, &kind, target).await?;
            let current_claim = claim_state(c, j.org, j.account, &kind, &sk).await?;
            let claim_ok = if current_claim.is_null() {
                f.claim_baseline.is_null()
            } else {
                claim_equal(c, key, j.org, j.account, &kind, &sk, &f, target).await?
            };
            let target_ok = now == f.target_baseline && now["archived"] != true;
            let parent_ok = if let Some(parent) = m.get::<Option<Uuid>, _>("parent_mapping_id") {
                let p=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(parent).bind(j.plan).bind(j.org.0).fetch_one(&mut *c).await?;
                mapping_ready(c, key, j, &p).await?
            } else {
                true
            };
            let supported = kind != "field"
                || matches!(
                    f.field_type.as_deref(),
                    Some("text" | "number" | "date" | "choice")
                ) && f.machine_name.is_some()
                    && (disposition == "map_existing"
                        || f.definition
                            .as_ref()
                            .is_some_and(|d| d.creation_reasons.is_empty()));
            let available = if disposition == "create_matching" {
                match kind.as_str(){
                    "tag"=>sqlx::query_scalar::<_,bool>("SELECT (SELECT count(*) FROM tag WHERE organization_id=$1)<200 AND NOT EXISTS(SELECT 1 FROM tag WHERE organization_id=$1 AND lower(name)=lower($2))").bind(j.org.0).bind(label).fetch_one(&mut *c).await?,
                    "field"=>sqlx::query_scalar::<_,bool>("SELECT (SELECT count(*) FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL)<50 AND NOT EXISTS(SELECT 1 FROM custom_field WHERE organization_id=$1 AND (lower(label)=lower($2) OR (source='fub' AND external_key=$3)))").bind(j.org.0).bind(label).bind(&f.machine_name).fetch_one(&mut *c).await?,
                    "option"=>sqlx::query_scalar::<_,bool>("SELECT (SELECT count(*) FROM custom_field_option WHERE organization_id=$1 AND field_id=$3 AND archived_at IS NULL)<50 AND NOT EXISTS(SELECT 1 FROM custom_field_option WHERE organization_id=$1 AND field_id=$3 AND lower(label)=lower($2))").bind(j.org.0).bind(label).bind(m.get::<Option<Uuid>,_>("target_field_id")).fetch_one(&mut *c).await?,_=>false}
            } else {
                !now.is_null()
                    && (kind != "field"
                        || now["field_type"] == json!(f.field_type)
                            && (now["source"].is_null()
                                || now["source"] == "fub"
                                    && now["external_key"] == json!(f.machine_name)))
            };
            if claim_ok && target_ok && parent_ok && supported && available {
                if current_claim.is_null() {
                    sqlx::query("INSERT INTO migration_metadata_catalog_claim(organization_id,source_account_id,kind,source_key,target_id,admitted_import_id,admitted_plan_id,admitted_mapping_id,evidence_nonce,evidence_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(j.org.0).bind(j.account).bind(&kind).bind(&sk).bind(target).bind(j.root).bind(j.plan).bind(id).bind(m.get::<Vec<u8>,_>("nonce")).bind(m.get::<Vec<u8>,_>("ciphertext")).execute(&mut *c).await?;
                    added += 32
                        + (m.get::<Vec<u8>, _>("nonce").len()
                            + m.get::<Vec<u8>, _>("ciphertext").len())
                            as i64;
                }
                if disposition == "create_matching" {
                    let table = match kind.as_str() {
                        "tag" => "tag",
                        "field" => "custom_field",
                        _ => "custom_field_option",
                    };
                    j.permit(c, id, target, &kind, &sk, table).await?;
                    match kind.as_str() {
                        "tag" => {
                            sqlx::query("INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,$4)").bind(target).bind(j.org.0).bind(j.actor).bind(label).execute(&mut *c).await?;
                        }
                        "field" => {
                            sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id,source,external_key) VALUES($1,$2,$3,$4,(SELECT COALESCE(max(position),-1)+1 FROM custom_field WHERE organization_id=$2 AND archived_at IS NULL),$5,'fub',$6)").bind(target).bind(j.org.0).bind(label).bind(&f.field_type).bind(j.actor).bind(&f.machine_name).execute(&mut *c).await?;
                        }
                        "option" => {
                            sqlx::query("INSERT INTO custom_field_option(id,organization_id,field_id,label,position) VALUES($1,$2,$3,$4,(SELECT COALESCE(max(position),-1)+1 FROM custom_field_option WHERE organization_id=$2 AND field_id=$3 AND archived_at IS NULL))").bind(target).bind(j.org.0).bind(m.get::<Option<Uuid>,_>("target_field_id")).bind(label).execute(&mut *c).await?;
                        }
                        _ => return Err(MigrationError::InvalidImportChoice),
                    }
                    outcome = "created";
                } else {
                    outcome = "already_present";
                }
            }
        }
    }
    added += j
        .result(
            c,
            key,
            id,
            &kind,
            outcome,
            None,
            json!({"mapping_id":id,"outcome":outcome}),
        )
        .await?;
    Ok(added)
}
async fn people(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    m: &PgRow,
) -> Result<i64, MigrationError> {
    let unit: Uuid = m.get("id");
    let live_person: Option<Uuid> = m.get("person_id");
    let person: Uuid = m
        .get::<Option<Uuid>, _>("expected_person_id")
        .or(live_person)
        .unwrap_or(Uuid::nil());
    let baseline: Value = open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        unit,
        "baseline",
        &m.get::<Vec<u8>, _>("baseline_nonce"),
        &m.get::<Vec<u8>, _>("baseline_ciphertext"),
    )?;
    let identity:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_admitted_metadata_import a ON a.admission_id=r.admission_id AND a.organization_id=r.organization_id JOIN migration_import_identity i ON i.organization_id=r.organization_id AND i.source_account_id=a.source_account_id AND i.family='people' AND i.source_id=r.source_id AND i.target_id=r.person_id AND i.admission_result_id=r.id AND i.admission_item_id=r.item_id AND i.admission_id=r.admission_id JOIN person p ON p.id=r.person_id AND p.organization_id=r.organization_id WHERE a.id=$1 AND a.organization_id=$2 AND r.id=$3 AND r.person_id=$4 AND r.source_id=$5 AND r.disposition='settled')").bind(j.root).bind(j.org.0).bind(m.get::<Uuid,_>("admission_result_id")).bind(person).bind(m.get::<String,_>("source_person_id")).fetch_one(&mut *c).await?;
    // Org namespace serialization precedes deterministic Person locks; no profile
    // command can enter the review workspace through this metadata permit.
    sqlx::query("SELECT id FROM person WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(person)
        .bind(j.org.0)
        .fetch_optional(&mut *c)
        .await?;
    let ops=sqlx::query("SELECT * FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4 ORDER BY id").bind(unit).bind(j.root).bind(j.plan).bind(j.org.0).fetch_all(&mut *c).await?;
    let tags: Vec<Uuid> = sqlx::query_scalar(
        "SELECT tag_id FROM person_tag WHERE person_id=$1 AND organization_id=$2 ORDER BY tag_id",
    )
    .bind(person)
    .bind(j.org.0)
    .fetch_all(&mut *c)
    .await?;
    let mut prepared = Vec::with_capacity(ops.len());
    let mut new_tags = std::collections::BTreeSet::new();
    for o in ops {
        let data: FrozenOperation = j.decode(key, &o, "operation")?;
        let map = if let Some(id) = o.get::<Option<Uuid>, _>("mapping_id") {
            sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(id).bind(j.plan).bind(j.org.0).fetch_optional(&mut *c).await?
        } else {
            None
        };
        let ready = if let Some(ref map) = map {
            mapping_ready(c, key, j, map).await?
        } else {
            false
        };
        if ready
            && o.get::<String, _>("kind") == "tag_link"
            && o.get::<String, _>("disposition") == "eligible"
        {
            if let Some(id) = o.get::<Option<Uuid>, _>("target_id") {
                if !tags.contains(&id) {
                    new_tags.insert(id);
                }
            }
        }
        prepared.push((o, data, map, ready));
    }
    let tag_cap = tags.len() + new_tags.len() > 20;
    let mut results = Vec::new();
    for (o, mut data, map, ready) in prepared {
        let mut outcome = o.get::<String, _>("disposition");
        let kind: String = o.get("kind");
        let mut reason = None;
        if matches!(outcome.as_str(), "eligible" | "already_present") {
            if !identity || m.get::<String, _>("disposition") != "eligible" || !ready {
                outcome = "held".into();
                reason = Some("identity_or_catalog_changed");
            } else if let (Some(map), Some(target)) = (map, o.get::<Option<Uuid>, _>("target_id")) {
                let sk: Vec<u8> = map.get("source_key");
                if kind == "tag_link" {
                    let was = baseline["tags"]
                        .as_array()
                        .ok_or(MigrationError::Crypto)?
                        .contains(&json!(target));
                    let now = tags.contains(&target);
                    if was != now {
                        outcome = "held".into();
                        reason = Some("native_baseline_changed");
                    } else if now {
                        outcome = "already_present".into();
                    } else if tag_cap {
                        outcome = "held".into();
                        reason = Some("person_tag_limit");
                    } else {
                        j.permit(c, unit, target, "tag", &sk, "person_tag").await?;
                        sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) VALUES($1,$2,$3,$4)").bind(j.org.0).bind(person).bind(target).bind(j.actor).execute(&mut *c).await?;
                        outcome = "applied".into();
                    }
                } else {
                    let old = baseline["values"]
                        .as_array()
                        .ok_or(MigrationError::Crypto)?
                        .iter()
                        .find(|v| v["field_id"] == json!(target))
                        .cloned();
                    let now:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(v)-ARRAY['created_at','updated_at'] FROM person_custom_field_value v WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(j.org.0).bind(person).bind(target).fetch_optional(&mut *c).await?;
                    if old != now {
                        outcome = "held".into();
                        reason = Some("native_baseline_changed");
                    } else {
                        if let Some(NativeValue::Choice(raw)) = data.value.clone() {
                            let options=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE parent_mapping_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY id").bind(map.get::<Uuid,_>("id")).bind(j.plan).bind(j.org.0).fetch_all(&mut *c).await?;
                            let mut selected = None;
                            for option in options {
                                let frozen: FrozenMapping = j.decode(key, &option, "mapping")?;
                                if frozen.raw_choice.as_deref() == Some(raw.as_str())
                                    && mapping_ready(c, key, j, &option).await?
                                {
                                    selected = option.get::<Option<Uuid>, _>("target_id");
                                    break;
                                }
                            }
                            data.value = selected.map(|id| NativeValue::Choice(id.to_string()));
                        }
                        if let Some(value) = data.value.as_ref() {
                            match metadata_worker::value_state(c, j.org, person, target, value)
                                .await?
                            {
                                Some(true) => outcome = "already_present".into(),
                                Some(false) => {
                                    outcome = "held".into();
                                    reason = Some("local_value_conflict");
                                }
                                None => {
                                    let (text, number, date, option) =
                                        metadata_worker::native_parts(value)?;
                                    let field_type = match value {
                                        NativeValue::Text(_) => "text",
                                        NativeValue::Number(_) => "number",
                                        NativeValue::Date(_) => "date",
                                        NativeValue::Choice(_) => "choice",
                                    };
                                    j.permit(
                                        c,
                                        unit,
                                        target,
                                        "field",
                                        &sk,
                                        "person_custom_field_value",
                                    )
                                    .await?;
                                    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,number_value,date_value,option_id,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,$4,$5,CAST($6::text AS numeric),$7,$8,$9,'migration',$10)").bind(j.org.0).bind(person).bind(target).bind(field_type).bind(text).bind(number).bind(date).bind(option).bind(j.actor).bind(j.root).execute(&mut *c).await?;
                                    outcome = "applied".into();
                                }
                            }
                        } else {
                            outcome = "held".into();
                            reason = Some("choice_mapping_unavailable");
                        }
                    }
                }
            } else {
                outcome = "held".into();
            }
        }
        results.push(json!({"operation_id":o.get::<Uuid,_>("id"),"mapping_id":o.get::<Option<Uuid>,_>("mapping_id"),"kind":kind,"outcome":outcome,"reason":reason}));
    }
    let outcome = if results.iter().any(|r| r["outcome"] == "held") || !identity {
        "held"
    } else if results.iter().any(|r| r["outcome"] == "applied") {
        "applied"
    } else if results.iter().any(|r| r["outcome"] == "already_present") {
        "already_present"
    } else if results.iter().any(|r| r["outcome"] == "source_null") {
        "source_null"
    } else {
        "not_supplied"
    };
    let bytes=j.result(c,key,unit,"people",outcome,live_person,json!({"admission_result_id":m.get::<Uuid,_>("admission_result_id"),"source_person_id":m.get::<String,_>("source_person_id"),"operations":results,"outcome":outcome})).await?;
    sqlx::query("UPDATE migration_admitted_metadata_manifest SET disposition='settled',settled_at=clock_timestamp() WHERE id=$1 AND import_id=$2 AND organization_id=$3").bind(unit).bind(j.root).bind(j.org.0).execute(c).await?;
    Ok(bytes)
}
async fn unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    candidate: &PgRow,
) -> Result<bool, MigrationError> {
    let org = OrganizationId::new(candidate.get("organization_id"));
    let root: Uuid = candidate.get("id");
    let mut tx = pool.begin().await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, org).await?;
    let admin:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE)").bind(org.0).bind(candidate.get::<Uuid,_>("executor_user_id")).fetch_one(&mut *tx).await?;
    if !admin {
        return Err(MigrationError::Forbidden);
    }
    super::store::lock_org(&mut tx, org).await?;
    // All child writers serialize at the Org barrier before ledger/root locks.
    sqlx::query("SELECT s.id FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(candidate.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *tx).await?;
    let r=sqlx::query("SELECT * FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(root).bind(org.0).fetch_one(&mut *tx).await?;
    if r.get::<Uuid, _>("executor_user_id") != candidate.get::<Uuid, _>("executor_user_id") {
        return Ok(false);
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_import i JOIN organization o ON o.id=i.organization_id AND o.workspace_revision=i.workspace_revision AND o.workspace_mode='migration_review' JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.parent_import_id AND w.plan_id=i.parent_plan_id JOIN migration_import p ON p.id=i.parent_import_id AND p.organization_id=i.organization_id AND p.confirmed_plan_id=i.parent_plan_id AND p.state='completed' JOIN migration_people_admission a ON a.id=i.admission_id AND a.organization_id=i.organization_id AND a.parent_import_id=i.parent_import_id AND a.parent_plan_id=i.parent_plan_id AND a.state IN ('completed','cancelled') JOIN migration_metadata_catalog_readiness q ON q.organization_id=i.organization_id AND q.state='ready' WHERE i.id=$1 AND i.organization_id=$2 AND (i.state='queued' OR i.state='running' AND i.lease_expires_at<=clock_timestamp()))").bind(root).bind(org.0).fetch_one(&mut *tx).await?;
    if !valid {
        tx.commit().await?;
        return Ok(false);
    }
    let j = Job {
        root,
        org,
        plan: r
            .get::<Option<Uuid>, _>("confirmed_plan_id")
            .ok_or(MigrationError::Conflict)?,
        snapshot: r.get("snapshot_id"),
        account: r.get("source_account_id"),
        actor: r.get("executor_user_id"),
        lease: Uuid::new_v4(),
    };
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',pause_reason=NULL WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(j.lease).execute(&mut *tx).await?;
    let phase: String = r.get("phase");
    let checkpoint = r
        .get::<Option<Uuid>, _>("checkpoint_id")
        .unwrap_or(Uuid::nil());
    let mapping = if phase == "catalog" {
        let previous:Option<String>=sqlx::query_scalar("SELECT kind FROM migration_admitted_metadata_mapping WHERE id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4").bind(checkpoint).bind(root).bind(j.plan).bind(org.0).fetch_optional(&mut *tx).await?;
        sqlx::query("SELECT m.* FROM migration_admitted_metadata_mapping m WHERE m.import_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND (m.kind,m.id)>($4,$5) AND NOT EXISTS(SELECT 1 FROM migration_admitted_metadata_result x WHERE x.import_id=$1 AND x.organization_id=$3 AND x.unit_id=m.id) ORDER BY m.kind,m.id LIMIT 1").bind(root).bind(j.plan).bind(org.0).bind(previous.unwrap_or_default()).bind(checkpoint).fetch_optional(&mut *tx).await?
    } else {
        None
    };
    let manifest = if mapping.is_none() {
        sqlx::query("SELECT m.* FROM migration_admitted_metadata_manifest m WHERE m.import_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND m.id>$4 AND m.disposition IN ('eligible','held') ORDER BY m.id LIMIT 1").bind(root).bind(j.plan).bind(org.0).bind(if phase=="people"{checkpoint}else{Uuid::nil()}).fetch_optional(&mut *tx).await?
    } else {
        None
    };
    let children = if let Some(m) = mapping.as_ref() {
        let f: FrozenMapping = j.decode(key, m, "mapping")?;
        if m.get::<String, _>("kind") == "field"
            && m.get::<String, _>("disposition") == "create_matching"
            && f.field_type.as_deref() == Some("choice")
        {
            let children=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE parent_mapping_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY id LIMIT 51").bind(m.get::<Uuid,_>("id")).bind(j.plan).bind(org.0).fetch_all(&mut *tx).await?;
            if children.len() > 50 {
                return Err(MigrationError::StorageLimit);
            }
            let mut ordered = Vec::with_capacity(children.len());
            for child in children {
                let value: FrozenMapping = j.decode(key, &child, "mapping")?;
                let ordinal = value
                    .definition
                    .as_ref()
                    .and_then(|d| d.choices.iter().find(|o| o.raw == value.raw_choice))
                    .map(|o| o.ordinal)
                    .ok_or(MigrationError::Crypto)?;
                ordered.push((ordinal, child));
            }
            ordered.sort_by_key(|(ordinal, _)| *ordinal);
            ordered.into_iter().map(|(_, row)| row).collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let selected = mapping.as_ref().or(manifest.as_ref());
    if let Some(row) = selected {
        let mut bound = if mapping.is_some() {
            (row.get::<Vec<u8>, _>("nonce").len() + row.get::<Vec<u8>, _>("ciphertext").len())
                as i64
                + 4096
        } else {
            row.get::<i64, _>("item_byte_bound")
        };
        for child in &children {
            bound += (child.get::<Vec<u8>, _>("nonce").len()
                + child.get::<Vec<u8>, _>("ciphertext").len()) as i64
                + 4096;
        }
        // Confirmation holds the maximum first unit. Subsequent units reserve
        // independently; only this owner's unused work reservation is released.
        let old:Option<Uuid>=sqlx::query_scalar("SELECT token FROM migration_admitted_metadata_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='work'").bind(root).bind(org.0).fetch_optional(&mut *tx).await?;
        if let Some(t) = old {
            settle(&mut tx, org, root, t, 0).await?
        }
        let token = reserve(&mut tx, org, root, j.plan, j.snapshot, "work", bound).await?;
        let mut added = if let Some(m) = mapping.as_ref() {
            catalog(&mut tx, key, &j, m).await?
        } else {
            people(
                &mut tx,
                key,
                &j,
                manifest.as_ref().ok_or(MigrationError::Conflict)?,
            )
            .await?
        };
        for child in &children {
            added += catalog(&mut tx, key, &j, child).await?;
        }
        let alive:bool=sqlx::query_scalar("SELECT lease_token=$3 AND lease_expires_at>clock_timestamp() AND state='running' FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(j.lease).fetch_one(&mut *tx).await?;
        if !alive {
            return Err(MigrationError::Conflict);
        }
        sqlx::query("UPDATE migration_admitted_metadata_import SET checkpoint_id=$3,state='queued',phase=$4,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(row.get::<Uuid,_>("id")).bind(if mapping.is_some(){"catalog"}else{"people"}).execute(&mut *tx).await?;
        settle(&mut tx, org, root, token, added).await?;
    } else {
        release(&mut tx, org, root).await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET state='completed',phase='complete',lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}
pub async fn run_once(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    if a::prepare_once(pool, key).await? {
        return Ok(true);
    }
    let candidate=sqlx::query("SELECT id,organization_id,executor_user_id,snapshot_id,confirmed_plan_id FROM migration_admitted_metadata_import WHERE state='queued' OR (state='running' AND lease_expires_at<=clock_timestamp()) ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    match unit(pool, key, &candidate).await {
        Ok(value) => Ok(value),
        Err(error) => {
            // The failed unit transaction has dropped and rolled back. Persist
            // only a closed pause reason; restoring a dependency never retries it.
            let reason = match &error {
                MigrationError::Crypto => "key_or_integrity",
                MigrationError::StorageLimit => "storage_limit",
                MigrationError::Forbidden => "authority",
                _ => "unit_failed",
            };
            sqlx::query("UPDATE migration_admitted_metadata_import SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND state IN ('queued','running') AND executor_user_id=$4 AND confirmed_plan_id=$5").bind(candidate.get::<Uuid,_>("id")).bind(candidate.get::<Uuid,_>("organization_id")).bind(reason).bind(candidate.get::<Uuid,_>("executor_user_id")).bind(candidate.get::<Option<Uuid>,_>("confirmed_plan_id")).execute(pool).await?;
            Err(error)
        }
    }
}
