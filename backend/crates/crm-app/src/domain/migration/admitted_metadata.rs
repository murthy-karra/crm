//! D-082 admitted-People metadata root.  This deliberately has no path through
//! the original metadata child: its cohort is terminal admission results.
use super::{
    core_change_source::Group,
    core_change_store, crypto,
    metadata_source::{self, Entity, Record},
    metadata_store,
    snapshot_source::Stream,
    MigrationError,
};
use crate::{
    auth::workspace::{self, ReleaseReadiness},
    config::RawPayloadKey,
    domain::envelope::CommandContext,
    ids::OrganizationId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const ENGINE: &str = "fub-admitted-metadata-v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub request_id: Uuid,
    pub admission_id: Uuid,
    pub source_report_id: Uuid,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub cursor: Option<Uuid>,
    pub limit: Option<u16>,
    pub admission_id: Option<Uuid>,
}
impl Page {
    fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(50);
        if !(1..=50).contains(&n) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: Uuid,
}
/// The mapping patch is deliberately bounded and addresses only mappings in
/// this immutable preview.  A changed choice creates a fresh plan below; a
/// confirmed plan is never edited in place.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPatch {
    pub request_id: Uuid,
    pub mappings: Vec<MappingChoice>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MappingChoice {
    pub id: Uuid,
    pub disposition: String,
    pub target_id: Option<Uuid>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Confirm {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub plan_revision: i64,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPage {
    pub cursor: Option<Uuid>,
    pub limit: Option<u16>,
}
impl PlanPage {
    fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(50);
        if !(1..=50).contains(&n) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}

#[derive(Serialize)]
struct FrozenSource {
    report_id: Uuid,
    capture_id: Uuid,
    capture_sequence: i64,
    ordinal: i32,
    qualified: bool,
    conflict: bool,
    record: Record,
}

#[derive(Serialize)]
struct FrozenMapping {
    source_id: String,
    label: Option<String>,
    machine_name: Option<String>,
    raw_choice: Option<String>,
    reasons: Vec<String>,
}

#[derive(Serialize)]
struct FrozenOperation {
    source_id: String,
    source_field: Option<String>,
    source_tag: Option<String>,
    reasons: Vec<String>,
}

fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    value: &T,
) -> Result<crypto::Sealed, MigrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(MigrationError::StorageLimit);
    }
    crypto::seal_snapshot(
        key,
        org,
        snapshot,
        row,
        &format!("admitted-metadata-v1:{plan}:{purpose}"),
        &bytes,
    )
    .map_err(|_| MigrationError::Crypto)
}

fn source_key(
    key: &RawPayloadKey,
    org: OrganizationId,
    account: i64,
    kind: &str,
    raw: &[u8],
) -> Vec<u8> {
    metadata_store::source_key(key, org, account, kind, raw)
}

