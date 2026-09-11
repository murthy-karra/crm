//! DB-only comparison over immutable source sequence and encrypted destination inputs.
use super::{
    crypto,
    snapshot::{self, PageQuery, SnapshotPolicy, SnapshotRequest},
    store, MigrationError,
};
use crate::{
    config::RawPayloadKey,
    domain::envelope::CommandContext,
    ids::{OrganizationId, UserId},
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
async fn destination(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Value, MigrationError> {
    // A single statement gives one MVCC observation for all destination inputs.
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('stages',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',id,'name',name) ORDER BY id) FROM stage WHERE organization_id=$1),'[]'::jsonb),'members',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',u.id,'email',u.email,'name',u.display_name) ORDER BY u.id) FROM app_user u JOIN organization_membership m ON m.user_id=u.id WHERE m.organization_id=$1 AND m.status='active'),'[]'::jsonb),'custom_fields',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',f.id,'label',f.label,'field_type',f.field_type,'source',f.source,'external_key',f.external_key,'archived',f.archived_at IS NOT NULL,'options',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',o.id,'label',o.label,'archived',o.archived_at IS NOT NULL) ORDER BY o.id) FROM custom_field_option o WHERE o.organization_id=$1 AND o.field_id=f.id),'[]'::jsonb)) ORDER BY f.id) FROM custom_field f WHERE f.organization_id=$1),'[]'::jsonb),'person_present',EXISTS(SELECT 1 FROM person WHERE organization_id=$1))").bind(org.0).fetch_one(conn).await?)
}
fn bytes(v: &Value) -> Result<Vec<u8>, MigrationError> {
    serde_json::to_vec(v).map_err(|_| MigrationError::Crypto)
}
fn fingerprint(
    key: &RawPayloadKey,
    org: OrganizationId,
    destination: &Value,
) -> Result<[u8; 32], MigrationError> {
    Ok(crypto::snapshot_hmac(
        key,
        org,
        "preview_destination",
        &bytes(destination)?,
    ))
}
async fn preview_row(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    sqlx::query("SELECT * FROM migration_snapshot_preview WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3 FOR UPDATE").bind(id).bind(run).bind(org.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)
}
fn input(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    id: Uuid,
    r: &sqlx::postgres::PgRow,
) -> Result<Value, MigrationError> {
    serde_json::from_slice(
        &crypto::open_snapshot(
            key,
            org,
            run,
            id,
            "preview_input",
            r.get("input_nonce"),
            r.get("input_ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?,
    )
    .map_err(|_| MigrationError::Crypto)
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="generate"))]
pub async fn generate(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    run: Uuid,
    cmd: SnapshotRequest,
) -> Result<Value, MigrationError> {
    let mut tx = snapshot::begin(pool, ctx).await?;
    let r = snapshot::row(&mut tx, ctx.organization_id, run).await?;
    let digest = crypto::request_digest(
        key,
        "generate_snapshot_preview",
        &bytes(&json!([run, ctx.actor_user_id.0]))?,
    );
    if let Some(v) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "generate_snapshot_preview",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    if !matches!(
        r.get::<String, _>("state").as_str(),
        "completed" | "completed_with_gaps" | "paused" | "cancelled"
    ) || r.get::<i64, _>("accepted_captures") == 0
    {
        return Err(MigrationError::Conflict);
    }
    let destination = destination(&mut tx, ctx.organization_id).await?;
    let streams = snapshot::streams(&mut tx, ctx.organization_id, run).await?;
    let invalid=sqlx::query("SELECT family,count(*) AS count FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND source_id IS NULL GROUP BY family").bind(run).bind(ctx.organization_id.0).bind(r.get::<i64,_>("capture_sequence")).fetch_all(&mut *tx).await?;
    let invalid:Vec<Value>=invalid.into_iter().map(|r|json!({"family":r.get::<String,_>("family"),"count":r.get::<i64,_>("count").to_string()})).collect();
    let input = bytes(
        &json!({"destination":destination,"coverage":snapshot::coverage(&streams),"streams":streams,"invalid_ids":invalid,"source_scope":"Credential-visible core records; observation over time; first import requires a new empty Organization."}),
    )?;
    if input.len() > snapshot::PREVIEW_RESERVATION as usize - 1024 {
        return Err(MigrationError::Conflict);
    }
    let id = Uuid::new_v4();
    let sealed = crypto::seal_snapshot(key, ctx.organization_id, run, id, "preview_input", &input)
        .map_err(|_| MigrationError::Crypto)?;
    let fp = fingerprint(key, ctx.organization_id, &destination)?;
    // The input row and its exact reservation commit together; there is never a
    // queued report with missing inputs. FK permits reservation after row insertion.
    sqlx::query("INSERT INTO migration_snapshot_preview(id,snapshot_id,organization_id,requested_by_user_id,engine_version,input_version,capture_sequence,input_nonce,input_ciphertext,destination_fingerprint,state) VALUES($1,$2,$3,$4,$5,'1',$6,$7,$8,$9,'queued')").bind(id).bind(run).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(snapshot::ENGINE).bind(r.get::<i64,_>("capture_sequence")).bind(sealed.nonce.as_slice()).bind(&sealed.ciphertext).bind(fp.as_slice()).execute(&mut *tx).await?;
    let token = Uuid::new_v4();
    let actual = sealed.ciphertext.len() as i64 + 24 + 32;
    if !snapshot::reserve(
        &mut tx,
        policy,
        ctx.organization_id,
        run,
        Some(id),
        token,
        actual,
    )
    .await?
    {
        return Err(MigrationError::Conflict);
    }
    snapshot::release(&mut tx, ctx.organization_id, run, token, actual).await?;
    let result = json!({"preview_id":id,"state":"queued"});
    snapshot::save(
        &mut tx,
        key,
        ctx,
        "generate_snapshot_preview",
        cmd.request_id,
        &digest,
        &result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, actor_id=%ctx.actor_user_id.0, operation="retry"))]
