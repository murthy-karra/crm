use super::*;
use crate::domain::migration::{
    crypto, import_source::Entity, people_admission_source as source,
    people_mapping_repair::SourceKey, snapshot_source::Stream,
};

/// One positive hold per transaction. The page never walks the entire book for
/// a Person, and keeps unqualified newer observations as visible held candidates.
pub(crate) async fn discover(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    run: &PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = run.get("id");
    let checkpoint: Option<Uuid> = run.get("recovery_checkpoint_id");
    let original = run.get::<Option<Uuid>, _>("recovery_original_plan_id");
    let row = if let Some(plan) = original {
        sqlx::query("SELECT m.id,m.source_id,m.nonce,m.ciphertext FROM migration_import_manifest m WHERE m.import_id=$1 AND m.organization_id=$2 AND m.plan_id=$3 AND m.disposition='held' AND ($4::uuid IS NULL OR m.id>$4) AND EXISTS(SELECT 1 FROM migration_import_mapping z WHERE z.id IN(m.stage_mapping_id,m.assignee_mapping_id) AND z.organization_id=m.organization_id AND z.plan_id=m.plan_id AND z.disposition='hold') ORDER BY m.id LIMIT 1")
            .bind(run.get::<Uuid,_>("parent_import_id")).bind(org.0).bind(plan).bind(checkpoint).fetch_optional(&mut *conn).await?
    } else {
        sqlx::query("SELECT id,source_id FROM migration_people_admission_item WHERE admission_id=$1 AND organization_id=$2 AND plan_id=$3 AND ($4::uuid IS NULL OR id>$4) AND ((NOT $5 AND disposition='held_mapping_gap') OR ($5 AND disposition='eligible' AND settled_at IS NULL)) ORDER BY id LIMIT 1")
            .bind(run.get::<Option<Uuid>,_>("recovery_admission_id")).bind(org.0).bind(run.get::<Option<Uuid>,_>("recovery_admission_plan_id")).bind(checkpoint).bind(run.get::<bool,_>("recovery_remainder")).fetch_optional(&mut *conn).await?
    };
    let Some(row) = row else {
        sqlx::query("UPDATE migration_people_admission SET recovery_candidates_complete=true,recovery_draft_revision=CASE WHEN EXISTS(SELECT 1 FROM migration_people_recovery_choice c WHERE c.admission_id=$1 AND c.organization_id=$2) THEN 1 ELSE 0 END,state='paused',pause_reason='awaiting_mapping_choices',lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
        return Ok(());
    };
    let anchor: Uuid = row.get("id");
    let source_id: String = row.get("source_id");
    if !source::valid_source_id(&source_id) {
        return Err(MigrationError::SourceNotEligible);
    }
    let mut anchor_qualified = true;
    if original.is_some() {
        let body: Value = crate::domain::migration::imports::open(
            key,
            org,
            run.get("original_snapshot_id"),
            run.get("parent_plan_id"),
            anchor,
            "manifest",
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        if !body["reasons"].as_array().is_some_and(|r| {
            r.iter().any(|v| {
                matches!(
                    v.as_str(),
                    Some("stage_mapping_held" | "assignee_mapping_held")
                )
            })
        }) {
            return Err(MigrationError::SourceNotEligible);
        }
        anchor_qualified = body["reasons"].as_array().is_some_and(|reasons| {
            reasons.iter().all(|v| {
                matches!(
                    v.as_str(),
                    Some("stage_mapping_held" | "assignee_mapping_held")
                )
            })
        });
    }
    let observed = source::retained_record(
        conn,
        key,
        org,
        run.get("newer_snapshot_id"),
        Stream::People,
        run.get("newer_sequence"),
        &source_id,
    )
    .await?;
    let (stage, assignee, evidence) = match observed {
        source::Observation::Qualified(retained) => {
            let semantic = crypto::snapshot_hmac(
                key,
                org,
                &format!("semantic:{}", Stream::People.representation()),
                &retained.record.canonical,
            );
            let keys = if anchor_qualified && retained.record.reasons.is_empty() {
                match &retained.record.entity {
                    Entity::People(p)
                        if p.stage_label.as_ref().is_none_or(|v| v.len() <= 60 * 1024) =>
                    {
                        (
                            Some(match &p.stage_label {
                                Some(label) => SourceKey::StageLabel(label.clone()),
                                None => SourceKey::MissingStage,
                            }),
                            p.assignee_key
                                .as_ref()
                                .map(|v| SourceKey::Assignee(v.clone())),
                        )
                    }
                    _ => (None, None),
                }
            } else {
                (None, None)
            };
            (
                keys.0,
                keys.1,
                json!({"source_id":source_id,"anchor":anchor,"anchor_qualified":anchor_qualified,"original":original.is_some(),"capture_id":retained.capture_id,"ordinal":retained.ordinal,"semantic_hmac":semantic.to_vec()}),
            )
        }
        source::Observation::Absent => (
            None,
            None,
            json!({"source_id":source_id,"anchor":anchor,"anchor_qualified":anchor_qualified,"original":original.is_some(),"hold":"source_absent"}),
        ),
        source::Observation::Unqualified(reason) => (
            None,
            None,
            json!({"source_id":source_id,"anchor":anchor,"anchor_qualified":anchor_qualified,"original":original.is_some(),"hold":reason}),
        ),
    };
    let candidate = Uuid::new_v4();
    let sealed = s::seal(key, org, id, candidate, "recovery-candidate", &evidence)?;
    let stage_hash = stage.as_ref().map(|v| v.digest(key, org)).transpose()?;
    let assignee_hash = assignee.as_ref().map(|v| v.digest(key, org)).transpose()?;
    let mut additions = Vec::new();
    for value in [&stage, &assignee].into_iter().flatten() {
        let hash = value.digest(key, org)?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_people_recovery_key WHERE admission_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4)").bind(id).bind(org.0).bind(value.kind()).bind(hash.as_slice()).fetch_one(&mut *conn).await?;
        if !exists {
            let key_id = Uuid::new_v4();
            let encoded = s::seal(key, org, id, key_id, "recovery-key", value)?;
            additions.push((key_id, value, hash, encoded));
        }
    }
    let bytes = (source_id.len()
        + sealed.nonce.len()
        + sealed.ciphertext.len()
        + stage_hash.map_or(0, |_| 32)
        + assignee_hash.map_or(0, |_| 32)
        + additions
            .iter()
            .map(|(_, _, _, v)| 32 + v.nonce.len() + v.ciphertext.len())
            .sum::<usize>()) as i64;
    let reservation = s::reserve(conn, org, id, "prepare", None, bytes, policy).await?;
    for (key_id, value, hash, encoded) in additions {
        sqlx::query("INSERT INTO migration_people_recovery_key(id,admission_id,organization_id,kind,source_key_hmac,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(key_id).bind(id).bind(org.0).bind(value.kind()).bind(hash.as_slice()).bind(encoded.nonce.as_slice()).bind(encoded.ciphertext).execute(&mut *conn).await?;
    }
    sqlx::query("INSERT INTO migration_people_recovery_candidate(id,admission_id,organization_id,source_id,parent_import_id,parent_plan_id,original_manifest_id,anchor_admission_id,anchor_plan_id,anchor_item_id,stage_source_hmac,assignee_source_hmac,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
        .bind(candidate).bind(id).bind(org.0).bind(&source_id).bind(run.get::<Uuid,_>("parent_import_id")).bind(run.get::<Uuid,_>("parent_plan_id")).bind(original.map(|_|anchor)).bind(run.get::<Option<Uuid>,_>("recovery_admission_id")).bind(run.get::<Option<Uuid>,_>("recovery_admission_plan_id")).bind(if original.is_none(){Some(anchor)}else{None}).bind(stage_hash.as_ref().map(|v|v.as_slice())).bind(assignee_hash.as_ref().map(|v|v.as_slice())).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_admission SET recovery_checkpoint_id=$3,recovery_candidate_count=recovery_candidate_count+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(anchor).execute(&mut *conn).await?;
    s::release(conn, org, id, reservation, bytes).await?;
    if run.get::<bool, _>("recovery_remainder") {
        for source in [&stage, &assignee].into_iter().flatten() {
            inherit_choice(conn, key, policy, org, run, source).await?;
        }
    }
    Ok(())
}

pub(crate) async fn catalog(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    run: &PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = run.get("id");
    let old: String = run.get("preparation_checkpoint_key");
    let source_id:Option<String>=sqlx::query_scalar("SELECT source_id FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family='stages' AND source_id>$3 AND capture_sequence<=$4 GROUP BY source_id ORDER BY source_id LIMIT 1").bind(run.get::<Uuid,_>("newer_snapshot_id")).bind(org.0).bind(&old).bind(run.get::<i64,_>("newer_sequence")).fetch_optional(&mut *conn).await?;
    let Some(source_id) = source_id else {
        s::checkpoint(conn, org, id, "").await?;
        sqlx::query("UPDATE migration_people_admission SET recovery_catalog_complete=true WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
        return Ok(());
    };
    let observed = source::retained_record(
        conn,
        key,
        org,
        run.get("newer_snapshot_id"),
        Stream::Stages,
        run.get("newer_sequence"),
        &source_id,
    )
    .await?;
    let (hash, semantic, qualified) = match observed {
        source::Observation::Qualified(v) => {
            let hash = match &v.record.entity {
                Entity::Stage(stage) => stage
                    .label
                    .as_ref()
                    .filter(|s| s.len() <= 60 * 1024)
                    .map(|s| SourceKey::StageLabel(s.clone()).digest(key, org))
                    .transpose()?,
                _ => None,
            };
            let semantic = crypto::snapshot_hmac(
                key,
                org,
                &format!("semantic:{}", Stream::Stages.representation()),
                &v.record.canonical,
            );
            (
                hash,
                Some(semantic),
                stage_mapping_reasons_allowed(&v.record.reasons),
            )
        }
        _ => (None, None, false),
    };
    let delta = source_id.len() as i64 - old.len() as i64;
    let bytes =
        source_id.len() as i64 + hash.map_or(0, |_| 32) + semantic.map_or(0, |_| 32) + delta.max(0);
    let reserve = s::reserve(conn, org, id, "prepare", None, bytes.max(1), policy).await?;
    sqlx::query("INSERT INTO migration_people_recovery_catalog(admission_id,organization_id,source_id,source_key_hmac,semantic_hmac,qualified) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(org.0).bind(&source_id).bind(hash.as_ref().map(|h|h.as_slice())).bind(semantic.as_ref().map(|h|h.as_slice())).bind(qualified).execute(&mut *conn).await?;
    if delta < 0 {
        s::checkpoint(conn, org, id, &source_id).await?;
    }
    sqlx::query("UPDATE migration_people_admission SET preparation_checkpoint_key=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(source_id).execute(&mut *conn).await?;
    s::release(conn, org, id, reserve, bytes).await?;
    Ok(())
}

async fn inherit_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    run: &PgRow,
    source: &SourceKey,
) -> Result<(), MigrationError> {
    let id: Uuid = run.get("id");
    let hash = source.digest(key, org)?;
    let entry=sqlx::query("SELECT k.id,EXISTS(SELECT 1 FROM migration_people_recovery_choice c WHERE c.key_id=k.id AND c.admission_id=k.admission_id AND c.organization_id=k.organization_id) chosen FROM migration_people_recovery_key k WHERE k.admission_id=$1 AND k.organization_id=$2 AND k.kind=$3 AND k.source_key_hmac=$4").bind(id).bind(org.0).bind(source.kind()).bind(hash.as_slice()).fetch_one(&mut *conn).await?;
    if entry.get::<bool, _>("chosen") {
        return Ok(());
    }
    let old=sqlx::query("SELECT c.* FROM migration_people_recovery_choice c JOIN migration_people_admission a ON a.id=c.admission_id AND a.organization_id=c.organization_id WHERE a.id=$1 AND a.organization_id=$2 AND c.kind=$3 AND c.source_key_hmac=$4 AND c.revision<=a.recovery_frozen_revision ORDER BY c.revision DESC LIMIT 1").bind(run.get::<Option<Uuid>,_>("recovery_admission_id")).bind(org.0).bind(source.kind()).bind(hash.as_slice()).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let body: Value = s::open(
        key,
        org,
        old.get("admission_id"),
        old.get("id"),
        "recovery-choice",
        old.get("nonce"),
        old.get("ciphertext"),
    )?;
    let choice = Uuid::new_v4();
    let sealed = s::seal(key, org, id, choice, "recovery-choice", &body)?;
    use sha2::{Digest, Sha256};
    let previous:Option<Vec<u8>>=sqlx::query_scalar("SELECT recovery_draft_digest FROM migration_people_admission WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let mut chain = Sha256::new();
    chain.update(
        previous
            .as_deref()
            .unwrap_or(b"people-recovery-choice-chain-v1"),
    );
    chain.update(choice.as_bytes());
    chain.update(&sealed.ciphertext);
    let digest = chain.finalize();
    let bytes = (32
        + sealed.nonce.len()
        + sealed.ciphertext.len()
        + if previous.is_none() { 32 } else { 0 }) as i64;
    let reservation = s::reserve(conn, org, id, "prepare", None, bytes, policy).await?;
    sqlx::query("INSERT INTO migration_people_recovery_choice(id,admission_id,organization_id,key_id,revision,kind,source_key_hmac,disposition,target_id,nonce,ciphertext,predecessor_choice_id) VALUES($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10,$11)").bind(choice).bind(id).bind(org.0).bind(entry.get::<Uuid,_>("id")).bind(source.kind()).bind(hash.as_slice()).bind(old.get::<String,_>("disposition")).bind(old.get::<Option<Uuid>,_>("target_id")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(old.get::<Uuid,_>("id")).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_admission SET recovery_draft_digest=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(digest.as_slice()).execute(&mut *conn).await?;
    s::release(conn, org, id, reservation, bytes).await?;
    Ok(())
}

pub(super) fn stage_mapping_reasons_allowed(reasons: &[String]) -> bool {
    reasons.iter().all(|reason| {
        matches!(
            reason.as_str(),
            "stage_create_native_nul" | "stage_create_label_too_large"
        )
    })
}