async fn qualified_streams(
    conn: &mut sqlx::PgConnection,
    org: OrganizationId,
    snapshot: Uuid,
    boundary: i64,
) -> Result<(), MigrationError> {
    let rows = sqlx::query("SELECT stream,state,content_gaps,accepted_captures FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 AND stream IN ('people','custom_fields') ORDER BY stream")
        .bind(snapshot).bind(org.0).fetch_all(&mut *conn).await?;
    if rows.len() != 2
        || rows.iter().any(|r| {
            r.get::<String, _>("state") != "completed"
                || r.get::<i64, _>("content_gaps") != 0
                || r.get::<i64, _>("accepted_captures") == 0
        })
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let past: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream IN ('people','custom_fields') AND sequence>$3")
        .bind(snapshot).bind(org.0).bind(boundary).fetch_one(&mut *conn).await?;
    if past != 0 {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}

/// Resolves a report-owned retained observation back to its encrypted capture,
/// then verifies the lossless metadata parse against the snapshot record.  The
/// report group selects the source boundary; raw capture alone never does.
async fn frozen_source(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    report: Uuid,
    snapshot: Uuid,
    family: &str,
    source_id: &str,
    plan: Uuid,
) -> Result<Option<(Uuid, FrozenSource, crypto::Sealed)>, MigrationError> {
    let group = sqlx::query("SELECT id,nonce,ciphertext FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4")
        .bind(report).bind(org.0).bind(family).bind(source_id).fetch_optional(&mut *conn).await?;
    let Some(group) = group else {
        return Ok(None);
    };
    let group_id: Uuid = group.get("id");
    let aggregate: Group = core_change_store::open(
        key,
        org,
        report,
        group_id,
        "group",
        &group.get::<Vec<u8>, _>("nonce"),
        &group.get::<Vec<u8>, _>("ciphertext"),
    )?;
    let evidence = aggregate
        .evidence
        .iter()
        .find(|e| e.side == "newer" && e.snapshot_id == snapshot && e.stream == family)
        .cloned();
    let Some(evidence) = evidence else {
        return Ok(None);
    };
    let cap = sqlx::query("SELECT * FROM migration_snapshot_capture WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3")
        .bind(evidence.capture_id).bind(snapshot).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Crypto)?;
    let stream = Stream::parse(family).ok_or(MigrationError::SourceNotEligible)?;
    let raw = crypto::open_snapshot(
        key,
        org,
        snapshot,
        evidence.capture_id,
        "capture",
        &cap.get::<Vec<u8>, _>("nonce"),
        &cap.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    let mut record = metadata_source::extract_page(stream, &raw)
        .map_err(|_| MigrationError::SourceNotEligible)?
        .get(usize::try_from(evidence.ordinal).map_err(|_| MigrationError::Crypto)?)
        .cloned()
        .ok_or(MigrationError::Crypto)?;
    let snapshot_row = sqlx::query("SELECT family,source_id,representation,semantic_hmac FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 AND ordinal=$4")
        .bind(evidence.capture_id).bind(snapshot).bind(org.0).bind(evidence.ordinal).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Crypto)?;
    let semantic = crypto::snapshot_hmac(
        key,
        org,
        &format!("semantic:{}", stream.representation()),
        &record.canonical,
    );
    let qualified = cap.get::<bool, _>("accepted")
        && !cap.get::<bool, _>("truncated")
        && (200..300).contains(&cap.get::<i32, _>("http_status"))
        && cap.get::<String, _>("classification") == "success"
        && cap.get::<String, _>("representation") == stream.representation()
        && cap.get::<i64, _>("raw_byte_len") == raw.len() as i64
        && record.source_id.as_deref() == Some(source_id)
        && snapshot_row.get::<String, _>("family") == family
        && snapshot_row
            .get::<Option<String>, _>("source_id")
            .as_deref()
            == Some(source_id)
        && snapshot_row.get::<String, _>("representation") == stream.representation()
        && snapshot_row.get::<Vec<u8>, _>("semantic_hmac") == semantic;
    record.canonical.clear();
    let frozen = FrozenSource {
        report_id: report,
        capture_id: evidence.capture_id,
        capture_sequence: cap.get("sequence"),
        ordinal: evidence.ordinal,
        qualified,
        conflict: aggregate.conflicted(),
        record,
    };
    let row_id = Uuid::new_v4();
    let sealed = seal(key, org, snapshot, plan, row_id, "source", &frozen)?;
    Ok(Some((row_id, frozen, sealed)))
}

