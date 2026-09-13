//! Fenced one-Person admission worker. It never updates an existing Person.
use super::{
    import_source::Entity, people_admission_source as source, people_admission_store as s,
    snapshot::SnapshotPolicy, snapshot_source::Stream, MigrationError,
};
use crate::{auth::workspace::ReleaseReadiness, config::RawPayloadKey, ids::OrganizationId};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT id,organization_id,state,lifecycle_revision,lease_epoch FROM migration_people_admission WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(c) = candidate else { return Ok(false) };
    let id: Uuid = c.get("id");
    let org = OrganizationId(c.get("organization_id"));
    let state: String = c.get("state");
    // queued->running is part of the unit transaction. A unit failure rolls it
    // back, so the failure fence must match the claim seen by this dispatcher.
    let expected_revision = c.get::<i64, _>("lifecycle_revision");
    let expected_epoch = c.get::<i64, _>("lease_epoch");
    let outcome = match state.as_str() {
        "preparing" => prepare_page(pool, key, policy, org, id).await,
        _ => execute_one(pool, key, policy, release, org, id).await,
    };
    if let Err(error) = outcome {
        pause_after_failure(pool, org, id, expected_revision, expected_epoch, &error).await?;
        return Err(error);
    }
    Ok(true)
}

/// The unit transaction has rolled back before this lock is acquired. Pausing
/// separately fences a retry from replaying an uncertain native write.
async fn pause_after_failure(
    pool: &PgPool,
    org: OrganizationId,
    id: Uuid,
    expected_revision: i64,
    expected_epoch: i64,
    error: &MigrationError,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    s::lock_run(&mut tx, org, id).await?;
    let reason = match error {
        MigrationError::StorageLimit => "storage_limit",
        MigrationError::SourceNotEligible | MigrationError::Crypto => "source_integrity",
        MigrationError::ReleaseNotReady => "release_not_ready",
        MigrationError::Conflict | MigrationError::ImportBusy => "lease_conflict",
        MigrationError::Forbidden => "authority_changed",
        MigrationError::Database(_) => "database_failure",
        _ => "source_not_eligible",
    };
    sqlx::query("UPDATE migration_people_admission SET state='paused',pause_reason=$5,lease_token=NULL,lease_expires_at=NULL,lifecycle_revision=lifecycle_revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state IN ('preparing','queued','running') AND lifecycle_revision=$3 AND lease_epoch=$4")
        .bind(id).bind(org.0).bind(expected_revision).bind(expected_epoch).bind(reason).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn prepare_page(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    let run = s::lock_run(&mut tx, org, id).await?;
    let frozen = s::validate_run(&mut tx, key, org, &run).await?;
    lock_initiator_admin(&mut tx, org, run.get("initiated_by_user_id")).await?;
    if run.get::<String, _>("preparation_phase") != "groups" {
        qualify_one_stream(&mut tx, key, policy, org, id, &run).await?;
        tx.commit().await?;
        return Ok(());
    }
    let plan=if let Some(v)=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 AND state='building' ORDER BY revision DESC LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *tx).await?{v}else{let p=Uuid::new_v4();let revision:i64=sqlx::query_scalar("SELECT COALESCE(max(revision),0)+1 FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *tx).await?;let input=s::seal(key,org,id,p,"inputs",&frozen)?;let bytes=(input.nonce.len()+input.ciphertext.len()) as i64;let token=s::reserve(&mut tx,org,id,"prepare",None,bytes.max(1),policy).await?;sqlx::query("INSERT INTO migration_people_admission_plan(id,admission_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) VALUES($1,$2,$3,$4,'building',$5,$6)").bind(p).bind(id).bind(org.0).bind(revision).bind(input.nonce.as_slice()).bind(input.ciphertext).execute(&mut *tx).await?;s::release(&mut tx,org,id,token,bytes).await?;p};
    let checkpoint: String = run.get("preparation_checkpoint_key");
    // A qualified candidate may consume the entire 16 MiB raw-input budget,
    // so one descriptor is the bounded preparation transaction unit.
    let rows=sqlx::query("SELECT source_key,source_id FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND family='people' AND source_key>$3 ORDER BY source_key LIMIT 1").bind(run.get::<Uuid,_>("report_id")).bind(org.0).bind(&checkpoint).fetch_all(&mut *tx).await?;
    if rows.is_empty() {
        let totals=sqlx::query("SELECT total_count,eligible_count,already_imported_count,already_admitted_count,excluded_original_count,held_count,intended_contact_count FROM migration_people_admission_plan WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).fetch_one(&mut *tx).await?;
        let rolling: Option<Vec<u8>> = sqlx::query_scalar("SELECT digest FROM migration_people_admission_plan WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(plan).bind(org.0).fetch_one(&mut *tx).await?;
        let empty_digest = rolling.is_none();
        let rolling =
            rolling.unwrap_or_else(|| Sha256::digest(b"people-admission-items-v1").to_vec());
        let digest=Sha256::digest(serde_json::to_vec(&json!({"admission":id,"plan":plan,"frozen_inputs":frozen,"items_digest":rolling,"counts":{"total":totals.get::<i64,_>("total_count"),"eligible":totals.get::<i64,_>("eligible_count"),"already_imported":totals.get::<i64,_>("already_imported_count"),"already_admitted":totals.get::<i64,_>("already_admitted_count"),"excluded_original":totals.get::<i64,_>("excluded_original_count"),"held":totals.get::<i64,_>("held_count"),"contacts":totals.get::<i64,_>("intended_contact_count")}})).map_err(|_|MigrationError::Crypto)?);
        let reservation = if empty_digest {
            Some(
                s::reserve(
                    &mut tx,
                    org,
                    id,
                    "prepare",
                    None,
                    digest.len() as i64,
                    policy,
                )
                .await?,
            )
        } else {
            None
        };
        sqlx::query("UPDATE migration_people_admission_plan SET state='ready',digest=$3,sealed_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(digest.as_slice()).execute(&mut *tx).await?;
        if let Some(reservation) = reservation {
            s::release(&mut tx, org, id, reservation, digest.len() as i64).await?;
        }
        sqlx::query("UPDATE migration_people_admission SET state='ready',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(());
    }
    let mut last = checkpoint;
    for row in rows {
        let source_id: Option<String> = row.get("source_id");
        let key_id = source_id.clone().unwrap_or_else(|| row.get("source_key"));
        last = row.get("source_key");
        let (disp, record, capture, ordinal, stage, assignee, stage_mapping, assignee_mapping) =
            classify(&mut tx, key, org, &run, &key_id).await?;
        let item = Uuid::new_v4();
        let target = Uuid::new_v4();
        let projection=record.as_ref().map(|v|json!({"first_name":v.0,"last_name":v.1,"stage_id":stage,"assigned_user_id":assignee})).unwrap_or(Value::Null);
        let fields = record
            .as_ref()
            .map(|value| value.3.clone())
            .unwrap_or(Value::Null);
        let provenance = json!({"source_id":key_id,"source_capture_id":capture,"source_ordinal":ordinal,"original_snapshot_id":run.get::<Uuid,_>("original_snapshot_id"),"original_sequence":run.get::<i64,_>("original_sequence").to_string(),"newer_snapshot_id":run.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":run.get::<i64,_>("newer_sequence").to_string(),"plan_id":run.get::<Uuid,_>("parent_plan_id"),"engine_version":run.get::<String,_>("engine_version"),"stage_mapping_id":stage_mapping,"assignee_mapping_id":assignee_mapping,"fields":fields,"coverage":"core_only_notes_tasks_metadata_history_deferred"});
        let p = s::seal(key, org, id, item, "projection", &projection)?;
        let e = s::seal(key, org, id, item, "provenance", &provenance)?;
        let contacts = if let Some((_, _, contacts, _)) = record.as_ref() {
            let mut sealed = Vec::with_capacity(contacts.len());
            for contact in contacts {
                let contact_id = Uuid::new_v4();
                sealed.push((
                    contact_id,
                    contact,
                    s::seal(key, org, id, contact_id, "contact", contact)?,
                ));
            }
            sealed
        } else {
            Vec::new()
        };
        let contact_hashes = contacts.iter().map(|(contact_id, contact, sealed)| json!({"id":contact_id,"kind":contact.kind,"order":contact.import_order,"cipher":Sha256::digest(&sealed.ciphertext).to_vec()})).collect::<Vec<_>>();
        let bound = (key_id.len()
            + source_id.as_ref().map_or(0, String::len)
            + p.nonce.len()
            + p.ciphertext.len()
            + e.nonce.len()
            + e.ciphertext.len()
            + contacts
                .iter()
                .map(|(_, _, sealed)| sealed.nonce.len() + sealed.ciphertext.len())
                .sum::<usize>()) as i64;
        let token = s::reserve(&mut tx, org, id, "prepare", None, bound.max(1), policy).await?;
        sqlx::query("INSERT INTO migration_people_admission_item(id,admission_id,plan_id,organization_id,source_key,source_id,prospective_person_id,disposition,source_capture_id,source_ordinal,stage_mapping_id,assignee_mapping_id,projection_nonce,projection_ciphertext,provenance_nonce,provenance_ciphertext,item_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)").bind(item).bind(id).bind(plan).bind(org.0).bind(&key_id).bind(source_id.as_deref()).bind(target).bind(&disp).bind(capture).bind(ordinal).bind(stage_mapping).bind(assignee_mapping).bind(p.nonce.as_slice()).bind(p.ciphertext.as_slice()).bind(e.nonce.as_slice()).bind(e.ciphertext.as_slice()).bind(bound.max(1)).execute(&mut *tx).await?;
        if !contacts.is_empty() {
            for (contact_id, c, sealed) in contacts {
                sqlx::query("INSERT INTO migration_people_admission_contact(id,item_id,admission_id,organization_id,kind,import_order,value_nonce,value_ciphertext,primary_contact) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(contact_id).bind(item).bind(id).bind(org.0).bind(&c.kind).bind(c.import_order).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(c.import_order==0).execute(&mut *tx).await?;
                sqlx::query("UPDATE migration_people_admission_plan SET intended_contact_count=intended_contact_count+1 WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).execute(&mut *tx).await?;
            }
        }
        let col = match disp.as_str() {
            "eligible" => "eligible_count",
            "already_imported" => "already_imported_count",
            "already_admitted" => "already_admitted_count",
            "excluded_original" => "excluded_original_count",
            _ => "held_count",
        };
        sqlx::query(&format!("UPDATE migration_people_admission_plan SET total_count=total_count+1,{col}={col}+1,prepared_bytes=prepared_bytes+$3 WHERE id=$1 AND organization_id=$2")).bind(plan).bind(org.0).bind(bound).execute(&mut *tx).await?;
        let descriptor = json!({"id":item,"source_key":key_id,"source_id":source_id,"prospective_person_id":target,"disposition":disp,"capture":capture,"ordinal":ordinal,"stage_mapping_id":stage_mapping,"assignee_mapping_id":assignee_mapping,"projection_cipher":Sha256::digest(&p.ciphertext).to_vec(),"provenance_cipher":Sha256::digest(&e.ciphertext).to_vec(),"contacts":contact_hashes});
        let previous: Option<Vec<u8>> = sqlx::query_scalar("SELECT digest FROM migration_people_admission_plan WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(plan).bind(org.0).fetch_one(&mut *tx).await?;
        let first_item = previous.is_none();
        let mut rolling = Sha256::new();
        rolling.update(
            previous.unwrap_or_else(|| Sha256::digest(b"people-admission-items-v1").to_vec()),
        );
        rolling.update(serde_json::to_vec(&descriptor).map_err(|_| MigrationError::Crypto)?);
        let next = rolling.finalize();
        if first_item {
            // Reservations have one row per purpose. Releasing the persisted
            // item charge first is safe inside this transaction and makes the
            // rolling-digest charge a separate exact retained delta.
            s::release(&mut tx, org, id, token, bound).await?;
            let digest_reservation =
                s::reserve(&mut tx, org, id, "prepare", None, next.len() as i64, policy).await?;
            sqlx::query("UPDATE migration_people_admission_plan SET digest=$3,prepared_bytes=prepared_bytes+$4 WHERE id=$1 AND organization_id=$2 AND state='building'").bind(plan).bind(org.0).bind(next.as_slice()).bind(next.len() as i64).execute(&mut *tx).await?;
            s::release(&mut tx, org, id, digest_reservation, next.len() as i64).await?;
        } else {
            sqlx::query("UPDATE migration_people_admission_plan SET digest=$3 WHERE id=$1 AND organization_id=$2 AND state='building'").bind(plan).bind(org.0).bind(next.as_slice()).execute(&mut *tx).await?;
            s::release(&mut tx, org, id, token, bound).await?;
        }
    }
    persist_checkpoint(
        &mut tx,
        org,
        id,
        policy,
        &run.get::<String, _>("preparation_checkpoint_key"),
        "groups",
        &last,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
async fn qualify_one_stream(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    org: OrganizationId,
    id: Uuid,
    run: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let phase: String = run.get("preparation_phase");
    let (snapshot, sequence, stream, next) = match phase.as_str() {
        "original_people" => (
            run.get("original_snapshot_id"),
            run.get("original_sequence"),
            Stream::People,
            "newer_people",
        ),
        "newer_people" => (
            run.get("newer_snapshot_id"),
            run.get("newer_sequence"),
            Stream::People,
            "newer_users",
        ),
        "newer_users" => (
            run.get("newer_snapshot_id"),
            run.get("newer_sequence"),
            Stream::Users,
            "newer_stages",
        ),
        "newer_stages" => (
            run.get("newer_snapshot_id"),
            run.get("newer_sequence"),
            Stream::Stages,
            "groups",
        ),
        _ => return Err(MigrationError::SourceNotEligible),
    };
    let cursor = if run
        .get::<String, _>("preparation_checkpoint_key")
        .is_empty()
    {
        source::QualificationCursor::default()
    } else {
        serde_json::from_str(&run.get::<String, _>("preparation_checkpoint_key"))
            .map_err(|_| MigrationError::SourceNotEligible)?
    };
    let page = source::qualify_stream_page(
        tx,
        key,
        org,
        snapshot,
        run.get("source_account_id"),
        stream,
        sequence,
        cursor,
    )
    .await?;
    if page.raw_bytes > s::CAPTURE_LIMIT {
        return Err(MigrationError::StorageLimit);
    }
    let (phase, checkpoint) = if page.complete {
        (next, String::new())
    } else {
        (
            phase.as_str(),
            serde_json::to_string(&page.cursor).map_err(|_| MigrationError::Crypto)?,
        )
    };
    let old: String = run.get("preparation_checkpoint_key");
    persist_checkpoint(tx, org, id, policy, &old, phase, &checkpoint).await?;
    Ok(())
}

async fn persist_checkpoint(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: OrganizationId,
    id: Uuid,
    policy: &SnapshotPolicy,
    old: &str,
    phase: &str,
    checkpoint: &str,
) -> Result<(), MigrationError> {
    let delta = checkpoint.len() as i64 - old.len() as i64;
    if delta > 0 {
        let token = s::reserve(tx, org, id, "prepare", None, delta, policy).await?;
        sqlx::query("UPDATE migration_people_admission SET preparation_phase=$3,preparation_checkpoint_key=$4,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(org.0).bind(phase).bind(checkpoint).execute(&mut **tx).await?;
        s::release(tx, org, id, token, delta).await?;
    } else {
        s::checkpoint(tx, org, id, checkpoint).await?;
        sqlx::query("UPDATE migration_people_admission SET preparation_phase=$3,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(org.0).bind(phase).execute(&mut **tx).await?;
    }
    Ok(())
}
async fn classify(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &sqlx::postgres::PgRow,
    source_id: &str,
) -> Result<
    (
        String,
        Option<(
            Option<String>,
            Option<String>,
            Vec<super::import_source::ContactInput>,
            Value,
        )>,
        Option<Uuid>,
        Option<i32>,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
    ),
    MigrationError,
> {
    // Existing identity has precedence over any later raw observation. Its
    // target must still exist; a tombstone is a held condition, never a new
    // Person admission.
    let identity = sqlx::query("SELECT i.admission_id,EXISTS(SELECT 1 FROM person p WHERE p.id=i.target_id AND p.organization_id=i.organization_id) live FROM migration_import_identity i WHERE i.organization_id=$1 AND i.source_account_id=$2 AND i.family='people' AND i.source_id=$3")
        .bind(org.0).bind(run.get::<i64,_>("source_account_id")).bind(source_id).fetch_optional(&mut *conn).await?;
    if let Some(identity) = identity {
        let disposition = if !identity.get::<bool, _>("live") {
            "held_identity"
        } else if identity.get::<Option<Uuid>, _>("admission_id").is_some() {
            "already_admitted"
        } else {
            "already_imported"
        };
        return Ok((disposition.into(), None, None, None, None, None, None, None));
    }
    // Original presence excludes admission even if the current retained
    // stream later becomes malformed or lacks that record.
    if source::original_contains(
        conn,
        org,
        run.get("original_snapshot_id"),
        run.get("original_sequence"),
        source_id,
    )
    .await?
    {
        return Ok((
            "excluded_original".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
    }
    let newer = source::retained_record(
        conn,
        key,
        org,
        run.get("newer_snapshot_id"),
        Stream::People,
        run.get("newer_sequence"),
        source_id,
    )
    .await?;
    let newer = match newer {
        source::Observation::Qualified(value) => value,
        source::Observation::Unqualified(reason) => {
            tracing::warn!(admission_id = %run.get::<Uuid, _>("id"), source_id, reason, "admission source observation is unqualified");
            return Ok((
                "held_evidence_gap".into(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ));
        }
        source::Observation::Absent => {
            return Ok((
                "held_evidence_gap".into(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ));
        }
    };
    let capture = newer.capture_id;
    let ordinal = newer.ordinal;
    let raw_bytes = newer.raw_bytes;
    let record = newer.record;
    if !record.reasons.is_empty() {
        return Ok((
            "held_evidence_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
            None,
            None,
        ));
    }
    let mappings = match source::resolve_mappings(conn, key, org, run, &record).await {
        Ok(mappings) if raw_bytes.saturating_add(mappings.raw_bytes) <= s::CAPTURE_LIMIT => {
            mappings
        }
        Ok(_) | Err(MigrationError::SourceNotEligible) => {
            return Ok((
                "held_mapping_gap".into(),
                None,
                Some(capture),
                Some(ordinal),
                None,
                None,
                None,
                None,
            ))
        }
        Err(error) => return Err(error),
    };
    let canonical = String::from_utf8(record.canonical).map_err(|_| MigrationError::Crypto)?;
    let transformations =
        serde_json::to_string(&record.transformations).map_err(|_| MigrationError::Crypto)?;
    let reasons = serde_json::to_string(&record.reasons).map_err(|_| MigrationError::Crypto)?;
    let mut fields = serde_json::to_value(record.provenance).map_err(|_| MigrationError::Crypto)?;
    let Some(fields) = fields.as_object_mut() else {
        return Err(MigrationError::Crypto);
    };
    fields.insert("canonical".into(), Value::String(canonical));
    fields.insert("transformations".into(), Value::String(transformations));
    fields.insert("reasons".into(), Value::String(reasons));
    let fields = Value::Object(fields.clone());
    if !field_summary_fits(&fields)? {
        return Ok((
            "held_evidence_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
            None,
            None,
        ));
    }
    let Entity::People(person) = record.entity else {
        return Ok((
            "held_evidence_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
            None,
            None,
        ));
    };
    Ok((
        "eligible".into(),
        Some((person.first_name, person.last_name, person.contacts, fields)),
        Some(capture),
        Some(ordinal),
        Some(mappings.stage_id),
        mappings.assignee_id,
        Some(mappings.stage_mapping_id),
        mappings.assignee_mapping_id,
    ))
}

fn field_summary_fits(fields: &Value) -> Result<bool, MigrationError> {
    let Some(fields) = fields.as_object() else {
        return Ok(false);
    };
    let mut bytes = 2usize;
    for (name, value) in fields {
        if name.len() > 2048 {
            return Ok(false);
        }
        let text = value.as_str().ok_or(MigrationError::Crypto)?;
        let mut end = text.len().min(256);
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        let entry = json!({name: {"prefix": &text[..end], "total_bytes": text.len().to_string(), "truncated": text.len() > 256}});
        bytes = bytes
            .checked_add(
                serde_json::to_vec(&entry)
                    .map_err(|_| MigrationError::Crypto)?
                    .len(),
            )
            .ok_or(MigrationError::StorageLimit)?;
        if bytes > 128 * 1024 - 16 * 1024 {
            return Ok(false);
        }
    }
    Ok(true)
}
async fn execute_one(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    let run = s::lock_run(&mut tx, org, id).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_people_admission(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let lease = if run.get::<String, _>("state") == "queued" {
        let token = Uuid::new_v4();
        sqlx::query("UPDATE migration_people_admission SET state='running',lease_token=$3,lease_epoch=lease_epoch+1,lifecycle_revision=lifecycle_revision+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(token).execute(&mut *tx).await?;
        token
    } else {
        let unexpired: bool = sqlx::query_scalar("SELECT lease_expires_at > clock_timestamp() FROM migration_people_admission WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(org.0).fetch_optional(&mut *tx).await?.unwrap_or(false);
        if !unexpired {
            return Err(MigrationError::Conflict);
        }
        run.get::<Option<Uuid>, _>("lease_token")
            .ok_or(MigrationError::Conflict)?
    };
    let plan:Uuid=sqlx::query_scalar("SELECT confirmed_admission_plan_id FROM migration_people_admission WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *tx).await?;
    let plan_row=sqlx::query("SELECT inputs_nonce,inputs_ciphertext FROM migration_people_admission_plan WHERE id=$1 AND admission_id=$2 AND organization_id=$3 AND state='ready' FOR UPDATE").bind(plan).bind(id).bind(org.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Conflict)?;
    let sealed_inputs: Value = s::open(
        key,
        org,
        id,
        plan,
        "inputs",
        &plan_row.get::<Vec<u8>, _>("inputs_nonce"),
        &plan_row.get::<Vec<u8>, _>("inputs_ciphertext"),
    )?;
    if sealed_inputs != s::validate_run(&mut tx, key, org, &run).await? {
        return Err(MigrationError::SourceNotEligible);
    }
    let item=sqlx::query("SELECT * FROM migration_people_admission_item WHERE admission_id=$1 AND organization_id=$2 AND plan_id=$3 AND disposition='eligible' AND settled_at IS NULL ORDER BY id LIMIT 1 FOR UPDATE").bind(id).bind(org.0).bind(plan).fetch_optional(&mut *tx).await?;
    let Some(item) = item else {
        sqlx::query("UPDATE migration_people_admission SET state='completed',completed_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        s::release_control(&mut tx, org, id).await?;
        tx.commit().await?;
        return Ok(());
    };
    let person: Uuid = item.get("prospective_person_id");
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person WHERE id=$1)")
        .bind(person)
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        settle_hold(&mut tx, org, id, &item, "held_target", lease, policy).await?;
        tx.commit().await?;
        return Ok(());
    };
    let source_id: String = item
        .get::<Option<String>, _>("source_id")
        .unwrap_or_else(|| item.get("source_key"));
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "admission:{}/{}",
            run.get::<i64, _>("source_account_id"),
            source_id
        ))
        .execute(&mut *tx)
        .await?;
    let prior:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='people' AND source_id=$3)").bind(org.0).bind(run.get::<i64,_>("source_account_id")).bind(&source_id).fetch_one(&mut *tx).await?;
    if prior {
        settle_hold(&mut tx, org, id, &item, "held_identity", lease, policy).await?;
        tx.commit().await?;
        return Ok(());
    }
    let projection: Value = s::open(
        key,
        org,
        id,
        item.get("id"),
        "projection",
        &item.get::<Vec<u8>, _>("projection_nonce"),
        &item.get::<Vec<u8>, _>("projection_ciphertext"),
    )?;
    lock_initiator_admin(&mut tx, org, run.get("initiated_by_user_id")).await?;
    recheck_mapping_targets(&mut tx, org, &item, &projection).await?;
    let (disposition, current, capture, ordinal, stage, assignee, stage_mapping, assignee_mapping) =
        classify(&mut tx, key, org, &run, &source_id).await?;
    let expected = current.map(|value| {
        json!({
            "first_name": value.0,
            "last_name": value.1,
            "stage_id": stage,
            "assigned_user_id": assignee,
        })
    });
    if disposition != "eligible"
        || capture != item.get("source_capture_id")
        || ordinal != item.get("source_ordinal")
        || stage_mapping != item.get("stage_mapping_id")
        || assignee_mapping != item.get("assignee_mapping_id")
        || expected.as_ref() != Some(&projection)
    {
        return Err(MigrationError::SourceNotEligible);
    }
    // Native output adds only its variable ledger fields here. Projection,
    // provenance, and contacts were charged when they were persisted during
    // preparation and must never be charged a second time.
    let provenance_bytes = item.get::<Vec<u8>, _>("provenance_nonce").len()
        + item.get::<Vec<u8>, _>("provenance_ciphertext").len();
    let work_bytes =
        (2 * source_id.len() + "settled".len() + "people".len() + provenance_bytes) as i64;
    let work_reservation = s::reserve(
        &mut tx,
        org,
        id,
        "work",
        Some(lease),
        work_bytes.max(1),
        policy,
    )
    .await?;
    let permit = json!({"lease":lease,"item":item.get::<Uuid,_>("id")}).to_string();
    sqlx::query("SELECT set_config('crm.people_admission_permit',$1,true)")
        .bind(permit)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) VALUES($1,$2,$3,$4,$5,$6)").bind(person).bind(org.0).bind(projection["first_name"].as_str()).bind(projection["last_name"].as_str()).bind(Uuid::parse_str(projection["stage_id"].as_str().ok_or(MigrationError::Crypto)?).map_err(|_|MigrationError::Crypto)?).bind(projection["assigned_user_id"].as_str().and_then(|x|Uuid::parse_str(x).ok())).execute(&mut *tx).await?;
    for c in sqlx::query("SELECT * FROM migration_people_admission_contact WHERE item_id=$1 AND admission_id=$2 ORDER BY kind,import_order").bind(item.get::<Uuid,_>("id")).bind(id).fetch_all(&mut *tx).await?{let contact:super::import_source::ContactInput=s::open(key,org,id,c.get("id"),"contact",&c.get::<Vec<u8>,_>("value_nonce"),&c.get::<Vec<u8>,_>("value_ciphertext"))?;sqlx::query("INSERT INTO contact_method(id,organization_id,person_id,kind,value,normalized_value,import_order) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(c.get::<Uuid,_>("id")).bind(org.0).bind(person).bind(contact.kind).bind(contact.value).bind(contact.normalized_value).bind(c.get::<i32,_>("import_order")).execute(&mut *tx).await?;}
    let result = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query("INSERT INTO migration_people_admission_result(id,admission_id,item_id,organization_id,person_id,source_id,disposition,actor_user_id) VALUES($1,$2,$3,$4,$5,$6,'settled',$7)").bind(result).bind(id).bind(item.get::<Uuid,_>("id")).bind(org.0).bind(person).bind(&source_id).bind(run.get::<Uuid,_>("initiated_by_user_id")).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id,admission_id,admission_item_id,admission_result_id) VALUES($1,$2,'people',$3,$4,$5,$6,$7,$8,$9)").bind(org.0).bind(run.get::<i64,_>("source_account_id")).bind(&source_id).bind(person).bind(run.get::<Uuid,_>("parent_import_id")).bind(run.get::<Uuid,_>("parent_plan_id")).bind(id).bind(item.get::<Uuid,_>("id")).bind(result).execute(&mut *tx).await?;
    let provenance=sqlx::query("SELECT provenance_nonce,provenance_ciphertext FROM migration_people_admission_item WHERE id=$1").bind(item.get::<Uuid,_>("id")).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO person_admission_provenance(id,organization_id,person_id,admission_id,item_id,result_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::new_v4()).bind(org.0).bind(person).bind(id).bind(item.get::<Uuid,_>("id")).bind(result).bind(provenance.get::<Vec<u8>,_>("provenance_nonce")).bind(provenance.get::<Vec<u8>,_>("provenance_ciphertext")).execute(&mut *tx).await?;
    let admitted_fact = Uuid::new_v4();
    sqlx::query("INSERT INTO person_admitted(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,admission_id,plan_id,item_id,result_id) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,$8,$9,$10)").bind(admitted_fact).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(person).bind(id).bind(plan).bind(item.get::<Uuid,_>("id")).bind(result).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO stage_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_stage_id,to_stage_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration_admission')").bind(Uuid::new_v4()).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(admitted_fact).bind(person).bind(Uuid::parse_str(projection["stage_id"].as_str().ok_or(MigrationError::Crypto)?).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO assignment_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_user_id,to_user_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration_admission')").bind(Uuid::new_v4()).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(admitted_fact).bind(person).bind(projection["assigned_user_id"].as_str().and_then(|value|Uuid::parse_str(value).ok())).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission_item SET settled_result_id=$3,settled_at=clock_timestamp(),disposition='settled' WHERE id=$1 AND admission_id=$2").bind(item.get::<Uuid,_>("id")).bind(id).bind(result).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission SET settled_items=settled_items+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
    s::release(&mut tx, org, id, work_reservation, work_bytes).await?;
    tx.commit().await?;
    Ok(())
}

async fn lock_initiator_admin(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: OrganizationId,
    user: Uuid,
) -> Result<(), MigrationError> {
    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE",
    )
    .bind(org.0)
    .bind(user)
    .fetch_optional(&mut **tx)
    .await?;
    if active.is_some() {
        Ok(())
    } else {
        Err(MigrationError::Forbidden)
    }
}

async fn recheck_mapping_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: OrganizationId,
    item: &sqlx::postgres::PgRow,
    projection: &Value,
) -> Result<(), MigrationError> {
    let stage: Uuid = Uuid::parse_str(
        projection["stage_id"]
            .as_str()
            .ok_or(MigrationError::Crypto)?,
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mapping: Option<Uuid> = sqlx::query_scalar(
        "SELECT target_id FROM migration_import_mapping WHERE id=$1 AND organization_id=$2 AND kind='stage' FOR SHARE",
    )
    .bind(item.get::<Option<Uuid>, _>("stage_mapping_id").ok_or(MigrationError::SourceNotEligible)?)
    .bind(org.0)
    .fetch_optional(&mut **tx)
    .await?;
    if mapping != Some(stage) {
        return Err(MigrationError::SourceNotEligible);
    }
    let live: bool = sqlx::query_scalar("SELECT crm_people_admission_lock_stage($1,$2)")
        .bind(org.0)
        .bind(stage)
        .fetch_one(&mut **tx)
        .await?;
    if !live {
        return Err(MigrationError::SourceNotEligible);
    }
    let assigned = projection["assigned_user_id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok());
    let mapping_id: Option<Uuid> = item.get("assignee_mapping_id");
    match (assigned, mapping_id) {
        (None, None) => Ok(()),
        (Some(user), Some(mapping_id)) => {
            let target: Option<Uuid> = sqlx::query_scalar(
                "SELECT target_id FROM migration_import_mapping WHERE id=$1 AND organization_id=$2 AND kind='assignee' FOR SHARE",
            )
            .bind(mapping_id).bind(org.0).fetch_optional(&mut **tx).await?;
            if target != Some(user) {
                return Err(MigrationError::SourceNotEligible);
            }
            let active: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' FOR SHARE")
                .bind(org.0).bind(user).fetch_optional(&mut **tx).await?;
            if active == Some(user) {
                Ok(())
            } else {
                Err(MigrationError::SourceNotEligible)
            }
        }
        _ => Err(MigrationError::SourceNotEligible),
    }
}

async fn settle_hold(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: OrganizationId,
    id: Uuid,
    item: &sqlx::postgres::PgRow,
    disposition: &str,
    lease: Uuid,
    policy: &SnapshotPolicy,
) -> Result<(), MigrationError> {
    let source: String = item
        .get::<Option<String>, _>("source_id")
        .unwrap_or_else(|| item.get("source_key"));
    let bytes = (source.len() + disposition.len()) as i64;
    let reservation = s::reserve(
        tx,
        org,
        id,
        "work",
        Some(lease),
        bytes.max(1),
        policy,
    )
    .await?;
    let result = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_admission_result(id,admission_id,item_id,organization_id,source_id,disposition,actor_user_id) SELECT $1,$2,$3,$4,COALESCE(source_id,source_key),$5,initiated_by_user_id FROM migration_people_admission_item i JOIN migration_people_admission a ON a.id=i.admission_id WHERE i.id=$3").bind(result).bind(id).bind(item.get::<Uuid,_>("id")).bind(org.0).bind(disposition).execute(&mut **tx).await?;
    sqlx::query("UPDATE migration_people_admission_item SET disposition=$3,settled_result_id=$4,settled_at=clock_timestamp() WHERE id=$1 AND admission_id=$2").bind(item.get::<Uuid,_>("id")).bind(id).bind(disposition).bind(result).execute(&mut **tx).await?;
    s::release(tx, org, id, reservation, bytes).await?;
    Ok(())
}
