//! Fenced retained-only preparation/execution loop for admitted People refreshes.
use super::{
    admitted_people_refresh_source as source, admitted_people_refresh_store as s, crypto,
    import_source::{Entity, ExtractedRecord},
    imports,
    snapshot::SnapshotPolicy,
    store, MigrationError,
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

// Frozen source/mapping evidence is intentionally passed explicitly; combining
// it would obscure which trusted snapshot and plan authorize the lookup.
#[allow(clippy::too_many_arguments)]
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

async fn execution_mapping_target_valid(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    refresh: &sqlx::postgres::PgRow,
    item: &sqlx::postgres::PgRow,
    kind: &str,
    target: Uuid,
) -> Result<bool, MigrationError> {
    // The item carries an immutable original import result. Re-open approved
    // mapping evidence and compare its frozen identity with the live target;
    // target existence alone is insufficient at commit time.
    let snapshot: Uuid = sqlx::query_scalar(
        "SELECT a.original_snapshot_id FROM migration_people_admission_result ar
          JOIN migration_people_admission_item ai ON ai.id=ar.item_id AND ai.admission_id=ar.admission_id AND ai.organization_id=ar.organization_id
          JOIN migration_people_admission a ON a.id=ar.admission_id AND a.organization_id=ar.organization_id
          WHERE ar.id=$1 AND ar.organization_id=$2 AND ar.disposition='settled'",
    )
    .bind(item.get::<Option<Uuid>, _>("admission_result_id"))
    .bind(org.0)
    .fetch_optional(&mut *conn)
    .await?
    .flatten()
    .ok_or(MigrationError::SourceNotEligible)?;
    let rows = sqlx::query(
        "SELECT * FROM migration_import_mapping
          WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND target_id=$4
            AND qualified=true ORDER BY id",
    )
    .bind(refresh.get::<Uuid, _>("parent_plan_id"))
    .bind(org.0)
    .bind(kind)
    .bind(target)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(false);
    }
    let live = match kind {
        "stage" => json!(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT name FROM stage WHERE organization_id=$1 AND id=$2",
            )
            .bind(org.0)
            .bind(target)
            .fetch_one(&mut *conn)
            .await?
        ),
        "assignee" => json!(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT u.email FROM organization_membership m
              JOIN app_user u ON u.id=m.user_id
              WHERE m.organization_id=$1 AND m.user_id=$2 AND m.status='active'",
            )
            .bind(org.0)
            .bind(target)
            .fetch_one(&mut *conn)
            .await?
        ),
        _ => return Ok(false),
    };
    for mapping in rows {
        let disposition: String = mapping.get("disposition");
        if !matches!(
            (kind, disposition.as_str()),
            ("stage", "existing" | "create") | ("assignee", "member")
        ) {
            continue;
        }
        let frozen: Value = imports::open(
            key,
            org,
            snapshot,
            refresh.get("parent_plan_id"),
            mapping.get("id"),
            "mapping",
            &mapping.get::<Vec<u8>, _>("nonce"),
            &mapping.get::<Vec<u8>, _>("ciphertext"),
        )?;
        if frozen["target"][if kind == "stage" { "name" } else { "email" }] == live {
            return Ok(true);
        }
    }
    Ok(false)
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

#[derive(Clone, Copy)]
struct WorkerCandidate {
    id: Uuid,
    org: OrganizationId,
    lease_token: Option<Uuid>,
}

fn pause_reason(error: &MigrationError) -> Option<&'static str> {
    match error {
        MigrationError::Crypto => Some("retained_integrity_failed"),
        MigrationError::StorageLimit => Some("storage_budget_exhausted"),
        MigrationError::Forbidden => Some("initiator_not_authorized"),
        MigrationError::SourceNotEligible | MigrationError::SourceAccountMismatch => {
            Some("retained_evidence_invalid")
        }
        MigrationError::Conflict => Some("source_binding_changed"),
        MigrationError::ReleaseNotReady => Some("release_not_ready"),
        MigrationError::Database(sqlx::Error::Database(error))
            if error
                .code()
                .is_some_and(|code| code == "57014" || code == "25P03") =>
        {
            Some("work_unit_timed_out")
        }
        _ => None,
    }
}

