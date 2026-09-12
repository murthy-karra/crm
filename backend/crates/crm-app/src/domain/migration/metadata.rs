//! D-066 source-free child metadata commands and scoped retained queries.
use super::{
    metadata_model::{Counts, Destination, Mapping, Target},
    metadata_store as s,
    snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{
    auth::workspace, config::RawPayloadKey, domain::envelope::CommandContext, ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
pub const ENGINE: &str = "fub-metadata-import-v1";

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Choice {
    Hold,
    CreateMatching,
    MapExisting { target_id: Uuid },
}
impl<'de> Deserialize<'de> for Choice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Hold(Empty),
            CreateMatching(Empty),
            MapExisting { target_id: Uuid },
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Hold(_) => Self::Hold,
            Wire::CreateMatching(_) => Self::CreateMatching,
            Wire::MapExisting { target_id } => Self::MapExisting { target_id },
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMetadataImport {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPatch {
    pub mapping_id: Uuid,
    pub choice: Choice,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplanMetadataImport {
    pub request_id: Uuid,
    pub expected_plan_revision: String,
    #[serde(default)]
    pub mappings: Vec<MappingPatch>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgments {
    pub held_count: String,
    pub review_only: bool,
    pub remaining_data: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmMetadataImport {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub confirmation_digest: String,
    pub workspace_revision: String,
    pub acknowledgments: Acknowledgments,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub request_id: Uuid,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPage {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub kind: Option<String>,
    pub disposition: Option<String>,
    pub field_id: Option<Uuid>,
    pub parent_import_id: Option<Uuid>,
}
impl MetadataPage {
    pub(crate) fn limit(&self) -> Result<i64, MigrationError> {
        let n = self.limit.unwrap_or(50);
        if !(1..=50).contains(&n) {
            return Err(MigrationError::InvalidInput);
        }
        Ok(i64::from(n))
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Patch {
    pub kind: String,
    pub source_key: Vec<u8>,
    pub choice: Choice,
}

pub(crate) async fn eligible(
    conn: &mut PgConnection,
    org: OrganizationId,
    parent: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    if !sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM migration_import WHERE id=$1 AND organization_id=$2)",
    )
    .bind(parent)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?
    {
        return Err(MigrationError::NotFound);
    }
    sqlx::query("SELECT p.*,o.workspace_revision AS current_workspace_revision FROM migration_import p JOIN migration_workspace w ON w.organization_id=p.organization_id AND w.import_id=p.id AND w.plan_id=p.confirmed_plan_id JOIN organization o ON o.id=p.organization_id JOIN migration_snapshot s ON s.id=p.snapshot_id AND s.organization_id=p.organization_id JOIN migration_snapshot_preview v ON v.id=p.preview_id AND v.snapshot_id=s.id AND v.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 AND p.state='completed' AND o.workspace_mode='migration_review' AND s.profile_version='fub-core-v1' AND s.state IN ('completed','completed_with_gaps') AND s.capture_sequence=p.capture_sequence AND s.source_account_id=p.source_account_id AND v.state='completed' AND v.capture_sequence=p.capture_sequence AND (SELECT count(*) FROM migration_snapshot_stream t WHERE t.snapshot_id=s.id AND t.organization_id=s.organization_id AND t.stream IN ('people','users','stages','custom_fields') AND t.state='completed')=4").bind(parent).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)
}
pub(crate) async fn validate_binding(
    conn: &mut PgConnection,
    r: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let p = eligible(
        conn,
        OrganizationId::new(r.get("organization_id")),
        r.get("parent_import_id"),
    )
    .await?;
    if p.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(r.get("parent_plan_id"))
        || p.get::<Uuid, _>("snapshot_id") != r.get::<Uuid, _>("snapshot_id")
        || p.get::<i64, _>("current_workspace_revision") != r.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}
pub(crate) async fn destination(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Destination, MigrationError> {
    let mut out = Destination::default();
    for r in sqlx::query("SELECT id,name FROM tag WHERE organization_id=$1 ORDER BY id LIMIT 201")
        .bind(org.0)
        .fetch_all(&mut *conn)
        .await?
    {
        out.tags.push(Target {
            id: r.get("id"),
            kind: "tag".into(),
            field_id: None,
            label: r.get("name"),
            field_type: None,
            source: None,
            external_key: None,
            position: 0,
        });
    }
    for r in sqlx::query("SELECT id,label,field_type,source,external_key,position FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL ORDER BY position,id LIMIT 51").bind(org.0).fetch_all(&mut *conn).await?{out.fields.push(Target{id:r.get("id"),kind:"field".into(),field_id:None,label:r.get("label"),field_type:Some(r.get("field_type")),source:r.get("source"),external_key:r.get("external_key"),position:r.get("position")});}
    for r in sqlx::query("SELECT o.id,o.field_id,o.label,o.position FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.organization_id=$1 AND o.archived_at IS NULL AND f.archived_at IS NULL ORDER BY o.field_id,o.position,o.id LIMIT 2551").bind(org.0).fetch_all(&mut *conn).await?{out.options.push(Target{id:r.get("id"),kind:"option".into(),field_id:Some(r.get("field_id")),label:r.get("label"),field_type:None,source:None,external_key:None,position:r.get("position")});}
    if out.tags.len() > 200
        || out.fields.len() > 50
        || out.fields.iter().any(|f| {
            out.options
                .iter()
                .filter(|o| o.field_id == Some(f.id))
                .count()
                > 50
        })
        || s::bytes(&out)?.len() > 8 * 1024 * 1024
    {
        return Err(MigrationError::InvalidImportChoice);
    }
    Ok(out)
}
pub(crate) fn decode_destination(
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
) -> Result<Destination, MigrationError> {
    s::open(
        key,
        OrganizationId::new(r.get("organization_id")),
        r.get("snapshot_id"),
        p.get("id"),
        p.get("id"),
        "destination",
        &p.get::<Vec<u8>, _>("destination_nonce"),
        &p.get::<Vec<u8>, _>("destination_ciphertext"),
    )
}
pub(crate) async fn view(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    r: &sqlx::postgres::PgRow,
) -> Result<Value, MigrationError> {
    let id: Uuid = r.get("id");
    let latest: Uuid = r.get("latest_plan_id");
    let p = s::plan(conn, org, id, latest).await?;
    let plan_counts = Counts::load(p.get("counts"))?;
    let mut counts = if r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some() {
        Counts::load(r.get("counts"))?.wire()
    } else {
        plan_counts.wire()
    };
    let issues=sqlx::query("SELECT code,record_count FROM migration_metadata_issue WHERE plan_id=$1 AND organization_id=$2 ORDER BY code LIMIT 100").bind(latest).bind(org.0).fetch_all(&mut *conn).await?.iter().map(|r|json!({"code":r.get::<String,_>("code"),"count":r.get::<i64,_>("record_count").to_string()})).collect::<Vec<_>>();
    counts["issues"] = json!(issues);
    let mut plan_wire = plan_counts.wire();
    plan_wire["issues"] = counts["issues"].clone();
    let policy = s::current_policy()?;
    let storage=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *conn).await?;
    let cancel:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_metadata_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='cancel'").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let state: String = r.get("state");
    let confirmed: Option<Uuid> = r.get("confirmed_plan_id");
    let plan_state: String = p.get("state");
    let eligible_operations = plan_counts.tags.eligible
        + plan_counts.fields.eligible
        + plan_counts.options.eligible
        + plan_counts.tag_links.eligible
        + plan_counts.values.eligible;
    let expires: Option<DateTime<Utc>> = p.get("expires_at");
    let source_gaps=sqlx::query("SELECT stream,state FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 AND state<>'completed' ORDER BY stream").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_all(&mut *conn).await?.iter().map(|x|format!("{}:{}",x.get::<String,_>("stream"),x.get::<String,_>("state"))).collect::<Vec<_>>();
    let _ = key;
    Ok(
        json!({"id":id,"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"snapshot_id":r.get::<Uuid,_>("snapshot_id"),"source_account_id":r.get::<i64,_>("source_account_id").to_string(),"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"workspace_revision":r.get::<i64,_>("workspace_revision").to_string(),"engine_version":ENGINE,"state":state,"phase":r.get::<String,_>("phase"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"confirmed_at":r.get::<Option<DateTime<Utc>>,_>("confirmed_at"),"confirmed_plan_id":confirmed,"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),"cancellation_reserved_bytes":cancel.to_string(),"release_ready":false,
    "policy":{"run_byte_limit":storage.get::<i64,_>("run_byte_limit").to_string(),"org_byte_limit":storage.get::<i64,_>("byte_limit").to_string(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"run_retained_bytes":storage.get::<i64,_>("retained_bytes").to_string(),"run_reserved_bytes":storage.get::<i64,_>("reserved_bytes").to_string(),"org_retained_bytes":storage.get::<i64,_>("org_retained").to_string(),"org_reserved_bytes":storage.get::<i64,_>("org_reserved").to_string(),"unit_byte_limit":s::UNIT.to_string(),"policy_revision":policy.revision()},
    "coverage":{"embedded_tags_only":true,"custom_fields_complete":true,"metadata_excluded_people":plan_counts.people.excluded.to_string(),"remaining_data":["standalone_tags","notes","tasks","history","mail","media","activation"],"source_gaps":source_gaps},"counts":counts,
    "latest_plan":{"id":latest,"revision":p.get::<i64,_>("revision").to_string(),"state":plan_state,"phase":p.get::<String,_>("phase"),"pause_reason":p.get::<Option<String>,_>("pause_reason"),"expires_at":expires,"confirmation_digest":p.get::<Option<Vec<u8>>,_>("confirmation_digest").map(|v|s::hex(&v)),"counts":plan_wire,"max_added_byte_bound":p.get::<i64,_>("max_added_byte_bound").to_string()},
    "actions":{"replan":confirmed.is_none() && !matches!(state.as_str(),"cancelled"|"completed"),"confirm":confirmed.is_none() && state=="proposed" && plan_state=="ready" && eligible_operations>0 && expires.is_some_and(|v|v>Utc::now()),"retry":state=="paused","cancel":!matches!(state.as_str(),"cancelled"|"completed")}}),
    )
}
#[allow(clippy::too_many_arguments)] // Keep tenant, immutable evidence and lease/receipt scopes explicit.
async fn new_plan(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    id: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    revision: i64,
    parent: Option<Uuid>,
    patch: Vec<Patch>,
) -> Result<i64, MigrationError> {
    let dst = destination(conn, org).await?;
    let a = s::seal(key, org, snapshot, plan, plan, "patch", &patch)?;
    let d = s::seal(key, org, snapshot, plan, plan, "destination", &dst)?;
    let mut size = s::sealed_bytes(&a) + s::sealed_bytes(&d);
    sqlx::query("INSERT INTO migration_metadata_plan(id,import_id,snapshot_id,organization_id,revision,parent_plan_id,inherit_plan_id,state,patch_nonce,patch_ciphertext,destination_nonce,destination_ciphertext,counts) VALUES($1,$2,$3,$4,$5,$6,$6,'building',$7,$8,$9,$10,$11)").bind(plan).bind(id).bind(snapshot).bind(org.0).bind(revision).bind(parent).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(d.nonce.as_slice()).bind(d.ciphertext).bind(serde_json::to_value(Counts::default()).map_err(|_|MigrationError::Crypto)?).execute(&mut *conn).await?;
    // Own edits exist before a later replan can supersede this building plan.
    // Inheritance walks older materialized choices in bounded worker units.
    for item in &patch {
        size += insert_choice(conn, key, org, id, snapshot, plan, item).await?;
    }
    Ok(size)
}
#[allow(clippy::too_many_arguments)] // Preserve tenant and encrypted plan scopes explicitly.
pub(crate) async fn insert_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    patch: &Patch,
) -> Result<i64, MigrationError> {
    let id = Uuid::new_v4();
    let sealed = s::seal(key, org, snapshot, plan, id, "choice", &patch.choice)?;
    let size = s::sealed_bytes(&sealed) + 32;
    sqlx::query("INSERT INTO migration_metadata_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(plan).bind(child).bind(org.0).bind(&patch.kind).bind(&patch.source_key).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(size)
}
pub async fn propose(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: PlanMetadataImport,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
    let mut tx=s::begin(pool,ctx,false).await?;
    if let Some(v)=s::replay(&mut tx,key,ctx,"plan",cmd.request_id,&cmd).await?{return Ok(v)}
    let p=eligible(&mut tx,ctx.organization_id,cmd.parent_import_id).await?;
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_metadata_import WHERE parent_import_id=$1 AND organization_id=$2)").bind(cmd.parent_import_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?{return Err(MigrationError::ImportConflict)}
    let id=Uuid::new_v4();let plan=Uuid::new_v4();let snapshot:Uuid=p.get("snapshot_id");let cancel=Uuid::new_v4();
    sqlx::query("INSERT INTO migration_metadata_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state,cancel_reservation_token,counts) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'proposed',$11,$12)").bind(id).bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(p.get::<Uuid,_>("confirmed_plan_id")).bind(snapshot).bind(p.get::<Uuid,_>("preview_id")).bind(p.get::<i64,_>("source_account_id")).bind(p.get::<i64,_>("capture_sequence")).bind(p.get::<i64,_>("current_workspace_revision")).bind(ctx.actor_user_id.0).bind(cancel).bind(serde_json::to_value(Counts::default()).unwrap()).execute(&mut *tx).await?;
    let size=new_plan(&mut tx,key,ctx.organization_id,id,snapshot,plan,1,None,vec![]).await?;
    sqlx::query("UPDATE migration_metadata_import SET latest_plan_id=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
    if !s::reserve(&mut tx,ctx.organization_id,id,snapshot,plan,cancel,cancel,s::CANCEL_RESERVATION,"cancel").await?{return Err(MigrationError::StorageLimit)}
    s::charge(&mut tx,ctx.organization_id,id,snapshot,plan,size).await?;
    let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});
    s::receipt(&mut tx,key,ctx,"plan",cmd.request_id,&cmd,id,snapshot,plan,&mut response).await?;tx.commit().await?;Ok(response)
}).await
}
pub async fn replan(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ReplanMetadataImport,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
    if cmd.mappings.len()>50{return Err(MigrationError::InvalidImportChoice)}let mut tx=s::begin(pool,ctx,false).await?;
    if let Some(v)=s::replay(&mut tx,key,ctx,"replan",cmd.request_id,&(id,&cmd)).await?{return Ok(v)}
    let r=s::run(&mut tx,ctx.organization_id,id).await?;validate_binding(&mut tx,&r).await?;
    if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some() || matches!(r.get::<String,_>("state").as_str(),"cancelled"|"completed"){return Err(MigrationError::ImportConflict)}
    let old:Uuid=r.get("latest_plan_id");let p=s::plan(&mut tx,ctx.organization_id,id,old).await?;
    if cmd.expected_plan_revision!=p.get::<i64,_>("revision").to_string(){return Err(MigrationError::ImportConflict)}
    let mut seen=std::collections::BTreeSet::new();let mut patch=Vec::new();
    for item in &cmd.mappings{if !seen.insert(item.mapping_id){return Err(MigrationError::InvalidImportChoice)}let q=sqlx::query("SELECT * FROM migration_metadata_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4").bind(item.mapping_id).bind(old).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;validate_choice(&mut tx,key,&r,&p,&q,&item.choice).await?;patch.push(Patch{kind:q.get("kind"),source_key:q.get("source_key"),choice:item.choice.clone()});}
    let plan=Uuid::new_v4();let snapshot:Uuid=r.get("snapshot_id");s::release(&mut tx,ctx.organization_id,id,"work",0).await?;
    let size=new_plan(&mut tx,key,ctx.organization_id,id,snapshot,plan,p.get::<i64,_>("revision")+1,Some(old),patch).await?;s::charge(&mut tx,ctx.organization_id,id,snapshot,plan,size).await?;
    sqlx::query("UPDATE migration_metadata_plan SET state='superseded' WHERE id=$1 AND organization_id=$2").bind(old).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_metadata_import SET latest_plan_id=$3,state='proposed',phase='preparation',executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,checkpoint_id=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,"replan",cmd.request_id,&(id,&cmd),id,snapshot,plan,&mut response).await?;tx.commit().await?;Ok(response)
}).await
}
pub async fn replay_confirmation(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: &ConfirmMetadataImport,
) -> Result<Option<Value>, MigrationError> {
    let mut tx = s::begin(pool, ctx, false).await?;
    s::run(&mut tx, ctx.organization_id, id).await?;
    s::replay(&mut tx, key, ctx, "confirm", cmd.request_id, &(id, cmd)).await
}
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmMetadataImport,
    release: &workspace::ReleaseReadiness,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
    let mut tx=s::begin(pool,ctx,false).await?;let r=s::run(&mut tx,ctx.organization_id,id).await?;
    if let Some(v)=s::replay(&mut tx,key,ctx,"confirm",cmd.request_id,&(id,&cmd)).await?{return Ok(v)}
    release.require_metadata(&mut tx).await.map_err(|_|MigrationError::ReleaseNotReady)?;validate_binding(&mut tx,&r).await?;
    if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some() || r.get::<String,_>("state")!="proposed" || r.get::<Uuid,_>("latest_plan_id")!=cmd.plan_id{return Err(MigrationError::ImportConflict)}
    let p=s::plan(&mut tx,ctx.organization_id,id,cmd.plan_id).await?;
    if p.get::<String,_>("state")!="ready" || p.get::<i64,_>("revision").to_string()!=cmd.plan_revision || p.get::<Option<Vec<u8>>,_>("confirmation_digest").map(|v|s::hex(&v)).as_deref()!=Some(cmd.confirmation_digest.as_str()) || cmd.workspace_revision!=r.get::<i64,_>("workspace_revision").to_string(){return Err(MigrationError::ImportConflict)}
    if !p.get::<Option<DateTime<Utc>>,_>("expires_at").is_some_and(|v|v>Utc::now()){return Err(MigrationError::ImportExpired)}
    let counts=Counts::load(p.get("counts"))?;
    if cmd.acknowledgments.held_count!=counts.held_count.to_string() || !cmd.acknowledgments.review_only || !cmd.acknowledgments.remaining_data{return Err(MigrationError::InvalidImportChoice)}
    if counts.tags.eligible+counts.fields.eligible+counts.options.eligible+counts.tag_links.eligible+counts.values.eligible==0{return Err(MigrationError::SourceNotEligible)}
    super::metadata_worker::validate_catalog(&mut tx,key,&r,&p).await?;
    sqlx::query("UPDATE migration_metadata_import SET confirmed_plan_id=$3,state='queued',phase='catalog',executor_user_id=$4,confirmed_at=now(),updated_at=now(),counts=$5,checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.plan_id).bind(ctx.actor_user_id.0).bind(p.get::<Value,_>("counts")).execute(&mut *tx).await?;
    let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,"confirm",cmd.request_id,&(id,&cmd),id,r.get("snapshot_id"),cmd.plan_id,&mut response).await?;tx.commit().await?;Ok(response)
}).await
}
pub async fn action(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ImportRequest,
    retry: bool,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
    let action=if retry{"retry"}else{"cancel"};let mut tx=s::begin(pool,ctx,false).await?;
    if let Some(v)=s::replay(&mut tx,key,ctx,action,cmd.request_id,&(id,&cmd)).await?{return Ok(v)}
    let r=s::run(&mut tx,ctx.organization_id,id).await?;let plan:Uuid=r.get("latest_plan_id");let state:String=r.get("state");
    if matches!(state.as_str(),"cancelled"|"completed") || (retry && state!="paused"){return Err(MigrationError::ImportConflict)}
    s::release(&mut tx,ctx.organization_id,id,"work",0).await?;
    if retry{validate_binding(&mut tx,&r).await?;sqlx::query("UPDATE migration_metadata_plan SET state=CASE WHEN state='paused' THEN 'building' ELSE state END,pause_reason=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).execute(&mut *tx).await?;}
    let next=if retry{if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some(){"queued"}else{"proposed"}}else{"cancelled"};
    sqlx::query("UPDATE migration_metadata_import SET state=$3,executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,cancel_reservation_token=CASE WHEN $3='cancelled' THEN NULL ELSE cancel_reservation_token END,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(next).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,action,cmd.request_id,&(id,&cmd),id,r.get("snapshot_id"),plan,&mut response).await?;tx.commit().await?;Ok(response)
}).await
}
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy, async {
        let mut tx = s::begin(pool, ctx, false).await?;
        let r = s::run(&mut tx, ctx.organization_id, id).await?;
        view(&mut tx, key, ctx.organization_id, &r).await
    })
    .await
}

