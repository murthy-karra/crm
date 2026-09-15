//! D-088: scoped mapping approvals shared only by the two People refresh engines.
//! SQL table names come from Owner, never from request data.
use super::{
    admitted_people_refresh_store, crypto,
    import_source::{Entity, ExtractedRecord},
    imports, people_refresh_store, MigrationError,
};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, Row};
use uuid::Uuid;

pub const CAPABILITY: &str = "fub-people-mapping-repair-v1";

/// Discover one held descriptor per transaction. Ciphertexts are opened only
/// after the immutable successful Person owner has been verified.
pub(crate) async fn discover(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    root: &PgRow,
) -> Result<(), MigrationError> {
    let p = owner.prefix();
    let org = OrganizationId::new(root.get("organization_id"));
    let id = root.get::<Uuid, _>("id");
    let success_column = if owner == Owner::Original {
        "original_result_id"
    } else {
        "admission_result_id"
    };
    let remainder:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {p} s WHERE s.id=$1 AND s.organization_id=$2 AND s.state='cancelled' AND s.repair_source_refresh_id IS NOT NULL AND s.confirmed_refresh_plan_id=$3 AND s.report_id=$4)"))
        .bind(root.get::<Uuid,_>("repair_source_refresh_id")).bind(org.0).bind(root.get::<Uuid,_>("repair_source_plan_id")).bind(root.get::<Uuid,_>("report_id")).fetch_one(&mut *conn).await?;
    // Each branch uses a selective descriptor index; one retained payload is opened per turn.
    let row=sqlx::query(&format!("WITH candidates AS ((SELECT id FROM {p}_item WHERE refresh_id=$1 AND plan_id=$2 AND organization_id=$3 AND NOT $6 AND disposition='held_mapping_gap' AND ($4::uuid IS NULL OR id>$4) ORDER BY id LIMIT 1) UNION (SELECT item_id FROM {p}_result WHERE refresh_id=$1 AND organization_id=$3 AND NOT $6 AND $5='results' AND disposition IN ('held_mapping_gap','held_stale') AND ($4::uuid IS NULL OR item_id>$4) ORDER BY item_id LIMIT 1) UNION (SELECT id FROM {p}_item WHERE refresh_id=$1 AND plan_id=$2 AND organization_id=$3 AND $6 AND settled_at IS NULL AND disposition IN ('eligible','already_current') AND ($4::uuid IS NULL OR id>$4) ORDER BY id LIMIT 1)) SELECT i.id,i.source_id,i.person_id,i.disposition,i.{success_column} AS successful_result_id FROM candidates c JOIN {p}_item i ON i.id=c.id AND i.refresh_id=$1 AND i.plan_id=$2 AND i.organization_id=$3 WHERE i.source_id IS NOT NULL AND i.person_id IS NOT NULL ORDER BY i.id LIMIT 1"))
        .bind(root.get::<Uuid,_>("repair_source_refresh_id")).bind(root.get::<Uuid,_>("repair_source_plan_id")).bind(org.0).bind(root.get::<Option<Uuid>,_>("repair_checkpoint_id")).bind(root.get::<String,_>("repair_anchor_kind")).bind(remainder).fetch_optional(&mut *conn).await?;
    let Some(row) = row else {
        sqlx::query(&format!("UPDATE {p} SET repair_draft_revision=CASE WHEN EXISTS(SELECT 1 FROM {p}_repair_choice c WHERE c.refresh_id=$1 AND c.organization_id=$2) THEN 1 ELSE 0 END,repair_candidates_complete=true,state='paused',pause_reason='awaiting_mapping_choices',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2"))
            .bind(id).bind(org.0).execute(conn).await?;
        return Ok(());
    };
    let anchor = row.get::<Uuid, _>("id");
    let source_id = row.get::<String, _>("source_id");
    let person = row.get::<Uuid, _>("person_id");
    let owned=match owner {
        Owner::Original=>sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.manifest_id=r.manifest_id AND mi.import_id=r.import_id AND mi.source_id=r.source_id AND mi.family='people' AND mi.target_id=r.person_id WHERE r.import_id=$1 AND r.organization_id=$2 AND r.source_id=$3 AND r.person_id=$4 AND r.disposition='imported' AND mi.source_account_id=$5)")
            .bind(root.get::<Uuid,_>("parent_import_id")).bind(org.0).bind(&source_id).bind(person).bind(root.get::<i64,_>("source_account_id")).fetch_one(&mut *conn).await?,
        Owner::Admitted=>sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_people_admission_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.admission_result_id=r.id AND mi.source_id=r.source_id AND mi.family='people' AND mi.target_id=r.person_id WHERE r.admission_id=$1 AND r.organization_id=$2 AND r.source_id=$3 AND r.person_id=$4 AND r.disposition='settled' AND mi.source_account_id=$5)")
            .bind(root.get::<Uuid,_>("admission_id")).bind(org.0).bind(&source_id).bind(person).bind(root.get::<i64,_>("source_account_id")).fetch_one(&mut *conn).await?,
    };
    let repaired:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {p}_repair_candidate c JOIN {p}_item i ON i.refresh_id=c.refresh_id AND i.organization_id=c.organization_id AND i.source_id=c.source_id JOIN {p}_result z ON z.item_id=i.id AND z.refresh_id=i.refresh_id AND z.organization_id=i.organization_id WHERE c.organization_id=$1 AND c.anchor_item_id=$2 AND z.disposition IN ('settled','settled_noop'))"))
        .bind(org.0).bind(anchor).fetch_one(&mut *conn).await?;
    if owned && !repaired {
        let record = match owner {
            Owner::Original => super::people_refresh_source::retained_person(
                conn,
                key,
                org,
                root.get("newer_snapshot_id"),
                &source_id,
            )
            .await?
            .map(|v| v.0),
            Owner::Admitted => match super::admitted_people_refresh_source::retained_person(
                conn,
                key,
                org,
                root.get("newer_snapshot_id"),
                &source_id,
            )
            .await?
            {
                super::admitted_people_refresh_source::RetainedPerson::Present(v, _, _) => Some(*v),
                _ => None,
            },
        };
        if let Some(record) = record {
            if let (Ok(stage), Ok(assignee)) = (stage_key(&record), assignee_key(&record)) {
                let mut mapping_gap =
                    remainder || row.get::<String, _>("disposition") == "held_mapping_gap";
                for source in [&stage, &assignee].into_iter().flatten() {
                    match select(conn, key, owner, org, root, person, source).await {
                        Err(MigrationError::SourceNotEligible) => mapping_gap = true,
                        Err(e) => return Err(e),
                        Ok(_) => {}
                    }
                }
                if mapping_gap && (stage.is_some() || assignee.is_some()) {
                    for source in [&stage, &assignee].into_iter().flatten() {
                        let entry = Uuid::new_v4();
                        let sealed = owner.seal(key, org, id, entry, "repair-key", source)?;
                        sqlx::query(&format!("INSERT INTO {p}_repair_key(id,refresh_id,organization_id,kind,source_key_hmac,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(refresh_id,organization_id,kind,source_key_hmac) DO NOTHING"))
                            .bind(entry).bind(id).bind(org.0).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
                    }
                    if remainder {
                        for source in [&stage, &assignee].into_iter().flatten() {
                            let old:Option<PgRow>=sqlx::query(&format!("SELECT c.* FROM {p}_repair_choice c JOIN {p} r ON r.id=c.refresh_id AND r.organization_id=c.organization_id WHERE c.refresh_id=$1 AND c.organization_id=$2 AND c.kind=$3 AND c.source_key_hmac=$4 AND c.revision<=r.repair_frozen_revision ORDER BY c.revision DESC LIMIT 1"))
                                .bind(root.get::<Uuid,_>("repair_source_refresh_id")).bind(org.0).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).fetch_optional(&mut *conn).await?;
                            if let Some(old) = old {
                                let body: Value = owner.open(
                                    key,
                                    org,
                                    old.get("refresh_id"),
                                    old.get("id"),
                                    "repair-choice",
                                    &old.get::<Vec<u8>, _>("nonce"),
                                    &old.get::<Vec<u8>, _>("ciphertext"),
                                )?;
                                let choice = Uuid::new_v4();
                                let sealed =
                                    owner.seal(key, org, id, choice, "repair-choice", &body)?;
                                sqlx::query(&format!("INSERT INTO {p}_repair_choice(id,refresh_id,organization_id,revision,kind,source_key_hmac,disposition,target_id,nonce,ciphertext,approved_by_user_id,inherited_choice_id,inherited_refresh_id) VALUES($1,$2,$3,1,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(refresh_id,revision,kind,source_key_hmac) DO NOTHING"))
                                    .bind(choice).bind(id).bind(org.0).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).bind(old.get::<String,_>("disposition")).bind(old.get::<Option<Uuid>,_>("target_id")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(root.get::<Uuid,_>("initiated_by_user_id")).bind(old.get::<Uuid,_>("id")).bind(old.get::<Uuid,_>("refresh_id")).execute(&mut *conn).await?;
                            }
                        }
                    }
                    let stage_hmac = stage.map(|v| v.digest(key, org)).transpose()?;
                    let assignee_hmac = assignee.map(|v| v.digest(key, org)).transpose()?;
                    sqlx::query(&format!("INSERT INTO {p}_repair_candidate(refresh_id,organization_id,anchor_refresh_id,anchor_plan_id,anchor_item_id,source_id,person_id,stage_source_hmac,assignee_source_hmac,successful_result_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"))
                        .bind(id).bind(org.0).bind(root.get::<Uuid,_>("repair_source_refresh_id")).bind(root.get::<Uuid,_>("repair_source_plan_id")).bind(anchor).bind(source_id).bind(person).bind(stage_hmac.as_ref().map(|v|v.as_slice())).bind(assignee_hmac.as_ref().map(|v|v.as_slice())).bind(row.get::<Uuid,_>("successful_result_id")).execute(&mut *conn).await?;
                    sqlx::query(&format!("UPDATE {p} SET repair_candidate_count=repair_candidate_count+1 WHERE id=$1 AND organization_id=$2")).bind(id).bind(org.0).execute(&mut *conn).await?;
                }
            }
        }
    }
    sqlx::query(&format!("UPDATE {p} SET repair_checkpoint_id=$3,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")).bind(id).bind(org.0).bind(anchor).execute(conn).await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Original,
    Admitted,
}
impl Owner {
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Original => "migration_people_refresh",
            Self::Admitted => "migration_admitted_people_refresh",
        }
    }
    pub(crate) fn cohort_column(self) -> &'static str {
        match self {
            Self::Original => "parent_import_id",
            Self::Admitted => "admission_id",
        }
    }
    pub(crate) fn seal<T: Serialize>(
        self,
        key: &RawPayloadKey,
        org: OrganizationId,
        root: Uuid,
        row: Uuid,
        purpose: &str,
        value: &T,
    ) -> Result<crypto::Sealed, MigrationError> {
        match self {
            Self::Original => people_refresh_store::seal(key, org, root, row, purpose, value),
            Self::Admitted => {
                admitted_people_refresh_store::seal(key, org, root, row, purpose, value)
            }
        }
    }
    #[allow(clippy::too_many_arguments)] // Keep every authenticated envelope coordinate explicit.
    pub(crate) fn open<T: serde::de::DeserializeOwned>(
        self,
        key: &RawPayloadKey,
        org: OrganizationId,
        root: Uuid,
        row: Uuid,
        purpose: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<T, MigrationError> {
        match self {
            Self::Original => {
                people_refresh_store::open(key, org, root, row, purpose, nonce, ciphertext)
            }
            Self::Admitted => {
                admitted_people_refresh_store::open(key, org, root, row, purpose, nonce, ciphertext)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "key",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SourceKey {
    StageLabel(String),
    MissingStage,
    Assignee(String),
}
impl SourceKey {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::StageLabel(_) | Self::MissingStage => "stage",
            Self::Assignee(_) => "assignee",
        }
    }
    pub(crate) fn digest(
        &self,
        key: &RawPayloadKey,
        org: OrganizationId,
    ) -> Result<[u8; 32], MigrationError> {
        let bytes = serde_json::to_vec(self).map_err(|_| MigrationError::Crypto)?;
        Ok(crypto::snapshot_hmac(
            key,
            org,
            "people-repair-source-key-v1",
            &bytes,
        ))
    }
}

/// None is an absent instruction; MissingStage is an explicitly supplied clear.
pub(crate) fn stage_key(record: &ExtractedRecord) -> Result<Option<SourceKey>, MigrationError> {
    let Some(raw) = record.provenance.get("stage") else {
        return Ok(None);
    };
    // Catalog and choice envelopes are capped at 64 KiB. Leave room for their
    // authenticated wrapper and target snapshot; preserve oversized raw evidence
    // as a hold instead of repeatedly failing a database CHECK during discovery.
    if raw.len() > 60 * 1024 {
        return Err(MigrationError::SourceNotEligible);
    }
    match serde_json::from_str::<Value>(raw).map_err(|_| MigrationError::Crypto)? {
        Value::Null => Ok(Some(SourceKey::MissingStage)),
        Value::String(label) if label.trim().is_empty() => Ok(Some(SourceKey::MissingStage)),
        Value::String(label) if !label.contains('\0') => {
            Ok(Some(SourceKey::StageLabel(label.trim().into())))
        }
        _ => Err(MigrationError::SourceNotEligible),
    }
}

pub(crate) fn assignee_key(record: &ExtractedRecord) -> Result<Option<SourceKey>, MigrationError> {
    let Entity::People(person) = &record.entity else {
        return Err(MigrationError::SourceNotEligible);
    };
    if record
        .provenance
        .get("assignedTo")
        .is_some_and(|raw| raw != "null")
        || record
            .reasons
            .iter()
            .any(|reason| reason.starts_with("assignment_"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(person
        .assignee_key
        .as_ref()
        .map(|key| SourceKey::Assignee(key.clone())))
}

#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub choice_id: Uuid,
    pub choice_refresh_id: Uuid,
    pub head_id: Option<Uuid>,
    pub head_version: i64,
    pub target_id: Option<Uuid>,
}

/// Returns None only when no scoped repair approval exists. A selected invalid
/// or unresolved approval is an error, never permission to fall back to old data.
pub(crate) async fn resolve(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &PgRow,
    person: Uuid,
    source: &SourceKey,
) -> Result<Option<Resolved>, MigrationError> {
    let p = owner.prefix();
    let hmac = source.digest(key, org)?;
    let head = sqlx::query(&format!("SELECT h.binding_id,h.version,b.choice_id,b.choice_refresh_id FROM {p}_mapping_head h JOIN {p}_mapping_binding b ON b.id=h.binding_id AND b.organization_id=h.organization_id WHERE h.organization_id=$1 AND h.person_id=$2 AND h.source_account_id=$3 AND h.kind=$4 AND h.source_key_hmac=$5"))
        .bind(org.0).bind(person).bind(root.get::<i64,_>("source_account_id")).bind(source.kind()).bind(hmac.as_slice()).fetch_optional(&mut *conn).await?;
    let head_id = head.as_ref().map(|r| r.get::<Uuid, _>("binding_id"));
    let head_version = head.as_ref().map_or(0, |r| r.get::<i64, _>("version"));
    let explicit = if let Some(revision) = root.get::<Option<i64>, _>("repair_frozen_revision") {
        sqlx::query(&format!("SELECT * FROM {p}_repair_choice WHERE refresh_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4 AND revision<=$5 ORDER BY revision DESC LIMIT 1"))
            .bind(root.get::<Uuid,_>("id")).bind(org.0).bind(source.kind()).bind(hmac.as_slice()).bind(revision).fetch_optional(&mut *conn).await?
    } else {
        None
    };
    let choice = match (explicit, head) {
        (Some(row), _) => row,
        (None, Some(head)) => sqlx::query(&format!(
            "SELECT * FROM {p}_repair_choice WHERE id=$1 AND refresh_id=$2 AND organization_id=$3"
        ))
        .bind(head.get::<Uuid, _>("choice_id"))
        .bind(head.get::<Uuid, _>("choice_refresh_id"))
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(MigrationError::Crypto)?,
        (None, None) => return Ok(None),
    };
    let body: Value = owner.open(
        key,
        org,
        choice.get("refresh_id"),
        choice.get("id"),
        "repair-choice",
        &choice.get::<Vec<u8>, _>("nonce"),
        &choice.get::<Vec<u8>, _>("ciphertext"),
    )?;
    if body["source"] != serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?
        || choice.get::<String, _>("kind") != source.kind()
        || choice.get::<Vec<u8>, _>("source_key_hmac") != hmac
    {
        return Err(MigrationError::Crypto);
    }
    let target = choice.get::<Option<Uuid>, _>("target_id");
    let disposition = choice.get::<String, _>("disposition");
    let current = target_snapshot(conn, org, source.kind(), &disposition, target).await?;
    if current != body["target"] {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(Some(Resolved {
        choice_id: choice.get("id"),
        choice_refresh_id: choice.get("refresh_id"),
        head_id,
        head_version,
        target_id: target,
    }))
}

pub(crate) async fn target_snapshot(
    conn: &mut PgConnection,
    org: OrganizationId,
    kind: &str,
    disposition: &str,
    target: Option<Uuid>,
) -> Result<Value, MigrationError> {
    match (kind, disposition, target) {
        ("stage", "existing", Some(id)) | ("assignee", "member", Some(id)) => {
            sqlx::query_scalar::<_, Option<Value>>("SELECT crm_mapping_repair_target($1,$2,$3)")
                .bind(org.0)
                .bind(kind)
                .bind(id)
                .fetch_one(conn)
                .await?
                .ok_or(MigrationError::SourceNotEligible)
        }
        ("assignee", "unassigned", None) => Ok(Value::Null),
        _ => Err(MigrationError::SourceNotEligible),
    }
}

pub(crate) async fn measured_bytes(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: Uuid,
) -> Result<i64, MigrationError> {
    let p = owner.prefix();
    Ok(sqlx::query_scalar(&format!("SELECT COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key_hmac)) FROM {p}_repair_key WHERE refresh_id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT sum(COALESCE(octet_length(native_fingerprint),0)+COALESCE(octet_length(mapping_evidence_nonce),0)+COALESCE(octet_length(mapping_evidence_ciphertext),0)+COALESCE(octet_length(repair_stage_source_hmac),0)+COALESCE(octet_length(repair_assignee_source_hmac),0)) FROM {p}_item WHERE refresh_id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT sum(octet_length(h.source_key_hmac)) FROM {p}_mapping_head h JOIN {p}_mapping_binding b ON b.id=h.binding_id AND b.organization_id=h.organization_id WHERE b.refresh_id=$1 AND b.organization_id=$2),0)::bigint + COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key_hmac)) FROM {p}_repair_choice WHERE refresh_id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT sum(octet_length(source_id)+COALESCE(octet_length(stage_source_hmac),0)+COALESCE(octet_length(assignee_source_hmac),0)) FROM {p}_repair_candidate WHERE refresh_id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT sum(octet_length(source_id)+octet_length(source_key_hmac)) FROM {p}_mapping_binding WHERE refresh_id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT octet_length(repair_choices_digest) FROM {p} WHERE id=$1 AND organization_id=$2),0)::bigint + COALESCE((SELECT sum(octet_length(repair_choices_digest)) FROM {p}_plan WHERE refresh_id=$1 AND organization_id=$2),0)::bigint"))
        .bind(root).bind(org.0).fetch_one(conn).await?)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selected {
    source: SourceKey,
    original_id: Option<Uuid>,
    choice_id: Option<Uuid>,
    choice_refresh_id: Option<Uuid>,
    head_id: Option<Uuid>,
    head_version: i64,
    pub target_id: Option<Uuid>,
}

/// Resolve the exact source mapping, including the original mapping row ID.
pub(crate) async fn select(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &PgRow,
    person: Uuid,
    source: &SourceKey,
) -> Result<Selected, MigrationError> {
    if let Some(resolved) = resolve(conn, key, owner, org, root, person, source).await? {
        return Ok(Selected {
            source: source.clone(),
            original_id: None,
            choice_id: Some(resolved.choice_id),
            choice_refresh_id: Some(resolved.choice_refresh_id),
            head_id: resolved.head_id,
            head_version: resolved.head_version,
            target_id: resolved.target_id,
        });
    }
    let (source_key, label_hmac) = match source {
        SourceKey::StageLabel(label) => (
            None,
            Some(crypto::snapshot_hmac(
                key,
                org,
                "import-stage-label",
                label.as_bytes(),
            )),
        ),
        SourceKey::MissingStage => (Some("missing"), None),
        SourceKey::Assignee(id) => (Some(id.as_str()), None),
    };
    let rows=sqlx::query("SELECT m.*,i.snapshot_id FROM migration_import_mapping m JOIN migration_import i ON i.id=$1 AND i.confirmed_plan_id=m.plan_id AND i.organization_id=m.organization_id WHERE m.plan_id=$2 AND m.organization_id=$3 AND m.kind=$4 AND (($5::text IS NOT NULL AND m.source_key=$5) OR ($5::text IS NULL AND m.label_hmac=$6)) ORDER BY m.id LIMIT 2")
        .bind(root.get::<Uuid,_>("parent_import_id")).bind(root.get::<Uuid,_>("parent_plan_id")).bind(org.0).bind(source.kind()).bind(source_key).bind(label_hmac.as_ref().map(|h|h.as_slice())).fetch_all(&mut *conn).await?;
    if rows.len() != 1 {
        return Err(MigrationError::SourceNotEligible);
    }
    let row = &rows[0];
    if !row.get::<bool, _>("qualified")
        || row.get::<Vec<u8>, _>("ciphertext").len() > people_refresh_store::CAPTURE_LIMIT as usize
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let body: Value = imports::open(
        key,
        org,
        row.get("snapshot_id"),
        root.get("parent_plan_id"),
        row.get("id"),
        "mapping",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let disposition = row.get::<String, _>("disposition");
    let disposition = if disposition == "create" && source.kind() == "stage" {
        "existing"
    } else {
        disposition.as_str()
    };
    let target = row.get::<Option<Uuid>, _>("target_id");
    let current = target_snapshot(conn, org, source.kind(), disposition, target).await?;
    let field = if source.kind() == "stage" {
        "name"
    } else {
        "email"
    };
    if (target.is_some() && body["target"][field] != current[field])
        || (target.is_none() && !body["target"].is_null())
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(Selected {
        source: source.clone(),
        original_id: Some(row.get("id")),
        choice_id: None,
        choice_refresh_id: None,
        head_id: None,
        head_version: 0,
        target_id: target,
    })
}

pub(crate) fn assignment_clear(record: &ExtractedRecord) -> Result<bool, MigrationError> {
    if assignee_key(record)?.is_some() {
        return Ok(false);
    }
    let (Some(user), Some(pond)) = (
        record.provenance.get("assignedUserId"),
        record.provenance.get("assignedPondId"),
    ) else {
        return Ok(false);
    };
    if user == "null" && pond == "null" {
        Ok(true)
    } else {
        Err(MigrationError::SourceNotEligible)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    stage: Option<Selected>,
    assignee: Option<Selected>,
    clear_assignee: bool,
    source_semantic_hmac: Vec<u8>,
}

/// Freeze exact source-key approvals for an executable item. Omitted source
/// fields carry no new authority: execution requires their baseline value intact.
#[allow(clippy::too_many_arguments)] // Explicit owner, tenant and frozen evidence are security inputs.
pub(crate) async fn freeze_item(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &PgRow,
    item: Uuid,
    person: Uuid,
    record: &ExtractedRecord,
) -> Result<i64, MigrationError> {
    let stage = match stage_key(record)? {
        Some(source) => Some(select(conn, key, owner, org, root, person, &source).await?),
        None => None,
    };
    let assignee = match assignee_key(record)? {
        Some(source) => Some(select(conn, key, owner, org, root, person, &source).await?),
        None => None,
    };
    let clear_assignee = assignment_clear(record)?;
    let evidence = Evidence {
        stage,
        assignee,
        clear_assignee,
        source_semantic_hmac: crypto::snapshot_hmac(
            key,
            org,
            &format!(
                "semantic:{}",
                super::snapshot_source::Stream::People.representation()
            ),
            &record.canonical,
        )
        .to_vec(),
    };
    let id = root.get::<Uuid, _>("id");
    let p = owner.prefix();
    let sealed = owner.seal(key, org, id, item, "mapping-evidence", &evidence)?;
    let stage = evidence.stage.as_ref();
    let assignee = evidence.assignee.as_ref();
    let stage_hmac = stage.map(|s| s.source.digest(key, org)).transpose()?;
    let assignee_hmac = assignee.map(|s| s.source.digest(key, org)).transpose()?;
    let has_new_choice = [stage, assignee]
        .into_iter()
        .flatten()
        .any(|s| s.choice_refresh_id == Some(id));
    let changed=sqlx::query(&format!("UPDATE {p}_item SET stage_mapping_id=$4,assignee_mapping_id=$5,repair_stage_choice_id=$6,repair_stage_choice_refresh_id=$7,repair_assignee_choice_id=$8,repair_assignee_choice_refresh_id=$9,repair_stage_source_hmac=$10,repair_assignee_source_hmac=$11,repair_stage_head_id=$12,repair_assignee_head_id=$13,repair_stage_head_version=$14,repair_assignee_head_version=$15,mapping_evidence_nonce=$16,mapping_evidence_ciphertext=$17,native_fingerprint=crm_mapping_repair_native_fingerprint(organization_id,person_id),repair_approval_only=(disposition='already_current' AND $18) WHERE id=$1 AND refresh_id=$2 AND organization_id=$3 AND settled_at IS NULL"))
        .bind(item).bind(id).bind(org.0).bind(stage.and_then(|s|s.original_id)).bind(assignee.and_then(|s|s.original_id))
        .bind(stage.and_then(|s|s.choice_id)).bind(stage.and_then(|s|s.choice_refresh_id)).bind(assignee.and_then(|s|s.choice_id)).bind(assignee.and_then(|s|s.choice_refresh_id))
        .bind(stage_hmac.as_ref().map(|h|h.as_slice())).bind(assignee_hmac.as_ref().map(|h|h.as_slice()))
        .bind(stage.and_then(|s|s.head_id)).bind(assignee.and_then(|s|s.head_id)).bind(stage.map_or(0,|s|s.head_version)).bind(assignee.map_or(0,|s|s.head_version))
        .bind(sealed.nonce.as_slice()).bind(&sealed.ciphertext).bind(has_new_choice).execute(&mut *conn).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    let cohort = owner.cohort_column();
    sqlx::query(&format!("UPDATE {p}_item i SET baseline_result_id=b.result_id,baseline_version=b.version FROM {p}_baseline b WHERE i.id=$1 AND i.refresh_id=$2 AND i.organization_id=$3 AND b.organization_id=i.organization_id AND b.{cohort}=$4 AND b.source_id=i.source_id AND b.person_id=i.person_id"))
        .bind(item).bind(id).bind(org.0).bind(root.get::<Uuid,_>(cohort)).execute(&mut *conn).await?;
    let bytes = (32
        + sealed.nonce.len()
        + sealed.ciphertext.len()
        + stage_hmac.map_or(0, |_| 32)
        + assignee_hmac.map_or(0, |_| 32)) as i64;
    Ok(bytes)
}

fn decode_evidence(
    owner: Owner,
    key: &RawPayloadKey,
    org: OrganizationId,
    root: Uuid,
    item: &PgRow,
) -> Result<Option<Evidence>, MigrationError> {
    let Some(nonce) = item.get::<Option<Vec<u8>>, _>("mapping_evidence_nonce") else {
        return Ok(None);
    };
    let ciphertext = item
        .get::<Option<Vec<u8>>, _>("mapping_evidence_ciphertext")
        .ok_or(MigrationError::Crypto)?;
    owner
        .open(
            key,
            org,
            root,
            item.get("id"),
            "mapping-evidence",
            &nonce,
            &ciphertext,
        )
        .map(Some)
}

/// This applies to no-op/approval-only outcomes as well as business updates.
#[allow(clippy::too_many_arguments)] // Explicit owner, tenant and frozen evidence are security inputs.
pub(crate) async fn validate_item(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &PgRow,
    item: &PgRow,
    baseline: &Value,
    proposed: &Value,
) -> Result<bool, MigrationError> {
    let p = owner.prefix();
    let cohort = owner.cohort_column();
    let person = item
        .get::<Option<Uuid>, _>("person_id")
        .ok_or(MigrationError::Conflict)?;
    let head=sqlx::query(&format!("SELECT result_id,version FROM {p}_baseline WHERE organization_id=$1 AND {cohort}=$2 AND source_id=$3 AND person_id=$4 FOR UPDATE"))
        .bind(org.0).bind(root.get::<Uuid,_>(cohort)).bind(item.get::<Option<String>,_>("source_id")).bind(person).fetch_optional(&mut *conn).await?;
    let Some(head) = head else { return Ok(false) };
    if head.get::<Option<Uuid>, _>("result_id") != item.get::<Option<Uuid>, _>("baseline_result_id")
        || head.get::<i64, _>("version") != item.get::<i64, _>("baseline_version")
    {
        return Ok(false);
    }
    let Some(evidence) = decode_evidence(owner, key, org, root.get("id"), item)? else {
        return Ok(false);
    };
    if owner == Owner::Original {
        let source_id = item
            .get::<Option<String>, _>("source_id")
            .ok_or(MigrationError::Conflict)?;
        let identity: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_import_result r JOIN migration_import_identity mi ON mi.organization_id=r.organization_id AND mi.manifest_id=r.manifest_id AND mi.import_id=r.import_id AND mi.source_id=r.source_id AND mi.target_id=r.person_id AND mi.family='people' WHERE r.id=$1 AND r.import_id=$2 AND r.organization_id=$3 AND r.source_id=$4 AND r.person_id=$5 AND r.disposition='imported' AND mi.source_account_id=$6)")
            .bind(item.get::<Option<Uuid>,_>("original_result_id")).bind(root.get::<Uuid,_>("parent_import_id")).bind(org.0).bind(&source_id).bind(person).bind(root.get::<i64,_>("source_account_id")).fetch_one(&mut *conn).await?;
        if !identity {
            return Ok(false);
        }
        let retained = match super::people_refresh_source::retained_person(
            conn,
            key,
            org,
            root.get("newer_snapshot_id"),
            &source_id,
        )
        .await
        {
            Ok(value) => value,
            Err(MigrationError::SourceNotEligible) => return Ok(false),
            Err(error) => return Err(error),
        };
        let Some((record, capture, ordinal)) = retained else {
            return Ok(false);
        };
        let semantic = crypto::snapshot_hmac(
            key,
            org,
            &format!(
                "semantic:{}",
                super::snapshot_source::Stream::People.representation()
            ),
            &record.canonical,
        );
        if item.get::<Option<Uuid>, _>("source_capture_id") != Some(capture)
            || item.get::<Option<i32>, _>("source_ordinal") != Some(ordinal)
            || evidence.source_semantic_hmac != semantic
        {
            return Ok(false);
        }
    }
    for (kind, frozen, field) in [
        ("stage", evidence.stage.as_ref(), "stage_id"),
        ("assignee", evidence.assignee.as_ref(), "assigned_user_id"),
    ] {
        if let Some(frozen) = frozen {
            let current = match select(conn, key, owner, org, root, person, &frozen.source).await {
                Ok(value) => value,
                Err(MigrationError::SourceNotEligible) => return Ok(false),
                Err(error) => return Err(error),
            };
            if current.original_id != frozen.original_id
                || current.choice_id != frozen.choice_id
                || current.choice_refresh_id != frozen.choice_refresh_id
                || current.head_id != frozen.head_id
                || current.head_version != frozen.head_version
                || current.target_id != frozen.target_id
                || json!(current.target_id) != proposed[field]
            {
                return Ok(false);
            }
            let original_column = if kind == "stage" {
                "stage_mapping_id"
            } else {
                "assignee_mapping_id"
            };
            let choice_column = if kind == "stage" {
                "repair_stage_choice_id"
            } else {
                "repair_assignee_choice_id"
            };
            if item.get::<Option<Uuid>, _>(original_column) != frozen.original_id
                || item.get::<Option<Uuid>, _>(choice_column) != frozen.choice_id
            {
                return Ok(false);
            }
        } else if kind == "assignee" && evidence.clear_assignee {
            if !proposed[field].is_null() {
                return Ok(false);
            }
        } else {
            if proposed[field] != baseline[field] {
                return Ok(false);
            }
            let target = proposed[field]
                .as_str()
                .and_then(|v| Uuid::parse_str(v).ok());
            let disposition = match (kind, target) {
                ("stage", Some(_)) => "existing",
                ("assignee", Some(_)) => "member",
                ("assignee", None) => "unassigned",
                _ => return Ok(false),
            };
            match target_snapshot(conn, org, kind, disposition, target).await {
                Ok(_) => {}
                Err(MigrationError::SourceNotEligible) => return Ok(false),
                Err(error) => return Err(error),
            }
        }
    }
    Ok(true)
}

pub(crate) async fn settle_bindings(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &PgRow,
    item: &PgRow,
    result: Uuid,
) -> Result<i64, MigrationError> {
    let id = root.get::<Uuid, _>("id");
    let p = owner.prefix();
    let Some(evidence) = decode_evidence(owner, key, org, id, item)? else {
        return Ok(0);
    };
    let mut bytes = 0;
    for selected in [evidence.stage, evidence.assignee].into_iter().flatten() {
        if selected.choice_refresh_id != Some(id) {
            continue;
        }
        let person = item
            .get::<Option<Uuid>, _>("person_id")
            .ok_or(MigrationError::Conflict)?;
        let source = item
            .get::<Option<String>, _>("source_id")
            .ok_or(MigrationError::Conflict)?;
        let hmac = selected.source.digest(key, org)?;
        let binding = Uuid::new_v4();
        sqlx::query(&format!("INSERT INTO {p}_mapping_binding(id,organization_id,person_id,source_account_id,source_id,kind,source_key_hmac,refresh_id,plan_id,item_id,result_id,choice_id,choice_refresh_id,previous_binding_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$8,$13)"))
            .bind(binding).bind(org.0).bind(person).bind(root.get::<i64,_>("source_account_id")).bind(&source).bind(selected.source.kind()).bind(hmac.as_slice()).bind(id).bind(item.get::<Uuid,_>("plan_id")).bind(item.get::<Uuid,_>("id")).bind(result).bind(selected.choice_id.ok_or(MigrationError::Crypto)?).bind(selected.head_id).execute(&mut *conn).await?;
        if let Some(previous) = selected.head_id {
            let previous_owner: Uuid = sqlx::query_scalar(&format!(
                "SELECT refresh_id FROM {p}_mapping_binding WHERE id=$1 AND organization_id=$2"
            ))
            .bind(previous)
            .bind(org.0)
            .fetch_one(&mut *conn)
            .await?;
            match owner {
                Owner::Original => {
                    people_refresh_store::debit_baseline_owner(conn, org, previous_owner, 32)
                        .await?
                }
                Owner::Admitted => {
                    admitted_people_refresh_store::debit_retained_owner(
                        conn,
                        org,
                        previous_owner,
                        32,
                    )
                    .await?
                }
            };
        }
        let changed = if let Some(previous) = selected.head_id {
            sqlx::query(&format!("UPDATE {p}_mapping_head SET binding_id=$6,version=version+1 WHERE organization_id=$1 AND person_id=$2 AND source_account_id=$3 AND kind=$4 AND source_key_hmac=$5 AND binding_id=$7 AND version=$8"))
                .bind(org.0).bind(person).bind(root.get::<i64,_>("source_account_id")).bind(selected.source.kind()).bind(hmac.as_slice()).bind(binding).bind(previous).bind(selected.head_version).execute(&mut *conn).await?.rows_affected()
        } else {
            sqlx::query(&format!("INSERT INTO {p}_mapping_head(organization_id,person_id,source_account_id,kind,source_key_hmac,binding_id,version) VALUES($1,$2,$3,$4,$5,$6,1) ON CONFLICT DO NOTHING"))
                .bind(org.0).bind(person).bind(root.get::<i64,_>("source_account_id")).bind(selected.source.kind()).bind(hmac.as_slice()).bind(binding).execute(&mut *conn).await?.rows_affected()
        };
        if changed != 1 {
            return Err(MigrationError::Conflict);
        }
        bytes += source.len() as i64 + 64;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_source_stage_stays_a_hold_before_catalog_encryption() {
        let raw =
            serde_json::to_vec(&json!({"people":[{"id":1,"stage":"x".repeat(64*1024)}]})).unwrap();
        let record = super::super::import_source::extract_page(
            super::super::snapshot_source::Stream::People,
            &raw,
        )
        .unwrap()
        .remove(0);
        assert!(matches!(
            stage_key(&record),
            Err(MigrationError::SourceNotEligible)
        ));
    }
}
