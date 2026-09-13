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
    let candidate=sqlx::query("SELECT id,organization_id,state FROM migration_people_admission WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(c) = candidate else { return Ok(false) };
    let id: Uuid = c.get("id");
    let org = OrganizationId(c.get("organization_id"));
    match c.get::<String, _>("state").as_str() {
        "preparing" => prepare_page(pool, key, policy, org, id).await?,
        _ => execute_one(pool, key, policy, release, org, id).await?,
    };
    Ok(true)
}
async fn prepare_page(
    pool: &PgPool,
    key: &RawPayloadKey,
    _policy: &SnapshotPolicy,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    let run = s::lock_run(&mut tx, org, id).await?;
    let frozen = s::validate_run(&mut tx, key, org, &run).await?;
    if run.get::<String, _>("preparation_phase") != "groups" {
        qualify_one_stream(&mut tx, key, org, id, &run).await?;
        tx.commit().await?;
        return Ok(());
    }
    let plan=if let Some(v)=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2 AND state='building' ORDER BY revision DESC LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *tx).await?{v}else{let p=Uuid::new_v4();let revision:i64=sqlx::query_scalar("SELECT COALESCE(max(revision),0)+1 FROM migration_people_admission_plan WHERE admission_id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *tx).await?;let input=s::seal(key,org,id,p,"inputs",&frozen)?;sqlx::query("INSERT INTO migration_people_admission_plan(id,admission_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) VALUES($1,$2,$3,$4,'building',$5,$6)").bind(p).bind(id).bind(org.0).bind(revision).bind(input.nonce.as_slice()).bind(input.ciphertext).execute(&mut *tx).await?;p};
    let checkpoint: String = run.get("preparation_checkpoint_key");
    // A qualified candidate may consume the entire 16 MiB raw-input budget,
    // so one descriptor is the bounded preparation transaction unit.
    let rows=sqlx::query("SELECT source_key,source_id FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND family='people' AND source_key>$3 ORDER BY source_key LIMIT 1").bind(run.get::<Uuid,_>("report_id")).bind(org.0).bind(&checkpoint).fetch_all(&mut *tx).await?;
    if rows.is_empty() {
        let totals=sqlx::query("SELECT total_count,eligible_count,already_imported_count,already_admitted_count,excluded_original_count,held_count,intended_contact_count FROM migration_people_admission_plan WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).fetch_one(&mut *tx).await?;
        let digest=Sha256::digest(serde_json::to_vec(&json!({"admission":id,"plan":plan,"total":totals.get::<i64,_>("total_count"),"eligible":totals.get::<i64,_>("eligible_count"),"already_imported":totals.get::<i64,_>("already_imported_count"),"already_admitted":totals.get::<i64,_>("already_admitted_count"),"excluded_original":totals.get::<i64,_>("excluded_original_count"),"held":totals.get::<i64,_>("held_count")})).map_err(|_|MigrationError::Crypto)?);
        sqlx::query("UPDATE migration_people_admission_plan SET state='ready',digest=$3,sealed_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(digest.as_slice()).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_people_admission SET state='ready',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(());
    }
    let mut last = checkpoint;
    for row in rows {
        let source_id: Option<String> = row.get("source_id");
        let key_id = source_id.clone().unwrap_or_else(|| row.get("source_key"));
        last = row.get("source_key");
        let (disp, record, capture, ordinal, stage, assignee) =
            classify(&mut tx, key, org, &run, &key_id).await?;
        let item = Uuid::new_v4();
        let target = Uuid::new_v4();
        let projection=record.as_ref().map(|v|json!({"first_name":v.0,"last_name":v.1,"stage_id":stage,"assigned_user_id":assignee})).unwrap_or(Value::Null);
        let provenance = json!({"source_id":key_id,"source_capture_id":capture,"source_ordinal":ordinal,"original_snapshot_id":run.get::<Uuid,_>("original_snapshot_id"),"original_sequence":run.get::<i64,_>("original_sequence").to_string(),"newer_snapshot_id":run.get::<Uuid,_>("newer_snapshot_id"),"newer_sequence":run.get::<i64,_>("newer_sequence").to_string(),"coverage":"core_only_notes_tasks_metadata_history_deferred"});
        let p = s::seal(key, org, id, item, "projection", &projection)?;
        let e = s::seal(key, org, id, item, "provenance", &provenance)?;
        let bound =
            (p.nonce.len() + p.ciphertext.len() + e.nonce.len() + e.ciphertext.len()) as i64;
        sqlx::query("INSERT INTO migration_people_admission_item(id,admission_id,plan_id,organization_id,source_key,source_id,prospective_person_id,disposition,source_capture_id,source_ordinal,projection_nonce,projection_ciphertext,provenance_nonce,provenance_ciphertext,item_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(item).bind(id).bind(plan).bind(org.0).bind(&key_id).bind(source_id).bind(target).bind(&disp).bind(capture).bind(ordinal).bind(p.nonce.as_slice()).bind(p.ciphertext).bind(e.nonce.as_slice()).bind(e.ciphertext).bind(bound.max(1)).execute(&mut *tx).await?;
        if let Some((_, _, contacts)) = record {
            for c in contacts {
                let contact_id = Uuid::new_v4();
                let sealed = s::seal(key, org, id, contact_id, "contact", &c)?;
                sqlx::query("INSERT INTO migration_people_admission_contact(id,item_id,admission_id,organization_id,kind,import_order,value_nonce,value_ciphertext,primary_contact) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(contact_id).bind(item).bind(id).bind(org.0).bind(c.kind).bind(c.import_order).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(c.import_order==0).execute(&mut *tx).await?;
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
    }
    sqlx::query("UPDATE migration_people_admission SET preparation_checkpoint_key=$3,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(last).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn qualify_one_stream(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &RawPayloadKey,
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
        &mut **tx,
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
    sqlx::query("UPDATE migration_people_admission SET preparation_phase=$3,preparation_checkpoint_key=$4,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
        .bind(id).bind(org.0).bind(phase).bind(checkpoint).execute(&mut **tx).await?;
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
        )>,
        Option<i32>,
        Option<Uuid>,
        Option<Uuid>,
    ),
    MigrationError,
> {
    let newer =
        source::retained_person(conn, key, org, run.get("newer_snapshot_id"), source_id).await?;
    let Some((record, capture, ordinal)) = newer else {
        return Ok(("held_evidence_gap".into(), None, None, None, None, None));
    };
    if !record.reasons.is_empty() {
        return Ok((
            "held_evidence_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    }
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
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    }
    let identity=sqlx::query("SELECT admission_id FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='people' AND source_id=$3").bind(org.0).bind(run.get::<i64,_>("source_account_id")).bind(source_id).fetch_optional(&mut *conn).await?;
    if let Some(v) = identity {
        return Ok((
            if v.get::<Option<Uuid>, _>("admission_id").is_some() {
                "already_admitted"
            } else {
                "already_imported"
            }
            .into(),
            None,
            Some(capture),
            Some(ordinal),
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
        ));
    };
    let Some(stage_label) = person.stage_label.as_deref() else {
        return Ok((
            "held_mapping_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    };
    // The original mapping key is the retained stage source ID, while People
    // carries its stage label.  Resolve only through a frozen qualified mapping
    // whose already-approved target still has that exact name; this is a
    // revalidation of the old choice, never a new name-based mapping.
    let stage:Option<Uuid>=sqlx::query_scalar("SELECT m.target_id FROM migration_import_mapping m JOIN stage s ON s.id=m.target_id AND s.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind='stage' AND m.qualified AND m.disposition IN ('existing','create') AND s.name=$3 ORDER BY m.id LIMIT 1").bind(run.get::<Uuid,_>("parent_plan_id")).bind(org.0).bind(stage_label).fetch_optional(&mut *conn).await?;
    let Some(stage) = stage else {
        return Ok((
            "held_mapping_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    };
    let live: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM stage WHERE id=$1 AND organization_id=$2)")
            .bind(stage)
            .bind(org.0)
            .fetch_one(&mut *conn)
            .await?;
    if !live {
        return Ok((
            "held_target".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    };
    let assignee=match person.assignee_key.as_deref(){None=>None,Some(k)=>sqlx::query_scalar("SELECT target_id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='assignee' AND source_key=$3 AND qualified AND disposition='member'").bind(run.get::<Uuid,_>("parent_plan_id")).bind(org.0).bind(k).fetch_optional(&mut *conn).await?};
    if person.assignee_key.is_some() && assignee.is_none() {
        return Ok((
            "held_mapping_gap".into(),
            None,
            Some(capture),
            Some(ordinal),
            None,
            None,
        ));
    };
    Ok((
        "eligible".into(),
        Some((person.first_name, person.last_name, person.contacts)),
        Some(capture),
        Some(ordinal),
        Some(stage),
        assignee,
    ))
}
async fn execute_one(
    pool: &PgPool,
    key: &RawPayloadKey,
    _policy: &SnapshotPolicy,
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
        sqlx::query("UPDATE migration_people_admission SET state='running',lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(token).execute(&mut *tx).await?;
        token
    } else {
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
        settle_hold(&mut tx, org, id, &item, "held_target").await?;
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
        settle_hold(&mut tx, org, id, &item, "held_identity").await?;
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
    sqlx::query("INSERT INTO person_admitted(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,admission_id,plan_id,item_id,result_id) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,$8,$9,$10)").bind(Uuid::new_v4()).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(person).bind(id).bind(plan).bind(item.get::<Uuid,_>("id")).bind(result).execute(&mut *tx).await?;
    let admitted_fact = Uuid::new_v4();
    sqlx::query("INSERT INTO stage_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_stage_id,to_stage_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration_admission')").bind(Uuid::new_v4()).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(admitted_fact).bind(person).bind(Uuid::parse_str(projection["stage_id"].as_str().ok_or(MigrationError::Crypto)?).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO assignment_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_user_id,to_user_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration_admission')").bind(Uuid::new_v4()).bind(org.0).bind(run.get::<Uuid,_>("initiated_by_user_id")).bind(now).bind(id).bind(admitted_fact).bind(person).bind(projection["assigned_user_id"].as_str().and_then(|value|Uuid::parse_str(value).ok())).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission_item SET settled_result_id=$3,settled_at=clock_timestamp(),disposition='settled' WHERE id=$1 AND admission_id=$2").bind(item.get::<Uuid,_>("id")).bind(id).bind(result).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission SET settled_items=settled_items+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
async fn settle_hold(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org: OrganizationId,
    id: Uuid,
    item: &sqlx::postgres::PgRow,
    disposition: &str,
) -> Result<(), MigrationError> {
    let result = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_admission_result(id,admission_id,item_id,organization_id,source_id,disposition,actor_user_id) SELECT $1,$2,$3,$4,COALESCE(source_id,source_key),$5,initiated_by_user_id FROM migration_people_admission_item i JOIN migration_people_admission a ON a.id=i.admission_id WHERE i.id=$3").bind(result).bind(id).bind(item.get::<Uuid,_>("id")).bind(org.0).bind(disposition).execute(&mut **tx).await?;
    sqlx::query("UPDATE migration_people_admission_item SET disposition=$3,settled_result_id=$4,settled_at=clock_timestamp() WHERE id=$1 AND admission_id=$2").bind(item.get::<Uuid,_>("id")).bind(id).bind(disposition).bind(result).execute(&mut **tx).await?;
    Ok(())
}
