//! Bounded database-only preparation and execution of immutable People manifests.
//! No source reader or provider is available to this worker.
use super::{
    crypto,
    import_source::{self, Entity, ExtractedRecord},
    imports::{self, AssigneeChoice, StageChoice},
    snapshot_source::Stream,
    MigrationError,
};
use crate::{
    auth::workspace,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Mapping {
    pub source: Option<ExtractedRecord>,
    pub choice: Value,
    pub reasons: Vec<String>,
    pub suggestions: Vec<Value>,
    pub target: Option<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub reasons: Vec<String>,
    pub transformations: Vec<String>,
    pub person_id: Uuid,
}
#[derive(Clone)]
struct Job {
    id: Uuid,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    token: Uuid,
    actor: UserId,
    boundary: i64,
}
impl Job {
    fn context(&self) -> CommandContext {
        CommandContext {
            organization_id: self.org,
            actor_user_id: self.actor,
            origin: Origin::Migration,
            correlation_id: CorrelationId::new(self.id),
        }
    }
}
pub fn spawn(
    pool: PgPool,
    key: RawPayloadKey,
    policy: super::snapshot::SnapshotPolicy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            for _ in 0..32 {
                match run_once(&pool, &key, &policy).await {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(e) => {
                        tracing::warn!(outcome=%e,"People import sweep failed");
                        break;
                    }
                }
            }
        }
    })
}
#[tracing::instrument(name = "migration.people_import.unit", skip_all)]
async fn run_once_inner(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT i.id,i.organization_id FROM migration_import i JOIN migration_import_plan p ON p.id=i.latest_plan_id AND p.organization_id=i.organization_id WHERE i.state IN ('proposed','queued','running') AND ((i.state='proposed' AND p.state='building') OR i.state IN ('queued','running')) AND (i.lease_expires_at IS NULL OR i.lease_expires_at<=now()) ORDER BY i.created_at,i.id LIMIT 1").fetch_optional(pool).await?;
    let Some(c) = candidate else { return Ok(false) };
    let org = OrganizationId::new(c.get("organization_id"));
    let id: Uuid = c.get("id");
    let mut tx = tokio::time::timeout(workspace::WAIT, pool.begin())
        .await
        .map_err(|_| MigrationError::Database(sqlx::Error::PoolTimedOut))??;
    workspace::bounded_lock_wait(&mut tx).await?;
    workspace::shared(&mut tx, org).await?;
    super::store::lock_org(&mut tx, org).await?;
    let r = imports::run(&mut tx, org, id).await?;
    if r.get::<Option<chrono::DateTime<Utc>>, _>("lease_expires_at")
        .is_some_and(|v| v > Utc::now())
    {
        return Ok(false);
    }
    if !matches!(
        r.get::<String, _>("state").as_str(),
        "proposed" | "queued" | "running"
    ) {
        return Ok(false);
    }
    let plan: Uuid = r.get("latest_plan_id");
    let actor = UserId::new(r.get("executor_user_id"));
    let j = Job {
        id,
        org,
        snapshot: r.get("snapshot_id"),
        plan,
        token: Uuid::new_v4(),
        actor,
        boundary: r.get("capture_sequence"),
    };
    imports::release_all(&mut tx, org, id).await?;
    let active=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' AND role='admin')").bind(org.0).bind(actor.0).fetch_one(&mut *tx).await?;
    if !active {
        pause(&mut tx, &j, "authority_changed").await?;
        tx.commit().await?;
        return Ok(true);
    }
    sqlx::query("UPDATE migration_import SET lease_token=$3,lease_expires_at=now()+interval '60 seconds',updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut tx = imports::begin(pool, &j.context(), false).await?;
    let r = imports::run(&mut tx, org, id).await?;
    if r.get::<Option<Uuid>, _>("lease_token") != Some(j.token)
        || r.get::<Uuid, _>("latest_plan_id") != plan
    {
        return Ok(true);
    }
    let p = imports::plan(&mut tx, org, id, plan).await?;
    let result = if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some() {
        execute(&mut tx, key, &j, &r).await
    } else {
        prepare(&mut tx, key, &j, &p).await
    };
    match result {
        Ok(()) => {
            sqlx::query("UPDATE migration_import SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND lease_token=$3").bind(id).bind(org.0).bind(j.token).execute(&mut *tx).await?;
            tx.commit().await?;
        }
        Err(error) => {
            if let MigrationError::Database(ref e) = error {
                tracing::warn!(
                    sqlstate = e.as_database_error().and_then(|v| v.code()).as_deref(),
                    "People import database unit failed"
                );
            }
            tx.rollback().await?;
            let mut tx = imports::begin(pool, &j.context(), false).await?;
            let r = imports::run(&mut tx, org, id).await?;
            if r.get::<Option<Uuid>, _>("lease_token") == Some(j.token) {
                imports::release_all(&mut tx, org, id).await?;
                let reason = match error {
                    MigrationError::StorageLimit => "storage_limit",
                    MigrationError::SourceNotEligible => "source_evidence_unavailable",
                    MigrationError::InvalidImportChoice => "mapping_target_changed",
                    MigrationError::Crypto => "retained_key_unavailable",
                    MigrationError::Database(_) => "storage_unavailable",
                    _ => "target_unavailable",
                };
                pause(&mut tx, &j, reason).await?;
                tx.commit().await?;
            }
        }
    }
    Ok(true)
}
async fn pause(conn: &mut PgConnection, j: &Job, reason: &str) -> Result<(), MigrationError> {
    tracing::info!(organization_id=%j.org,import_id=%j.id,plan_id=%j.plan,pause_reason=reason,"People import paused");
    sqlx::query("UPDATE migration_import SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(reason).execute(&mut *conn).await?;
    sqlx::query("UPDATE migration_import_plan SET state=CASE WHEN state='building' THEN 'paused' ELSE state END,pause_reason=CASE WHEN state='building' THEN $3 ELSE pause_reason END WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(reason).execute(conn).await?;
    Ok(())
}
async fn preparation_reservation(conn: &mut PgConnection, j: &Job) -> Result<Uuid, MigrationError> {
    let token = Uuid::new_v4();
    if !imports::reserve(
        conn,
        j.org,
        j.id,
        j.snapshot,
        j.plan,
        j.token,
        token,
        imports::UNIT,
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    Ok(token)
}
async fn phase(conn: &mut PgConnection, j: &Job, value: &str) -> Result<i64, MigrationError> {
    let old = sqlx::query_scalar::<_, String>(
        "SELECT checkpoint_source_id FROM migration_import_plan WHERE id=$1 AND organization_id=$2",
    )
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE migration_import_plan SET phase=$3,checkpoint_family='',checkpoint_source_id='' WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(value).execute(conn).await?;
    Ok(-(old.len() as i64))
}
async fn prepare(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if p.get::<String, _>("state") != "building" {
        return Ok(());
    }
    match p.get::<String, _>("phase").as_str() {
        "copying_choices" => copy_choices(conn, key, j, p).await,
        "captures" => capture(conn, key, j, p).await,
        "mappings" => mappings(conn, key, j, p).await,
        "people" => manifest(conn, key, j, p).await,
        _ => Err(MigrationError::ImportConflict),
    }
}
async fn insert_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    kind: &str,
    source_key: &str,
    value: &Value,
) -> Result<i64, MigrationError> {
    let id = Uuid::new_v4();
    let s = imports::seal(key, j.org, j.snapshot, j.plan, id, "choice", value)?;
    let n = imports::sealed_bytes(&s) + source_key.len() as i64;
    sqlx::query("INSERT INTO migration_import_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(kind).bind(source_key).bind(s.nonce.as_slice()).bind(s.ciphertext).execute(conn).await?;
    Ok(n)
}
async fn copy_choices(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let token = preparation_reservation(conn, j).await?;
    let mut added = 0;
    let parent: Option<Uuid> = p.get("parent_plan_id");
    let patch: Value = imports::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        j.plan,
        "patch",
        &p.get::<Vec<u8>, _>("patch_nonce"),
        &p.get::<Vec<u8>, _>("patch_ciphertext"),
    )?;
    let rows = if let Some(parent) = parent {
        sqlx::query("SELECT id,kind,source_key,nonce,ciphertext FROM migration_import_choice WHERE plan_id=$1 AND organization_id=$2 AND (kind,source_key)>($3,$4) ORDER BY kind,source_key LIMIT 50").bind(parent).bind(j.org.0).bind(p.get::<String,_>("checkpoint_family")).bind(p.get::<String,_>("checkpoint_source_id")).fetch_all(&mut *conn).await?
    } else {
        vec![]
    };
    if rows.is_empty() {
        for (kind, field) in [
            ("stage", "stage_mappings"),
            ("assignee", "assignee_mappings"),
        ] {
            for v in patch[field].as_array().ok_or(MigrationError::Crypto)? {
                let source_key = v["source_key"].as_str().ok_or(MigrationError::Crypto)?;
                let exists=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_import_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4)").bind(j.plan).bind(j.org.0).bind(kind).bind(source_key).fetch_one(&mut *conn).await?;
                if !exists {
                    added += insert_choice(conn, key, j, kind, source_key, &v["choice"]).await?;
                }
            }
        }
        added += phase(conn, j, "captures").await?;
    } else {
        for r in &rows {
            let kind: String = r.get("kind");
            let source_key: String = r.get("source_key");
            let field = if kind == "stage" {
                "stage_mappings"
            } else {
                "assignee_mappings"
            };
            let value = if let Some(v) = patch[field]
                .as_array()
                .and_then(|a| a.iter().find(|v| v["source_key"] == source_key))
            {
                v["choice"].clone()
            } else {
                imports::open(
                    key,
                    j.org,
                    j.snapshot,
                    parent.unwrap(),
                    r.get("id"),
                    "choice",
                    &r.get::<Vec<u8>, _>("nonce"),
                    &r.get::<Vec<u8>, _>("ciphertext"),
                )?
            };
            added += insert_choice(conn, key, j, &kind, &source_key, &value).await?;
        }
        let last = rows.last().unwrap();
        added += last.get::<String, _>("source_key").len() as i64
            - p.get::<String, _>("checkpoint_source_id").len() as i64;
        sqlx::query("UPDATE migration_import_plan SET checkpoint_family=$3,checkpoint_source_id=$4 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(last.get::<String,_>("kind")).bind(last.get::<String,_>("source_key")).execute(&mut *conn).await?;
    }
    imports::settle(conn, j.org, j.id, token, added).await
}
async fn capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let next=sqlx::query("SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 AND stream IN ('people','users','stages') ORDER BY sequence LIMIT 1").bind(j.snapshot).bind(j.org.0).bind(p.get::<i64,_>("checkpoint_capture")).bind(j.boundary).fetch_optional(&mut *conn).await?;
    let Some(next) = next else {
        let delta = phase(conn, j, "mappings").await?;
        imports::charge(conn, j.org, j.id, j.snapshot, j.plan, delta).await?;
        return Ok(());
    };
    let token = preparation_reservation(conn, j).await?;
    let capture=sqlx::query("SELECT * FROM migration_snapshot_capture WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(next.get::<Uuid,_>("id")).bind(j.snapshot).bind(j.org.0).fetch_one(&mut *conn).await?;
    let capture_id: Uuid = capture.get("id");
    let sequence: i64 = capture.get("sequence");
    let family: String = capture.get("stream");
    let stream = match family.as_str() {
        "people" => Stream::People,
        "stages" => Stream::Stages,
        "users" => Stream::Users,
        _ => return Err(MigrationError::SourceNotEligible),
    };
    let raw = crypto::open_snapshot(
        key,
        j.org,
        j.snapshot,
        capture_id,
        "capture",
        &capture.get::<Vec<u8>, _>("nonce"),
        &capture.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let extracted = import_source::extract_page(stream, &raw);
    let records=sqlx::query("SELECT id,source_id,ordinal,family,representation,semantic_hmac FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101").bind(capture_id).bind(j.snapshot).bind(j.org.0).fetch_all(&mut *conn).await?;
    if records.len() > 100 {
        return Err(MigrationError::SourceNotEligible);
    }
    let accepted = capture.get::<bool, _>("accepted")
        && !capture.get::<bool, _>("truncated")
        && (200..300).contains(&capture.get::<i32, _>("http_status"))
        && capture.get::<String, _>("classification") == "success"
        && capture.get::<String, _>("representation") == stream.representation()
        && capture.get::<i64, _>("raw_byte_len") == raw.len() as i64;
    let mut added = 0;
    let mut invalid = 0;
    for r in &records {
        let Some(source_id) = r.get::<Option<String>, _>("source_id") else {
            invalid += 1;
            continue;
        };
        let ordinal: i32 = r.get("ordinal");
        let item = usize::try_from(ordinal)
            .ok()
            .and_then(|v| extracted.as_ref().ok().and_then(|a| a.get(v)))
            .cloned();
        let mut item = item.unwrap_or(ExtractedRecord {
            source_id: Some(source_id.clone()),
            canonical: vec![],
            entity: Entity::Invalid,
            reasons: vec!["source_integrity".into()],
            transformations: vec![],
            provenance: Default::default(),
        });
        let semantic = crypto::snapshot_hmac(
            key,
            j.org,
            &format!("semantic:{}", stream.representation()),
            &item.canonical,
        );
        let integrity = item.source_id.as_deref() == Some(&source_id)
            && r.get::<String, _>("family") == family
            && r.get::<String, _>("representation") == stream.representation()
            && r.get::<Vec<u8>, _>("semantic_hmac") == semantic;
        let qualified = accepted && integrity;
        item.canonical.clear();
        let existing=sqlx::query("SELECT id,qualified,conflict,semantic_hmac,nonce,ciphertext FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4 FOR UPDATE").bind(j.plan).bind(j.org.0).bind(&family).bind(&source_id).fetch_optional(&mut *conn).await?;
        let id = existing
            .as_ref()
            .map(|v| v.get::<Uuid, _>("id"))
            .unwrap_or_else(Uuid::new_v4);
        let disagreement = existing
            .as_ref()
            .is_some_and(|v| v.get::<Vec<u8>, _>("semantic_hmac") != semantic)
            || !integrity;
        if let Some(e) = &existing {
            // A rejected identical observation never becomes the winner. An accepted
            // equal value may qualify an earlier rejected identical observation.
            if e.get::<bool, _>("qualified") || !qualified {
                sqlx::query("UPDATE migration_import_source SET conflict=conflict OR $3,observations=observations+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(j.org.0).bind(disagreement).execute(&mut *conn).await?;
                continue;
            }
        }
        let sealed = imports::seal(key, j.org, j.snapshot, j.plan, id, "source", &item)?;
        let size = imports::sealed_bytes(&sealed)
            + source_id.len() as i64
            + stream.representation().len() as i64
            + 32;
        if let Some(e) = existing {
            added += size
                - ((e.get::<Vec<u8>, _>("nonce").len() + e.get::<Vec<u8>, _>("ciphertext").len())
                    as i64
                    + source_id.len() as i64
                    + stream.representation().len() as i64
                    + 32);
            sqlx::query("UPDATE migration_import_source SET record_id=$3,capture_id=$4,ordinal=$5,representation=$6,semantic_hmac=$7,qualified=$8,conflict=conflict OR $9,observations=observations+1,nonce=$10,ciphertext=$11 WHERE id=$1 AND organization_id=$2").bind(id).bind(j.org.0).bind(r.get::<Uuid,_>("id")).bind(capture_id).bind(ordinal).bind(stream.representation()).bind(semantic.as_slice()).bind(qualified).bind(disagreement).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        } else {
            added += size;
            sqlx::query("INSERT INTO migration_import_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,ordinal,representation,semantic_hmac,qualified,conflict,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)").bind(id).bind(j.plan).bind(j.id).bind(j.snapshot).bind(j.org.0).bind(&family).bind(source_id).bind(r.get::<Uuid,_>("id")).bind(capture_id).bind(ordinal).bind(stream.representation()).bind(semantic.as_slice()).bind(qualified).bind(disagreement).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        }
    }
    // An accepted raw item without its required immutable observation is damaged
    // retained evidence, including a missing whole capture record set.
    if accepted && extracted.as_ref().is_ok_and(|v| v.len() != records.len()) {
        return Err(MigrationError::SourceNotEligible);
    }
    sqlx::query("UPDATE migration_import_plan SET checkpoint_capture=$3,invalid_ids=invalid_ids+$4 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(sequence).bind(invalid).execute(&mut *conn).await?;
    imports::settle(conn, j.org, j.id, token, added).await
}
fn decode_source(
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
) -> Result<ExtractedRecord, MigrationError> {
    imports::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        r.get("id"),
        "source",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
}
async fn choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    kind: &str,
    k: &str,
) -> Result<Value, MigrationError> {
    let row=sqlx::query("SELECT id,nonce,ciphertext FROM migration_import_choice WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(j.plan).bind(j.org.0).bind(kind).bind(k).fetch_optional(conn).await?;
    match row {
        Some(r) => imports::open(
            key,
            j.org,
            j.snapshot,
            j.plan,
            r.get("id"),
            "choice",
            &r.get::<Vec<u8>, _>("nonce"),
            &r.get::<Vec<u8>, _>("ciphertext"),
        ),
        None => Ok(json!({"kind":"hold"})),
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "Qualified evidence position and exact source mapping keys are independent"
)]
async fn mapping(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    kind: &str,
    k: &str,
    source: Option<ExtractedRecord>,
    qualified: bool,
    sequence: i64,
    ordinal: i32,
) -> Result<(Uuid, i64), MigrationError> {
    let existing=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(j.plan).bind(j.org.0).bind(kind).bind(k).fetch_optional(&mut *conn).await?;
    if let Some(id) = existing {
        return Ok((id, 0));
    }
    let id = Uuid::new_v4();
    let mut body = Mapping {
        source,
        choice: choice(conn, key, j, kind, k).await?,
        reasons: vec![],
        suggestions: vec![],
        target: None,
    };
    let mut disposition = "hold";
    let mut target = None;
    let mut label_hmac = None;
    if !qualified {
        body.reasons.push("source_unqualified".into())
    }
    if kind == "stage" {
        let stage = body.source.as_ref().and_then(|r| match &r.entity {
            Entity::Stage(v) => Some(v),
            _ => None,
        });
        let label = stage.and_then(|v| v.label.as_deref());
        if let Some(label) = label {
            label_hmac = Some(
                crypto::snapshot_hmac(key, j.org, "import-stage-label", label.as_bytes()).to_vec(),
            );
            // PostgreSQL text cannot represent U+0000. Preserve/HMAC the exact
            // source label while skipping only this optional native suggestion.
            if !label.contains('\0') {
                let rows=sqlx::query("SELECT id,name,position FROM stage WHERE organization_id=$1 AND name=$2 ORDER BY id LIMIT 2").bind(j.org.0).bind(label).fetch_all(&mut *conn).await?;
                body.suggestions=rows.into_iter().map(|v|json!({"id":v.get::<Uuid,_>("id"),"name":v.get::<String,_>("name"),"position":v.get::<i16,_>("position")})).collect();
            }
        }
        if qualified {
            match serde_json::from_value::<StageChoice>(body.choice.clone())
                .map_err(|_| MigrationError::Crypto)?
            {
                StageChoice::Hold => body.reasons.push("stage_choice_required".into()),
                StageChoice::Existing { stage_id } => {
                    let r = sqlx::query(
                        "SELECT id,name,position FROM stage WHERE organization_id=$1 AND id=$2",
                    )
                    .bind(j.org.0)
                    .bind(stage_id)
                    .fetch_optional(&mut *conn)
                    .await?;
                    if let Some(r) = r {
                        disposition = "existing";
                        target = Some(stage_id);
                        body.target = Some(
                            json!({"id":stage_id,"name":r.get::<String,_>("name"),"position":r.get::<i16,_>("position")}),
                        );
                    } else {
                        body.reasons.push("destination_stage_missing".into())
                    }
                }
                StageChoice::Create => {
                    if stage.is_some_and(|v| v.can_create) && body.suggestions.is_empty() {
                        disposition = "create";
                        target = Some(Uuid::new_v4());
                        body.target = Some(json!({"id":target,"name":label}));
                    } else {
                        body.reasons.push("stage_creation_unavailable".into())
                    }
                }
            }
        }
    } else {
        let user = body.source.as_ref().and_then(|r| match &r.entity {
            Entity::User(v) => Some(v),
            _ => None,
        });
        if let Some(email) = user
            .and_then(|v| v.email.as_deref())
            .filter(|v| !v.contains('\0'))
        {
            let rows=sqlx::query("SELECT u.id,u.email,u.display_name FROM app_user u JOIN organization_membership m ON m.user_id=u.id WHERE m.organization_id=$1 AND m.status='active' AND lower(u.email)=lower($2) ORDER BY u.id LIMIT 2").bind(j.org.0).bind(email).fetch_all(&mut *conn).await?;
            body.suggestions=rows.into_iter().map(|v|json!({"id":v.get::<Uuid,_>("id"),"email":v.get::<String,_>("email"),"name":v.get::<String,_>("display_name")})).collect();
        }
        if qualified {
            match serde_json::from_value::<AssigneeChoice>(body.choice.clone())
                .map_err(|_| MigrationError::Crypto)?
            {
                AssigneeChoice::Hold => body.reasons.push("assignee_choice_required".into()),
                AssigneeChoice::Unassigned => disposition = "unassigned",
                AssigneeChoice::Member { user_id } => {
                    let r=sqlx::query("SELECT u.id,u.email,u.display_name FROM app_user u JOIN organization_membership m ON m.user_id=u.id WHERE m.organization_id=$1 AND m.status='active' AND u.id=$2").bind(j.org.0).bind(user_id).fetch_optional(&mut *conn).await?;
                    if let Some(r) = r {
                        disposition = "member";
                        target = Some(user_id);
                        body.target = Some(
                            json!({"id":user_id,"email":r.get::<String,_>("email"),"name":r.get::<String,_>("display_name")}),
                        );
                    } else {
                        body.reasons.push("destination_member_missing".into())
                    }
                }
            }
        }
    }
    let sealed = imports::seal(key, j.org, j.snapshot, j.plan, id, "mapping", &body)?;
    let added = imports::sealed_bytes(&sealed)
        + k.len() as i64
        + label_hmac.as_ref().map_or(0, Vec::len) as i64;
    let bound = body
        .target
        .as_ref()
        .and_then(|v| v["name"].as_str())
        .map_or(0, |v| v.len() as i64 * 2 + 1024);
    sqlx::query("INSERT INTO migration_import_mapping(id,plan_id,import_id,organization_id,kind,source_key,qualified,disposition,target_id,nonce,ciphertext,source_sequence,source_ordinal,added_byte_bound,label_hmac) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(kind).bind(k).bind(qualified).bind(disposition).bind(target).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(sequence).bind(ordinal).bind(bound).bind(label_hmac).execute(&mut *conn).await?;
    Ok((id, added))
}
async fn mappings(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let row=sqlx::query("SELECT s.id FROM migration_import_source s WHERE s.plan_id=$1 AND s.organization_id=$2 AND s.family IN ('stages','users') AND (s.family,s.source_id)>($3,$4) ORDER BY s.family,s.source_id LIMIT 1").bind(j.plan).bind(j.org.0).bind(p.get::<String,_>("checkpoint_family")).bind(p.get::<String,_>("checkpoint_source_id")).fetch_optional(&mut *conn).await?;
    let Some(r) = row else {
        let delta = phase(conn, j, "people").await?;
        imports::charge(conn, j.org, j.id, j.snapshot, j.plan, delta).await?;
        return Ok(());
    };
    let token = preparation_reservation(conn, j).await?;
    let r=sqlx::query("SELECT s.*,c.sequence FROM migration_import_source s JOIN migration_snapshot_capture c ON c.id=s.capture_id AND c.organization_id=s.organization_id WHERE s.id=$1 AND s.plan_id=$2 AND s.organization_id=$3").bind(r.get::<Uuid,_>("id")).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
    let item = decode_source(key, j, &r)?;
    let qualified = r.get::<bool, _>("qualified")
        && !r.get::<bool, _>("conflict")
        && item.reasons.iter().all(|v| v.starts_with("stage_create_"));
    let family: String = r.get("family");
    let source: String = r.get("source_id");
    let (_, mut added) = mapping(
        conn,
        key,
        j,
        if family == "stages" {
            "stage"
        } else {
            "assignee"
        },
        &source,
        Some(item),
        qualified,
        r.get("sequence"),
        r.get("ordinal"),
    )
    .await?;
    added += source.len() as i64 - p.get::<String, _>("checkpoint_source_id").len() as i64;
    sqlx::query("UPDATE migration_import_plan SET checkpoint_family=$3,checkpoint_source_id=$4 WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(family).bind(source).execute(&mut *conn).await?;
    imports::settle(conn, j.org, j.id, token, added).await
}
async fn manifest(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    p: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let row=sqlx::query("SELECT id FROM migration_import_source WHERE plan_id=$1 AND organization_id=$2 AND family='people' AND source_id>$3 ORDER BY source_id LIMIT 1").bind(j.plan).bind(j.org.0).bind(p.get::<String,_>("checkpoint_source_id")).fetch_optional(&mut *conn).await?;
    let Some(r) = row else {
        return ready(conn, key, j).await;
    };
    let token = preparation_reservation(conn, j).await?;
    let r = sqlx::query(
        "SELECT * FROM migration_import_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
    )
    .bind(r.get::<Uuid, _>("id"))
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    let item = decode_source(key, j, &r)?;
    let source: String = r.get("source_id");
    let mut reasons = item.reasons.clone();
    let mut added = 0;
    if !r.get::<bool, _>("qualified") {
        reasons.push("source_unqualified".into())
    }
    if r.get::<bool, _>("conflict") {
        reasons.push("source_conflict".into())
    }
    let mut stage = None;
    let mut assignee = None;
    let mut count = 0;
    let mut native_bytes = 0;
    if let Entity::People(person) = &item.entity {
        count = person.contacts.len() as i64;
        native_bytes = person.first_name.as_ref().map_or(0, String::len)
            + person.last_name.as_ref().map_or(0, String::len)
            + person
                .contacts
                .iter()
                .map(|v| v.kind.len() + v.value.len() + v.normalized_value.len() + 64)
                .sum::<usize>();
        if let Some(label) = &person.stage_label {
            let hmac = crypto::snapshot_hmac(key, j.org, "import-stage-label", label.as_bytes());
            let rows=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='stage' AND label_hmac=$3 ORDER BY source_key LIMIT 2").bind(j.plan).bind(j.org.0).bind(hmac.as_slice()).fetch_all(&mut *conn).await?;
            if rows.len() == 1 {
                stage = Some(rows[0]);
            } else {
                let k = format!("unresolved:{}", imports::hex(&hmac));
                let (id, n) = mapping(conn, key, j, "stage", &k, None, false, 0, 0).await?;
                stage = Some(id);
                added += n;
            }
        } else {
            let (id, n) = mapping(conn, key, j, "stage", "missing", None, true, 0, 0).await?;
            stage = Some(id);
            added += n;
        }
        if let Some(k) = &person.assignee_key {
            let existing=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='assignee' AND source_key=$3").bind(j.plan).bind(j.org.0).bind(k).fetch_optional(&mut *conn).await?;
            if let Some(id) = existing {
                assignee = Some(id)
            } else {
                let (id, n) = mapping(
                    conn,
                    key,
                    j,
                    "assignee",
                    k,
                    None,
                    k.starts_with("pond:"),
                    0,
                    0,
                )
                .await?;
                assignee = Some(id);
                added += n;
            }
        }
    } else {
        reasons.push("unsupported_record_shape".into())
    }
    for id in [stage, assignee].into_iter().flatten() {
        let disposition=sqlx::query_scalar::<_,String>("UPDATE migration_import_mapping SET dependent_count=dependent_count+1 WHERE id=$1 AND organization_id=$2 RETURNING disposition").bind(id).bind(j.org.0).fetch_one(&mut *conn).await?;
        if disposition == "hold" {
            reasons.push(
                if Some(id) == stage {
                    "stage_mapping_held"
                } else {
                    "assignee_mapping_held"
                }
                .into(),
            )
        }
    }
    let overlap=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM (SELECT DISTINCT kind,key_hmac FROM migration_snapshot_contact_key WHERE snapshot_id=$1 AND organization_id=$2 AND source_id=$3 AND capture_sequence<=$4) k WHERE EXISTS(SELECT 1 FROM migration_snapshot_contact_key o WHERE o.snapshot_id=$1 AND o.organization_id=$2 AND o.kind=k.kind AND o.key_hmac=k.key_hmac AND o.source_id<>$3 AND o.capture_sequence<=$4)").bind(j.snapshot).bind(j.org.0).bind(&source).bind(j.boundary).fetch_one(&mut *conn).await?;
    // Exact encrypted provenance serialization plus native fields, per-contact
    // IDs/order receipts and fixed audit envelopes. The bound is input-derived.
    let provenance_len = imports::bytes(&item)?.len() as i64;
    let bound =
        provenance_len + native_bytes as i64 + count * 128 + source.len() as i64 * 3 + 16 * 1024;
    if bound > imports::UNIT {
        reasons.push("import_item_byte_limit".into())
    }
    reasons.sort();
    reasons.dedup();
    let eligible = reasons.is_empty();
    let id = Uuid::new_v4();
    let body = Manifest {
        reasons: reasons.clone(),
        transformations: item.transformations.clone(),
        person_id: Uuid::new_v4(),
    };
    let s = imports::seal(key, j.org, j.snapshot, j.plan, id, "manifest", &body)?;
    added += imports::sealed_bytes(&s) + source.len() as i64;
    sqlx::query("INSERT INTO migration_import_manifest(id,plan_id,import_id,organization_id,source_id,source_row_id,disposition,stage_mapping_id,assignee_mapping_id,contact_count,overlap_count,added_byte_bound,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(id).bind(j.plan).bind(j.id).bind(j.org.0).bind(&source).bind(r.get::<Uuid,_>("id")).bind(if eligible{"eligible"}else{"held"}).bind(stage).bind(assignee).bind(count).bind(overlap).bind(bound).bind(s.nonce.as_slice()).bind(s.ciphertext).execute(&mut *conn).await?;
    added += source.len() as i64 - p.get::<String, _>("checkpoint_source_id").len() as i64;
    let assigned = if let Some(id) = assignee {
        sqlx::query_scalar::<_,bool>("SELECT disposition='member' FROM migration_import_mapping WHERE id=$1 AND organization_id=$2").bind(id).bind(j.org.0).fetch_one(&mut *conn).await?
    } else {
        false
    };
    sqlx::query("UPDATE migration_import_plan SET checkpoint_source_id=$3,source_people=source_people+1,eligible_people=eligible_people+$4,held_people=held_people+$5,contacts=contacts+$6,overlap_people=overlap_people+$7,assigned_people=assigned_people+$8,unassigned_people=unassigned_people+$9,max_added_byte_bound=greatest(max_added_byte_bound,$10) WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(source).bind(i64::from(eligible)).bind(i64::from(!eligible)).bind(if eligible{count}else{0}).bind(i64::from(overlap>0)).bind(i64::from(eligible&&assigned)).bind(i64::from(eligible&&!assigned)).bind(if eligible{bound}else{0}).execute(&mut *conn).await?;
    for code in &reasons {
        sqlx::query("INSERT INTO migration_import_issue(plan_id,organization_id,code,record_count) VALUES($1,$2,$3,1) ON CONFLICT(plan_id,organization_id,code) DO UPDATE SET record_count=migration_import_issue.record_count+1").bind(j.plan).bind(j.org.0).bind(code).execute(&mut *conn).await?;
    }
    imports::settle(conn, j.org, j.id, token, added).await
}
async fn ready(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
) -> Result<(), MigrationError> {
    let counts=sqlx::query("SELECT source_people,eligible_people,held_people,contacts FROM migration_import_plan WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
    imports::charge(conn, j.org, j.id, j.snapshot, j.plan, 32).await?;
    let digest = crypto::snapshot_hmac(
        key,
        j.org,
        "import-confirm",
        &imports::bytes(
            &json!({"engine":imports::ENGINE,"run":j.id,"plan":j.plan,"boundary":j.boundary,"source_people":counts.get::<i64,_>("source_people"),"eligible":counts.get::<i64,_>("eligible_people"),"held":counts.get::<i64,_>("held_people"),"contacts":counts.get::<i64,_>("contacts")}),
        )?,
    );
    sqlx::query("UPDATE migration_import_plan SET phase='ready',state='ready',confirmation_digest=$3,expires_at=now()+interval '10 minutes',completed_at=now(),stages_to_create=(SELECT count(*) FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND disposition='create' AND EXISTS(SELECT 1 FROM migration_import_manifest x WHERE x.stage_mapping_id=migration_import_mapping.id AND x.organization_id=$2 AND x.disposition='eligible')) WHERE id=$1 AND organization_id=$2").bind(j.plan).bind(j.org.0).bind(digest.as_slice()).execute(conn).await?;
    Ok(())
}
async fn execute(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let bound=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_workspace w JOIN organization o ON o.id=w.organization_id WHERE w.organization_id=$1 AND w.import_id=$2 AND w.plan_id=$3 AND o.workspace_mode='migration_review')").bind(j.org.0).bind(j.id).bind(j.plan).fetch_one(&mut *conn).await?;
    if !bound {
        return Err(MigrationError::ImportConflict);
    }
    sqlx::query("UPDATE migration_import SET state='running' WHERE id=$1 AND organization_id=$2")
        .bind(j.id)
        .bind(j.org.0)
        .execute(&mut *conn)
        .await?;
    // Only this private executor can obtain the narrowly scoped transaction permit.
    let permit = WritePermit { job: j };
    sqlx::query("SELECT set_config('crm.import_token',$1,true)")
        .bind(j.token.to_string())
        .execute(&mut *conn)
        .await?;
    match r.get::<String, _>("phase").as_str() {
        "stages" => create_stage(conn, key, &permit, r).await,
        "people" => create_person(conn, key, &permit, r).await,
        _ => Err(MigrationError::ImportConflict),
    }
}
struct WritePermit<'a> {
    job: &'a Job,
}
async fn create_stage(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    permit: &WritePermit<'_>,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let j = permit.job;
    let row=sqlx::query("SELECT id,added_byte_bound FROM migration_import_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='stage' AND disposition='create' AND EXISTS(SELECT 1 FROM migration_import_manifest x WHERE x.stage_mapping_id=migration_import_mapping.id AND x.organization_id=$2 AND x.disposition='eligible') AND (source_sequence,source_ordinal,source_key)>($3,$4,$5) ORDER BY source_sequence,source_ordinal,source_key LIMIT 1").bind(j.plan).bind(j.org.0).bind(r.get::<i64,_>("checkpoint_stage_sequence")).bind(r.get::<i32,_>("checkpoint_stage_ordinal")).bind(r.get::<String,_>("checkpoint_stage_id")).fetch_optional(&mut *conn).await?;
    let Some(m) = row else {
        sqlx::query(
            "UPDATE migration_import SET phase='people' WHERE id=$1 AND organization_id=$2",
        )
        .bind(j.id)
        .bind(j.org.0)
        .execute(conn)
        .await?;
        return Ok(());
    };
    let token = Uuid::new_v4();
    if !imports::reserve(
        conn,
        j.org,
        j.id,
        j.snapshot,
        j.plan,
        j.token,
        token,
        m.get::<i64, _>("added_byte_bound").max(1024),
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    let m = sqlx::query(
        "SELECT * FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
    )
    .bind(m.get::<Uuid, _>("id"))
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    let body: Mapping = imports::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        m.get("id"),
        "mapping",
        &m.get::<Vec<u8>, _>("nonce"),
        &m.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let target = m.get::<Uuid, _>("target_id");
    let name = body
        .target
        .as_ref()
        .and_then(|v| v["name"].as_str())
        .ok_or(MigrationError::Crypto)?;
    let exists=sqlx::query_scalar::<_,Uuid>("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='stages' AND source_id=$3").bind(j.org.0).bind(r.get::<i64,_>("source_account_id")).bind(m.get::<String,_>("source_key")).fetch_optional(&mut *conn).await?;
    let mut actual = 0;
    if exists.is_none() {
        let position = sqlx::query_scalar::<_, i32>(
            "SELECT COALESCE(max(position)::integer,-1)+1 FROM stage WHERE organization_id=$1",
        )
        .bind(j.org.0)
        .fetch_one(&mut *conn)
        .await?;
        let position = i16::try_from(position).map_err(|_| MigrationError::InvalidImportChoice)?;
        sqlx::query("INSERT INTO stage(id,organization_id,name,position) VALUES($1,$2,$3,$4)")
            .bind(target)
            .bind(j.org.0)
            .bind(name)
            .bind(position)
            .execute(&mut *conn)
            .await?;
        sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id,mapping_id) VALUES($1,$2,'stages',$3,$4,$5,$6,$7)").bind(j.org.0).bind(r.get::<i64,_>("source_account_id")).bind(m.get::<String,_>("source_key")).bind(target).bind(j.id).bind(j.plan).bind(m.get::<Uuid,_>("id")).execute(&mut *conn).await?;
        actual = m.get::<String, _>("source_key").len() as i64;
    } else if exists != Some(target) {
        return Err(MigrationError::ImportConflict);
    }
    actual += m.get::<String, _>("source_key").len() as i64
        - r.get::<String, _>("checkpoint_stage_id").len() as i64;
    sqlx::query("UPDATE migration_import SET checkpoint_stage_id=$3,checkpoint_stage_sequence=$4,checkpoint_stage_ordinal=$5 WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(m.get::<String,_>("source_key")).bind(m.get::<i64,_>("source_sequence")).bind(m.get::<i32,_>("source_ordinal")).execute(&mut *conn).await?;
    imports::settle(conn, j.org, j.id, token, actual).await
}
async fn selected_target(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    j: &Job,
    id: Option<Uuid>,
    kind: &str,
) -> Result<Option<Uuid>, MigrationError> {
    let Some(id) = id else { return Ok(None) };
    let m = sqlx::query(
        "SELECT * FROM migration_import_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
    )
    .bind(id)
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    let target: Option<Uuid> = m.get("target_id");
    let body: Mapping = imports::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        id,
        "mapping",
        &m.get::<Vec<u8>, _>("nonce"),
        &m.get::<Vec<u8>, _>("ciphertext"),
    )?;
    match m.get::<String, _>("disposition").as_str() {
        "unassigned" => Ok(None),
        "member" if kind == "assignee" => {
            let row=sqlx::query("SELECT u.email FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.user_id=$2 AND m.status='active' FOR SHARE OF m").bind(j.org.0).bind(target).fetch_optional(conn).await?;
            if row.is_some_and(|r| {
                body.target
                    .as_ref()
                    .is_some_and(|v| v["email"] == r.get::<String, _>("email"))
            }) {
                Ok(target)
            } else {
                Err(MigrationError::InvalidImportChoice)
            }
        }
        "existing" | "create" if kind == "stage" => {
            let row = sqlx::query("SELECT name FROM stage WHERE organization_id=$1 AND id=$2")
                .bind(j.org.0)
                .bind(target)
                .fetch_optional(conn)
                .await?;
            if row.is_some_and(|r| {
                body.target
                    .as_ref()
                    .is_some_and(|v| v["name"] == r.get::<String, _>("name"))
            }) {
                Ok(target)
            } else {
                Err(MigrationError::InvalidImportChoice)
            }
        }
        _ => Err(MigrationError::InvalidImportChoice),
    }
}
async fn create_person(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    permit: &WritePermit<'_>,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let j = permit.job;
    let row=sqlx::query("SELECT id,disposition,added_byte_bound FROM migration_import_manifest WHERE plan_id=$1 AND organization_id=$2 AND source_id>$3 ORDER BY source_id LIMIT 1").bind(j.plan).bind(j.org.0).bind(r.get::<String,_>("checkpoint_source_id")).fetch_optional(&mut *conn).await?;
    let Some(m) = row else {
        imports::release_cancel(conn, j.org, j.id, 0).await?;
        sqlx::query("UPDATE migration_import SET state='completed',phase='complete',completed_at=now(),pause_reason=NULL WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).execute(conn).await?;
        return Ok(());
    };
    let token = Uuid::new_v4();
    let eligible = m.get::<String, _>("disposition") == "eligible";
    let amount = if eligible {
        m.get::<i64, _>("added_byte_bound")
    } else {
        16 * 1024
    };
    if !imports::reserve(
        conn, j.org, j.id, j.snapshot, j.plan, j.token, token, amount,
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    let m = sqlx::query(
        "SELECT * FROM migration_import_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3",
    )
    .bind(m.get::<Uuid, _>("id"))
    .bind(j.plan)
    .bind(j.org.0)
    .fetch_one(&mut *conn)
    .await?;
    let source: String = m.get("source_id");
    let manifest: Manifest = imports::open(
        key,
        j.org,
        j.snapshot,
        j.plan,
        m.get("id"),
        "manifest",
        &m.get::<Vec<u8>, _>("nonce"),
        &m.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let result_id = Uuid::new_v4();
    let mut person_id = None;
    let mut count = 0i64;
    let mut actual = 0i64;
    let mut disposition = "held";
    let provenance = if eligible {
        let source_row=sqlx::query("SELECT * FROM migration_import_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(m.get::<Uuid,_>("source_row_id")).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
        let item = decode_source(key, j, &source_row)?;
        let Entity::People(person) = &item.entity else {
            return Err(MigrationError::Crypto);
        };
        let old=sqlx::query_scalar::<_,Uuid>("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND source_account_id=$2 AND family='people' AND source_id=$3").bind(j.org.0).bind(r.get::<i64,_>("source_account_id")).bind(&source).fetch_optional(&mut *conn).await?;
        if let Some(id) = old {
            person_id = Some(id);
            disposition = "already_imported";
        } else {
            let stage = selected_target(conn, key, j, m.get("stage_mapping_id"), "stage")
                .await?
                .ok_or(MigrationError::InvalidImportChoice)?;
            let assignee =
                selected_target(conn, key, j, m.get("assignee_mapping_id"), "assignee").await?;
            let id = manifest.person_id;
            sqlx::query("INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(j.org.0).bind(&person.first_name).bind(&person.last_name).bind(stage).bind(assignee).execute(&mut *conn).await?;
            person_id = Some(id);
            disposition = "imported";
            count = person.contacts.len() as i64;
            sqlx::query("INSERT INTO migration_import_identity(organization_id,source_account_id,family,source_id,target_id,import_id,plan_id,manifest_id) VALUES($1,$2,'people',$3,$4,$5,$6,$7)").bind(j.org.0).bind(r.get::<i64,_>("source_account_id")).bind(&source).bind(id).bind(j.id).bind(j.plan).bind(m.get::<Uuid,_>("id")).execute(&mut *conn).await?;
            actual += source.len() as i64;
            let occurred = Utc::now();
            let fact = Uuid::new_v4();
            sqlx::query("INSERT INTO person_imported(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,person_id,import_id,plan_id,source_record_id,capture_id) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,$8,$9,$10)").bind(fact).bind(j.org.0).bind(j.actor.0).bind(occurred).bind(j.id).bind(id).bind(j.id).bind(j.plan).bind(source_row.get::<Uuid,_>("record_id")).bind(source_row.get::<Uuid,_>("capture_id")).execute(&mut *conn).await?;
            sqlx::query("INSERT INTO stage_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_stage_id,to_stage_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration')").bind(Uuid::new_v4()).bind(j.org.0).bind(j.actor.0).bind(occurred).bind(j.id).bind(fact).bind(id).bind(stage).execute(&mut *conn).await?;
            sqlx::query("INSERT INTO assignment_changed(id,organization_id,actor_kind,on_behalf_of_user_id,origin,occurred_at,correlation_id,causation_id,person_id,from_user_id,to_user_id,reason) VALUES($1,$2,'system',$3,'migration',$4,$5,$6,$7,NULL,$8,'migration')").bind(Uuid::new_v4()).bind(j.org.0).bind(j.actor.0).bind(occurred).bind(j.id).bind(fact).bind(id).bind(assignee).execute(&mut *conn).await?;
        }
        imports::seal(
            key,
            j.org,
            j.snapshot,
            j.plan,
            result_id,
            "provenance",
            &item,
        )?
    } else {
        imports::seal(
            key,
            j.org,
            j.snapshot,
            j.plan,
            result_id,
            "provenance",
            &json!({"reasons":manifest.reasons}),
        )?
    };
    actual += imports::sealed_bytes(&provenance) + source.len() as i64;
    sqlx::query("INSERT INTO migration_import_result(id,import_id,plan_id,organization_id,manifest_id,source_id,person_id,disposition,contact_count,provenance_nonce,provenance_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(result_id).bind(j.id).bind(j.plan).bind(j.org.0).bind(m.get::<Uuid,_>("id")).bind(&source).bind(person_id).bind(disposition).bind(count).bind(provenance.nonce.as_slice()).bind(provenance.ciphertext).execute(&mut *conn).await?;
    if disposition == "imported" {
        let source_row=sqlx::query("SELECT * FROM migration_import_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(m.get::<Uuid,_>("source_row_id")).bind(j.plan).bind(j.org.0).fetch_one(&mut *conn).await?;
        let item = decode_source(key, j, &source_row)?;
        let Entity::People(person) = item.entity else {
            return Err(MigrationError::Crypto);
        };
        for c in person.contacts {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO contact_method(id,organization_id,person_id,kind,value,normalized_value,import_order) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(id).bind(j.org.0).bind(person_id).bind(&c.kind).bind(&c.value).bind(&c.normalized_value).bind(c.import_order).execute(&mut *conn).await?;
            sqlx::query("INSERT INTO migration_import_contact(result_id,organization_id,contact_id,kind,import_order) VALUES($1,$2,$3,$4,$5)").bind(result_id).bind(j.org.0).bind(id).bind(&c.kind).bind(c.import_order).execute(&mut *conn).await?;
        }
    }
    actual += source.len() as i64 - r.get::<String, _>("checkpoint_source_id").len() as i64;
    sqlx::query("UPDATE migration_import SET checkpoint_source_id=$3,imported_people=imported_people+$4,imported_contacts=imported_contacts+$5,settled_people=settled_people+1 WHERE id=$1 AND organization_id=$2").bind(j.id).bind(j.org.0).bind(source).bind(i64::from(disposition=="imported")).bind(count).execute(&mut *conn).await?;
    imports::settle(conn, j.org, j.id, token, actual).await
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &super::snapshot::SnapshotPolicy,
) -> Result<bool, MigrationError> {
    imports::with_policy(policy, run_once_inner(pool, key)).await
}