async fn validate_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
    row: &sqlx::postgres::PgRow,
    choice: &Choice,
) -> Result<(), MigrationError> {
    if matches!(choice, Choice::Hold) {
        return Ok(());
    }
    let org = OrganizationId::new(r.get("organization_id"));
    let data: Mapping = s::open(
        key,
        org,
        r.get("snapshot_id"),
        p.get("id"),
        row.get("id"),
        "mapping",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )?;
    if !row.get::<bool, _>("qualified")
        || data.reasons.iter().any(|r| {
            matches!(
                r.as_str(),
                "source_integrity"
                    | "source_field_key_collision"
                    | "colliding_source_choices"
                    | "duplicate_source_choice"
            )
        })
    {
        return Err(MigrationError::InvalidImportChoice);
    }
    match choice {
        Choice::CreateMatching => {
            if !data.create_matching_available(row.get("qualified")) {
                return Err(MigrationError::InvalidImportChoice);
            }
        }
        Choice::MapExisting { target_id } => {
            let dst = destination(conn, org).await?;
            let kind: String = row.get("kind");
            let t = dst
                .by_id(*target_id)
                .filter(|t| t.kind == kind)
                .ok_or(MigrationError::InvalidImportChoice)?;
            if kind == "field"
                && (t.field_type != data.field.as_ref().and_then(|f| f.field_type.clone())
                    || (t.source.is_some()
                        && (t.source.as_deref() != Some("fub")
                            || t.external_key != data.source_name)))
            {
                return Err(MigrationError::InvalidImportChoice);
            }
            if kind == "option" && t.field_id != row.get::<Option<Uuid>, _>("target_field_id") {
                return Err(MigrationError::InvalidImportChoice);
            }
        }
        Choice::Hold => {}
    }
    Ok(())
}

pub use super::metadata_queries::{
    aliases, field, issues, list, mapping_field, mappings, provenance, provenance_field, records,
    result_field, results, targets,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_choices_reject_hidden_payload_fields() {
        for value in [
            serde_json::json!({"kind":"hold","target_id":Uuid::new_v4()}),
            serde_json::json!({"kind":"create_matching","label":"client injection"}),
            serde_json::json!({"kind":"map_existing","target_id":Uuid::new_v4(),"field_type":"text"}),
        ] {
            assert!(serde_json::from_value::<Choice>(value).is_err());
        }
    }
}