/// Persist a closed recovery state only for the work instance that actually
/// failed. A cancellation, a terminal transition, or a different lease wins
/// the race and is never overwritten or charged by this recovery path.
async fn pause_failed_candidate(
    pool: &PgPool,
    candidate: WorkerCandidate,
    reason: &str,
) -> Result<(), MigrationError> {
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    store::lock_org(&mut tx, candidate.org).await?;
    let row = sqlx::query(
        "SELECT state,lease_token FROM migration_admitted_people_refresh
          WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(candidate.id)
    .bind(candidate.org.0)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(());
    };
    let state: String = row.get("state");
    let owns = match candidate.lease_token {
        Some(lease) => {
            state == "running" && row.get::<Option<Uuid>, _>("lease_token") == Some(lease)
        }
        None => matches!(state.as_str(), "preparing" | "queued"),
    };
    if owns {
        sqlx::query(
            "UPDATE migration_admitted_people_refresh
                SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,
                    updated_at=clock_timestamp()
              WHERE id=$1 AND organization_id=$2",
        )
        .bind(candidate.id)
        .bind(candidate.org.0)
        .bind(reason)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(refresh_id=%candidate.id, organization_id=%candidate.org.0, reason, "admitted Admitted Admitted People refresh paused");
    Ok(())
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    let r=sqlx::query("SELECT id,organization_id,lease_token FROM migration_admitted_people_refresh WHERE state IN ('preparing','queued','running') ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(r) = r else { return Ok(false) };
    let candidate = WorkerCandidate {
        id: r.get("id"),
        org: OrganizationId::new(r.get("organization_id")),
        lease_token: r.get("lease_token"),
    };
    let attempt = async {
    let id = candidate.id;
    let org = candidate.org;
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    store::lock_org(&mut tx, org).await?;
    let r = sqlx::query(
        "SELECT * FROM migration_admitted_people_refresh WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(org.0)
    .fetch_one(&mut *tx)
    .await?;
    let state: String = r.get("state");
    // The candidate was selected before this fenced row lock. A concurrent
    // cancel/completion must remain terminal rather than being paused or
    // otherwise resurrected by this worker.
    if !matches!(state.as_str(), "preparing" | "queued" | "running") {
        tx.commit().await?;
        return Ok(false);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_admitted_people_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let initiator_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM migration_admitted_people_refresh x
             JOIN migration_workspace w ON w.organization_id=x.organization_id
              AND w.import_id=x.parent_import_id AND w.plan_id=x.parent_plan_id
             JOIN organization_membership m ON m.organization_id=x.organization_id
              AND m.user_id=x.initiated_by_user_id
            WHERE x.id=$1 AND x.organization_id=$2
              AND m.role='admin' AND m.status='active')",
    )
    .bind(id)
    .bind(org.0)
    .fetch_one(&mut *tx)
    .await?;
    if !initiator_active {
        sqlx::query("UPDATE migration_admitted_people_refresh SET state='paused',pause_reason='initiator_not_authorized',lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND state IN ('preparing','queued','running')")
            .bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(true);
    }
    if state == "preparing" {
        // A preparation turn owns one bounded reservation. Its exact retained
        // delta is charged atomically with the rows it made durable.
        let before = s::measured_bytes(&mut tx, org, id).await?;
        let token = match s::reserve(&mut tx, org, id, "prepare", None, 8 * 1024 * 1024, policy)
            .await
        {
            Ok(token) => token,
            Err(MigrationError::StorageLimit) => {
                sqlx::query("UPDATE migration_admitted_people_refresh SET state='paused',pause_reason='storage_budget_exhausted',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
                    .bind(id).bind(org.0).execute(&mut *tx).await?;
                tx.commit().await?;
                return Ok(true);
            }
            Err(error) => return Err(error),
        };
        prepare(&mut tx, key, &r).await?;
        let after = s::measured_bytes(&mut tx, org, id).await?;
        s::release_delta(&mut tx, org, id, token, after.saturating_sub(before)).await?;
        tx.commit().await?;
        return Ok(true);
    };
    if r.get::<String, _>("state") == "queued" {
        sqlx::query("UPDATE migration_admitted_people_refresh SET state='running',lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(Uuid::new_v4()).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(true);
    };
    let lease_token: Option<Uuid> = r.get("lease_token");
    let expired = r
        .get::<Option<chrono::DateTime<Utc>>, _>("lease_expires_at")
        .is_none_or(|v| v <= Utc::now());
    if lease_token.is_none() || expired {
        sqlx::query("UPDATE migration_admitted_people_refresh SET state='paused',pause_reason='lease_expired',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(true);
    };
    let next_bound: Option<i64> = sqlx::query_scalar(
        "SELECT item_byte_bound FROM migration_admitted_people_refresh_item
          WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND settled_at IS NULL
          ORDER BY id LIMIT 1",
    )
    .bind(id)
    .bind(org.0)
    .bind(r.get::<Uuid, _>("confirmed_refresh_plan_id"))
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(bound) = next_bound {
        let reservation_token = match s::reserve(&mut tx, org, id, "work", lease_token, bound, policy).await {
            Ok(token) => token,
            Err(MigrationError::StorageLimit) => {
                sqlx::query("UPDATE migration_admitted_people_refresh SET state='paused',pause_reason='storage_budget_exhausted',lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND lease_token=$3")
                    .bind(id).bind(org.0).bind(lease_token).execute(&mut *tx).await?;
                tx.commit().await?;
                return Ok(true);
            }
            Err(error) => return Err(error),
        };
        let actual = execute_noop(&mut tx, key, &r).await?;
        let current = sqlx::query(
            "SELECT state,lease_token FROM migration_admitted_people_refresh
              WHERE id=$1 AND organization_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(org.0)
        .fetch_one(&mut *tx)
        .await?;
        if current.get::<String, _>("state") != "completed"
            && (current.get::<String, _>("state") != "running"
                || current.get::<Option<Uuid>, _>("lease_token") != lease_token)
        {
            return Err(MigrationError::Conflict);
        }
        s::release(&mut tx, org, id, reservation_token, actual).await?;
    } else {
        execute_noop(&mut tx, key, &r).await?;
        if r.get::<String, _>("state") == "running" {
            if let Some(cancel) = sqlx::query_scalar::<_, Uuid>(
                "SELECT token FROM migration_admitted_people_refresh_reservation WHERE refresh_id=$1 AND organization_id=$2 AND purpose='cancel'",
            )
            .bind(id)
            .bind(org.0)
            .fetch_optional(&mut *tx)
            .await?
            {
                s::release(&mut tx, org, id, cancel, 0).await?;
            }
        }
    }
    tx.commit().await?;
    Ok(true)
    }.await;
    match attempt {
        Ok(value) => Ok(value),
        Err(error) => match pause_reason(&error) {
            Some(reason) => {
                pause_failed_candidate(pool, candidate, reason).await?;
                Ok(true)
            }
            None => Err(error),
        },
    }
}
async fn prepare(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    // A recovery from the short-lived pre-direct-seal phase can only seal the
    // cohort plan already prepared; it must never traverse report groups.
    if r.get::<String, _>("preparation_phase") == "groups" {
        return seal_prepared_plan(conn, r).await;
    }
    let building=sqlx::query("SELECT id,revision FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 AND state='building' ORDER BY revision DESC LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *conn).await?;
    let (plan, _revision) = if let Some(building) = building {
        (building.get("id"), building.get("revision"))
    } else {
        let plan = Uuid::new_v4();
        let revision:i64=sqlx::query_scalar("SELECT COALESCE(max(revision),0)+1 FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
        let inputs = json!({"report_id":r.get::<Uuid,_>("report_id"),"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"newer_snapshot_id":r.get::<Uuid,_>("newer_snapshot_id")});
        let sealed = s::seal(key, org, id, plan, "inputs", &inputs)?;
        let input_bytes = sealed_bytes(&sealed);
        sqlx::query("INSERT INTO migration_admitted_people_refresh_plan(id,refresh_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext) VALUES($1,$2,$3,$4,'building',$5,$6)").bind(plan).bind(id).bind(org.0).bind(revision).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_admitted_people_refresh_plan SET prepared_bytes=prepared_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(input_bytes).execute(&mut *conn).await?;
        (plan, revision)
    };
    let checkpoint: String = r.get("preparation_checkpoint_key");
    let rows=sqlx::query("SELECT ar.id AS admission_result_id,ar.person_id,ar.source_id,ai.id AS admission_item_id,ai.source_key,ai.stage_mapping_id,ai.assignee_mapping_id,ai.projection_nonce,ai.projection_ciphertext,a.id AS admission_id,a.original_snapshot_id,a.parent_import_id,a.parent_plan_id FROM migration_people_admission_result ar JOIN migration_people_admission_item ai ON ai.id=ar.item_id AND ai.admission_id=ar.admission_id AND ai.organization_id=ar.organization_id JOIN migration_people_admission a ON a.id=ar.admission_id AND a.organization_id=ar.organization_id WHERE ar.admission_id=$1 AND ar.organization_id=$2 AND ar.disposition='settled' AND ar.source_id>$3 ORDER BY ar.source_id LIMIT 50").bind(r.get::<Uuid,_>("admission_id")).bind(org.0).bind(&checkpoint).fetch_all(&mut *conn).await?;
    let has_more = rows.len() == 50;
    let mut last_source = checkpoint;
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
        last_source = source_id.clone();
        // B is the exact encrypted admission projection plus committed contact
        // UUIDs. It is never reconstructed from today's native Person state.
        let mut b: Value = super::people_admission_store::open(
            key,
            org,
            row.get("admission_id"),
            row.get("admission_item_id"),
            "projection",
            &row.get::<Vec<u8>, _>("projection_nonce"),
            &row.get::<Vec<u8>, _>("projection_ciphertext"),
        )?;
        let owned = sqlx::query("SELECT id,kind,import_order,value_nonce,value_ciphertext FROM migration_people_admission_contact WHERE admission_id=$1 AND item_id=$2 AND organization_id=$3 ORDER BY kind,import_order")
            .bind(row.get::<Uuid,_>("admission_id")).bind(row.get::<Uuid,_>("admission_item_id")).bind(org.0).fetch_all(&mut *conn).await?;
        let contacts = b
            .as_object_mut()
            .ok_or(MigrationError::Crypto)?
            .entry("contacts")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or(MigrationError::Crypto)?;
        if !contacts.is_empty() {
            return Err(MigrationError::Crypto);
        }
        for owned in owned {
            let input: super::import_source::ContactInput = super::people_admission_store::open(
                key,
                org,
                row.get("admission_id"),
                owned.get("id"),
                "contact",
                &owned.get::<Vec<u8>, _>("value_nonce"),
                &owned.get::<Vec<u8>, _>("value_ciphertext"),
            )?;
            contacts.push(json!({
                "id": owned.get::<Uuid, _>("id"),
                "kind": owned.get::<String, _>("kind"),
                "value": input.value,
                "normalized_value": input.normalized_value,
                "import_order": owned.get::<i32, _>("import_order"),
            }));
        }
        // A later settled result is the only successor to original import
        // provenance. Current native state is deliberately absent from B.
        let previous=sqlx::query("SELECT refresh_id,projection_row_id,projection_nonce,projection_ciphertext FROM migration_admitted_people_refresh_baseline WHERE organization_id=$1 AND admission_id=$2 AND source_id=$3").bind(org.0).bind(r.get::<Uuid,_>("admission_id")).bind(&source_id).fetch_optional(&mut *conn).await?;
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
        let baseline_bytes = sealed_bytes(&sealed_baseline);
        let baseline_inserted = sqlx::query("INSERT INTO migration_admitted_people_refresh_baseline(organization_id,admission_id,source_id,person_id,refresh_id,admission_result_id,projection_row_id,projection_nonce,projection_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(organization_id,admission_id,source_id) DO NOTHING")
            .bind(org.0).bind(r.get::<Uuid,_>("admission_id")).bind(&source_id).bind(row.get::<Uuid,_>("person_id")).bind(id).bind(row.get::<Uuid,_>("admission_result_id")).bind(item).bind(sealed_baseline.nonce.as_slice()).bind(sealed_baseline.ciphertext).execute(&mut *conn).await?.rows_affected();
        if baseline_inserted == 1 {
            sqlx::query("UPDATE migration_admitted_people_refresh_plan SET prepared_bytes=prepared_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(baseline_bytes).execute(&mut *conn).await?;
        }
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
                row.get("admission_result_id"),
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
        let (newer, capture, ordinal) = match newer {
            source::RetainedPerson::Missing => {
                held += 1;
                insert_item(
                    conn,
                    key,
                    org,
                    id,
                    plan,
                    item,
                    &source_id,
                    row.get("admission_result_id"),
                    row.get("person_id"),
                    "not_seen_again",
                    &b,
                    &c,
                    // An absent Person in the newer retained capture is explicit
                    // non-deletion evidence. Preserve B as the displayed proposal
                    // so the item cannot look like a synthetic clear/removal.
                    &b,
                    None,
                    None,
                    &[],
                )
                .await?;
                continue;
            }
            source::RetainedPerson::EvidenceGap => {
                held += 1;
                insert_item(
                    conn,
                    key,
                    org,
                    id,
                    plan,
                    item,
                    &source_id,
                    row.get("admission_result_id"),
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
            }
            source::RetainedPerson::Present(newer, capture, ordinal) => (newer, capture, ordinal),
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
                    row.get("admission_result_id"),
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
            row.get("original_snapshot_id"),
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
                    row.get("admission_result_id"),
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
            row.get("original_snapshot_id"),
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
                    row.get("admission_result_id"),
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
        no_instruction += instructions.len() as i64;
        let disposition = if c != b {
            "held_local_change"
        } else if n == b {
            "already_current"
        } else {
            "eligible"
        };
        match disposition {
            "eligible" => {
                eligible += 1;
                let (names, assignments, contacts) = clear_counts(&b, &n);
                name_clears += names;
                assignment_clears += assignments;
                contact_removals += contacts;
            }
            "already_current" => current += 1,
            _ => held += 1,
        }
        insert_item(
            conn,
            key,
            org,
            id,
            plan,
            item,
            &source_id,
            row.get("admission_result_id"),
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
    sqlx::query("UPDATE migration_admitted_people_refresh_plan SET eligible_count=eligible_count+$3,already_current_count=already_current_count+$4,held_count=held_count+$5,no_instruction_count=no_instruction_count+$6,name_clear_count=name_clear_count+$7,assignment_clear_count=assignment_clear_count+$8,contact_removal_count=contact_removal_count+$9 WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(eligible).bind(current).bind(held).bind(no_instruction).bind(name_clears).bind(assignment_clears).bind(contact_removals).execute(&mut *conn).await?;
    if has_more {
        sqlx::query("UPDATE migration_admitted_people_refresh SET preparation_checkpoint_key=$3,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(&last_source).execute(conn).await?;
        return Ok(());
    }
    sqlx::query("UPDATE migration_admitted_people_refresh SET preparation_checkpoint_key='',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .execute(&mut *conn)
        .await?;
    seal_prepared_plan(conn, r).await
}

async fn seal_prepared_plan(
    conn: &mut sqlx::PgConnection,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let plan=sqlx::query("SELECT id,revision FROM migration_admitted_people_refresh_plan WHERE refresh_id=$1 AND organization_id=$2 AND state='building' ORDER BY revision DESC LIMIT 1 FOR UPDATE").bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Crypto)?;
    let plan_id: Uuid = plan.get("id");
    let revision: i64 = plan.get("revision");
    // Every cohort identity is walked in `prepare`. There is no original-import
    // report traversal here: it would manufacture closed items for People
    // outside the one terminal admission's settled cohort.
    let totals=sqlx::query("SELECT eligible_count,already_current_count,held_count,excluded_count,no_instruction_count,name_clear_count,assignment_clear_count,contact_removal_count FROM migration_admitted_people_refresh_plan WHERE id=$1 AND organization_id=$2").bind(plan_id).bind(org.0).fetch_one(&mut *conn).await?;
    let plan_digest = digest(
        &json!({"refresh":id,"plan":plan_id,"revision":revision,"eligible":totals.get::<i64,_>("eligible_count"),"current":totals.get::<i64,_>("already_current_count"),"held":totals.get::<i64,_>("held_count"),"excluded":totals.get::<i64,_>("excluded_count"),"no_instruction":totals.get::<i64,_>("no_instruction_count"),"name_clears":totals.get::<i64,_>("name_clear_count"),"assignment_clears":totals.get::<i64,_>("assignment_clear_count"),"contact_removals":totals.get::<i64,_>("contact_removal_count")}),
    );
    sqlx::query("UPDATE migration_admitted_people_refresh_plan SET prepared_bytes=prepared_bytes+$3,state='ready',digest=$4,sealed_at=clock_timestamp(),expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND organization_id=$2").bind(plan_id).bind(org.0).bind(i64::try_from(plan_digest.len()).unwrap_or(i64::MAX)).bind(plan_digest.as_slice()).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_admitted_people_refresh SET state='ready',updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
    Ok(())
}
fn sealed_bytes(value: &crypto::Sealed) -> i64 {
    i64::try_from(value.nonce.len().saturating_add(value.ciphertext.len())).unwrap_or(i64::MAX)
}

// This writes the complete immutable preview tuple in one place. Keeping each
// scoped evidence/provenance input explicit prevents accidental cross-item use.
#[allow(clippy::too_many_arguments)]
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
    // Only executable eligible units contribute confirmation acknowledgments.
    // Held/excluded rows can carry an empty explanation projection, which must
    // never be interpreted as a request to clear their baseline fields.
    let (name_clears, assignment_clears, contact_removals) = if disposition == "eligible" {
        clear_counts(b, n)
    } else {
        (0, 0, 0)
    };
    if serde_json::to_vec(instructions)
        .map_err(|_| MigrationError::Crypto)?
        .len()
        > 1024
    {
        return Err(MigrationError::StorageLimit);
    }
    let instructions = s::seal(key, org, refresh, item, "instructions", &instructions)?;
    // Derive a conservative exact upper bound from the complete encrypted
    // preview plus the largest possible result/baseline receipts. It is never
    // a fixed magic allocation, and the schema enforces the 64 MiB ceiling.
    let mut item_bound = sealed_bytes(&a)
        .saturating_add(sealed_bytes(&d))
        .saturating_add(sealed_bytes(&e))
        .saturating_add(sealed_bytes(&instructions));
    let mut contacts: Vec<(&str, Value, Uuid, crypto::Sealed)> = Vec::new();
    for (side, projection) in [("baseline", b), ("current", c), ("proposed", n)] {
        for contact in projection["contacts"].as_array().into_iter().flatten() {
            let contact_id = Uuid::new_v4();
            let sealed = s::seal(key, org, refresh, contact_id, "contact", contact)?;
            item_bound = item_bound.saturating_add(sealed_bytes(&sealed));
            contacts.push((side, contact.clone(), contact_id, sealed));
        }
    }
    // `before` + `after` result, the independently encrypted immutable
    // provenance copy for a settled update, and a possible baseline head.
    item_bound = item_bound
        .saturating_add(sealed_bytes(&a).saturating_mul(2))
        .saturating_add(sealed_bytes(&e).saturating_mul(3))
        // The result and transferred baseline each persist their source ID.
        .saturating_add(
            i64::try_from(source_id.len())
                .unwrap_or(i64::MAX)
                .saturating_mul(2),
        );
    if item_bound > s::ITEM_LIMIT {
        return Err(MigrationError::StorageLimit);
    }
    let prepared = item_bound
        .saturating_sub(sealed_bytes(&a).saturating_mul(2))
        .saturating_sub(sealed_bytes(&e).saturating_mul(3));
    sqlx::query("INSERT INTO migration_admitted_people_refresh_item(id,refresh_id,plan_id,organization_id,source_key,source_id,person_id,admission_id,admission_item_id,admission_result_id,baseline_version,disposition,proposed_nonce,proposed_ciphertext,baseline_nonce,baseline_ciphertext,current_nonce,current_ciphertext,instructions_nonce,instructions_ciphertext,name_clear_count,assignment_clear_count,contact_removal_count,source_capture_id,source_ordinal,item_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,(SELECT admission_id FROM migration_people_admission_result WHERE id=$8 AND organization_id=$4),(SELECT item_id FROM migration_people_admission_result WHERE id=$8 AND organization_id=$4),$8,(SELECT version FROM migration_admitted_people_refresh_baseline WHERE organization_id=$4 AND admission_id=(SELECT admission_id FROM migration_people_admission_result WHERE id=$8 AND organization_id=$4) AND source_id=$6),$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)").bind(item).bind(refresh).bind(plan).bind(org.0).bind(source_id).bind(source_id).bind(person).bind(result).bind(disposition).bind(e.nonce.as_slice()).bind(e.ciphertext).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(d.nonce.as_slice()).bind(d.ciphertext).bind(instructions.nonce.as_slice()).bind(instructions.ciphertext).bind(name_clears).bind(assignment_clears).bind(contact_removals).bind(capture).bind(ordinal).bind(item_bound).execute(&mut *conn).await?;
    for (side, contact, contact_id, sealed) in contacts {
        sqlx::query("INSERT INTO migration_admitted_people_refresh_contact(id,item_id,refresh_id,organization_id,side,contact_id,kind,import_order,value_nonce,value_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(item_id,side,kind,import_order) DO NOTHING").bind(contact_id).bind(item).bind(refresh).bind(org.0).bind(side).bind(contact["id"].as_str().and_then(|v|Uuid::parse_str(v).ok())).bind(contact["kind"].as_str()).bind(contact["import_order"].as_i64().unwrap_or_default() as i32).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    }
    // Only rows made durable during preparation are accumulated here; future
    // result/baseline receipts are accounted in their own execution unit.
    sqlx::query("UPDATE migration_admitted_people_refresh_plan SET prepared_bytes=prepared_bytes+$3 WHERE id=$1 AND organization_id=$2").bind(plan).bind(org.0).bind(prepared).execute(&mut *conn).await?;
    Ok(())
}

async fn execute_noop(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
) -> Result<i64, MigrationError> {
    let id: Uuid = r.get("id");
    let org = OrganizationId::new(r.get("organization_id"));
    let item=sqlx::query("SELECT * FROM migration_admitted_people_refresh_item WHERE refresh_id=$1 AND organization_id=$2 AND plan_id=$3 AND settled_at IS NULL ORDER BY id LIMIT 1 FOR UPDATE").bind(id).bind(org.0).bind(r.get::<Uuid, _>("confirmed_refresh_plan_id")).fetch_optional(&mut *conn).await?;
    let Some(item) = item else {
        sqlx::query("UPDATE migration_admitted_people_refresh SET state='completed',completed_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
        return Ok(0);
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
    let planned_disposition: String = item.get("disposition");
    // Closed held/excluded units and already-current units settle a durable
    // outcome without touching a Person. The frozen preview disposition is
    // never overwritten; result rows carry the final outcome.
    let mut disposition = if planned_disposition == "already_current" {
        "settled_noop"
    } else {
        planned_disposition.as_str()
    };
    if planned_disposition == "eligible" {
        let person = item
            .get::<Option<Uuid>, _>("person_id")
            .ok_or(MigrationError::Conflict)?;
        let authorized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_people_refresh x JOIN migration_workspace w ON w.organization_id=x.organization_id AND w.import_id=x.parent_import_id AND w.plan_id=x.parent_plan_id JOIN organization_membership m ON m.organization_id=x.organization_id AND m.user_id=x.initiated_by_user_id WHERE x.id=$1 AND x.organization_id=$2 AND x.state='running' AND x.lease_token=$3 AND x.lease_expires_at>clock_timestamp() AND m.role='admin' AND m.status='active')").bind(id).bind(org.0).bind(r.get::<Uuid,_>("lease_token")).fetch_one(&mut *conn).await?;
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
            let stage_ok =
                execution_mapping_target_valid(conn, key, org, r, &item, "stage", target_stage)
                    .await?;
            let assignee_ok = match target_assignee {
                Some(user) => {
                    execution_mapping_target_valid(conn, key, org, r, &item, "assignee", user)
                        .await?
                }
                None => true,
            };
            if !stage_ok || !assignee_ok {
                disposition = "held_stale";
            }
            if disposition == "held_stale" {
            } else {
                let permit = json!({"lease":r.get::<Uuid,_>("lease_token"),"item":item_id});
                sqlx::query("SELECT set_config('crm.admitted_people_refresh_permit',$1,true)")
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
        if disposition == "settled" || disposition == "settled_noop" {
            &proposed
        } else {
            &baseline
        },
    )?;
    let result_bytes = sealed_bytes(&before).saturating_add(sealed_bytes(&after));
    // Provenance deliberately keeps an independently encrypted before/after
    // envelope. It is not a pointer to the result envelope, so charge its
    // physical bytes once in this same reservation/settlement transaction.
    let provenance_bytes = if disposition == "settled" {
        result_bytes
    } else {
        0
    };
    let source_id = item
        .get::<Option<String>, _>("source_id")
        .unwrap_or_else(|| item.get("source_key"));
    let source_bytes = i64::try_from(source_id.len()).unwrap_or(i64::MAX);
    let mut retained_delta = result_bytes
        .saturating_add(provenance_bytes)
        .saturating_add(source_bytes);
    let result = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_admitted_people_refresh_result(id,refresh_id,item_id,organization_id,person_id,source_id,disposition,before_nonce,before_ciphertext,after_nonce,after_ciphertext,actor_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(result).bind(id).bind(item.get::<Uuid,_>("id")).bind(org.0).bind(item.get::<Option<Uuid>,_>("person_id")).bind(&source_id).bind(disposition).bind(before.nonce.as_slice()).bind(&before.ciphertext).bind(after.nonce.as_slice()).bind(&after.ciphertext).bind(r.get::<Uuid,_>("initiated_by_user_id")).execute(&mut *conn).await?;
    if disposition == "settled" {
        let person = item
            .get::<Option<Uuid>, _>("person_id")
            .ok_or(MigrationError::Crypto)?;
        sqlx::query("INSERT INTO person_admitted_refresh_provenance(id,organization_id,person_id,refresh_id,item_id,result_id,before_nonce,before_ciphertext,after_nonce,after_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(Uuid::new_v4()).bind(org.0).bind(person).bind(id).bind(item_id).bind(result).bind(before.nonce.as_slice()).bind(before.ciphertext).bind(after.nonce.as_slice()).bind(after.ciphertext).execute(&mut *conn).await?;
    }
    if disposition == "settled"
        || (disposition == "settled_noop" && planned_disposition == "already_current")
    {
        let prior = sqlx::query(
            "SELECT refresh_id,octet_length(projection_nonce)+octet_length(projection_ciphertext) AS bytes
               FROM migration_admitted_people_refresh_baseline
              WHERE organization_id=$1 AND admission_id=$2 AND source_id=$3 FOR UPDATE",
        )
        .bind(org.0)
        .bind(r.get::<Uuid, _>("admission_id"))
        .bind(&source_id)
        .fetch_optional(&mut *conn)
        .await?;
        let head = s::seal(key, org, id, item_id, "last-baseline", &proposed)?;
        if let Some(prior) = prior {
            s::debit_retained_owner(
                conn,
                org,
                prior.get("refresh_id"),
                i64::from(prior.get::<i32, _>("bytes"))
                    .saturating_add(i64::try_from(source_id.len()).unwrap_or(i64::MAX)),
            )
            .await?;
        }
        retained_delta = retained_delta
            .saturating_add(sealed_bytes(&head))
            .saturating_add(source_bytes);
        sqlx::query("UPDATE migration_admitted_people_refresh_baseline SET refresh_id=$4,result_id=$5,projection_row_id=$6,projection_nonce=$7,projection_ciphertext=$8,version=version+1,updated_at=clock_timestamp() WHERE organization_id=$1 AND admission_id=$2 AND source_id=$3").bind(org.0).bind(r.get::<Uuid,_>("admission_id")).bind(source_id).bind(id).bind(result).bind(item_id).bind(head.nonce.as_slice()).bind(head.ciphertext).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE migration_admitted_people_refresh_item SET settled_result_id=$3,settled_at=clock_timestamp() WHERE id=$1 AND refresh_id=$2").bind(item_id).bind(id).bind(result).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_admitted_people_refresh SET settled_items=settled_items+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(conn).await?;
    Ok(retained_delta)
}
