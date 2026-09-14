//! D-082 admitted-People metadata root.  This deliberately has no path through
//! the original metadata child: its cohort is terminal admission results.
use super::{
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

mod preparation;
pub(crate) use preparation::run_once as prepare_once;

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
    pub expected_plan_revision: String,
    #[serde(default)]
    pub mappings: Vec<super::metadata::MappingPatch>,
    pub source_report_id: Option<Uuid>,
}
#[derive(Default, Serialize, Deserialize)]
struct PlanChoices {
    #[serde(default)]
    patches: Vec<super::metadata::Patch>,
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

#[derive(Serialize, Deserialize)]
struct FrozenSource {
    report_id: Uuid,
    capture_id: Uuid,
    capture_sequence: i64,
    ordinal: i32,
    qualified: bool,
    conflict: bool,
    record: Record,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FrozenMapping {
    pub(crate) source_id: String,
    pub(crate) label: Option<String>,
    pub(crate) machine_name: Option<String>,
    pub(crate) field_type: Option<String>,
    pub(crate) raw_choice: Option<String>,
    pub(crate) definition: Option<metadata_source::FieldInput>,
    #[serde(default)]
    pub(crate) target_baseline: Value,
    #[serde(default)]
    pub(crate) claim_baseline: Value,
    pub(crate) reasons: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct FrozenOperation {
    pub(crate) source_id: String,
    pub(crate) source_field: Option<String>,
    pub(crate) source_tag: Option<String>,
    pub(crate) value: Option<metadata_source::NativeValue>,
    pub(crate) reasons: Vec<String>,
}

pub(crate) fn seal<T: Serialize>(
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot: Uuid,
    plan: Uuid,
    row: Uuid,
    purpose: &str,
    value: &T,
) -> Result<crypto::Sealed, MigrationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 64 * 1024 * 1024 {
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
    conn: &mut sqlx::PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2")
 .bind(id).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::NotFound)
}
pub async fn list(pool: &PgPool, ctx: &CommandContext, q: Page) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let limit = q.limit()?;
    let after = q.cursor.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT i.*,COALESCE(r.state='ready',false) AS shared_claims_ready,(SELECT count(*) FROM migration_people_admission_result x WHERE x.admission_id=i.admission_id AND x.organization_id=i.organization_id AND x.disposition='settled' AND x.person_id IS NOT NULL) AS settled_people,(i.state='cancelled' AND i.successor_import_id IS NULL) AS remainder_available FROM migration_admitted_metadata_import i LEFT JOIN migration_metadata_catalog_readiness r ON r.organization_id=i.organization_id WHERE i.organization_id=$1 AND i.id>$2 AND ($3::uuid IS NULL OR i.admission_id=$3) ORDER BY i.id LIMIT $4")
 .bind(ctx.organization_id.0).bind(after).bind(q.admission_id).bind(limit+1).fetch_all(&mut *tx).await?;
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
    let mut tx = lifecycle_tx(pool, ctx).await?;
    Ok(detail(&find(&mut tx, ctx.organization_id, id).await?))
}
async fn checked_plan(
    conn: &mut sqlx::PgConnection,
    org: OrganizationId,
    import: Uuid,
    plan: Uuid,
) -> Result<(), MigrationError> {
    let found: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3)")
        .bind(plan).bind(import).bind(org.0).fetch_one(&mut *conn).await?;
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
    let mut tx = lifecycle_tx(pool, ctx).await?;
    checked_plan(&mut tx, ctx.organization_id, import, plan).await?;
    let limit = page.limit()?;
    let after = page.cursor.unwrap_or(Uuid::nil());
    let rows = sqlx::query("SELECT id,kind,source_id,target_id,target_field_id,disposition FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND id>$4 ORDER BY id LIMIT $5")
        .bind(import).bind(plan).bind(ctx.organization_id.0).bind(after).bind(limit + 1).fetch_all(&mut *tx).await?;
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
    let mut tx = lifecycle_tx(pool, ctx).await?;
    checked_plan(&mut tx, ctx.organization_id, import, plan).await?;
    let limit = page.limit()?;
    let after = page.cursor.unwrap_or(Uuid::nil());
    let rows = sqlx::query("SELECT id,admission_result_id,person_id,source_person_id,disposition,item_byte_bound FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND id>$4 ORDER BY id LIMIT $5")
        .bind(import).bind(plan).bind(ctx.organization_id.0).bind(after).bind(limit + 1).fetch_all(&mut *tx).await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<Value> = rows.into_iter().take(limit as usize).map(|r|json!({"id":r.get::<Uuid,_>("id"),"admission_result_id":r.get::<Uuid,_>("admission_result_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"source_id":r.get::<String,_>("source_person_id"),"disposition":r.get::<String,_>("disposition"),"item_byte_bound":r.get::<i64,_>("item_byte_bound").to_string()})).collect();
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
    let mut tx = lifecycle_tx(pool, ctx).await?;
    checked_plan(&mut tx, ctx.organization_id, import, plan).await?;
    let rows=sqlx::query("SELECT code,count FROM migration_admitted_metadata_issue WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 ORDER BY code LIMIT 50").bind(import).bind(plan).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
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
    super::store::require_admin(conn, ctx).await?;
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
        let bytes = 32 + (sealed.nonce.len() + sealed.ciphertext.len()) as i64;
        let inserted = sqlx::query("INSERT INTO migration_metadata_catalog_claim(organization_id,source_account_id,kind,source_key,target_id,original_import_id,original_plan_id,original_mapping_id,evidence_nonce,evidence_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(organization_id,source_account_id,kind,source_key) DO NOTHING")
            .bind(ctx.organization_id.0).bind(row.get::<i64,_>("source_account_id")).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(row.get::<Uuid,_>("target_id")).bind(row.get::<Uuid,_>("import_id")).bind(row.get::<Uuid,_>("plan_id")).bind(mapping).bind(sealed.nonce).bind(sealed.ciphertext).bind(snapshot).execute(&mut *conn).await?;
        if inserted.rows_affected() == 0 {
            let equal: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4 AND target_id=$5 AND original_mapping_id=$6)")
                .bind(ctx.organization_id.0).bind(row.get::<i64,_>("source_account_id")).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(row.get::<Uuid,_>("target_id")).bind(mapping).fetch_one(&mut *conn).await?;
            if !equal {
                return Err(MigrationError::InvalidImportChoice);
            }
            continue;
        }
        let allowance=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(snapshot).bind(ctx.organization_id.0).fetch_one(&mut *conn).await?;
        let policy = super::snapshot::SnapshotPolicy::default();
        if bytes
            > allowance
                .get::<i64, _>("run_byte_limit")
                .min(policy.run_ceiling_bytes)
                .saturating_sub(allowance.get("retained_bytes"))
                .saturating_sub(allowance.get("reserved_bytes"))
            || bytes
                > allowance
                    .get::<i64, _>("byte_limit")
                    .min(policy.org_ceiling_bytes)
                    .saturating_sub(allowance.get("org_retained"))
                    .saturating_sub(allowance.get("org_reserved"))
        {
            return Err(MigrationError::StorageLimit);
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

#[allow(clippy::too_many_arguments)] // Tenant, evidence and immutable owner scopes stay explicit.
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
    let Some(r)=sqlx::query("SELECT * FROM migration_admitted_metadata_source WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND family=$4 AND source_id=$5").bind(import).bind(plan).bind(org.0).bind(family).bind(source_id).fetch_optional(conn).await? else {return Ok(None)};
    let mut frozen: FrozenSource = super::admitted_metadata_worker::open(
        key,
        org,
        snapshot,
        plan,
        r.get("id"),
        "source",
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )?;
    if frozen.report_id != report {
        return Err(MigrationError::Crypto);
    }
    frozen.qualified = r.get("qualified");
    frozen.conflict = r.get("conflict");
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

#[allow(clippy::too_many_arguments)] // Tenant, evidence and immutable owner scopes stay explicit.
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
    let source_key = if kind == "tag" {
        if let Some(label) = frozen.label.as_deref() {
            let group: String = sqlx::query_scalar("SELECT lower($1::text)")
                .bind(label)
                .fetch_one(&mut *conn)
                .await?;
            source_key(key, org, account, "tag-group", group.as_bytes())
        } else {
            source_key(key, org, account, "tag-raw", raw)
        }
    } else {
        source_key(key, org, account, kind, raw)
    };
    let sealed = seal(key, org, snapshot, plan, id, "mapping", &frozen)?;
    sqlx::query("INSERT INTO migration_admitted_metadata_mapping(id,import_id,plan_id,organization_id,kind,source_key,source_id,parent_mapping_id,target_field_id,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'held',$10,$11) ON CONFLICT(plan_id,organization_id,kind,source_key) DO NOTHING")
        .bind(id).bind(import).bind(plan).bind(org.0).bind(kind).bind(&source_key).bind(source_id).bind(parent).bind(target_field).bind(sealed.nonce).bind(sealed.ciphertext).bind(snapshot).execute(&mut *conn).await?;
    let existing: Uuid = sqlx::query_scalar("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4")
        .bind(plan).bind(org.0).bind(kind).bind(source_key).fetch_one(&mut *conn).await?;
    Ok(existing)
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
    tx.commit().await?;
    let mut tx = lifecycle_tx(pool, ctx).await?;
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
    let inserted=sqlx::query("INSERT INTO migration_admitted_metadata_import(id,organization_id,parent_import_id,parent_plan_id,admission_id,source_report_id,snapshot_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,engine_version,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'proposed') ON CONFLICT DO NOTHING").bind(id).bind(ctx.organization_id.0).bind(a.get::<Uuid,_>("parent_import_id")).bind(a.get::<Uuid,_>("parent_plan_id")).bind(cmd.admission_id).bind(cmd.source_report_id).bind(a.get::<Uuid,_>("report_snapshot")).bind(a.get::<i64,_>("source_account_id")).bind(a.get::<i64,_>("report_sequence")).bind(a.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(ENGINE).execute(&mut *tx).await?;
    if inserted.rows_affected() == 1 {
        sqlx::query("INSERT INTO migration_admitted_metadata_plan(id,import_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext,counts,snapshot_id,source_report_id,source_output_revision,capture_sequence) VALUES($1,$2,$3,1,'building',$4,$5,$6,$7,$8,$9,$10)").bind(plan).bind(id).bind(ctx.organization_id.0).bind(vec![0u8;24]).bind(vec![0u8;16]).bind(json!({})).bind(a.get::<Uuid,_>("report_snapshot")).bind(cmd.source_report_id).bind(a.get::<Uuid,_>("output_revision")).bind(a.get::<i64,_>("report_sequence")).execute(&mut *tx).await?;
        let inputs = seal(
            key,
            ctx.organization_id,
            a.get("report_snapshot"),
            plan,
            plan,
            "inputs",
            &json!({"report_id":cmd.source_report_id,"snapshot_id":a.get::<Uuid,_>("report_snapshot"),"admission_id":cmd.admission_id,"source_account_id":a.get::<i64,_>("source_account_id").to_string(),"parser":"metadata_source_v1","output_revision":a.get::<Option<Uuid>,_>("output_revision")}),
        )?;
        qualified_streams(
            &mut tx,
            ctx.organization_id,
            a.get("report_snapshot"),
            a.get("report_sequence"),
        )
        .await?;
        sqlx::query("UPDATE migration_admitted_metadata_plan SET inputs_nonce=$2,inputs_ciphertext=$3 WHERE id=$1").bind(plan).bind(inputs.nonce).bind(inputs.ciphertext).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET latest_plan_id=$3 WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
    }
    let resolved = if inserted.rows_affected() == 1 {
        super::admitted_metadata_worker::charge(&mut tx, ctx.organization_id, id).await?;
        super::admitted_metadata_worker::reserve(
            &mut tx,
            ctx.organization_id,
            id,
            plan,
            a.get("report_snapshot"),
            "cancel",
            metadata_store::CANCEL_RESERVATION,
        )
        .await?;
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
    let row = sqlx::query("SELECT digest,nonce,ciphertext,snapshot_id FROM migration_admitted_metadata_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action=$3 AND request_id=$4")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request_id).fetch_optional(&mut *conn).await?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<Vec<u8>, _>("digest") != digest {
        return Err(MigrationError::Conflict);
    }
    let snapshot: Uuid = row.get("snapshot_id");
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

#[allow(clippy::too_many_arguments)] // Tenant, evidence and immutable owner scopes stay explicit.
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
    sqlx::query("INSERT INTO migration_admitted_metadata_receipt(organization_id,actor_user_id,action,request_id,import_id,digest,nonce,ciphertext,snapshot_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(action).bind(request_id).bind(import).bind(digest.as_slice()).bind(sealed.nonce).bind(sealed.ciphertext).bind(snapshot).execute(&mut *conn).await?;
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

/// A mapping patch starts a new immutable preview. The worker inherits choices
/// by exact source key and freezes each destination in bounded transactions.
pub async fn apply_mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    import: Uuid,
    cmd: MappingPatch,
) -> Result<Value, MigrationError> {
    if cmd.mappings.len() > 50 {
        return Err(MigrationError::InvalidInput);
    }
    let revision = cmd
        .expected_plan_revision
        .parse::<i64>()
        .map_err(|_| MigrationError::InvalidInput)?;
    if revision < 1 || revision.to_string() != cmd.expected_plan_revision {
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
        return Ok(v);
    }
    let root=sqlx::query("SELECT * FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(import).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if root.get::<String, _>("state") != "proposed"
        || root.get::<Option<Uuid>, _>("confirmed_plan_id").is_some()
    {
        return Err(MigrationError::Conflict);
    }
    let previous: Uuid = root
        .get::<Option<Uuid>, _>("latest_plan_id")
        .ok_or(MigrationError::Conflict)?;
    let old=sqlx::query("SELECT * FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE").bind(previous).bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if old.get::<i64, _>("revision") != revision || old.get::<String, _>("state") != "ready" {
        return Err(MigrationError::Conflict);
    }
    let report_id = cmd.source_report_id.unwrap_or(root.get("source_report_id"));
    let changed = report_id != root.get::<Uuid, _>("source_report_id");
    if changed && !cmd.mappings.is_empty() {
        return Err(MigrationError::InvalidInput);
    }
    let report = sqlx::query(
        "SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2 FOR UPDATE",
    )
    .bind(report_id)
    .bind(ctx.organization_id.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(MigrationError::NotFound)?;
    core_change_store::validate(&mut tx, key, ctx.organization_id, &report).await?;
    let admission=sqlx::query("SELECT report_id,newer_completed_at FROM migration_people_admission WHERE id=$1 AND organization_id=$2").bind(root.get::<Uuid,_>("admission_id")).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if report.get::<String, _>("state") != "completed"
        || report.get::<Option<Uuid>, _>("output_revision").is_none()
        || report.get::<Uuid, _>("parent_import_id") != root.get::<Uuid, _>("parent_import_id")
        || report.get::<Uuid, _>("parent_plan_id") != root.get::<Uuid, _>("parent_plan_id")
        || report.get::<i64, _>("source_account_id") != root.get::<i64, _>("source_account_id")
        || report_id != admission.get::<Uuid, _>("report_id")
            && report.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                <= admission.get::<chrono::DateTime<chrono::Utc>, _>("newer_completed_at")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let snapshot: Uuid = report.get("newer_snapshot_id");
    qualified_streams(
        &mut tx,
        ctx.organization_id,
        snapshot,
        report.get("newer_sequence"),
    )
    .await?;
    let mut patches = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for patch in &cmd.mappings {
        if !seen.insert(patch.mapping_id) {
            return Err(MigrationError::InvalidInput);
        }
        let m=sqlx::query("SELECT kind,source_key FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4").bind(patch.mapping_id).bind(previous).bind(import).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
        patches.push(super::metadata::Patch {
            kind: m.get("kind"),
            source_key: m.get("source_key"),
            choice: patch.choice.clone(),
        });
    }
    if changed {
        let cancel:Uuid=sqlx::query_scalar("SELECT token FROM migration_admitted_metadata_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='cancel'").bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        super::admitted_metadata_worker::settle(&mut tx, ctx.organization_id, import, cancel, 0)
            .await?;
        super::admitted_metadata_worker::reserve(
            &mut tx,
            ctx.organization_id,
            import,
            previous,
            snapshot,
            "cancel",
            metadata_store::CANCEL_RESERVATION,
        )
        .await?;
    }
    let plan = Uuid::new_v4();
    let inputs = seal(
        key,
        ctx.organization_id,
        snapshot,
        plan,
        plan,
        "inputs",
        &PlanChoices { patches },
    )?;
    sqlx::query("INSERT INTO migration_admitted_metadata_plan(id,import_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext,counts,snapshot_id,source_report_id,source_output_revision,capture_sequence,previous_plan_id) VALUES($1,$2,$3,$4,'building',$5,$6,'{}',$7,$8,$9,$10,$11)").bind(plan).bind(import).bind(ctx.organization_id.0).bind(revision.checked_add(1).ok_or(MigrationError::Conflict)?).bind(inputs.nonce).bind(inputs.ciphertext).bind(snapshot).bind(report_id).bind(report.get::<Uuid,_>("output_revision")).bind(report.get::<i64,_>("newer_sequence")).bind(if changed{None}else{Some(previous)}).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_metadata_plan SET state='superseded' WHERE id=$1 AND organization_id=$2").bind(previous).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_admitted_metadata_import SET latest_plan_id=$3,snapshot_id=$4,source_report_id=$5,capture_sequence=$6,executor_user_id=$7,phase='preparation',pause_reason=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).bind(plan).bind(snapshot).bind(report_id).bind(report.get::<i64,_>("newer_sequence")).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let value = json!({"import_id":import,"plan_id":plan,"state":"building"});
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
    super::admitted_metadata_worker::charge(&mut tx, ctx.organization_id, import).await?;
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
    let selected=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND disposition IN ('create_matching','map_existing') ORDER BY kind,id").bind(import).bind(cmd.plan_id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    let binding=sqlx::query("SELECT snapshot_id,source_account_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    for m in selected {
        let f: FrozenMapping = super::admitted_metadata_worker::open(
            key,
            ctx.organization_id,
            binding.get("snapshot_id"),
            cmd.plan_id,
            m.get("id"),
            "mapping",
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let kind: String = m.get("kind");
        let target: Uuid = m
            .get::<Option<Uuid>, _>("target_id")
            .ok_or(MigrationError::InvalidImportChoice)?;
        let source_key: Vec<u8> = m.get("source_key");
        let now = super::admitted_metadata_worker::target_state(
            &mut tx,
            ctx.organization_id,
            &kind,
            target,
        )
        .await?;
        let claim = super::admitted_metadata_worker::claim_state(
            &mut tx,
            ctx.organization_id,
            binding.get("source_account_id"),
            &kind,
            &source_key,
        )
        .await?;
        if now != f.target_baseline
            || (claim != f.claim_baseline
                && !super::admitted_metadata_worker::claim_equal(
                    &mut tx,
                    key,
                    ctx.organization_id,
                    binding.get("source_account_id"),
                    &kind,
                    &source_key,
                    &f,
                    target,
                )
                .await?)
        {
            return Err(MigrationError::InvalidImportChoice);
        }
    }
    let executable:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND disposition IN ('create_matching','map_existing'))").bind(import).bind(cmd.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if !executable {
        return Err(MigrationError::InvalidImportChoice);
    }
    let snapshot:Uuid=sqlx::query_scalar("SELECT snapshot_id FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    let bound:i64=sqlx::query_scalar("SELECT GREATEST(4096,COALESCE((SELECT max(item_byte_bound) FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3),0),COALESCE((SELECT max(octet_length(m.nonce)+octet_length(m.ciphertext)+4096+COALESCE((SELECT sum(octet_length(o.nonce)+octet_length(o.ciphertext)+4096) FROM migration_admitted_metadata_mapping o WHERE o.parent_mapping_id=m.id AND o.plan_id=m.plan_id AND o.organization_id=m.organization_id),0)) FROM migration_admitted_metadata_mapping m WHERE m.import_id=$1 AND m.plan_id=$2 AND m.organization_id=$3),0))::bigint").bind(import).bind(cmd.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    if bound > 64 * 1024 * 1024 {
        return Err(MigrationError::StorageLimit);
    }
    sqlx::query("UPDATE migration_admitted_metadata_plan SET max_added_byte_bound=$3 WHERE id=$1 AND organization_id=$2").bind(cmd.plan_id).bind(ctx.organization_id.0).bind(bound).execute(&mut *tx).await?;
    super::admitted_metadata_worker::reserve(
        &mut tx,
        ctx.organization_id,
        import,
        cmd.plan_id,
        snapshot,
        "work",
        bound,
    )
    .await?;
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='queued',phase='catalog',confirmed_plan_id=$3,executor_user_id=$4,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).bind(cmd.plan_id).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
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
    super::admitted_metadata_worker::charge(&mut tx, ctx.organization_id, import).await?;
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
        "retry" if state == "paused" => {
            if root.get::<Option<Uuid>, _>("confirmed_plan_id").is_some() {
                "queued"
            } else {
                "proposed"
            }
        }
        _ => return Err(MigrationError::Conflict),
    };
    if next == "cancelled" {
        sqlx::query("UPDATE migration_admitted_metadata_manifest SET disposition='cancelled' WHERE import_id=$1 AND organization_id=$2 AND plan_id=(SELECT COALESCE(confirmed_plan_id,latest_plan_id) FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2) AND disposition='eligible'").bind(import).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_admitted_metadata_import SET state=$3,executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(import).bind(ctx.organization_id.0).bind(next).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
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
    if next == "cancelled" {
        let token:Uuid=sqlx::query_scalar("SELECT token FROM migration_admitted_metadata_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='cancel'").bind(import).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let actual:i64=sqlx::query_scalar("SELECT (octet_length(digest)+octet_length(nonce)+octet_length(ciphertext))::bigint FROM migration_admitted_metadata_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND action='cancel' AND request_id=$3").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_one(&mut *tx).await?;
        super::admitted_metadata_worker::settle(
            &mut tx,
            ctx.organization_id,
            import,
            token,
            actual,
        )
        .await?;
        super::admitted_metadata_worker::release(&mut tx, ctx.organization_id, import).await?;
    } else {
        super::admitted_metadata_worker::charge(&mut tx, ctx.organization_id, import).await?;
    }
    tx.commit().await?;
    Ok(value)
}