fn detail(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"parent_import_id":row.get::<Uuid,_>("parent_import_id"),
      "admission_id":row.get::<Uuid,_>("admission_id"),"source_report_id":row.get::<Uuid,_>("source_report_id"),
      "snapshot_id":row.get::<Uuid,_>("snapshot_id"),"source_account_id":row.get::<i64,_>("source_account_id").to_string(),
      "capture_sequence":row.get::<i64,_>("capture_sequence").to_string(),"workspace_revision":row.get::<i64,_>("workspace_revision").to_string(),
      "engine_version":row.get::<String,_>("engine_version"),"state":row.get::<String,_>("state"),"phase":row.get::<String,_>("phase"),
      "shared_claims_ready":row.get::<bool,_>("shared_claims_ready"),"cohort_counts":{"settled_people":row.get::<i64,_>("settled_people").to_string()},
      "remainder":{"available":row.get::<bool,_>("remainder_available")},"actions":{"replan":row.get::<String,_>("state")=="proposed","confirm":row.get::<String,_>("state")=="proposed","retry":row.get::<String,_>("state")=="paused","cancel":matches!(row.get::<String,_>("state").as_str(),"proposed"|"queued"|"running"|"paused"),"remainder":row.get::<bool,_>("remainder_available")}})
}
async fn find(
    pool: &PgPool,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
 .bind(id).bind(org.0).fetch_optional(pool).await?.ok_or(MigrationError::NotFound)
}
pub async fn list(pool: &PgPool, ctx: &CommandContext, q: Page) -> Result<Value, MigrationError> {
    let limit = q.limit()?;
    let after = q.cursor.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.organization_id=$1 AND i.id>$2 AND ($3::uuid IS NULL OR i.admission_id=$3) ORDER BY i.id LIMIT $4")
 .bind(ctx.organization_id.0).bind(after).bind(q.admission_id).bind(limit+1).fetch_all(pool).await?;
    let more = rows.len() as i64 > limit;
    let items = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| detail(&r))
        .collect::<Vec<_>>();
    let next = if more {
        items
            .last()
            .and_then(|v| v["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(json!({"imports":items,"next_cursor":next}))
}
pub async fn get(pool: &PgPool, ctx: &CommandContext, id: Uuid) -> Result<Value, MigrationError> {
    Ok(detail(&find(pool, ctx.organization_id, id).await?))
}
async fn checked_plan(
    pool: &PgPool,
    org: OrganizationId,
    import: Uuid,
    plan: Uuid,
) -> Result<(), MigrationError> {
    let found: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3)")
        .bind(plan).bind(import).bind(org.0).fetch_one(pool).await?;
    if found {
        Ok(())
    } else {
        Err(MigrationError::NotFound)
    }
}
pub async fn mappings(
    pool: &PgPool,
    ctx: &CommandContext,
    import: Uuid,
    plan: Uuid,
    page: PlanPage,
) -> Result<Value, MigrationError> {
    checked_plan(pool, ctx.organization_id, import, plan).await?;
    let limit = page.limit()?;
    let after = page.cursor.unwrap_or(Uuid::nil());
    let rows = sqlx::query("SELECT id,kind,source_id,target_id,target_field_id,disposition FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND id>$4 ORDER BY id LIMIT $5")
        .bind(import).bind(plan).bind(ctx.organization_id.0).bind(after).bind(limit + 1).fetch_all(pool).await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<Value> = rows.into_iter().take(limit as usize).map(|r| json!({"id":r.get::<Uuid,_>("id"),"kind":r.get::<String,_>("kind"),"source_id":r.get::<String,_>("source_id"),"target_id":r.get::<Option<Uuid>,_>("target_id"),"target_field_id":r.get::<Option<Uuid>,_>("target_field_id"),"disposition":r.get::<String,_>("disposition")})).collect();
    let next = if more {
        items
            .last()
            .and_then(|v| v["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(json!({"mappings":items,"next_cursor":next}))
}
pub async fn records(
    pool: &PgPool,
    ctx: &CommandContext,
    import: Uuid,
    plan: Uuid,
    page: PlanPage,
) -> Result<Value, MigrationError> {
    checked_plan(pool, ctx.organization_id, import, plan).await?;
    let limit = page.limit()?;
    let after = page.cursor.unwrap_or(Uuid::nil());
    let rows = sqlx::query("SELECT id,admission_result_id,person_id,source_person_id,disposition,item_byte_bound FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND id>$4 ORDER BY id LIMIT $5")
        .bind(import).bind(plan).bind(ctx.organization_id.0).bind(after).bind(limit + 1).fetch_all(pool).await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<Value> = rows.into_iter().take(limit as usize).map(|r|json!({"id":r.get::<Uuid,_>("id"),"admission_result_id":r.get::<Uuid,_>("admission_result_id"),"person_id":r.get::<Uuid,_>("person_id"),"source_id":r.get::<String,_>("source_person_id"),"disposition":r.get::<String,_>("disposition"),"item_byte_bound":r.get::<i64,_>("item_byte_bound").to_string()})).collect();
    let next = if more {
        items
            .last()
            .and_then(|v| v["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    Ok(json!({"records":items,"next_cursor":next}))
}
pub async fn issues(
    pool: &PgPool,
    ctx: &CommandContext,
    import: Uuid,
    plan: Uuid,
) -> Result<Value, MigrationError> {
    checked_plan(pool, ctx.organization_id, import, plan).await?;
    let rows=sqlx::query("SELECT code,count FROM migration_admitted_metadata_issue WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY code LIMIT 50").bind(import).bind(plan).bind(ctx.organization_id.0).fetch_all(pool).await?;
    Ok(
        json!({"issues":rows.into_iter().map(|r|json!({"code":r.get::<String,_>("code"),"count":r.get::<i64,_>("count").to_string()})).collect::<Vec<_>>()}),
    )
}
/// One-way shared-claim handover.  It is intentionally performed before an
/// admitted root exists: preview cancellation cannot re-enable an old writer.
/// The exclusive workspace lock drains every original metadata unit that uses
/// the shared lock, and the readiness row is committed only with all claims and
/// the original owner's exact retained-byte charges.
async fn handover(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    release: &ReleaseReadiness,
    ctx: &CommandContext,
) -> Result<(), MigrationError> {
    workspace::bounded_lock_wait(conn).await?;
    workspace::exclusive(conn, ctx.organization_id).await?;
    release.require_admitted_metadata(conn).await?;
    super::store::lock_org(conn, ctx.organization_id).await?;
    let readiness = sqlx::query("SELECT state,engine_version FROM migration_metadata_catalog_readiness WHERE organization_id=$1 FOR UPDATE")
        .bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?;
    if let Some(row) = readiness {
        if row.get::<String, _>("state") == "ready" {
            if row.get::<Option<String>, _>("engine_version").as_deref() != Some(ENGINE) {
                return Err(MigrationError::ReleaseNotReady);
            }
            return Ok(());
        }
    }
    // An old worker cannot retain a lease across the exclusive barrier.  An
    // unexpected running row is a failed drain, not a reason to publish a
    // partly populated registry.
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_import WHERE organization_id=$1 AND state='running')")
        .bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    if active {
        return Err(MigrationError::ImportBusy);
    }
    // Identities are append-only original evidence.  The workspace-exclusive
    // and Org namespace locks have already fenced their compatible writers;
    // do not take a row UPDATE lock here, which would unnecessarily require a
    // mutation privilege on that immutable table.
    let rows = sqlx::query("SELECT x.organization_id,x.source_account_id,x.kind,x.source_key,x.target_id,x.import_id,x.plan_id,x.mapping_id,i.snapshot_id FROM migration_metadata_identity x JOIN migration_metadata_import i ON i.id=x.import_id AND i.organization_id=x.organization_id WHERE x.organization_id=$1 ORDER BY x.source_account_id,x.kind,x.source_key FOR UPDATE OF i")
        .bind(ctx.organization_id.0).fetch_all(&mut *conn).await?;
    for row in rows {
        let mapping: Uuid = row.get("mapping_id");
        let snapshot: Uuid = row.get("snapshot_id");
        let reference = serde_json::to_vec(&json!({"original_mapping_id":mapping}))
            .map_err(|_| MigrationError::Crypto)?;
        let sealed = crypto::seal_snapshot(
            key,
            ctx.organization_id,
            snapshot,
            mapping,
            "admitted-metadata-claim-v1",
            &reference,
        )
        .map_err(|_| MigrationError::Crypto)?;
        let bytes = (sealed.nonce.len() + sealed.ciphertext.len()) as i64;
        let inserted = sqlx::query("INSERT INTO migration_metadata_catalog_claim(organization_id,source_account_id,kind,source_key,target_id,original_import_id,original_plan_id,original_mapping_id,evidence_nonce,evidence_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(organization_id,source_account_id,kind,source_key) DO NOTHING")
            .bind(ctx.organization_id.0).bind(row.get::<i64,_>("source_account_id")).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(row.get::<Uuid,_>("target_id")).bind(row.get::<Uuid,_>("import_id")).bind(row.get::<Uuid,_>("plan_id")).bind(mapping).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
        if inserted.rows_affected() == 0 {
            let equal: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4 AND target_id=$5 AND original_mapping_id=$6)")
                .bind(ctx.organization_id.0).bind(row.get::<i64,_>("source_account_id")).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(row.get::<Uuid,_>("target_id")).bind(mapping).fetch_one(&mut *conn).await?;
            if !equal {
                return Err(MigrationError::InvalidImportChoice);
            }
            continue;
        }
        let import: Uuid = row.get("import_id");
        sqlx::query("UPDATE migration_metadata_import SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2")
            .bind(import).bind(ctx.organization_id.0).bind(bytes).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot SET retained_bytes=retained_bytes+$3 WHERE id=$1 AND organization_id=$2")
            .bind(snapshot).bind(ctx.organization_id.0).bind(bytes).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_snapshot_storage SET retained_bytes=retained_bytes+$2 WHERE organization_id=$1")
            .bind(ctx.organization_id.0).bind(bytes).execute(&mut *conn).await?;
    }
    sqlx::query("INSERT INTO migration_metadata_catalog_readiness(organization_id,state,activated_at,activated_by_user_id,engine_version) VALUES($1,'ready',clock_timestamp(),$2,$3) ON CONFLICT(organization_id) DO UPDATE SET state='ready',activated_at=EXCLUDED.activated_at,activated_by_user_id=EXCLUDED.activated_by_user_id,engine_version=EXCLUDED.engine_version")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(ENGINE).execute(&mut *conn).await?;
    Ok(())
}

async fn insert_source(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    import: Uuid,
    plan: Uuid,
    snapshot: Uuid,
    report: Uuid,
    family: &str,
    source_id: &str,
) -> Result<Option<FrozenSource>, MigrationError> {
    let Some((id, frozen, sealed)) =
        frozen_source(conn, key, org, report, snapshot, family, source_id, plan).await?
    else {
        return Ok(None);
    };
    let semantic = crypto::snapshot_hmac(
        key,
        org,
        &format!("admitted-metadata-source:{family}"),
        &serde_json::to_vec(&frozen).map_err(|_| MigrationError::Crypto)?,
    );
    sqlx::query("INSERT INTO migration_admitted_metadata_source(id,import_id,plan_id,organization_id,family,source_id,semantic_hmac,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(plan_id,organization_id,family,source_id) DO NOTHING")
        .bind(id).bind(import).bind(plan).bind(org.0).bind(family).bind(source_id).bind(semantic.as_slice()).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
    Ok(Some(frozen))
}

async fn baseline(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    person: Uuid,
) -> Result<crypto::Sealed, MigrationError> {
    let tags: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(tag_id ORDER BY tag_id),'[]'::jsonb) FROM person_tag WHERE organization_id=$1 AND person_id=$2")
        .bind(org.0).bind(person).fetch_one(&mut *conn).await?;
    let values: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(v) - ARRAY['created_at','updated_at'] ORDER BY field_id),'[]'::jsonb) FROM person_custom_field_value v WHERE organization_id=$1 AND person_id=$2")
        .bind(org.0).bind(person).fetch_one(&mut *conn).await?;
    seal(
        key,
        org,
        snapshot,
        plan,
        row,
        "baseline",
        &json!({"person_id":person,"tags":tags,"values":values}),
    )
}

async fn insert_mapping(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    account: i64,
    import: Uuid,
    plan: Uuid,
    kind: &str,
    source_id: &str,
    raw: &[u8],
    parent: Option<Uuid>,
    target_field: Option<Uuid>,
    frozen: FrozenMapping,
) -> Result<Uuid, MigrationError> {
    let id = Uuid::new_v4();
    let source_key = source_key(key, org, account, kind, raw);
    let sealed = seal(key, org, snapshot, plan, id, "mapping", &frozen)?;
    sqlx::query("INSERT INTO migration_admitted_metadata_mapping(id,import_id,plan_id,organization_id,kind,source_key,source_id,parent_mapping_id,target_field_id,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'held',$10,$11) ON CONFLICT(plan_id,organization_id,kind,source_key) DO NOTHING")
        .bind(id).bind(import).bind(plan).bind(org.0).bind(kind).bind(&source_key).bind(source_id).bind(parent).bind(target_field).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
    let existing: Uuid = sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4")
        .bind(plan).bind(org.0).bind(kind).bind(source_key).fetch_one(&mut *conn).await?;
    Ok(existing)
}

async fn build_preparation(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    import: Uuid,
    plan: Uuid,
    report: Uuid,
    snapshot: Uuid,
    account: i64,
    admission: Uuid,
) -> Result<Value, MigrationError> {
    let boundary: i64 = sqlx::query_scalar("SELECT capture_sequence FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2")
        .bind(import).bind(org.0).fetch_one(&mut *conn).await?;
    qualified_streams(conn, org, snapshot, boundary).await?;
    // The manifest is keyed by immutable admission-result identity, rather
    // than an arbitrary preview size.  The unique plan/source key makes a
    // crash/replay safe; production worker turns may re-enter this query after
    // any committed key without expanding the cohort.
    let results = sqlx::query("SELECT ar.id,ar.item_id,ar.person_id,ar.source_id FROM migration_people_admission_result ar JOIN migration_people_admission_item ai ON ai.id=ar.item_id AND ai.admission_id=ar.admission_id AND ai.organization_id=ar.organization_id JOIN migration_import_identity mi ON mi.organization_id=ar.organization_id AND mi.family='people' AND mi.source_id=ar.source_id AND mi.target_id=ar.person_id AND mi.admission_id=ar.admission_id AND mi.admission_item_id=ar.item_id AND mi.admission_result_id=ar.id JOIN person p ON p.id=ar.person_id AND p.organization_id=ar.organization_id WHERE ar.organization_id=$1 AND ar.admission_id=$2 AND ar.disposition='settled' AND ar.person_id IS NOT NULL ORDER BY ar.id")
        .bind(org.0).bind(admission).fetch_all(&mut *conn).await?;
    if results.is_empty() {
        return Err(MigrationError::SourceNotEligible);
    }
    let fields = sqlx::query("SELECT source_id FROM migration_core_change_group WHERE report_id=$1 AND organization_id=$2 AND family='custom_fields' ORDER BY source_id")
        .bind(report).bind(org.0).fetch_all(&mut *conn).await?;
    let mut field_count = 0_i64;
    for row in fields {
        let source_id: Option<String> = row.get("source_id");
        let Some(source_id) = source_id else {
            continue;
        };
        let Some(source) = insert_source(
            conn,
            key,
            org,
            import,
            plan,
            snapshot,
            report,
            "custom_fields",
            &source_id,
        )
        .await?
        else {
            continue;
        };
        let Entity::Field(field) = source.record.entity else {
            continue;
        };
        let frozen = FrozenMapping {
            source_id: source_id.clone(),
            label: field.label.clone(),
            machine_name: field.name.clone(),
            raw_choice: None,
            reasons: field.reasons.clone(),
        };
        let field_id = insert_mapping(
            conn,
            key,
            org,
            snapshot,
            account,
            import,
            plan,
            "field",
            &source_id,
            source_id.as_bytes(),
            None,
            None,
            frozen,
        )
        .await?;
        field_count += 1;
        for choice in field.choices {
            let Some(raw) = choice.raw else {
                continue;
            };
            let material = serde_json::to_vec(&(source_id.as_str(), raw.as_str()))
                .map_err(|_| MigrationError::Crypto)?;
            insert_mapping(
                conn,
                key,
                org,
                snapshot,
                account,
                import,
                plan,
                "option",
                &source_id,
                &material,
                Some(field_id),
                None,
                FrozenMapping {
                    source_id: source_id.clone(),
                    label: choice.label,
                    machine_name: field.name.clone(),
                    raw_choice: Some(raw),
                    reasons: choice.reasons,
                },
            )
            .await?;
        }
    }
    let mut manifests = 0_i64;
    let mut held = 0_i64;
    let mut tag_count = 0_i64;
    for result in results {
        let result_id: Uuid = result.get("id");
        let person: Uuid = result.get("person_id");
        let source_id: String = result.get("source_id");
        let source = insert_source(
            conn, key, org, import, plan, snapshot, report, "people", &source_id,
        )
        .await?;
        let manifest_id = Uuid::new_v4();
        let mut disposition = "held";
        let mut reasons = vec!["source_evidence_unavailable".to_owned()];
        if let Some(source) = source {
            if source.qualified && !source.conflict && source.record.reasons.is_empty() {
                disposition = "eligible";
                reasons.clear();
                if let Entity::Person(person_source) = source.record.entity {
                    for tag in person_source.tags {
                        let Some(raw) = tag.raw else {
                            continue;
                        };
                        let label = tag.label.clone();
                        let material = if let Some(label) = &label {
                            label.as_bytes().to_vec()
                        } else {
                            raw.as_bytes().to_vec()
                        };
                        insert_mapping(
                            conn,
                            key,
                            org,
                            snapshot,
                            account,
                            import,
                            plan,
                            "tag",
                            &source_id,
                            &material,
                            None,
                            None,
                            FrozenMapping {
                                source_id: source_id.clone(),
                                label,
                                machine_name: None,
                                raw_choice: Some(raw),
                                reasons: tag.reasons,
                            },
                        )
                        .await?;
                        tag_count += 1;
                    }
                } else {
                    disposition = "held";
                    reasons = vec!["source_representation_mismatch".into()];
                }
            } else {
                reasons = vec!["source_integrity".into()];
            }
        }
        let baseline = baseline(conn, key, org, snapshot, plan, manifest_id, person).await?;
        let bound = (baseline.nonce.len() + baseline.ciphertext.len() + 256 * 1024) as i64;
        if bound > 64 * 1024 * 1024 {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query("INSERT INTO migration_admitted_metadata_manifest(id,import_id,plan_id,organization_id,admission_result_id,person_id,source_person_id,disposition,baseline_nonce,baseline_ciphertext,item_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(manifest_id).bind(import).bind(plan).bind(org.0).bind(result_id).bind(person).bind(&source_id).bind(disposition).bind(baseline.nonce).bind(baseline.ciphertext).bind(bound).execute(&mut *conn).await?;
        if disposition == "held" {
            held += 1;
        }
        // The record stores why no value operation is executable until an
        // explicit catalog choice is supplied; it is never an implicit target.
        let op = FrozenOperation {
            source_id: source_id.clone(),
            source_field: None,
            source_tag: None,
            reasons,
        };
        let op_id = Uuid::new_v4();
        let sealed = seal(key, org, snapshot, plan, op_id, "operation", &op)?;
        let op_key = source_key(key, org, account, "person", source_id.as_bytes());
        sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,source_key,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'tag_link',$6,'held',$7,$8)")
            .bind(op_id).bind(manifest_id).bind(import).bind(plan).bind(org.0).bind(op_key).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
        manifests += 1;
    }
    let counts = json!({"people":{"source":manifests.to_string(),"eligible":(manifests-held).to_string(),"held":held.to_string()},"catalog":{"fields":field_count.to_string(),"tags":tag_count.to_string()},"issues":{"mapping_held":(field_count+tag_count).to_string()}});
    let inputs = seal(
        key,
        org,
        snapshot,
        plan,
        plan,
        "inputs",
        &json!({"report_id":report,"snapshot_id":snapshot,"admission_id":admission,"source_account_id":account.to_string(),"parser":"metadata_source_v1"}),
    )?;
    sqlx::query("UPDATE migration_admitted_metadata_plan SET inputs_nonce=$3,inputs_ciphertext=$4,counts=$5,phase='preparation' WHERE id=$1 AND import_id=$2")
        .bind(plan).bind(import).bind(inputs.nonce).bind(inputs.ciphertext).bind(&counts).execute(&mut *conn).await?;
    Ok(counts)
}
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    release: &ReleaseReadiness,
    ctx: &CommandContext,
    cmd: Prepare,
) -> Result<Value, MigrationError> {
    let mut tx = pool.begin().await?;
    handover(&mut tx, key, release, ctx).await?;
    let a=sqlx::query("SELECT a.parent_import_id,a.parent_plan_id,a.report_id AS admission_report_id,a.source_account_id,a.newer_snapshot_id,a.newer_sequence,a.newer_completed_at AS admission_completed_at,a.workspace_revision,a.state,r.parent_import_id AS report_parent_import_id,r.parent_plan_id AS report_parent_plan_id,r.source_account_id AS report_account,r.newer_snapshot_id AS report_snapshot,r.newer_sequence AS report_sequence,r.created_at AS report_started_at,r.state AS report_state,r.output_revision FROM migration_people_admission a JOIN migration_core_change_report r ON r.id=$2 AND r.organization_id=a.organization_id WHERE a.id=$1 AND a.organization_id=$3 FOR UPDATE")
 .bind(cmd.admission_id).bind(cmd.source_report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::SourceNotEligible)?;
    let same_boundary = a.get::<Uuid, _>("admission_report_id") == cmd.source_report_id;
    if !matches!(
        a.get::<String, _>("state").as_str(),
        "completed" | "cancelled"
    ) || a.get::<String, _>("report_state") != "completed"
        || a.get::<Option<Uuid>, _>("output_revision").is_none()
        || a.get::<Uuid, _>("report_parent_import_id") != a.get::<Uuid, _>("parent_import_id")
        || a.get::<Uuid, _>("report_parent_plan_id") != a.get::<Uuid, _>("parent_plan_id")
        || a.get::<i64, _>("report_account") != a.get::<i64, _>("source_account_id")
        || (!same_boundary
            && a.get::<chrono::DateTime<chrono::Utc>, _>("report_started_at")
                <= a.get::<chrono::DateTime<chrono::Utc>, _>("admission_completed_at"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let report = sqlx::query(
        "SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(cmd.source_report_id)
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *tx)
    .await?;
    core_change_store::validate(&mut tx, key, ctx.organization_id, &report).await?;
    let settled:i64=sqlx::query_scalar("SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND disposition='settled' AND person_id IS NOT NULL").bind(cmd.admission_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if settled == 0 {
        return Err(MigrationError::SourceNotEligible);
    }
    let id = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let inserted=sqlx::query("INSERT INTO migration_admitted_metadata_import(id,organization_id,parent_import_id,parent_plan_id,admission_id,source_report_id,snapshot_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,engine_version,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'proposed') ON CONFLICT DO NOTHING").bind(id).bind(ctx.organization_id.0).bind(a.get::<Uuid,_>("parent_import_id")).bind(a.get::<Uuid,_>("parent_plan_id")).bind(cmd.admission_id).bind(cmd.source_report_id).bind(a.get::<Uuid,_>("newer_snapshot_id")).bind(a.get::<i64,_>("source_account_id")).bind(a.get::<i64,_>("newer_sequence")).bind(a.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(ENGINE).execute(&mut *tx).await?;
    if inserted.rows_affected() == 1 {
        sqlx::query("INSERT INTO migration_admitted_metadata_plan(id,import_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext,counts) VALUES($1,$2,$3,1,'building',$4,$5,$6)").bind(plan).bind(id).bind(ctx.organization_id.0).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(json!({})).execute(&mut *tx).await?;
        build_preparation(
            &mut tx,
            key,
            ctx.organization_id,
            id,
            plan,
            cmd.source_report_id,
            a.get("report_snapshot"),
            a.get("source_account_id"),
            cmd.admission_id,
        )
        .await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET latest_plan_id=$3 WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
    }
    let resolved = if inserted.rows_affected() == 1 {
        id
    } else {
        sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_import WHERE organization_id=$1 AND admission_id=$2 AND predecessor_import_id IS NULL").bind(ctx.organization_id.0).bind(cmd.admission_id).fetch_one(&mut *tx).await?
    };
    tx.commit().await?;
    Ok(json!({"import":get(pool,ctx,resolved).await?,"request_id":cmd.request_id}))
}

fn request_digest<T: Serialize>(
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    import: Uuid,
    value: &T,
) -> Result<[u8; 32], MigrationError> {
    let bytes = serde_json::to_vec(&(
        ctx.organization_id.0,
        ctx.actor_user_id.0,
        action,
        import,
        value,
    ))
    .map_err(|_| MigrationError::Crypto)?;
    Ok(crypto::request_digest(
        key,
        "admitted-metadata-request-v1",
        &bytes,
    ))
}

async fn replay_receipt(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request_id: Uuid,
    digest: &[u8; 32],
    import: Uuid,
) -> Result<Option<Value>, MigrationError> {
    let row = sqlx::query("SELECT digest,nonce,ciphertext FROM migration_admitted_metadata_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request_id).fetch_optional(&mut *conn).await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<Vec<u8>, _>("digest") != digest {
        return Err(MigrationError::Conflict);
    }
    let snapshot: Uuid = sqlx::query_scalar("SELECT snapshot_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2")
        .bind(import).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    let raw = crypto::open_snapshot(
        key,
        ctx.organization_id,
        snapshot,
        request_id,
        &format!("admitted-metadata-v1:{import}:receipt"),
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    serde_json::from_slice(&raw)
        .map(Some)
        .map_err(|_| MigrationError::Crypto)
}

async fn save_receipt<T: Serialize>(
    conn: &mut sqlx::PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request_id: Uuid,
    digest: &[u8; 32],
    import: Uuid,
    value: &T,
) -> Result<(), MigrationError> {
    let snapshot: Uuid = sqlx::query_scalar("SELECT snapshot_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2")
        .bind(import).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
    let raw = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    let sealed = crypto::seal_snapshot(
        key,
        ctx.organization_id,
        snapshot,
        request_id,
        &format!("admitted-metadata-v1:{import}:receipt"),
        &raw,
    )
    .map_err(|_| MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_admitted_metadata_receipt(organization_id,actor_user_id,action,request_id,import_id,digest,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request_id).bind(import).bind(digest.as_slice()).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
    Ok(())
}

async fn lifecycle_tx<'a>(
    pool: &'a PgPool,
    ctx: &CommandContext,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, MigrationError> {
    let mut tx = pool.begin().await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    super::store::require_admin(&mut tx, ctx).await?;
    super::store::lock_org(&mut tx, ctx.organization_id).await?;
    Ok(tx)
}

/// Freeze approved mapping choices and make the plan executable.  The only
/// mutation allowed before confirmation is this explicit plan transition;
/// confirmation freezes its revision and all later mapping changes require a
/// fresh preview.
pub async fn apply_mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: MappingPatch,
) -> Result<Value, MigrationError> {
    if cmd.mappings.is_empty() || cmd.mappings.len() > 50 {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let digest = request_digest(key, ctx, "mappings", import, &cmd)?;
    if let Some(v) = replay_receipt(
        &mut tx,
        key,
        ctx,
        "mappings",
        cmd.request_id,
        &digest,
        import,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(v);
    }
    let root = sqlx::query("SELECT latest_plan_id,state FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE")
        .bind(import).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if root.get::<String, _>("state") != "proposed" {
        return Err(MigrationError::Conflict);
    }
    let plan: Uuid = root
        .get::<Option<Uuid>, _>("latest_plan_id")
        .ok_or(MigrationError::Conflict)?;
    let state: String = sqlx::query_scalar("SELECT state FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE")
        .bind(plan).bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if state != "building" {
        return Err(MigrationError::Conflict);
    }
    for choice in &cmd.mappings {
        if !matches!(
            choice.disposition.as_str(),
            "hold" | "create_matching" | "map_existing"
        ) || (choice.disposition == "map_existing" && choice.target_id.is_none())
            || (choice.disposition != "map_existing" && choice.target_id.is_some())
        {
            return Err(MigrationError::InvalidInput);
        }
        let stored = if choice.disposition == "hold" {
            "held"
        } else {
            choice.disposition.as_str()
        };
        let updated = sqlx::query("UPDATE migration_admitted_metadata_mapping SET disposition=$5,target_id=$6 WHERE id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4 AND disposition='held'")
            .bind(choice.id).bind(import).bind(plan).bind(ctx.organization_id.0).bind(stored).bind(choice.target_id).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(MigrationError::Conflict);
        }
    }
    sqlx::query("UPDATE migration_admitted_metadata_plan SET state='ready',phase='catalog',expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND import_id=$2 AND organization_id=$3")
        .bind(plan).bind(import).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let value = json!({"import_id":import,"plan_id":plan,"state":"ready"});
    save_receipt(
        &mut tx,
        key,
        ctx,
        "mappings",
        cmd.request_id,
        &digest,
        import,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}

pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: Confirm,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let digest = request_digest(key, ctx, "confirm", import, &cmd)?;
    if let Some(v) = replay_receipt(
        &mut tx,
        key,
        ctx,
        "confirm",
        cmd.request_id,
        &digest,
        import,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(v);
    }
    let root=sqlx::query("SELECT state,latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(import).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if root.get::<String, _>("state") != "proposed"
        || root.get::<Option<Uuid>, _>("latest_plan_id") != Some(cmd.plan_id)
    {
        return Err(MigrationError::Conflict);
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 AND revision=$4 AND state='ready' AND expires_at>clock_timestamp())").bind(cmd.plan_id).bind(import).bind(ctx.organization_id.0).bind(cmd.plan_revision).fetch_one(&mut *tx).await?;
    if !valid {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='queued',phase='catalog',confirmed_plan_id=$3,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).bind(cmd.plan_id).execute(&mut *tx).await?;
    let value = json!({"import_id":import,"state":"queued"});
    save_receipt(
        &mut tx,
        key,
        ctx,
        "confirm",
        cmd.request_id,
        &digest,
        import,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}

pub async fn retry(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: Request,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, import, cmd, "retry").await
}
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: Request,
) -> Result<Value, MigrationError> {
    lifecycle(pool, key, ctx, import, cmd, "cancel").await
}
async fn lifecycle(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: Request,
    action: &str,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let digest = request_digest(key, ctx, action, import, &cmd)?;
    if let Some(v) =
        replay_receipt(&mut tx, key, ctx, action, cmd.request_id, &digest, import).await?
    {
        tx.commit().await?;
        return Ok(v);
    }
    let root=sqlx::query("SELECT state,confirmed_plan_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(import).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let state: String = root.get("state");
    let next = match action {
        "cancel" if !matches!(state.as_str(), "completed" | "cancelled") => "cancelled",
        "retry"
            if state == "paused" && root.get::<Option<Uuid>, _>("confirmed_plan_id").is_some() =>
        {
            "queued"
        }
        _ => return Err(MigrationError::Conflict),
    };
    if next == "cancelled" {
        sqlx::query("UPDATE migration_admitted_metadata_manifest SET disposition='cancelled' WHERE import_id=$1 AND organization_id=$2 AND disposition='eligible'").bind(import).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_admitted_metadata_import SET state=$3,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).bind(next).execute(&mut *tx).await?;
    let value = json!({"import_id":import,"state":next});
    save_receipt(
        &mut tx,
        key,
        ctx,
        action,
        cmd.request_id,
        &digest,
        import,
        &value,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}
