//! D-068 retained-only activity commands. Clients choose scoped mappings, never source payloads.
use super::{
    activity_model::{Counts, Destination, Mapping, Member},
    activity_source as source, activity_store as s,
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
pub const ENGINE: &str = "fub-activity-import-v1";
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Choice {
    Hold,
    LeaveUnmapped,
    MapExisting { target_id: Uuid },
    MapKind { native_kind: String },
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
            LeaveUnmapped(Empty),
            MapExisting { target_id: Uuid },
            MapKind { native_kind: String },
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Hold(_) => Self::Hold,
            Wire::LeaveUnmapped(_) => Self::LeaveUnmapped,
            Wire::MapExisting { target_id } => Self::MapExisting { target_id },
            Wire::MapKind { native_kind } => Self::MapKind { native_kind },
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareActivityImport {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPatch {
    pub mapping_id: Uuid,
    pub choice: Choice,
}
fn zone<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanActivityImport {
    pub request_id: Uuid,
    pub expected_plan_id: Uuid,
    #[serde(default)]
    pub choices: Vec<MappingPatch>,
    #[serde(
        default,
        deserialize_with = "zone",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_timezone: Option<Option<String>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmActivityImport {
    pub request_id: Uuid,
    pub plan_id: Uuid,
    pub expected_revision: String,
    pub acknowledge_held: String,
    pub acknowledge_source_only: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityAction {
    pub request_id: Uuid,
    pub expected_revision: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityPage {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub kind: Option<String>,
    pub disposition: Option<String>,
    pub issue: Option<String>,
    pub parent_import_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
}
impl ActivityPage {
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
    pub plan_id: Option<Uuid>,
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
    sqlx::query("SELECT p.*,o.workspace_revision AS current_workspace_revision FROM migration_import p JOIN migration_workspace w ON w.organization_id=p.organization_id AND w.import_id=p.id AND w.plan_id=p.confirmed_plan_id JOIN organization o ON o.id=p.organization_id JOIN migration_snapshot s ON s.id=p.snapshot_id AND s.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2 AND p.state='completed' AND o.workspace_mode='migration_review' AND s.profile_version='fub-core-v1' AND s.state IN ('completed','completed_with_gaps') AND s.capture_sequence=p.capture_sequence AND s.source_account_id=p.source_account_id AND (SELECT count(*) FROM migration_snapshot_stream t WHERE t.snapshot_id=s.id AND t.organization_id=s.organization_id AND t.stream IN ('people','users','notes','note_detail','tasks_open','tasks_completed') AND t.state='completed')=6").bind(parent).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)
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
        || p.get::<Uuid, _>("preview_id") != r.get::<Uuid, _>("preview_id")
        || p.get::<i64, _>("source_account_id") != r.get::<i64, _>("source_account_id")
        || p.get::<i64, _>("capture_sequence") != r.get::<i64, _>("capture_sequence")
        || p.get::<i64, _>("current_workspace_revision") != r.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(())
}
async fn destination(
    conn: &mut PgConnection,
    org: OrganizationId,
    source_timezone: Option<String>,
) -> Result<Destination, MigrationError> {
    if source_timezone
        .as_deref()
        .is_some_and(|v| !source::valid_timezone(v))
    {
        return Err(MigrationError::InvalidImportChoice);
    }
    let rows=sqlx::query("SELECT m.user_id,u.display_name,m.status,m.role FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 ORDER BY m.user_id LIMIT 10001").bind(org.0).fetch_all(conn).await?;
    if rows.len() > 10000 {
        return Err(MigrationError::InvalidImportChoice);
    }
    let out = Destination {
        members: rows
            .iter()
            .map(|r| Member {
                id: r.get("user_id"),
                display_name: r.get("display_name"),
                status: r.get("status"),
                role: r.get("role"),
            })
            .collect(),
        source_timezone,
        source_engine: source::ENGINE.into(),
        html_profile: source::HTML_PROFILE.into(),
        time_profile: source::TIME_PROFILE.into(),
        tzdb_version: source::TZDB_VERSION.into(),
    };
    if s::bytes(&out)?.len() > 8 * 1024 * 1024 {
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
        p.get("destination_nonce"),
        p.get("destination_ciphertext"),
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
    let pc = Counts::load(p.get("counts"))?;
    let confirmed: Option<Uuid> = r.get("confirmed_plan_id");
    let counts = if confirmed.is_some() {
        Counts::load(r.get("counts"))?
    } else {
        pc.clone()
    };
    let d = decode_destination(key, r, &p)?;
    let policy = s::current_policy()?;
    let storage=sqlx::query("SELECT s.run_byte_limit,s.retained_bytes,s.reserved_bytes,l.byte_limit,l.retained_bytes AS org_retained,l.reserved_bytes AS org_reserved FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2").bind(r.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *conn).await?;
    let cancel:i64=sqlx::query_scalar("SELECT COALESCE(sum(byte_count),0)::bigint FROM migration_activity_reservation WHERE import_id=$1 AND organization_id=$2 AND purpose='cancel'").bind(id).bind(org.0).fetch_one(&mut *conn).await?;
    let state: String = r.get("state");
    let expires: Option<DateTime<Utc>> = p.get("expires_at");
    Ok(
        json!({"id":id,"parent_import_id":r.get::<Uuid,_>("parent_import_id"),"parent_plan_id":r.get::<Uuid,_>("parent_plan_id"),"snapshot_id":r.get::<Uuid,_>("snapshot_id"),"source_account_id":r.get::<i64,_>("source_account_id").to_string(),"capture_sequence":r.get::<i64,_>("capture_sequence").to_string(),"workspace_revision":r.get::<i64,_>("workspace_revision").to_string(),"revision":r.get::<i64,_>("revision").to_string(),"activity_revision":r.get::<i64,_>("activity_revision").to_string(),"engine_version":ENGINE,"state":state,"phase":r.get::<String,_>("phase"),"pause_reason":r.get::<Option<String>,_>("pause_reason"),"created_at":r.get::<DateTime<Utc>,_>("created_at"),"updated_at":r.get::<DateTime<Utc>,_>("updated_at"),"confirmed_at":r.get::<Option<DateTime<Utc>>,_>("confirmed_at"),"confirmed_plan_id":confirmed,"completed_at":r.get::<Option<DateTime<Utc>>,_>("completed_at"),"retained_bytes":r.get::<i64,_>("retained_bytes").to_string(),"reserved_bytes":r.get::<i64,_>("reserved_bytes").to_string(),"native_row_bytes":r.get::<i64,_>("native_bytes").to_string(),"cancellation_reserved_bytes":cancel.to_string(),"release_ready":false,"counts":counts.wire(),
 "policy":{"run_byte_limit":storage.get::<i64,_>("run_byte_limit").to_string(),"org_byte_limit":storage.get::<i64,_>("byte_limit").to_string(),"run_ceiling_bytes":policy.run_ceiling_bytes.to_string(),"org_ceiling_bytes":policy.org_ceiling_bytes.to_string(),"run_retained_bytes":storage.get::<i64,_>("retained_bytes").to_string(),"run_reserved_bytes":storage.get::<i64,_>("reserved_bytes").to_string(),"org_retained_bytes":storage.get::<i64,_>("org_retained").to_string(),"org_reserved_bytes":storage.get::<i64,_>("org_reserved").to_string(),"unit_byte_limit":s::UNIT.to_string(),"policy_revision":policy.revision()},
 "coverage":{"remaining_data":["note_replies","reactions","reminders","recurrence","task_descriptions","history","mail","media","activation"],"native_review_only":true},
 "latest_plan":{"id":latest,"revision":p.get::<i64,_>("revision").to_string(),"state":p.get::<String,_>("state"),"phase":p.get::<String,_>("phase"),"expires_at":expires,"counts":pc.wire(),"source_timezone":d.source_timezone,"source_engine":d.source_engine,"html_profile":d.html_profile,"time_profile":d.time_profile,"tzdb_version":d.tzdb_version,"confirmation_digest":p.get::<Option<Vec<u8>>,_>("confirmation_digest").map(|v|s::hex(&v)),"max_added_byte_bound":p.get::<i64,_>("max_added_byte_bound").to_string()},
 "actions":{"replan":confirmed.is_none()&&!matches!(state.as_str(),"cancelled"|"completed"),"confirm":confirmed.is_none()&&state=="ready"&&pc.eligible()>0&&expires.is_some_and(|v|v>Utc::now()),"retry":state=="paused","cancel":!matches!(state.as_str(),"cancelled"|"completed")}}),
    )
}
#[allow(clippy::too_many_arguments)]
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
    timezone: Option<String>,
) -> Result<i64, MigrationError> {
    let dst = destination(conn, org, timezone).await?;
    let a = s::seal(key, org, snapshot, plan, plan, "patch", &patch)?;
    let d = s::seal(key, org, snapshot, plan, plan, "destination", &dst)?;
    let mut size = s::sealed_bytes(&a) + s::sealed_bytes(&d);
    sqlx::query("INSERT INTO migration_activity_plan(id,import_id,snapshot_id,organization_id,revision,parent_plan_id,inherit_plan_id,state,patch_nonce,patch_ciphertext,destination_nonce,destination_ciphertext,counts) VALUES($1,$2,$3,$4,$5,$6,$6,'building',$7,$8,$9,$10,$11)").bind(plan).bind(id).bind(snapshot).bind(org.0).bind(revision).bind(parent).bind(a.nonce.as_slice()).bind(a.ciphertext).bind(d.nonce.as_slice()).bind(d.ciphertext).bind(json!(Counts::default())).execute(&mut *conn).await?;
    for item in &patch {
        size += insert_choice(conn, key, org, id, snapshot, plan, item).await?
    }
    Ok(size)
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_choice(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    child: Uuid,
    snapshot: Uuid,
    plan: Uuid,
    p: &Patch,
) -> Result<i64, MigrationError> {
    let id = Uuid::new_v4();
    let a = s::seal(key, org, snapshot, plan, id, "choice", &p.choice)?;
    let size = s::sealed_bytes(&a) + 32;
    sqlx::query("INSERT INTO migration_activity_choice(id,plan_id,import_id,organization_id,kind,source_key,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(plan).bind(child).bind(org.0).bind(&p.kind).bind(&p.source_key).bind(a.nonce.as_slice()).bind(a.ciphertext).execute(conn).await?;
    Ok(size)
}
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    cmd: PrepareActivityImport,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
 let mut tx=s::begin(pool,ctx,false).await?;if let Some(v)=s::replay(&mut tx,key,ctx,"prepare",cmd.request_id,&cmd).await?{return Ok(v)}let p=eligible(&mut tx,ctx.organization_id,cmd.parent_import_id).await?;
 if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_activity_import WHERE parent_import_id=$1 AND organization_id=$2)").bind(cmd.parent_import_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?{return Err(MigrationError::ImportConflict)}
 let id=Uuid::new_v4();let plan=Uuid::new_v4();let snapshot:Uuid=p.get("snapshot_id");let cancel=Uuid::new_v4();
 sqlx::query("INSERT INTO migration_activity_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state,cancel_reservation_token,counts) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'preparing',$11,$12)").bind(id).bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(p.get::<Uuid,_>("confirmed_plan_id")).bind(snapshot).bind(p.get::<Uuid,_>("preview_id")).bind(p.get::<i64,_>("source_account_id")).bind(p.get::<i64,_>("capture_sequence")).bind(p.get::<i64,_>("current_workspace_revision")).bind(ctx.actor_user_id.0).bind(cancel).bind(json!(Counts::default())).execute(&mut *tx).await?;
 let size=new_plan(&mut tx,key,ctx.organization_id,id,snapshot,plan,1,None,vec![],None).await?;sqlx::query("UPDATE migration_activity_import SET latest_plan_id=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
 if !s::reserve(&mut tx,ctx.organization_id,id,snapshot,plan,cancel,cancel,s::CANCEL_RESERVATION,"cancel").await?{return Err(MigrationError::StorageLimit)}s::charge(&mut tx,ctx.organization_id,id,snapshot,plan,size).await?;let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,"prepare",cmd.request_id,&cmd,id,snapshot,plan,&mut response).await?;tx.commit().await?;Ok(response)}).await
}
pub async fn replan(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: PlanActivityImport,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
 if cmd.choices.len()>50{return Err(MigrationError::InvalidImportChoice)}let mut tx=s::begin(pool,ctx,false).await?;if let Some(v)=s::replay(&mut tx,key,ctx,"replan",cmd.request_id,&(id,&cmd)).await?{return Ok(v)}let r=s::run(&mut tx,ctx.organization_id,id).await?;validate_binding(&mut tx,&r).await?;
 if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some()||matches!(r.get::<String,_>("state").as_str(),"cancelled"|"completed")||r.get::<Uuid,_>("latest_plan_id")!=cmd.expected_plan_id{return Err(MigrationError::ImportConflict)}let old:Uuid=r.get("latest_plan_id");let p=s::plan(&mut tx,ctx.organization_id,id,old).await?;let d=decode_destination(key,&r,&p)?;let mut seen=std::collections::BTreeSet::new();let mut patch=vec![];
 for v in &cmd.choices{if !seen.insert(v.mapping_id){return Err(MigrationError::InvalidImportChoice)}let m=sqlx::query("SELECT * FROM migration_activity_mapping WHERE id=$1 AND plan_id=$2 AND import_id=$3 AND organization_id=$4").bind(v.mapping_id).bind(old).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;validate_choice(&mut tx,ctx.organization_id,m.get("kind"),&v.choice).await?;patch.push(Patch{kind:m.get("kind"),source_key:m.get("source_key"),choice:v.choice.clone()});}
 let plan=Uuid::new_v4();let snapshot:Uuid=r.get("snapshot_id");s::release(&mut tx,ctx.organization_id,id,"work",0).await?;let size=new_plan(&mut tx,key,ctx.organization_id,id,snapshot,plan,p.get::<i64,_>("revision")+1,Some(old),patch,cmd.source_timezone.clone().unwrap_or(d.source_timezone)).await?;s::charge(&mut tx,ctx.organization_id,id,snapshot,plan,size).await?;
 sqlx::query("UPDATE migration_activity_plan SET state='superseded' WHERE id=$1 AND organization_id=$2").bind(old).bind(ctx.organization_id.0).execute(&mut *tx).await?;sqlx::query("UPDATE migration_activity_import SET latest_plan_id=$3,state='preparing',phase='preparation',executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,checkpoint_id=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,"replan",cmd.request_id,&(id,&cmd),id,snapshot,plan,&mut response).await?;tx.commit().await?;Ok(response)}).await
}
pub async fn replay_confirmation(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: &ConfirmActivityImport,
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
    cmd: ConfirmActivityImport,
    release: &workspace::ReleaseReadiness,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
 let mut tx=s::begin(pool,ctx,true).await?;let r=s::run(&mut tx,ctx.organization_id,id).await?;if let Some(v)=s::replay(&mut tx,key,ctx,"confirm",cmd.request_id,&(id,&cmd)).await?{return Ok(v)}release.require_activity(&mut tx).await.map_err(|_|MigrationError::ReleaseNotReady)?;validate_binding(&mut tx,&r).await?;
 if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some()||r.get::<String,_>("state")!="ready"||r.get::<Uuid,_>("latest_plan_id")!=cmd.plan_id||r.get::<i64,_>("revision").to_string()!=cmd.expected_revision{return Err(MigrationError::ImportConflict)}let p=s::plan(&mut tx,ctx.organization_id,id,cmd.plan_id).await?;
 if p.get::<String,_>("state")!="ready"{return Err(MigrationError::ImportConflict)}
 if !p.get::<Option<DateTime<Utc>>,_>("expires_at").is_some_and(|v|v>Utc::now()){return Err(MigrationError::ImportExpired)}let counts=Counts::load(p.get("counts"))?;if cmd.acknowledge_held!=counts.held_count.to_string()||cmd.acknowledge_source_only!=counts.source_only_count.to_string(){return Err(MigrationError::InvalidImportChoice)}
 if counts.eligible()==0{return Err(MigrationError::ImportConflict)}
 super::activity_worker::validate_choices(&mut tx,key,&r,&p).await?;
 sqlx::query("UPDATE migration_activity_import SET confirmed_plan_id=$3,state='queued',phase='records',executor_user_id=$4,confirmed_at=now(),updated_at=now(),counts=$5,checkpoint_id=NULL,revision=revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.plan_id).bind(ctx.actor_user_id.0).bind(p.get::<Value,_>("counts")).execute(&mut *tx).await?;let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,"confirm",cmd.request_id,&(id,&cmd),id,r.get("snapshot_id"),cmd.plan_id,&mut response).await?;tx.commit().await?;Ok(response)}).await
}
pub async fn action(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ActivityAction,
    retry: bool,
    policy: &SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy,async{
 let action=if retry{"retry"}else{"cancel"};let mut tx=s::begin(pool,ctx,false).await?;if let Some(v)=s::replay(&mut tx,key,ctx,action,cmd.request_id,&(id,&cmd)).await?{return Ok(v)}let r=s::run(&mut tx,ctx.organization_id,id).await?;let plan:Uuid=r.get("latest_plan_id");let state:String=r.get("state");if matches!(state.as_str(),"cancelled"|"completed")||(retry&&state!="paused")||r.get::<i64,_>("revision").to_string()!=cmd.expected_revision{return Err(MigrationError::ImportConflict)}s::release(&mut tx,ctx.organization_id,id,"work",0).await?;
 if retry{validate_binding(&mut tx,&r).await?;sqlx::query("UPDATE migration_activity_plan SET state=CASE WHEN state='paused' THEN 'building' ELSE state END,pause_reason=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).execute(&mut *tx).await?;}let next=if retry{if r.get::<Option<Uuid>,_>("confirmed_plan_id").is_some(){"queued"}else{"preparing"}}else{"cancelled"};sqlx::query("UPDATE migration_activity_import SET state=$3,executor_user_id=$4,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,cancel_reservation_token=CASE WHEN $3='cancelled' THEN NULL ELSE cancel_reservation_token END,updated_at=now(),revision=revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(next).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;if retry{s::control(&mut tx,ctx.organization_id,id).await?;}let r=s::run(&mut tx,ctx.organization_id,id).await?;let mut response=json!({"import":view(&mut tx,key,ctx.organization_id,&r).await?});s::receipt(&mut tx,key,ctx,action,cmd.request_id,&(id,&cmd),id,r.get("snapshot_id"),plan,&mut response).await?;tx.commit().await?;Ok(response)}).await
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
        validate_binding(&mut tx, &r).await?;
        view(&mut tx, key, ctx.organization_id, &r).await
    })
    .await
}
pub(crate) async fn validate_choice(
    conn: &mut PgConnection,
    org: OrganizationId,
    kind: &str,
    c: &Choice,
) -> Result<(), MigrationError> {
    match c {
        Choice::Hold => Ok(()),
        Choice::LeaveUnmapped if kind != "task_kind" => Ok(()),
        Choice::MapKind { native_kind }
            if kind == "task_kind"
                && crate::domain::task::TaskKind::from_db_str(native_kind).is_some() =>
        {
            Ok(())
        }
        Choice::MapExisting { target_id } if kind != "task_kind" => {
            let r=sqlx::query("SELECT status FROM organization_membership WHERE organization_id=$1 AND user_id=$2 FOR SHARE").bind(org.0).bind(target_id).fetch_optional(conn).await?.ok_or(MigrationError::InvalidImportChoice)?;
            if kind == "task_assignee" && r.get::<String, _>("status") != "active" {
                return Err(MigrationError::InvalidImportChoice);
            }
            Ok(())
        }
        _ => Err(MigrationError::InvalidImportChoice),
    }
}
pub(crate) fn mapping_data(
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    p: Uuid,
    m: &sqlx::postgres::PgRow,
) -> Result<Mapping, MigrationError> {
    s::open(
        key,
        OrganizationId::new(r.get("organization_id")),
        r.get("snapshot_id"),
        p,
        m.get("id"),
        "mapping",
        m.get("nonce"),
        m.get("ciphertext"),
    )
}
pub use super::activity_queries::{field, list, mappings, observations, records, results, targets};
