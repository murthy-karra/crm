//! Fenced retained-only preparation/execution loop for People refreshes.
use super::{
    crypto,
    import_source::{Entity, ExtractedRecord},
    imports, people_refresh_source as source, people_refresh_store as s,
    snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{auth::workspace::ReleaseReadiness, config::RawPayloadKey, ids::OrganizationId};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

enum Instruction<T> {
    NoInstruction,
    Apply(T),
}

async fn original_mapping_target(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    kind: &str,
    source_key: Option<&str>,
    label_hmac: Option<&[u8]>,
) -> Result<Option<Uuid>, MigrationError> {
    let rows = if let Some(source_key) = source_key {
        sqlx::query("SELECT * FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4 ORDER BY id LIMIT 2")
            .bind(plan).bind(org.0).bind(kind).bind(source_key).fetch_all(&mut *conn).await?
    } else {
        sqlx::query("SELECT * FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND label_hmac=$4 ORDER BY source_key LIMIT 2")
            .bind(plan).bind(org.0).bind(kind).bind(label_hmac.ok_or(MigrationError::Crypto)?).fetch_all(&mut *conn).await?
    };
    if rows.len() != 1 {
        return Err(MigrationError::SourceNotEligible);
    }
    let row = &rows[0];
    let disposition: String = row.get("disposition");
    let target: Option<Uuid> = row.get("target_id");
    if !row.get::<bool, _>("qualified")
        || !matches!(
            (kind, disposition.as_str()),
            ("stage", "existing" | "create") | ("assignee", "member" | "unassigned")
        )
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let body: Value = imports::open(
        key,
        org,
        snapshot,
        plan,
        row.get("id"),
        "mapping",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    match (kind, target) {
        ("stage", Some(target)) => {
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM stage WHERE organization_id=$1 AND id=$2")
                    .bind(org.0)
                    .bind(target)
                    .fetch_optional(&mut *conn)
                    .await?;
            if body["target"]["name"] != json!(name) {
                return Err(MigrationError::SourceNotEligible);
            }
        }
        ("assignee", Some(target)) => {
            let email: Option<String> = sqlx::query_scalar("SELECT u.email FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.user_id=$2 AND m.status='active'")
                .bind(org.0).bind(target).fetch_optional(&mut *conn).await?;
            if body["target"]["email"] != json!(email) {
                return Err(MigrationError::SourceNotEligible);
            }
        }
        ("assignee", None) if disposition == "unassigned" => {}
        _ => return Err(MigrationError::SourceNotEligible),
    }
    Ok(target)
}

async fn stage_instruction(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    newer: &ExtractedRecord,
) -> Result<Instruction<Uuid>, MigrationError> {
    let Some(raw) = newer.provenance.get("stage") else {
        return Ok(Instruction::NoInstruction);
    };
    let raw: Value = serde_json::from_str(raw).map_err(|_| MigrationError::Crypto)?;
    let source_key = match raw {
        Value::Null => Some("missing".to_owned()),
        Value::String(label) if label.trim().is_empty() => Some("missing".to_owned()),
        Value::String(label) if !label.contains('\0') => {
            let label = label.trim();
            let hmac = crypto::snapshot_hmac(key, org, "import-stage-label", label.as_bytes());
            let target =
                original_mapping_target(conn, key, org, snapshot, plan, "stage", None, Some(&hmac))
                    .await?;
            return target
                .map(Instruction::Apply)
                .ok_or(MigrationError::SourceNotEligible);
        }
        _ => return Err(MigrationError::SourceNotEligible),
    };
    let target = original_mapping_target(
        conn,
        key,
        org,
        snapshot,
        plan,
        "stage",
        source_key.as_deref(),
        None,
    )
    .await?;
    target
        .map(Instruction::Apply)
        .ok_or(MigrationError::SourceNotEligible)
}

async fn assignment_instruction(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    newer: &ExtractedRecord,
) -> Result<Instruction<Option<Uuid>>, MigrationError> {
    let Entity::People(person) = &newer.entity else {
        return Err(MigrationError::SourceNotEligible);
    };
    let user = newer.provenance.get("assignedUserId");
    let pond = newer.provenance.get("assignedPondId");
    let assigned_to = newer.provenance.get("assignedTo");
    let assigned_to_is_clear = assigned_to.is_none_or(|raw| raw == "null");
    if !assigned_to_is_clear
        || newer
            .reasons
            .iter()
            .any(|reason| reason.starts_with("assignment_"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    // A qualified positive reference is an instruction even when the other
    // reference key was omitted. Both null keys are required only for a clear.
    if let Some(source_key) = person.assignee_key.as_deref() {
        let target = original_mapping_target(
            conn,
            key,
            org,
            snapshot,
            plan,
            "assignee",
            Some(source_key),
            None,
        )
        .await?;
        return Ok(Instruction::Apply(target));
    }
    let (Some(user), Some(pond)) = (user, pond) else {
        return Ok(Instruction::NoInstruction);
    };
    let user: Value = serde_json::from_str(user).map_err(|_| MigrationError::Crypto)?;
    let pond: Value = serde_json::from_str(pond).map_err(|_| MigrationError::Crypto)?;
    if user.is_null() && pond.is_null() {
        Ok(Instruction::Apply(None))
    } else {
        Err(MigrationError::SourceNotEligible)
    }
}
fn projection(v: &ExtractedRecord, stage: Option<Uuid>, assignee: Option<Uuid>) -> Value {
    match &v.entity {
        Entity::People(p) => {
            json!({"first_name":p.first_name,"last_name":p.last_name,"contacts":p.contacts.iter().map(|c|json!({"kind":c.kind,"value":c.value,"normalized_value":c.normalized_value,"import_order":c.import_order})).collect::<Vec<_>>(),"stage_id":stage,"assigned_user_id":assignee})
        }
        _ => json!({"invalid":true}),
    }
}
fn preserve_owned_ids(b: &Value, n: &mut Value) -> Result<(), MigrationError> {
    let baseline = b["contacts"].as_array().ok_or(MigrationError::Crypto)?;
    let desired = n["contacts"].as_array_mut().ok_or(MigrationError::Crypto)?;
    for contact in desired {
        let existing = baseline.iter().find(|old| {
            old["kind"] == contact["kind"] && old["normalized_value"] == contact["normalized_value"]
        });
        contact["id"] = match existing {
            Some(old) => old["id"].clone(),
            None => json!(Uuid::new_v4()),
        };
    }
    Ok(())
}
fn clear_counts(b: &Value, n: &Value) -> (i64, i64, i64) {
    let names = ["first_name", "last_name"]
        .into_iter()
        .filter(|field| !b[*field].is_null() && n[*field].is_null())
        .count() as i64;
    let assignment = i64::from(!b["assigned_user_id"].is_null() && n["assigned_user_id"].is_null());
    let removals = b["contacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|old| {
            !n["contacts"]
                .as_array()
                .is_some_and(|next| next.iter().any(|candidate| candidate["id"] == old["id"]))
        })
        .count() as i64;
    (names, assignment, removals)
}
fn digest(v: &Value) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(v).unwrap_or_default());
    h.finalize().into()
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    _policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    let r=sqlx::query("SELECT * FROM migration_people_refresh WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(r) = r else { return Ok(false) };
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_people_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let r = sqlx::query(
        "SELECT * FROM migration_people_refresh WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_one(&mut *tx)
    .await?;
    if r.get::<String, _>("state") == "preparing" {
        prepare(&mut tx, key, &r).await?;
        tx.commit().await?;
        return Ok(true);
    };
    if r.get::<String, _>("state") == "queued" {
        sqlx::query("UPDATE migration_people_refresh SET state='running',lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(Uuid::new_v4()).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(true);
    };
    let token: Option<Uuid> = r.get("lease_token");
    let expired = r
        .get::<Option<chrono::DateTime<Utc>>, _>("lease_expires_at")
        .is_none_or(|v| v <= Utc::now());
    if token.is_none() || expired {
        sqlx::query("UPDATE migration_people_refresh SET state='paused',pause_reason='lease_expired',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(true);
    };
    execute_noop(&mut tx, key, &r).await?;
    tx.commit().await?;
    Ok(true)
}
async fn prepare(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let plan = Uuid::new_v4();
    let revision: i64=sqlx::query_scalar("SELECT COALESCE(max(revision),0)+1 FROM migration_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let inputs = json!({"report_id":r.get::<Uuid,_>("report_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id")});
    let sealed = s::seal(key, org, id, plan, "inputs", &inputs)?;
    sqlx::query("INSERT INTO migration_people_refresh_plan(id,refresh_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) VALUES($1,$2,$3,$4,'building',$5,$6)").bind(plan).bind(id).bind(org.0).bind(revision).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    let rows=sqlx::query("SELECT ir.*,i.snapshot_id,m.stage_mapping_id,m.assignee_mapping_id FROM migration_import_result ir JOIN migration_import i ON i.id=ir.import_id AND i.organization_id=ir.organization_id JOIN migration_import_manifest m ON m.id=ir.manifest_id AND m.organization_id=ir.organization_id WHERE ir.import_id=$1 AND ir.organization_id=$2 AND ir.disposition='imported' ORDER BY ir.source_id LIMIT 25001").bind(r.get::<Uuid,_>("parent_import_id")).bind(org.0).fetch_all(&mut *conn).await?;
    if rows.len() > 25000 {
        return Err(MigrationError::StorageLimit);
    };
    let mut eligible = 0;
    let mut current = 0;
    let mut held = 0;
    let mut no_instruction = 0;
    let mut name_clears = 0;
    let mut assignment_clears = 0;
    let mut contact_removals = 0;
    for row in rows {
        let item = Uuid::new_v4();
        let source_id: String = row.get("source_id");
        let baseline: ExtractedRecord = imports::open(
            key,
            org,
            row.get("snapshot_id"),
            r.get("parent_plan_id"),
            row.get("id"),
            "provenance",
            &row.get::<Vec<u8>, _>("provenance_nonce"),
            &row.get::<Vec<u8>, _>("provenance_ciphertext"),
        )?;
        let stage: Option<Uuid> = sqlx::query_scalar(
            "SELECT target_id FROM migration_import_mapping WHERE id=$1 AND organization_id=$2",
        )
        .bind(row.get::<Option<Uuid>, _>("stage_mapping_id"))
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
        let assignee: Option<Uuid> = sqlx::query_scalar(
            "SELECT target_id FROM migration_import_mapping WHERE id=$1 AND organization_id=$2",
        )
        .bind(row.get::<Option<Uuid>, _>("assignee_mapping_id"))
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
        // B is original executable provenance plus the immutable contact-ID
        // receipt. It is never reconstructed from this current Person row.
        let mut b = projection(&baseline, stage, assignee);
        let owned = sqlx::query("SELECT contact_id,kind,import_order FROM migration_import_contact WHERE result_id=$1 AND organization_id=$2 ORDER BY kind,import_order")
            .bind(row.get::<Uuid,_>("id")).bind(org.0).fetch_all(&mut *conn).await?;
        let contacts = b["contacts"].as_array_mut().ok_or(MigrationError::Crypto)?;
        if contacts.len() != owned.len() {
            return Err(MigrationError::Crypto);
        }
        for (contact, owned) in contacts.iter_mut().zip(owned) {
            if contact["kind"] != owned.get::<String, _>("kind")
                || contact["import_order"] != owned.get::<i32, _>("import_order")
            {
                return Err(MigrationError::Crypto);
            }
            contact["id"] = json!(owned.get::<Uuid, _>("contact_id"));
        }
        // A later settled result is the only successor to original import
        // provenance. Current native state is deliberately absent from B.
        let previous=sqlx::query("SELECT refresh_id,projection_row_id,projection_nonce,projection_ciphertext FROM migration_people_refresh_baseline WHERE organization_id=$1 AND parent_import_id=$2 AND source_id=$3").bind(org.0).bind(r.get::<Uuid,_>("parent_import_id")).bind(&source_id).fetch_optional(&mut *conn).await?;
        if let Some(previous) = previous {
            b = s::open(
                key,
                org,
                previous.get("refresh_id"),
                previous.get("projection_row_id"),
                "last-baseline",
                &previous.get::<Vec<u8>, _>("projection_nonce"),
                &previous.get::<Vec<u8>, _>("projection_ciphertext"),
            )?;
        }
        // Seed only once from original executable provenance.
        let sealed_baseline = s::seal(key, org, id, item, "last-baseline", &b)?;
        sqlx::query("INSERT INTO migration_people_refresh_baseline(organization_id,parent_import_id,source_id,person_id,refresh_id,original_result_id,projection_row_id,projection_nonce,projection_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(organization_id,parent_import_id,source_id) DO NOTHING")
            .bind(org.0).bind(r.get::<Uuid,_>("parent_import_id")).bind(&source_id).bind(row.get::<Option<Uuid>,_>("person_id")).bind(id).bind(row.get::<Uuid,_>("id")).bind(item).bind(sealed_baseline.nonce.as_slice()).bind(sealed_baseline.ciphertext).execute(&mut *conn).await?;
        let native=sqlx::query("SELECT p.first_name,p.last_name,p.stage_id,p.assigned_user_id,COALESCE(jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'value',c.value,'normalized_value',c.normalized_value,'import_order',c.import_order) ORDER BY c.kind,c.import_order) FILTER(WHERE c.id IS NOT NULL),'[]') contacts FROM person p LEFT JOIN contact_method c ON c.person_id=p.id AND c.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 GROUP BY p.id").bind(row.get::<Option<Uuid>,_>("person_id")).bind(org.0).fetch_optional(&mut *conn).await?;
        let Some(native) = native else {
            held += 1;
            insert_item(
                conn,
                key,
                org,
                id,
                plan,
                item,
                &source_id,
                row.get("id"),
                row.get("person_id"),
                "held_target_missing",
                &b,
                &json!({}),
                &json!({}),
                None,
                None,
                &[],
            )
            .await?;
            continue;
        };
        let c = json!({"first_name":native.get::<Option<String>,_>("first_name"),"last_name":native.get::<Option<String>,_>("last_name"),"stage_id":native.get::<Uuid,_>("stage_id"),"assigned_user_id":native.get::<Option<Uuid>,_>("assigned_user_id"),"contacts":native.get::<Value,_>("contacts")});
        let newer =
            source::retained_person(conn, key, org, r.get("newer_snapshot_id"), &source_id).await?;
        let Some((newer, capture, ordinal)) = newer else {
            held += 1;
            insert_item(
                conn,
                key,
                org,
                id,
                plan,
                item,
                &source_id,
                row.get("id"),
                row.get("person_id"),
                "held_evidence_gap",
                &b,
                &c,
                &json!({}),
                None,
                None,
                &[],
            )
            .await?;
            continue;
        };
        let source::PeopleOverlay {
            projection: mut n,
            no_instruction: mut instructions,
        } = match source::overlay_people(&b, &newer) {
            Ok(overlay) => overlay,
            Err(MigrationError::SourceNotEligible) => {
                held += 1;
                insert_item(
                    conn,
                    key,
                    org,
                    id,
                    plan,
                    item,
                    &source_id,
                    row.get("id"),
                    row.get("person_id"),
                    "held_evidence_gap",
                    &b,
                    &c,
                    &json!({}),
                    Some(capture),
                    Some(ordinal),
                    &[],
                )
                .await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let new_stage = match stage_instruction(
            conn,
            key,
            org,
            row.get("snapshot_id"),
            r.get("parent_plan_id"),
            &newer,
        )
        .await
        {
            Ok(instruction) => instruction,
            Err(MigrationError::SourceNotEligible) => {
                held += 1;
                insert_item(
                    conn,
                    key,
                    org,
                    id,
                    plan,
                    item,
                    &source_id,
                    row.get("id"),
                    row.get("person_id"),
                    "held_mapping_gap",
                    &b,
                    &c,
                    &json!({}),
                    Some(capture),
                    Some(ordinal),
                    &instructions,
                )
                .await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        match new_stage {
            Instruction::NoInstruction => instructions.push("stage"),
            Instruction::Apply(stage) => n["stage_id"] = json!(stage),
        }
        let new_assignee = match assignment_instruction(
            conn,
            key,
            org,
            row.get("snapshot_id"),
            r.get("parent_plan_id"),
            &newer,
        )
        .await
        {
            Ok(instruction) => instruction,
            Err(MigrationError::SourceNotEligible) => {
                held += 1;
                insert_item(
                    conn,
                    key,
                    org,
                    id,
                    plan,
                    item,
                    &source_id,
                    row.get("id"),
                    row.get("person_id"),
                    "held_mapping_gap",
                    &b,
                    &c,
                    &json!({}),
                    Some(capture),
                    Some(ordinal),
                    &instructions,
                )
                .await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        match new_assignee {
            Instruction::NoInstruction => instructions.push("assignment"),
            Instruction::Apply(assignee) => n["assigned_user_id"] = json!(assignee),
        }
        preserve_owned_ids(&b, &mut n)?;
        let (names, assignments, contacts) = clear_counts(&b, &n);
        name_clears += names;
        assignment_clears += assignments;
        contact_removals += contacts;
        no_instruction += instructions.len() as i64;
        let disposition = if c != b {
            "held_local_change"
        } else if n == b {
            "already_current"
        } else {
            "eligible"
        };
        if disposition == "eligible" {
            eligible += 1
        } else {
            current += 1
        };
        insert_item(
            conn,
            key,
            org,
            id,
            plan,
            item,
            &source_id,
            row.get("id"),
            row.get("person_id"),
            disposition,
            &b,
            &c,
            &n,
            Some(capture),
            Some(ordinal),
            &instructions,
        )
        .await?;
    }
    let d = digest(
        &json!({"refresh":id,"plan":plan,"eligible":eligible,"current":current,"held":held,"no_instruction":no_instruction,"name_clears":name_clears,"assignment_clears":assignment_clears,"contact_removals":contact_removals}),
    );
    sqlx::query("UPDATE migration_people_refresh_plan SET state='ready',digest=$3,eligible_count=$4,already_current_count=$5,held_count=$6,no_instruction_count=$7,name_clear_count=$8,assignment_clear_count=$9,contact_removal_count=$10,sealed_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(d.as_slice()).bind(eligible).bind(current).bind(held).bind(no_instruction).bind(name_clears).bind(assignment_clears).bind(contact_removals).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_refresh SET state='ready',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
    Ok(())
}
async fn insert_item(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    refresh: Uuid,
    plan: Uuid,
    item: Uuid,
    source_id: &str,
    result: Uuid,
    person: Option<Uuid>,
    disposition: &str,
    b: &Value,
    c: &Value,
    n: &Value,
    capture: Option<Uuid>,
    ordinal: Option<i32>,
    instructions: &[&str],
) -> Result<(), MigrationError> {
    let a = s::seal(key, org, refresh, item, "baseline", b)?;
    let d = s::seal(key, org, refresh, item, "current", c)?;
    let e = s::seal(key, org, refresh, item, "proposed", n)?;
    let instructions = s::seal(key, org, refresh, item, "instructions", &instructions)?;
    sqlx::query("INSERT INTO migration_people_refresh_item(id,refresh_id,plan_id,organization_id,source_id,person_id,original_result_id,disposition,proposed_nonce,proposed_ciphertext,baseline_nonce,baseline_ciphertext,current_nonce,current_ciphertext,instructions_nonce,instructions_ciphertext,source_capture_id,source_ordinal,item_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,65536)").bind(item).bind(refresh).bind(plan).bind(org.0).bind(source_id).bind(person).bind(result).bind(disposition).bind(e.nonce.as_slice()).bind(e.ciphertext).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(d.nonce.as_slice()).bind(d.ciphertext).bind(instructions.nonce.as_slice()).bind(instructions.ciphertext).bind(capture).bind(ordinal).execute(&mut *conn).await?;
    for (side, projection) in [("baseline", b), ("current", c), ("proposed", n)] {
        for contact in projection["contacts"].as_array().unwrap_or(&vec![]) {
            let id = Uuid::new_v4();
            let sealed = s::seal(key, org, refresh, id, "contact", contact)?;
            sqlx::query("INSERT INTO migration_people_refresh_contact(id,item_id,refresh_id,organization_id,side,contact_id,kind,import_order,value_nonce,value_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(item_id,side,kind,import_order) DO NOTHING").bind(id).bind(item).bind(refresh).bind(org.0).bind(side).bind(contact["id"].as_str().and_then(|v|Uuid::parse_str(v).ok())).bind(contact["kind"].as_str()).bind(contact["import_order"].as_i64().unwrap_or_default() as i32).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        }
    }
    Ok(())
}
async fn execute_noop(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let item=sqlx::query("SELECT * FROM migration_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND settled_at IS NULL ORDER BY id LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *conn).await?;
    let Some(item) = item else {
        sqlx::query("UPDATE migration_people_refresh SET state='completed',completed_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
        return Ok(());
    };
    let item_id: Uuid = item.get("id");
    let baseline: Value = s::open(
        key,
        org,
        id,
        item_id,
        "baseline",
        &item.get::<Vec<u8>, _>("baseline_nonce"),
        &item.get::<Vec<u8>, _>("baseline_ciphertext"),
    )?;
    let expected: Value = s::open(
        key,
        org,
        id,
        item_id,
        "current",
        &item.get::<Vec<u8>, _>("current_nonce"),
        &item.get::<Vec<u8>, _>("current_ciphertext"),
    )?;
    let proposed: Value = s::open(
        key,
        org,
        id,
        item_id,
        "proposed",
        &item.get::<Vec<u8>, _>("proposed_nonce"),
        &item.get::<Vec<u8>, _>("proposed_ciphertext"),
    )?;
    let person = item
        .get::<Option<Uuid>, _>("person_id")
        .ok_or(MigrationError::Conflict)?;
    let authorized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_people_refresh x JOIN migration_workspace w ON w.organization_id=x.organization_id AND w.import_id=x.parent_import_id AND w.plan_id=x.parent_plan_id JOIN organization_membership m ON m.organization_id=x.organization_id AND m.user_id=x.initiated_by_user_id WHERE x.id=$1 AND x.organization_id=$2 AND x.state='running' AND x.lease_token=$3 AND x.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active')").bind(id).bind(org.0).bind(r.get::<Uuid,_>("lease_token")).fetch_one(&mut *conn).await?;
    if !authorized {
        return Err(MigrationError::Forbidden);
    }
    // PostgreSQL cannot lock an aggregate result. Lock the concrete Person
    // first, then assemble its complete owned/native contact projection.
    if sqlx::query("SELECT id FROM person WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(person)
        .bind(org.0)
        .fetch_optional(&mut *conn)
        .await?
        .is_none()
    {
        return Err(MigrationError::Conflict);
    }
    let native=sqlx::query("SELECT p.first_name,p.last_name,p.stage_id,p.assigned_user_id,COALESCE(jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'value',c.value,'normalized_value',c.normalized_value,'import_order',c.import_order) ORDER BY c.kind,c.import_order) FILTER(WHERE c.id IS NOT NULL),'[]') contacts FROM person p LEFT JOIN contact_method c ON c.person_id=p.id AND c.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 GROUP BY p.id").bind(person).bind(org.0).fetch_optional(&mut *conn).await?;
    let current=native.map(|v|json!({"first_name":v.get::<Option<String>,_>("first_name"),"last_name":v.get::<Option<String>,_>("last_name"),"stage_id":v.get::<Uuid,_>("stage_id"),"assigned_user_id":v.get::<Option<Uuid>,_>("assigned_user_id"),"contacts":v.get::<Value,_>("contacts")}));
    let mut disposition = "settled_noop";
    if item.get::<String, _>("disposition") == "eligible" {
        if current.as_ref() != Some(&expected) || expected != baseline {
            disposition = "held_stale";
        } else {
            let target_stage = Uuid::parse_str(
                proposed["stage_id"]
                    .as_str()
                    .ok_or(MigrationError::Crypto)?,
            )
            .map_err(|_| MigrationError::Crypto)?;
            let target_assignee = proposed["assigned_user_id"]
                .as_str()
                .and_then(|v| Uuid::parse_str(v).ok());
            let stage_ok: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM stage WHERE id=$1 AND organization_id=$2)",
            )
            .bind(target_stage)
            .bind(org.0)
            .fetch_one(&mut *conn)
            .await?;
            let assignee_ok=match target_assignee { Some(user)=>sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active')").bind(org.0).bind(user).fetch_one(&mut *conn).await?, None=>true };
            if !stage_ok || !assignee_ok {
                disposition = "held_stale";
            }
            if disposition == "held_stale" {
            } else {
                let permit = json!({"lease":r.get::<Uuid,_>("lease_token"),"item":item_id});
                sqlx::query("SELECT set_config('crm.people_refresh_permit',$1,true)")
                    .bind(permit.to_string())
                    .execute(&mut *conn)
                    .await?;
                let old_stage = Uuid::parse_str(
                    expected["stage_id"]
                        .as_str()
                        .ok_or(MigrationError::Crypto)?,
                )
                .map_err(|_| MigrationError::Crypto)?;
                let old_assignee = expected["assigned_user_id"]
                    .as_str()
                    .and_then(|v| Uuid::parse_str(v).ok());
                sqlx::query("UPDATE person SET first_name=$3,last_name=$4,stage_id=$5,assigned_user_id=$6,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(person).bind(org.0).bind(proposed["first_name"].as_str()).bind(proposed["last_name"].as_str()).bind(target_stage).bind(target_assignee).execute(&mut *conn).await?;
                let old = expected["contacts"]
                    .as_array()
                    .ok_or(MigrationError::Crypto)?;
                let next = proposed["contacts"]
                    .as_array()
                    .ok_or(MigrationError::Crypto)?;
                let old_ids = old
                    .iter()
                    .filter_map(|v| v["id"].as_str().and_then(|id| Uuid::parse_str(id).ok()))
                    .collect::<Vec<_>>();
                if !old_ids.is_empty() {
                    sqlx::query("UPDATE contact_method SET import_order=NULL WHERE organization_id=$1 AND person_id=$2 AND id=ANY($3)").bind(org.0).bind(person).bind(&old_ids).execute(&mut *conn).await?;
                }
                for contact in old {
                    if !next.iter().any(|v| v["id"] == contact["id"]) {
                        sqlx::query("DELETE FROM contact_method WHERE id=$1 AND organization_id=$2 AND person_id=$3").bind(Uuid::parse_str(contact["id"].as_str().ok_or(MigrationError::Crypto)?).map_err(|_|MigrationError::Crypto)?).bind(org.0).bind(person).execute(&mut *conn).await?;
                    }
                }
                for contact in next {
                    let cid =
                        Uuid::parse_str(contact["id"].as_str().ok_or(MigrationError::Crypto)?)
                            .map_err(|_| MigrationError::Crypto)?;
                    let exists = old.iter().any(|v| v["id"] == contact["id"]);
                    if exists {
                        sqlx::query("UPDATE contact_method SET value=$3,normalized_value=$4,import_order=$5 WHERE id=$1 AND organization_id=$2").bind(cid).bind(org.0).bind(contact["value"].as_str()).bind(contact["normalized_value"].as_str()).bind(contact["import_order"].as_i64().unwrap_or_default() as i32).execute(&mut *conn).await?;
                    } else {
                        sqlx::query("INSERT INTO contact_method(id,organization_id,person_id,kind,value,normalized_value,import_order) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(cid).bind(org.0).bind(person).bind(contact["kind"].as_str()).bind(contact["value"].as_str()).bind(contact["normalized_value"].as_str()).bind(contact["import_order"].as_i64().unwrap_or_default() as i32).execute(&mut *conn).await?;
                    }
                }
                let actor = r.get::<Uuid, _>("initiated_by_user_id");
                let now = Utc::now();
                if old_stage != target_stage {
                    sqlx::query("INSERT INTO stage_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,from_stage_id,to_stage_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,$8,'migration_refresh')").bind(Uuid::new_v4()).bind(org.0).bind(actor).bind(now).bind(id).bind(person).bind(old_stage).bind(target_stage).execute(&mut *conn).await?;
                }
                if old_assignee != target_assignee {
                    sqlx::query("INSERT INTO assignment_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,from_user_id,to_user_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,$8,'migration_refresh')").bind(Uuid::new_v4()).bind(org.0).bind(actor).bind(now).bind(id).bind(person).bind(old_assignee).bind(target_assignee).execute(&mut *conn).await?;
                }
                disposition = "settled";
            }
        }
    }
    let before = s::seal(key, org, id, item_id, "result-before", &baseline)?;
    let after = s::seal(
        key,
        org,
        id,
        item_id,
        "result-after",
        if disposition == "settled" {
            &proposed
        } else {
            &baseline
        },
    )?;
    let result = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_people_refresh_result(id,refresh_id,item_id,organization_id,person_id,source_id,disposition,before_nonce,before_ciphertext,after_nonce,after_ciphertext,actor_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(result).bind(id).bind(item.get::<Uuid,_>("id")).bind(org.0).bind(item.get::<Option<Uuid>,_>("person_id")).bind(item.get::<String,_>("source_id")).bind(disposition).bind(before.nonce.as_slice()).bind(before.ciphertext).bind(after.nonce.as_slice()).bind(after.ciphertext).bind(r.get::<Uuid,_>("initiated_by_user_id")).execute(&mut *conn).await?;
    if disposition == "settled" {
        let head = s::seal(key, org, id, item_id, "last-baseline", &proposed)?;
        sqlx::query("UPDATE migration_people_refresh_baseline SET refresh_id=$4,result_id=$5,projection_row_id=$6,projection_nonce=$7,projection_ciphertext=$8,updated_at=clock_timestamp() WHERE organization_id=$1 AND parent_import_id=$2 AND source_id=$3").bind(org.0).bind(r.get::<Uuid,_>("parent_import_id")).bind(item.get::<String,_>("source_id")).bind(id).bind(result).bind(item_id).bind(head.nonce.as_slice()).bind(head.ciphertext).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE migration_people_refresh_item SET disposition=$3,settled_result_id=$4,settled_at=clock_timestamp() WHERE id=$1 AND refresh_id=$2").bind(item_id).bind(id).bind(disposition).bind(result).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_people_refresh SET settled_items=settled_items+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
    Ok(())
}