pub async fn retry(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
    cmd: SnapshotRequest,
) -> Result<Value, MigrationError> {
    let mut tx = snapshot::begin(pool, ctx).await?;
    snapshot::row(&mut tx, ctx.organization_id, run).await?;
    let r = preview_row(&mut tx, ctx.organization_id, run, id).await?;
    let digest = crypto::request_digest(
        key,
        "retry_snapshot_preview",
        &bytes(&json!([run, id, ctx.actor_user_id.0]))?,
    );
    if let Some(v) = store::receipt(
        key,
        &mut tx,
        ctx.organization_id,
        "retry_snapshot_preview",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    if r.get::<String, _>("state") != "paused"
        || r.get::<String, _>("engine_version") != snapshot::ENGINE
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("UPDATE migration_snapshot_preview SET requested_by_user_id=$4,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    reclaim(&mut tx, ctx.organization_id, run, id).await?;
    let token = Uuid::new_v4();
    if !snapshot::reserve(
        &mut tx,
        policy,
        ctx.organization_id,
        run,
        Some(id),
        token,
        snapshot::PREVIEW_RESERVATION,
    )
    .await?
    {
        return Err(MigrationError::Conflict);
    }
    snapshot::release(&mut tx, ctx.organization_id, run, token, 0).await?;
    sqlx::query("UPDATE migration_snapshot_preview SET state='queued',pause_reason=NULL WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let result = json!({"preview_id":id,"state":"queued"});
    snapshot::save(
        &mut tx,
        key,
        ctx,
        "retry_snapshot_preview",
        cmd.request_id,
        &digest,
        &result,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
async fn reclaim(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    id: Uuid,
) -> Result<(), MigrationError> {
    let tokens=sqlx::query_scalar::<_,Uuid>("SELECT token FROM migration_snapshot_reservation WHERE snapshot_id=$1 AND organization_id=$2 AND preview_id=$3").bind(run).bind(org.0).bind(id).fetch_all(&mut *conn).await?;
    for token in tokens {
        snapshot::release(conn, org, run, token, 0).await?
    }
    Ok(())
}
async fn pause(
    conn: &mut PgConnection,
    org: OrganizationId,
    run: Uuid,
    id: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_snapshot_preview SET state='paused',pause_reason=$4,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).bind(reason).execute(conn).await?;
    Ok(())
}
/// One bounded DB-only batch. A separate claim transaction makes expired worker
/// recovery explicit; comparison and page publication share the fenced commit.
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT id,snapshot_id,organization_id FROM migration_snapshot_preview WHERE state='queued' OR (state='running' AND lease_expires_at<=now()) ORDER BY created_at LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    let id: Uuid = candidate.get("id");
    let run: Uuid = candidate.get("snapshot_id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let mut tx = pool.begin().await?;
    store::lock_org(&mut tx, org).await?;
    snapshot::row(&mut tx, org, run).await?;
    let p = preview_row(&mut tx, org, run, id).await?;
    if p.get::<String, _>("state") != "queued"
        && !(p.get::<String, _>("state") == "running"
            && p.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
                .is_some_and(|v| v <= Utc::now()))
    {
        return Ok(false);
    }
    let actor = UserId::new(p.get("requested_by_user_id"));
    let authorized=sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' AND role='admin' FOR SHARE").bind(org.0).bind(actor.0).fetch_optional(&mut *tx).await?.is_some();
    sqlx::query("UPDATE migration_snapshot_preview SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).execute(&mut *tx).await?;
    reclaim(&mut tx, org, run, id).await?;
    if !authorized
        || p.get::<String, _>("engine_version") != snapshot::ENGINE
        || p.get::<String, _>("input_version") != "1"
    {
        pause(
            &mut tx,
            org,
            run,
            id,
            if !authorized {
                "requester_not_authorized"
            } else {
                "comparison_version_unavailable"
            },
        )
        .await?;
        tx.commit().await?;
        return Ok(true);
    }
    let token = Uuid::new_v4();
    if !snapshot::reserve(
        &mut tx,
        policy,
        org,
        run,
        Some(id),
        token,
        snapshot::PREVIEW_RESERVATION,
    )
    .await?
    {
        pause(&mut tx, org, run, id, "storage_budget_exhausted").await?;
        tx.commit().await?;
        return Ok(true);
    }
    sqlx::query("UPDATE migration_snapshot_preview SET state='running',lease_token=$4,lease_expires_at=now()+interval '60 seconds' WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).bind(token).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    store::lock_org(&mut tx, org).await?;
    snapshot::row(&mut tx, org, run).await?;
    let p = preview_row(&mut tx, org, run, id).await?;
    if p.get::<Option<Uuid>, _>("lease_token") != Some(token)
        || p.get::<String, _>("state") != "running"
        || p.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
            .is_none_or(|v| v <= Utc::now())
    {
        return Ok(true);
    }
    let authorized=sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' AND role='admin' FOR SHARE").bind(org.0).bind(actor.0).fetch_optional(&mut *tx).await?.is_some();
    if !authorized {
        pause(&mut tx, org, run, id, "requester_not_authorized").await?;
        snapshot::release(&mut tx, org, run, token, 0).await?;
        tx.commit().await?;
        return Ok(true);
    }
    let inputs = match input(key, org, run, id, &p) {
        Ok(value) => value,
        Err(_) => {
            pause(&mut tx, org, run, id, "preview_input_unreadable").await?;
            snapshot::release(&mut tx, org, run, token, 0).await?;
            tx.commit().await?;
            return Ok(true);
        }
    };
    let boundary = p.get::<i64, _>("capture_sequence");
    let mut actual = 0_i64;
    let mut complete = false;
    if !p.get::<bool, _>("groups_built") {
        let kind: String = p.get("checkpoint_group_kind");
        let hash: Vec<u8> = p.get("checkpoint_group_hash");
        let groups=sqlx::query("SELECT kind,key_hmac,count(DISTINCT source_id) AS member_count FROM migration_snapshot_contact_key WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND (kind,key_hmac)>($4,$5) GROUP BY kind,key_hmac HAVING count(DISTINCT source_id)>1 ORDER BY kind,key_hmac LIMIT 50").bind(run).bind(org.0).bind(boundary).bind(&kind).bind(&hash).fetch_all(&mut *tx).await?;
        for g in &groups {
            let kind: String = g.get("kind");
            let hash: Vec<u8> = g.get("key_hmac");
            sqlx::query("INSERT INTO migration_snapshot_preview_group(id,preview_id,snapshot_id,organization_id,kind,key_hmac,member_count) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(id).bind(run).bind(org.0).bind(&kind).bind(&hash).bind(g.get::<i64,_>("member_count")).execute(&mut *tx).await?;
            actual += (kind.len() + hash.len()) as i64;
        }
        if let Some(last) = groups.last() {
            sqlx::query("UPDATE migration_snapshot_preview SET checkpoint_group_kind=$4,checkpoint_group_hash=$5 WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).bind(last.get::<String,_>("kind")).bind(last.get::<Vec<u8>,_>("key_hmac")).execute(&mut *tx).await?;
            actual += (last.get::<String, _>("kind").len()
                + last.get::<Vec<u8>, _>("key_hmac").len()) as i64
                - (kind.len() + hash.len()) as i64;
        }
        if groups.len() < 50 {
            sqlx::query("UPDATE migration_snapshot_preview SET groups_built=true WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).execute(&mut *tx).await?;
        }
    } else {
        let rows=sqlx::query("SELECT family,source_id FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND source_id IS NOT NULL AND (family,source_id)>($4,$5) GROUP BY family,source_id ORDER BY family,source_id LIMIT 50").bind(run).bind(org.0).bind(boundary).bind(p.get::<String,_>("checkpoint_family")).bind(p.get::<String,_>("checkpoint_source_id")).fetch_all(&mut *tx).await?;
        for r in &rows {
            let family: String = r.get("family");
            let source: String = r.get("source_id");
            let (projection, mut issues) =
                record_projection(&mut tx, key, org, run, boundary, &family, &source).await?;
            let groups = if family == "people" {
                sqlx::query_scalar::<_,i64>("SELECT count(DISTINCT g.id) FROM migration_snapshot_contact_key k JOIN migration_snapshot_preview_group g ON g.snapshot_id=k.snapshot_id AND g.organization_id=k.organization_id AND g.kind=k.kind AND g.key_hmac=k.key_hmac AND g.preview_id=$4 WHERE k.snapshot_id=$1 AND k.organization_id=$2 AND k.source_id=$5 AND k.capture_sequence<=$3").bind(run).bind(org.0).bind(boundary).bind(id).bind(&source).fetch_one(&mut *tx).await?
            } else {
                0
            };
            if groups > 0 {
                issues.push("normalized_contact_overlap".into())
            }
            super::snapshot_compare::compare(
                &family,
                &projection,
                &inputs["destination"],
                &mut issues,
            );
            if matches!(family.as_str(), "notes" | "tasks" | "people") {
                for field in ["assignedUserId", "createdById", "userId"] {
                    if let Some(source_user) =
                        super::snapshot_compare::source_identifier(projection.get(field))
                    {
                        if !source_user_matches(
                            &mut tx,
                            key,
                            org,
                            run,
                            boundary,
                            &source_user,
                            &inputs["destination"],
                        )
                        .await?
                        {
                            issues.push("unresolved_source_user".into())
                        }
                    }
                }
            }
            if matches!(family.as_str(), "notes" | "tasks") {
                let person = super::snapshot_compare::source_identifier(projection.get("personId"));
                let exists = if let Some(person) = person {
                    sqlx::query("SELECT 1 FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND family='people' AND source_id=$4 LIMIT 1").bind(run).bind(org.0).bind(boundary).bind(person).fetch_optional(&mut *tx).await?.is_some()
                } else {
                    false
                };
                if !exists {
                    issues.push("unresolved_source_person".into());
                }
            }
            issues.sort();
            issues.dedup();
            for issue in &issues {
                sqlx::query("INSERT INTO migration_snapshot_preview_issue(preview_id,snapshot_id,organization_id,family,issue_code,record_count) VALUES($1,$2,$3,$4,$5,1) ON CONFLICT(preview_id,organization_id,family,issue_code) DO UPDATE SET record_count=migration_snapshot_preview_issue.record_count+1").bind(id).bind(run).bind(org.0).bind(&family).bind(issue).execute(&mut *tx).await?;
            }
            let disposition = super::snapshot_compare::disposition(&issues);
            let record_id = Uuid::new_v4();
            let candidates =
                super::snapshot_compare::candidates(&family, &projection, &inputs["destination"]);
            let body = json!({"issues":issues,"projection":projection,"candidates":candidates});
            let plaintext = bytes(&body)?;
            if plaintext.len() > 36 * 1024 {
                return Err(MigrationError::Conflict);
            }
            let sealed =
                crypto::seal_snapshot(key, org, run, record_id, "preview_record", &plaintext)
                    .map_err(|_| MigrationError::Crypto)?;
            actual += (24 + sealed.ciphertext.len() + source.len()) as i64;
            sqlx::query("INSERT INTO migration_snapshot_preview_record(id,preview_id,snapshot_id,organization_id,family,source_id,disposition,nonce,ciphertext,overlap_group_count) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(record_id).bind(id).bind(run).bind(org.0).bind(&family).bind(&source).bind(disposition).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(groups).execute(&mut *tx).await?;
        }
        if let Some(last) = rows.last() {
            let old = p.get::<String, _>("checkpoint_source_id");
            let source = last.get::<String, _>("source_id");
            actual += source.len() as i64 - old.len() as i64;
            sqlx::query("UPDATE migration_snapshot_preview SET checkpoint_family=$4,checkpoint_source_id=$5 WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).bind(last.get::<String,_>("family")).bind(source).execute(&mut *tx).await?;
        }
        complete = rows.len() < 50;
    }
    let valid=sqlx::query("SELECT 1 FROM migration_snapshot_reservation WHERE token=$1 AND snapshot_id=$2 AND organization_id=$3 AND expires_at>now()").bind(token).bind(run).bind(org.0).fetch_optional(&mut *tx).await?.is_some();
    if !valid {
        return Err(MigrationError::Conflict);
    }
    snapshot::release(&mut tx, org, run, token, actual).await?;
    sqlx::query("UPDATE migration_snapshot_preview SET state=$4,completed_at=CASE WHEN $4='completed' THEN now() ELSE NULL END,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3").bind(id).bind(run).bind(org.0).bind(if complete{"completed"}else{"queued"}).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
async fn record_projection(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    boundary: i64,
    family: &str,
    source: &str,
) -> Result<(Value, Vec<String>), MigrationError> {
    let counts=sqlx::query("SELECT representation,count(DISTINCT semantic_hmac) AS variants,bool_or(content_gap) AS gap FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND family=$4 AND source_id=$5 GROUP BY representation ORDER BY representation").bind(run).bind(org.0).bind(boundary).bind(family).bind(source).fetch_all(&mut *conn).await?;
    let mut issues = Vec::new();
    if counts.iter().any(|r| r.get::<i64, _>("variants") > 1) {
        issues.push("comparable_source_variants".into())
    }
    if counts.iter().any(|r| r.get::<bool, _>("gap")) {
        issues.push("content_gap".into())
    }
    let rows=sqlx::query("SELECT * FROM (SELECT DISTINCT ON (representation,semantic_hmac) id,projection_nonce,projection_ciphertext,representation,semantic_hmac FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND capture_sequence<=$3 AND family=$4 AND source_id=$5 ORDER BY representation,semantic_hmac,capture_sequence,id) r ORDER BY CASE WHEN representation LIKE '%detail%' THEN 0 ELSE 1 END,representation,semantic_hmac LIMIT 2").bind(run).bind(org.0).bind(boundary).bind(family).bind(source).fetch_all(&mut *conn).await?;
    let mut projections = Vec::new();
    for r in &rows {
        let raw = crypto::open_snapshot(
            key,
            org,
            run,
            r.get("id"),
            "record",
            r.get("projection_nonce"),
            r.get("projection_ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        let value: Value = serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)?;
        projections.push(value);
    }
    if family == "notes"
        && !counts
            .iter()
            .any(|r| r.get::<String, _>("representation").contains("detail"))
    {
        issues.push("note_detail_unavailable".into())
    }
    let mut projection = projections.first().cloned().unwrap_or_else(|| json!({}));
    if counts.iter().any(|r| r.get::<i64, _>("variants") > 1) {
        projection = json!({"variants":projections,"variant_summary":counts.iter().map(|r|json!({"representation":r.get::<String,_>("representation"),"count":r.get::<i64,_>("variants").to_string()})).collect::<Vec<_>>(),"review_requires_decision":true,"display_limited":true})
    }
    Ok((projection, issues))
}
async fn source_user_matches(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    boundary: i64,
    source: &str,
    destination: &Value,
) -> Result<bool, MigrationError> {
    let (projection, issues) =
        record_projection(conn, key, org, run, boundary, "users", source).await?;
    if !issues.is_empty() {
        return Ok(false);
    }
    let Some(email) = projection
        .get("email")
        .and_then(Value::as_str)
        .and_then(crate::domain::contact::normalize_email)
    else {
        return Ok(false);
    };
    Ok(destination["members"].as_array().is_some_and(|members| {
        members
            .iter()
            .filter(|m| {
                m["email"]
                    .as_str()
                    .and_then(crate::domain::contact::normalize_email)
                    .as_ref()
                    == Some(&email)
            })
            .count()
            == 1
    }))
}
pub async fn detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
) -> Result<Value, MigrationError> {
    let mut tx = pool.begin().await?;
    store::require_admin(&mut tx, ctx).await?;
    let p = preview_row(&mut tx, ctx.organization_id, run, id).await?;
    let inputs = input(key, ctx.organization_id, run, id, &p)?;
    let current = destination(&mut tx, ctx.organization_id).await?;
    let stale = fingerprint(key, ctx.organization_id, &current)?.as_slice()
        != p.get::<Vec<u8>, _>("destination_fingerprint");
    let counts=sqlx::query("SELECT family,disposition,count(*) AS count FROM migration_snapshot_preview_record WHERE preview_id=$1 AND snapshot_id=$2 AND organization_id=$3 GROUP BY family,disposition ORDER BY family,disposition").bind(id).bind(run).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    let counts:Vec<Value>=counts.iter().map(|r|json!({"family":r.get::<String,_>("family"),"disposition":r.get::<String,_>("disposition"),"count":r.get::<i64,_>("count").to_string()})).collect();
    let issues=sqlx::query("SELECT family,issue_code,record_count FROM migration_snapshot_preview_issue WHERE preview_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY family,issue_code").bind(id).bind(run).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    let issues:Vec<Value>=issues.iter().map(|r|json!({"family":r.get::<String,_>("family"),"issue":r.get::<String,_>("issue_code"),"count":r.get::<i64,_>("record_count").to_string()})).collect();
    let state: String = p.get("state");
    let result = json!({"preview":{"id":id,"snapshot_id":run,"state":state,"pause_reason":p.get::<Option<String>,_>("pause_reason"),"engine_version":p.get::<String,_>("engine_version"),"capture_sequence":p.get::<i64,_>("capture_sequence").to_string(),"created_at":p.get::<DateTime<Utc>,_>("created_at"),"completed_at":p.get::<Option<DateTime<Utc>>,_>("completed_at"),"input_observed_at":p.get::<DateTime<Utc>,_>("input_observed_at"),"actions":if state=="paused"{vec!["retry"]}else{vec![]}},"coverage":inputs["coverage"],"counts":{"records":counts,"issues":issues,"invalid_ids":inputs["invalid_ids"],"streams":inputs["streams"]},"destination_stale":stale,"first_import_requires_new_empty_organization":true,"destination_has_people":inputs["destination"]["person_present"]});
    tx.commit().await?;
    Ok(result)
}
async fn readable(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
) -> Result<sqlx::postgres::PgRow, MigrationError> {
    store::require_admin(conn, ctx).await?;
    let p = preview_row(conn, ctx.organization_id, run, id).await?;
    if p.get::<String, _>("state") != "completed" {
        return Err(MigrationError::Conflict);
    }
    Ok(p)
}
pub async fn records(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
    q: PageQuery,
) -> Result<Value, MigrationError> {
    let limit = q.limit(50)?;
    let family = q.family.as_deref().ok_or(MigrationError::InvalidInput)?;
    if super::snapshot_source::Family::parse(family).is_none()
        || q.record_id.is_some()
        || q.disposition.as_deref().is_some_and(|d| {
            !matches!(
                d,
                "needs_decision" | "unsupported_value" | "unresolved_reference" | "reviewable"
            )
        })
    {
        return Err(MigrationError::InvalidInput);
    }
    let scope = format!("records:{id}:{family}:{:?}", q.disposition);
    let cursor =
        snapshot::decode_cursor(key, ctx.organization_id, run, &scope, q.cursor.as_deref())?
            .map(serde_json::from_value::<String>)
            .transpose()
            .map_err(|_| MigrationError::InvalidInput)?
            .unwrap_or_default();
    let mut tx = pool.begin().await?;
    readable(&mut tx, ctx, run, id).await?;
    let rows=sqlx::query("SELECT * FROM migration_snapshot_preview_record WHERE preview_id=$1 AND snapshot_id=$2 AND organization_id=$3 AND family=$4 AND ($5::text IS NULL OR disposition=$5) AND source_id>$6 ORDER BY source_id LIMIT $7").bind(id).bind(run).bind(ctx.organization_id.0).bind(family).bind(&q.disposition).bind(cursor).bind(limit+1).fetch_all(&mut *tx).await?;
    let next = if rows.len() > limit as usize {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            run,
            &scope,
            &json!(rows[limit as usize - 1].get::<String, _>("source_id")),
        )?)
    } else {
        None
    };
    let mut records = Vec::new();
    for r in rows.iter().take(limit as usize) {
        let record_id: Uuid = r.get("id");
        let body: Value = serde_json::from_slice(
            &crypto::open_snapshot(
                key,
                ctx.organization_id,
                run,
                record_id,
                "preview_record",
                r.get("nonce"),
                r.get("ciphertext"),
            )
            .map_err(|_| MigrationError::Crypto)?,
        )
        .map_err(|_| MigrationError::Crypto)?;
        records.push(json!({"id":record_id,"family":family,"source_id":r.get::<String,_>("source_id"),"disposition":r.get::<String,_>("disposition"),"issues":body["issues"],"projection":body["projection"],"candidates":body["candidates"],"overlap_group_count":r.get::<i64,_>("overlap_group_count").to_string()}));
    }
    tx.commit().await?;
    Ok(json!({"records":records,"next_cursor":next}))
}
pub async fn groups(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
    q: PageQuery,
) -> Result<Value, MigrationError> {
    let limit = q.limit(50)?;
    if q.family.is_some() || q.disposition.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let scope = format!("groups:{id}:{:?}", q.record_id);
    let cursor =
        snapshot::decode_cursor(key, ctx.organization_id, run, &scope, q.cursor.as_deref())?
            .map(serde_json::from_value::<Uuid>)
            .transpose()
            .map_err(|_| MigrationError::InvalidInput)?
            .unwrap_or(Uuid::nil());
    let mut tx = pool.begin().await?;
    let p = readable(&mut tx, ctx, run, id).await?;
    let source = if let Some(record) = q.record_id {
        Some(sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_snapshot_preview_record WHERE id=$1 AND preview_id=$2 AND snapshot_id=$3 AND organization_id=$4 AND family='people'").bind(record).bind(id).bind(run).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?)
    } else {
        None
    };
    let rows=sqlx::query("SELECT g.id,g.kind,g.member_count FROM migration_snapshot_preview_group g WHERE g.preview_id=$1 AND g.snapshot_id=$2 AND g.organization_id=$3 AND g.id>$4 AND ($5::text IS NULL OR EXISTS(SELECT 1 FROM migration_snapshot_contact_key k WHERE k.snapshot_id=$2 AND k.organization_id=$3 AND k.kind=g.kind AND k.key_hmac=g.key_hmac AND k.source_id=$5 AND k.capture_sequence<=$6)) ORDER BY g.id LIMIT $7").bind(id).bind(run).bind(ctx.organization_id.0).bind(cursor).bind(source).bind(p.get::<i64,_>("capture_sequence")).bind(limit+1).fetch_all(&mut *tx).await?;
    let next = if rows.len() > limit as usize {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            run,
            &scope,
            &json!(rows[limit as usize - 1].get::<Uuid, _>("id")),
        )?)
    } else {
        None
    };
    let groups:Vec<Value>=rows.iter().take(limit as usize).map(|r|json!({"id":r.get::<Uuid,_>("id"),"kind":r.get::<String,_>("kind"),"member_count":r.get::<i64,_>("member_count").to_string()})).collect();
    tx.commit().await?;
    Ok(json!({"groups":groups,"next_cursor":next}))
}
pub async fn members(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    run: Uuid,
    id: Uuid,
    group: Uuid,
    q: PageQuery,
) -> Result<Value, MigrationError> {
    let limit = q.limit(50)?;
    if q.family.is_some() || q.disposition.is_some() || q.record_id.is_some() {
        return Err(MigrationError::InvalidInput);
    }
    let scope = format!("members:{id}:{group}");
    let cursor =
        snapshot::decode_cursor(key, ctx.organization_id, run, &scope, q.cursor.as_deref())?
            .map(serde_json::from_value::<String>)
            .transpose()
            .map_err(|_| MigrationError::InvalidInput)?
            .unwrap_or_default();
    let mut tx = pool.begin().await?;
    let p = readable(&mut tx, ctx, run, id).await?;
    let g=sqlx::query("SELECT kind,key_hmac FROM migration_snapshot_preview_group WHERE id=$1 AND preview_id=$2 AND snapshot_id=$3 AND organization_id=$4").bind(group).bind(id).bind(run).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let rows=sqlx::query("SELECT DISTINCT k.source_id,r.id AS record_id FROM migration_snapshot_contact_key k JOIN migration_snapshot_preview_record r ON r.snapshot_id=k.snapshot_id AND r.organization_id=k.organization_id AND r.source_id=k.source_id AND r.family='people' AND r.preview_id=$4 WHERE k.snapshot_id=$1 AND k.organization_id=$2 AND k.capture_sequence<=$3 AND k.kind=$5 AND k.key_hmac=$6 AND k.source_id>$7 AND r.source_id>$7 ORDER BY k.source_id LIMIT $8").bind(run).bind(ctx.organization_id.0).bind(p.get::<i64,_>("capture_sequence")).bind(id).bind(g.get::<String,_>("kind")).bind(g.get::<Vec<u8>,_>("key_hmac")).bind(cursor).bind(limit+1).fetch_all(&mut *tx).await?;
    let next = if rows.len() > limit as usize {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            run,
            &scope,
            &json!(rows[limit as usize - 1].get::<String, _>("source_id")),
        )?)
    } else {
        None
    };
    let members:Vec<Value>=rows.iter().take(limit as usize).map(|r|json!({"source_id":r.get::<String,_>("source_id"),"record_id":r.get::<Uuid,_>("record_id")})).collect();
    tx.commit().await?;
    Ok(json!({"members":members,"next_cursor":next}))
}
