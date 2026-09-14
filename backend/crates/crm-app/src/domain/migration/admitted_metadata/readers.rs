//! Admitted-family evidence readers. Cursor purposes include the current root
//! generation; an old preview cannot lend a cursor to a replacement or endpoint.
use super::super::{
    metadata::{Choice, FieldQuery, MetadataPage},
    metadata_model as model, snapshot,
};
use super::*;
use serde::de::DeserializeOwned;
use sqlx::{postgres::PgRow, PgConnection};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Scope {
    org: OrganizationId,
    root: Uuid,
    plan: Uuid,
    snapshot: Uuid,
    revision: i64,
    generation: Uuid,
    evidence_plan: Uuid,
}
async fn scope(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
    plan: Uuid,
) -> Result<Scope, MigrationError> {
    let p=sqlx::query("SELECT p.*,i.latest_plan_id FROM migration_admitted_metadata_plan p JOIN migration_admitted_metadata_import i ON i.id=p.import_id AND i.organization_id=p.organization_id WHERE p.id=$1 AND p.import_id=$2 AND p.organization_id=$3").bind(plan).bind(root).bind(org.0).fetch_optional(c).await?.ok_or(MigrationError::NotFound)?;
    if p.get::<String, _>("state") == "building" {
        return Err(MigrationError::ImportBusy);
    }
    Ok(Scope {
        org,
        root,
        plan,
        snapshot: p.get("snapshot_id"),
        revision: p.get("revision"),
        generation: p.get("latest_plan_id"),
        evidence_plan: p.get::<Option<Uuid>, _>("evidence_plan_id").unwrap_or(plan),
    })
}
fn open<T: DeserializeOwned>(
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
    purpose: &str,
) -> Result<T, MigrationError> {
    super::super::admitted_metadata_worker::open(
        key,
        s.org,
        s.snapshot,
        s.plan,
        r.get("id"),
        purpose,
        &r.get::<Vec<u8>, _>("nonce"),
        &r.get::<Vec<u8>, _>("ciphertext"),
    )
}
fn purpose(
    endpoint: &str,
    s: Scope,
    q: &MetadataPage,
    entity: Option<Uuid>,
    field: Option<&str>,
) -> Result<String, MigrationError> {
    Ok(format!("admitted-metadata-{endpoint}:{}",serde_json::to_string(&json!({"root":s.root,"plan":s.plan,"revision":s.revision.to_string(),"generation":s.generation,"entity":entity,"field":field,"kind":q.kind,"disposition":q.disposition,"target_field":q.field_id,"limit":q.limit()?.to_string()})).map_err(|_|MigrationError::Crypto)?))
}
fn limit(
    q: &MetadataPage,
    kinds: &[&str],
    dispositions: &[&str],
    fields: bool,
) -> Result<i64, MigrationError> {
    if q.kind.as_deref().is_some_and(|v| !kinds.contains(&v))
        || q.disposition
            .as_deref()
            .is_some_and(|v| !dispositions.contains(&v))
        || q.parent_import_id.is_some()
        || q.field_id.is_some() && (!fields || q.kind.as_deref() != Some("option"))
    {
        return Err(MigrationError::InvalidInput);
    }
    q.limit()
}
fn cursor<T: DeserializeOwned>(
    key: &RawPayloadKey,
    s: Scope,
    p: &str,
    value: Option<&str>,
) -> Result<Option<T>, MigrationError> {
    snapshot::decode_cursor(key, s.org, s.root, p, value)?
        .map(|v| serde_json::from_value(v).map_err(|_| MigrationError::InvalidInput))
        .transpose()
}
struct Page {
    items: Vec<Value>,
    used: usize,
}
impl Page {
    fn new() -> Self {
        Self {
            items: vec![],
            used: 4096,
        }
    }
    fn push(&mut self, value: Value) -> Result<bool, MigrationError> {
        let size = metadata_store::bytes(&value)?.len();
        if size > 128 * 1024 {
            return Err(MigrationError::Crypto);
        }
        if self.used + size + 1 > 512 * 1024 {
            return Ok(false);
        }
        self.used += size + 1;
        self.items.push(value);
        Ok(true)
    }
    fn finish<T: Serialize>(
        self,
        key: &RawPayloadKey,
        s: Scope,
        p: &str,
        total: usize,
        last: Option<T>,
    ) -> Result<Value, MigrationError> {
        let next = if total > self.items.len() {
            Some(snapshot::encode_cursor(
                key,
                s.org,
                s.root,
                p,
                &serde_json::to_value(last.ok_or(MigrationError::Crypto)?)
                    .map_err(|_| MigrationError::Crypto)?,
            )?)
        } else {
            None
        };
        let value = json!({"items":self.items,"next_cursor":next});
        if metadata_store::bytes(&value)?.len() > 512 * 1024 {
            return Err(MigrationError::Crypto);
        }
        Ok(value)
    }
}
async fn mapping_row(c: &mut PgConnection, s: Scope, id: Uuid) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4").bind(id).bind(s.root).bind(s.plan).bind(s.org.0).fetch_optional(c).await?.ok_or(MigrationError::NotFound)
}
async fn source(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    family: &str,
    id: &str,
) -> Result<BTreeMap<String, String>, MigrationError> {
    let r=sqlx::query("SELECT * FROM migration_admitted_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4").bind(s.evidence_plan).bind(s.org.0).bind(family).bind(id).fetch_optional(c).await?;
    if let Some(r) = r {
        let f: FrozenSource = open(
            key,
            Scope {
                plan: s.evidence_plan,
                ..s
            },
            &r,
            "source",
        )?;
        Ok(f.record.provenance)
    } else {
        Ok(BTreeMap::from([("source_id".into(), id.to_owned())]))
    }
}
async fn mapping_source(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
) -> Result<(FrozenMapping, BTreeMap<String, String>), MigrationError> {
    let f = super::fidelity::mapping(c, key, s.org, s.snapshot, s.plan, r).await?;
    let fields = if r.get::<String, _>("kind") == "tag" {
        BTreeMap::from([("tag".into(), f.raw_choice.clone().unwrap_or_default())])
    } else {
        let mut fields = source(c, key, s, "custom_fields", &f.source_id).await?;
        if r.get::<String, _>("kind") == "option" {
            fields.insert("choice".into(), f.raw_choice.clone().unwrap_or_default());
        }
        fields
    };
    Ok((f, fields))
}
fn target(kind: &str, id: Uuid, field: Option<Uuid>, v: &Value) -> Value {
    if v.is_null() {
        Value::Null
    } else {
        json!({"id":id,"kind":kind,"field_id":field,"label":v["label"],"field_type":v.get("field_type"),"source_bound":v.get("source").is_some_and(|s|!s.is_null()),"archived":false})
    }
}
async fn mapping_view(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    let (f, source) = mapping_source(c, key, s, r).await?;
    let id: Uuid = r.get("id");
    let kind: String = r.get("kind");
    let disposition: String = r.get("disposition");
    let target_id: Option<Uuid> = r.get("target_id");
    let dependent: i64 = r.get("dependent_count");
    let aliases: i64 = r.get("alias_count");
    let choice = match disposition.as_str() {
        "create_matching" => Choice::CreateMatching,
        "map_existing" => Choice::MapExisting {
            target_id: target_id.ok_or(MigrationError::Crypto)?,
        },
        _ => Choice::Hold,
    };
    let qualified = f.reasons.iter().all(|v| v == "invalid_destination");
    let available = qualified
        && f.label.is_some()
        && !(kind == "field"
            && f.definition
                .as_ref()
                .is_some_and(|v| !v.creation_reasons.is_empty()));
    Ok(
        json!({"id":id,"kind":kind,"parent_mapping_id":r.get::<Option<Uuid>,_>("parent_mapping_id"),"dependency_result_id":r.get::<Option<Uuid>,_>("dependency_result_id"),"execute_unit":r.get::<bool,_>("execute_unit"),"source_id":r.get::<String,_>("source_id"),"disposition":disposition,"qualified":qualified,"create_matching_available":available,"choice":choice,"target_id":target_id,"field_id":r.get::<Option<Uuid>,_>("target_field_id"),"reasons":f.reasons,"suggestions":[],"dependent_count":dependent.to_string(),"source":model::summary("source",&source),"added_byte_bound":(r.get::<Vec<u8>,_>("nonce").len()+r.get::<Vec<u8>,_>("ciphertext").len()+4096).to_string(),"alias_count":aliases.to_string(),"target":target_id.map(|id|target(&kind,id,r.get("target_field_id"),&f.target_baseline))}),
    )
}
pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(&q, &["tag", "field", "option"], &[], false)?;
    let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
    let p = purpose("mappings", s, &q, None, None)?;
    let after: Uuid = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND ($4::text IS NULL OR kind=$4) ORDER BY id LIMIT $5").bind(plan).bind(ctx.organization_id.0).bind(after).bind(&q.kind).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for id in ids.iter().take(n as usize) {
        let r = mapping_row(&mut tx, s, *id).await?;
        if !page.push(mapping_view(&mut tx, key, s, &r).await?)? {
            break;
        }
        last = Some(*id);
    }
    page.finish(key, s, &p, ids.len(), last)
}
pub async fn targets(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(&q, &["tag", "field", "option"], &[], true)?;
    let kind = q.kind.as_deref().ok_or(MigrationError::InvalidInput)?;
    let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
    let p = purpose("targets", s, &q, None, None)?;
    let after: Uuid = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let rows=match kind{
 "tag"=>sqlx::query("SELECT id,name AS label,NULL::text AS field_type,NULL::uuid AS field_id,NULL::text AS source FROM tag WHERE organization_id=$1 AND id>$2 ORDER BY id LIMIT $3").bind(s.org.0).bind(after).bind(n+1).fetch_all(&mut *tx).await?,
 "field"=>sqlx::query("SELECT id,label,field_type,NULL::uuid AS field_id,source FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL AND id>$2 ORDER BY id LIMIT $3").bind(s.org.0).bind(after).bind(n+1).fetch_all(&mut *tx).await?,
 _=>sqlx::query("SELECT o.id,o.label,f.field_type,o.field_id,f.source FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.organization_id=$1 AND o.archived_at IS NULL AND f.archived_at IS NULL AND o.id>$2 AND ($3::uuid IS NULL OR o.field_id=$3) ORDER BY o.id LIMIT $4").bind(s.org.0).bind(after).bind(q.field_id).bind(n+1).fetch_all(&mut *tx).await?};
    let mut page = Page::new();
    let mut last = None;
    for r in rows.iter().take(n as usize) {
        let id: Uuid = r.get("id");
        if !page.push(json!({"id":id,"kind":kind,"field_id":r.get::<Option<Uuid>,_>("field_id"),"label":r.get::<String,_>("label"),"field_type":r.get::<Option<String>,_>("field_type"),"source_bound":r.get::<Option<String>,_>("source").is_some(),"archived":false}))?{break;}
        last = Some(id);
    }
    page.finish(key, s, &p, rows.len(), last)
}
pub async fn aliases(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    plan: Uuid,
    mapping: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(&q, &[], &[], false)?;
    let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
    let m = mapping_row(&mut tx, s, mapping).await?;
    let original_mapping = m
        .get::<Option<Uuid>, _>("source_mapping_id")
        .unwrap_or(mapping);
    let p = purpose("aliases", s, &q, Some(mapping), None)?;
    let after: Uuid = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT id,source_row_id,ordinal FROM migration_admitted_metadata_alias WHERE plan_id=$1 AND organization_id=$2 AND mapping_id=$3 AND id>$4 ORDER BY id LIMIT $5").bind(s.evidence_plan).bind(s.org.0).bind(original_mapping).bind(after).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for r in rows.iter().take(n as usize) {
        let raw=sqlx::query("SELECT * FROM migration_admitted_metadata_source WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(r.get::<Uuid,_>("source_row_id")).bind(s.evidence_plan).bind(s.org.0).fetch_one(&mut *tx).await?;
        let f: FrozenSource = open(
            key,
            Scope {
                plan: s.evidence_plan,
                ..s
            },
            &raw,
            "source",
        )?;
        let ordinal: i32 = r.get("ordinal");
        let Entity::Person(person) = f.record.entity else {
            return Err(MigrationError::Crypto);
        };
        let tag = person
            .tags
            .iter()
            .find(|v| i64::from(v.ordinal) == i64::from(ordinal))
            .ok_or(MigrationError::Crypto)?;
        let source = BTreeMap::from([("tag".into(), tag.raw.clone().unwrap_or_default())]);
        if !page.push(json!({"source_id":raw.get::<String,_>("source_id"),"ordinal":ordinal.to_string(),"record_id":raw.get::<Uuid,_>("id"),"source":model::summary("source",&source)}))?{break;}
        last = Some(r.get::<Uuid, _>("id"));
    }
    page.finish(key, s, &p, rows.len(), last)
}
async fn manifest(c: &mut PgConnection, s: Scope, id: Uuid) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_admitted_metadata_manifest WHERE id=$1 AND import_id=$2 AND plan_id=$3 AND organization_id=$4").bind(id).bind(s.root).bind(s.plan).bind(s.org.0).fetch_optional(c).await?.ok_or(MigrationError::NotFound)
}
async fn operations(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    manifest: Uuid,
) -> Result<Value, MigrationError> {
    let oversized:bool=sqlx::query_scalar("SELECT oversized FROM migration_admitted_metadata_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(manifest).bind(s.plan).bind(s.org.0).fetch_one(&mut *c).await?;
    if oversized {
        return Ok(
            json!({"groups":super::fidelity::groups(c,s.org,s.plan,manifest).await?,"hold_reason":"import_item_byte_limit","details":"Operation evidence is retained; this Person exceeds the supported unit reader. Inspect the retained source and mapping evidence."}),
        );
    }
    let total:i64=sqlx::query_scalar("SELECT COALESCE(sum(octet_length(nonce)+octet_length(ciphertext)),0)::bigint FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3").bind(manifest).bind(s.plan).bind(s.org.0).fetch_one(&mut *c).await?;
    if total > metadata_store::UNIT {
        return Err(MigrationError::StorageLimit);
    }
    let mut output = Vec::new();
    let mut after = Uuid::nil();
    let mut used = 0;
    loop {
        let rows=sqlx::query("SELECT * FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3 AND id>$4 ORDER BY id LIMIT 50").bind(manifest).bind(s.plan).bind(s.org.0).bind(after).fetch_all(&mut *c).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let f: FrozenOperation = open(key, s, &row, "operation")?;
            let id: Uuid = row.get("id");
            let value = json!({"id":id,"kind":row.get::<String,_>("kind"),"mapping_id":row.get::<Option<Uuid>,_>("mapping_id"),"target_id":row.get::<Option<Uuid>,_>("target_id"),"source_id":f.source_id,"source_field":f.source_field,"source_tag":f.source_tag,"disposition":row.get::<String,_>("disposition"),"value":f.value,"reasons":f.reasons});
            used += metadata_store::bytes(&value)?.len();
            if used > metadata_store::UNIT as usize {
                return Err(MigrationError::StorageLimit);
            }
            output.push(value);
            after = id;
        }
    }
    Ok(json!(output))
}
fn operation_counts(operations: &Value, result: bool) -> Result<model::Counts, MigrationError> {
    if let Some(groups) = operations.get("groups") {
        return super::fidelity::counts(groups, result);
    }
    let mut counts = model::Counts::default();
    for op in operations.as_array().ok_or(MigrationError::Crypto)? {
        let kind = op["kind"].as_str().ok_or(MigrationError::Crypto)?;
        let disposition = op[if result { "outcome" } else { "disposition" }]
            .as_str()
            .ok_or(MigrationError::Crypto)?;
        if result {
            counts.outcome(kind, disposition)
        } else {
            counts.planned(kind, disposition)
        }
    }
    Ok(counts)
}
async fn record_data(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
) -> Result<(BTreeMap<String, String>, Value, Vec<String>), MigrationError> {
    let source = source(c, key, s, "people", &r.get::<String, _>("source_person_id")).await?;
    let operations = operations(c, key, s, r.get("id")).await?;
    let baseline: Value = super::super::admitted_metadata_worker::open(
        key,
        s.org,
        s.snapshot,
        s.plan,
        r.get("id"),
        "baseline",
        &r.get::<Vec<u8>, _>("baseline_nonce"),
        &r.get::<Vec<u8>, _>("baseline_ciphertext"),
    )?;
    let mut reasons = baseline
        .get("reasons")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if r.get::<String, _>("disposition") == "held" && reasons.is_empty() {
        reasons.push("source_evidence_unavailable".into());
    }
    Ok((source, operations, reasons))
}
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(
        &q,
        &[],
        &["eligible", "held", "settled", "cancelled"],
        false,
    )?;
    let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
    let p = purpose("records", s, &q, None, None)?;
    let after: Uuid = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_admitted_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND id>$3 AND ($4::text IS NULL OR disposition=$4) ORDER BY id LIMIT $5").bind(plan).bind(s.org.0).bind(after).bind(&q.disposition).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for id in ids.iter().take(n as usize) {
        let r = manifest(&mut tx, s, *id).await?;
        let (source, operations, reasons) = record_data(&mut tx, key, s, &r).await?;
        let value = json!({"id":id,"source_id":r.get::<String,_>("source_person_id"),"person_id":r.get::<Option<Uuid>,_>("person_id"),"disposition":r.get::<String,_>("disposition"),"reasons":reasons,"counts":operation_counts(&operations,false)?.wire(),"added_byte_bound":r.get::<i64,_>("item_byte_bound").to_string(),"source":model::summary("source",&source),"operations":model::summary("operations",&model::operation_fields(&operations))});
        if !page.push(value)? {
            break;
        }
        last = Some(*id);
    }
    page.finish(key, s, &p, ids.len(), last)
}
async fn result_row(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
    id: Uuid,
) -> Result<PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_admitted_metadata_result WHERE id=$1 AND import_id=$2 AND organization_id=$3").bind(id).bind(root).bind(org.0).fetch_optional(c).await?.ok_or(MigrationError::NotFound)
}
async fn result_data(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
) -> Result<(BTreeMap<String, String>, Value), MigrationError> {
    let data: Value = open(key, s, r, "result")?;
    let source = if r.get::<String, _>("kind") == "people" {
        source(
            c,
            key,
            s,
            "people",
            data["source_person_id"]
                .as_str()
                .ok_or(MigrationError::Crypto)?,
        )
        .await?
    } else {
        let mapping = mapping_row(c, s, r.get("unit_id")).await?;
        mapping_source(c, key, s, &mapping).await?.1
    };
    Ok((source, data))
}
async fn result_view(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    s: Scope,
    r: &PgRow,
) -> Result<Value, MigrationError> {
    let (source, data) = result_data(c, key, s, r).await?;
    let kind: String = r.get("kind");
    let disposition: String = r.get("disposition");
    let mut counts = if kind == "people" {
        if let Some(groups) = data.get("groups") {
            super::fidelity::counts(groups, true)?
        } else {
            operation_counts(&data["operations"], true)?
        }
    } else {
        let mut c = model::Counts::default();
        c.outcome(&kind, &disposition);
        c
    };
    if kind == "people" {
        counts.people.settled = 1;
    }
    let source_id = if kind == "people" {
        data["source_person_id"].as_str().map(str::to_owned)
    } else {
        Some(
            mapping_row(c, s, r.get("unit_id"))
                .await?
                .get::<String, _>("source_id"),
        )
    };
    let mut reasons = std::collections::BTreeSet::new();
    if let Some(reason) = data["reason"].as_str() {
        reasons.insert(reason.to_owned());
    }
    if let Some(ops) = data["operations"].as_array() {
        for op in ops {
            if let Some(reason) = op["reason"].as_str() {
                reasons.insert(reason.to_owned());
            }
        }
    }
    if disposition == "held" && reasons.is_empty() {
        reasons.insert("held_at_commit".into());
    }
    Ok(
        json!({"id":r.get::<Uuid,_>("id"),"kind":kind,"source_id":source_id,"person_id":r.get::<Option<Uuid>,_>("person_id"),"mapping_id":if kind=="people"{None}else{Some(r.get::<Uuid,_>("unit_id"))},"record_id":r.get::<Option<Uuid>,_>("manifest_id"),"disposition":disposition,"committed_at":r.get::<chrono::DateTime<chrono::Utc>,_>("committed_at"),"counts":counts.wire(),"reasons":reasons,"source":model::summary("source",&source),"operations":model::summary("operations",&model::operation_fields(&data)),"admission_result_id":data.get("admission_result_id"),"admitted_metadata_import_id":s.root,"plan_id":s.plan}),
    )
}
async fn active_scope(
    c: &mut PgConnection,
    org: OrganizationId,
    root: Uuid,
) -> Result<Scope, MigrationError> {
    let plan:Uuid=sqlx::query_scalar("SELECT COALESCE(confirmed_plan_id,latest_plan_id) FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).fetch_optional(&mut *c).await?.ok_or(MigrationError::NotFound)?;
    scope(c, org, root, plan).await
}
pub async fn results(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(
        &q,
        &["tag", "field", "option", "people"],
        &[
            "created",
            "applied",
            "already_present",
            "held",
            "source_null",
            "not_supplied",
        ],
        false,
    )?;
    let s = active_scope(&mut tx, ctx.organization_id, root).await?;
    let p = purpose("results", s, &q, None, None)?;
    let after: Uuid = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_admitted_metadata_result WHERE import_id=$1 AND organization_id=$2 AND id>$3 AND ($4::text IS NULL OR kind=$4) AND ($5::text IS NULL OR disposition=$5) ORDER BY id LIMIT $6").bind(root).bind(s.org.0).bind(after).bind(&q.kind).bind(&q.disposition).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for id in ids.iter().take(n as usize) {
        let r = result_row(&mut tx, s.org, root, *id).await?;
        if r.get::<Uuid, _>("plan_id") != s.plan {
            return Err(MigrationError::Crypto);
        }
        if !page.push(result_view(&mut tx, key, s, &r).await?)? {
            break;
        }
        last = Some(*id);
    }
    page.finish(key, s, &p, ids.len(), last)
}
pub async fn issues(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    root: Uuid,
    plan: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(&q, &[], &[], false)?;
    let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
    let p = purpose("issues", s, &q, None, None)?;
    let after: String = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or_default();
    let rows=sqlx::query("SELECT code,count FROM migration_admitted_metadata_issue WHERE plan_id=$1 AND organization_id=$2 AND code>$3 ORDER BY code LIMIT $4").bind(plan).bind(s.org.0).bind(after).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for row in rows.iter().take(n as usize) {
        let code: String = row.get("code");
        if !page.push(json!({"code":code,"count":row.get::<i64,_>("count").to_string()}))? {
            break;
        }
        last = Some(code);
    }
    page.finish(key, s, &p, rows.len(), last)
}
async fn person(c: &mut PgConnection, org: OrganizationId, id: Uuid) -> Result<(), MigrationError> {
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM person WHERE id=$1 AND organization_id=$2)",
    )
    .bind(id)
    .bind(org.0)
    .fetch_one(c)
    .await?
    {
        Ok(())
    } else {
        Err(MigrationError::NotFound)
    }
}
pub async fn provenance(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    q: MetadataPage,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let n = limit(&q, &[], &[], false)?;
    person(&mut tx, ctx.organization_id, id).await?;
    let envelope = Scope {
        org: ctx.organization_id,
        root: id,
        plan: Uuid::nil(),
        snapshot: Uuid::nil(),
        revision: 0,
        generation: Uuid::nil(),
        evidence_plan: Uuid::nil(),
    };
    let p = purpose("provenance", envelope, &q, Some(id), None)?;
    let after: Uuid = cursor(key, envelope, &p, q.cursor.as_deref())?.unwrap_or(Uuid::nil());
    let rows=sqlx::query("SELECT id,import_id,plan_id FROM migration_admitted_metadata_result WHERE person_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT $4").bind(id).bind(ctx.organization_id.0).bind(after).bind(n+1).fetch_all(&mut *tx).await?;
    let mut page = Page::new();
    let mut last = None;
    for row in rows.iter().take(n as usize) {
        let s = scope(
            &mut tx,
            ctx.organization_id,
            row.get("import_id"),
            row.get("plan_id"),
        )
        .await?;
        let r = result_row(&mut tx, s.org, s.root, row.get("id")).await?;
        if r.get::<Option<Uuid>, _>("person_id") != Some(id) {
            return Err(MigrationError::NotFound);
        }
        if !page.push(result_view(&mut tx, key, s, &r).await?)? {
            break;
        }
        last = Some(row.get::<Uuid, _>("id"));
    }
    page.finish(key, envelope, &p, rows.len(), last)
}
#[derive(Clone, Copy)]
pub enum FieldOwner {
    Mapping { root: Uuid, plan: Uuid, id: Uuid },
    Record { root: Uuid, plan: Uuid, id: Uuid },
    Result { root: Uuid, id: Uuid },
    Provenance { person: Uuid, id: Uuid },
}
pub async fn field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    owner: FieldOwner,
    field: &str,
    q: FieldQuery,
) -> Result<Value, MigrationError> {
    if field.len() > 128 {
        return Err(MigrationError::InvalidInput);
    }
    let n = q.limit.unwrap_or(65536) as usize;
    if !(4..=65536).contains(&n) {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let (s, id, kind, source, operations) = match owner {
        FieldOwner::Mapping { root, plan, id } => {
            let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
            let r = mapping_row(&mut tx, s, id).await?;
            let (_, source) = mapping_source(&mut tx, key, s, &r).await?;
            (s, id, "mapping", source, Value::Null)
        }
        FieldOwner::Record { root, plan, id } => {
            let s = scope(&mut tx, ctx.organization_id, root, plan).await?;
            let r = manifest(&mut tx, s, id).await?;
            let (source, operations, _) = record_data(&mut tx, key, s, &r).await?;
            (s, id, "record", source, operations)
        }
        FieldOwner::Result { root, id } => {
            let r = result_row(&mut tx, ctx.organization_id, root, id).await?;
            let s = scope(&mut tx, ctx.organization_id, root, r.get("plan_id")).await?;
            let (source, operations) = result_data(&mut tx, key, s, &r).await?;
            (s, id, "result", source, operations)
        }
        FieldOwner::Provenance {
            person: id_person,
            id,
        } => {
            person(&mut tx, ctx.organization_id, id_person).await?;
            let r=sqlx::query("SELECT * FROM migration_admitted_metadata_result WHERE id=$1 AND person_id=$2 AND organization_id=$3").bind(id).bind(id_person).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
            let s = scope(
                &mut tx,
                ctx.organization_id,
                r.get("import_id"),
                r.get("plan_id"),
            )
            .await?;
            let (source, operations) = result_data(&mut tx, key, s, &r).await?;
            (s, id, "provenance", source, operations)
        }
    };
    let p = format!(
        "{}:bytes:{n}",
        purpose(
            &format!("{kind}-field"),
            s,
            &MetadataPage::default(),
            Some(id),
            Some(field)
        )?
    );
    let offset: usize = cursor(key, s, &p, q.cursor.as_deref())?.unwrap_or(0);
    let text = model::field_text(&source, &operations, field).ok_or(MigrationError::NotFound)?;
    let (part, end) = model::segment(&text, offset, n)?;
    let next = if end < text.len() {
        Some(snapshot::encode_cursor(
            key,
            s.org,
            s.root,
            &p,
            &json!(end),
        )?)
    } else {
        None
    };
    Ok(
        json!({"text":part,"full_utf8_bytes":text.len().to_string(),"offset_bytes":offset.to_string(),"next_cursor":next,"complete":end==text.len()}),
    )
}
