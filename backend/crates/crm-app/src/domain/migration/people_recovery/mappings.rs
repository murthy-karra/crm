use super::super::people_admission_queries as q;
use super::*;
use crate::domain::migration::{
    import_source::{Entity, ExtractedRecord},
    people_mapping_repair as repair,
};
use sha2::{Digest, Sha256};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditRecoveryMapping {
    pub request_id: Uuid,
    pub expected_draft_revision: i64,
    pub key_id: Uuid,
    pub disposition: String,
    pub target_id: Option<Uuid>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SealRecoveryChoices {
    pub request_id: Uuid,
    pub expected_draft_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryMappingPage {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}
fn editable(run: &PgRow, ctx: &CommandContext) -> Result<(), MigrationError> {
    if !is_recovery(run) {
        return Err(MigrationError::NotFound);
    }
    if run.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    if !run.get::<bool, _>("recovery_candidates_complete")
        || run
            .get::<Option<Uuid>, _>("confirmed_admission_plan_id")
            .is_some()
        || !matches!(run.get::<String, _>("state").as_str(), "paused" | "ready")
    {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}
pub async fn edit_mapping(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: EditRecoveryMapping,
) -> Result<Value, MigrationError> {
    if cmd.expected_draft_revision < 0 {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "recovery_mapping", Some(id), &cmd)?;
    if let Some(v) = s::replay(
        &mut tx,
        key,
        ctx,
        "recovery_mapping",
        cmd.request_id,
        &digest,
    )
    .await?
    {
        return Ok(v);
    }
    let run = s::lock_run(&mut tx, ctx.organization_id, id).await?;
    editable(&run, ctx)?;
    if run.get::<i64, _>("recovery_draft_revision") != cmd.expected_draft_revision {
        return Err(MigrationError::Conflict);
    }
    s::validate_run(&mut tx, key, ctx.organization_id, &run).await?;
    let row=sqlx::query("SELECT * FROM migration_people_recovery_key WHERE id=$1 AND admission_id=$2 AND organization_id=$3").bind(cmd.key_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let source: repair::SourceKey = s::open(
        key,
        ctx.organization_id,
        id,
        cmd.key_id,
        "recovery-key",
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let hash = source.digest(key, ctx.organization_id)?;
    if hash.as_slice() != row.get::<Vec<u8>, _>("source_key_hmac")
        || source.kind() != row.get::<String, _>("kind")
    {
        return Err(MigrationError::Crypto);
    }
    let target = if cmd.disposition == "hold" && cmd.target_id.is_none() {
        Value::Null
    } else {
        repair::target_snapshot(
            &mut tx,
            ctx.organization_id,
            source.kind(),
            &cmd.disposition,
            cmd.target_id,
        )
        .await?
    };
    let choice = Uuid::new_v4();
    let revision = cmd
        .expected_draft_revision
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    let sealed = s::seal(
        key,
        ctx.organization_id,
        id,
        choice,
        "recovery-choice",
        &json!({"source":source,"target":target}),
    )?;
    if sealed.ciphertext.len() > 65552 {
        return Err(MigrationError::InvalidInput);
    }
    super::super::store::require_admin(&mut tx, ctx).await?;
    let previous = run.get::<Option<Vec<u8>>, _>("recovery_draft_digest");
    let mut chain = Sha256::new();
    chain.update(
        previous
            .as_deref()
            .unwrap_or(b"people-recovery-choice-chain-v1"),
    );
    chain.update(choice.as_bytes());
    chain.update(&sealed.ciphertext);
    let draft_digest = chain.finalize();
    let bytes = (32
        + sealed.nonce.len()
        + sealed.ciphertext.len()
        + if previous.is_none() { 32 } else { 0 }) as i64;
    let reservation = s::reserve(
        &mut tx,
        ctx.organization_id,
        id,
        "prepare",
        None,
        bytes,
        policy,
    )
    .await?;
    sqlx::query("INSERT INTO migration_people_recovery_choice(id,admission_id,organization_id,key_id,revision,kind,source_key_hmac,disposition,target_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)").bind(choice).bind(id).bind(ctx.organization_id.0).bind(cmd.key_id).bind(revision).bind(source.kind()).bind(hash.as_slice()).bind(&cmd.disposition).bind(cmd.target_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission_plan SET state='superseded' WHERE admission_id=$1 AND organization_id=$2 AND state='ready'").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_people_admission SET recovery_draft_revision=$3,recovery_draft_digest=$4,state='paused',pause_reason='awaiting_mapping_choices',lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(revision).bind(draft_digest.as_slice()).execute(&mut *tx).await?;
    s::release(&mut tx, ctx.organization_id, id, reservation, bytes).await?;
    let v = json!({"admission_id":id,"choice_id":choice,"draft_revision":revision.to_string()});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "recovery_mapping",
        cmd.request_id,
        id,
        &digest,
        &v,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}
pub async fn mapping_page(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    page: RecoveryMappingPage,
) -> Result<Value, MigrationError> {
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = s::begin(pool, ctx).await?;
    let run = s::lock_run(&mut tx, ctx.organization_id, id).await?;
    if !is_recovery(&run) {
        return Err(MigrationError::NotFound);
    }
    super::super::store::require_admin(&mut tx, ctx).await?;
    if !run.get::<bool, _>("recovery_candidates_complete") {
        tx.commit().await?;
        return Ok(
            json!({"admission_id":id,"draft_revision":"0","candidates_complete":false,"candidate_count":"0","items":[],"next_cursor":null}),
        );
    }
    let bind = mapping_binding(ctx, id, &run, "recovery-mappings", i64::from(limit));
    let cursor = q::decode(key, ctx, &bind, page.cursor.as_deref())?;
    let after = cursor.as_ref().map(|v| v.last.id);
    let rows=sqlx::query("SELECT k.*,c.id choice_id,c.disposition,c.target_id,c.revision choice_revision,CASE WHEN k.kind='stage' THEN (SELECT count(*) FROM migration_people_recovery_candidate p WHERE p.admission_id=k.admission_id AND p.organization_id=k.organization_id AND p.stage_source_hmac=k.source_key_hmac) ELSE (SELECT count(*) FROM migration_people_recovery_candidate p WHERE p.admission_id=k.admission_id AND p.organization_id=k.organization_id AND p.assignee_source_hmac=k.source_key_hmac) END dependent_count FROM migration_people_recovery_key k LEFT JOIN LATERAL(SELECT id,disposition,target_id,revision FROM migration_people_recovery_choice c WHERE c.admission_id=k.admission_id AND c.organization_id=k.organization_id AND c.key_id=k.id ORDER BY revision DESC LIMIT 1)c ON true WHERE k.admission_id=$1 AND k.organization_id=$2 AND ($3::uuid IS NULL OR k.id>$3) ORDER BY k.id LIMIT $4").bind(id).bind(ctx.organization_id.0).bind(after).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
    let mut items = Vec::new();
    let mut bytes = 4096;
    let mut next = None;
    for row in rows {
        if items.len() == limit as usize {
            next = items
                .last()
                .and_then(|v: &Value| v["id"].as_str())
                .map(str::to_owned);
            break;
        }
        let source: repair::SourceKey = s::open(
            key,
            ctx.organization_id,
            id,
            row.get("id"),
            "recovery-key",
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        let mut source = serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?;
        let source_bytes = source["key"].as_str().map(str::len).unwrap_or(0);
        if let Some(text) = source["key"].as_str() {
            source["key"] = json!(q::prefix(text, 1024));
        }
        let item = json!({"id":row.get::<Uuid,_>("id"),"source":source,"source_key_bytes":source_bytes.to_string(),"source_key_truncated":source_bytes>1024,"kind":row.get::<String,_>("kind"),"choice_id":row.get::<Option<Uuid>,_>("choice_id"),"disposition":row.get::<Option<String>,_>("disposition"),"target_id":row.get::<Option<Uuid>,_>("target_id"),"choice_revision":row.get::<Option<i64>,_>("choice_revision").map(|v|v.to_string()),"dependent_count":row.get::<i64,_>("dependent_count").to_string()});
        let size = serde_json::to_vec(&item)
            .map_err(|_| MigrationError::Crypto)?
            .len();
        if bytes + size > 128 * 1024 {
            next = items
                .last()
                .and_then(|v: &Value| v["id"].as_str())
                .map(str::to_owned);
            break;
        }
        bytes += size;
        items.push(item);
    }
    let next = next
        .map(|last| {
            let last = Uuid::parse_str(&last).map_err(|_| MigrationError::Crypto)?;
            q::encode(
                key,
                ctx,
                bind,
                q::Position {
                    id: last,
                    time: None,
                },
                None,
                0,
            )
        })
        .transpose()?;
    tx.commit().await?;
    Ok(
        json!({"admission_id":id,"draft_revision":run.get::<i64,_>("recovery_draft_revision").to_string(),"candidates_complete":run.get::<bool,_>("recovery_candidates_complete"),"candidate_count":run.get::<i64,_>("recovery_candidate_count").to_string(),"items":items,"next_cursor":next}),
    )
}
fn mapping_binding(
    ctx: &CommandContext,
    id: Uuid,
    run: &PgRow,
    endpoint: &str,
    limit: i64,
) -> q::Binding {
    let mut bind = q::binding(
        ctx,
        id,
        endpoint,
        None,
        Some(run.get::<i64, _>("recovery_draft_revision").to_string()),
        limit,
    );
    bind.revision = Some(run.get("lifecycle_revision"));
    bind.digest = run.get("recovery_draft_digest");
    bind
}
pub async fn mapping_field(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
    key_id: Uuid,
    page: q::Page,
) -> Result<Value, MigrationError> {
    let n = q::field_limit(&page)?;
    let mut tx = s::begin(pool, ctx).await?;
    let run = s::lock_run(&mut tx, ctx.organization_id, id).await?;
    if !is_recovery(&run) {
        return Err(MigrationError::NotFound);
    }
    let row=sqlx::query("SELECT * FROM migration_people_recovery_key WHERE id=$1 AND admission_id=$2 AND organization_id=$3").bind(key_id).bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let source: repair::SourceKey = s::open(
        key,
        ctx.organization_id,
        id,
        key_id,
        "recovery-key",
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let source = serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?;
    let text = source["key"].as_str().ok_or(MigrationError::NotFound)?;
    let bind = mapping_binding(ctx, id, &run, &format!("recovery-key/{key_id}"), n as i64);
    let result = q::fragment(key, ctx, bind, page.cursor.as_deref(), text, n)?;
    tx.commit().await?;
    Ok(result)
}
pub async fn seal_choices(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: SealRecoveryChoices,
) -> Result<Value, MigrationError> {
    let mut tx = s::begin(pool, ctx).await?;
    let digest = s::digest(key, ctx, "seal_recovery", Some(id), &cmd)?;
    if let Some(v) = s::replay(&mut tx, key, ctx, "seal_recovery", cmd.request_id, &digest).await? {
        return Ok(v);
    }
    let run = s::lock_run(&mut tx, ctx.organization_id, id).await?;
    editable(&run, ctx)?;
    if run.get::<i64, _>("recovery_draft_revision") != cmd.expected_draft_revision {
        return Err(MigrationError::Conflict);
    }
    s::validate_run(&mut tx, key, ctx.organization_id, &run).await?;
    // The append-only choice chain binds every approved version. Freezing its
    // revision identifies each latest choice without a full catalog transaction.
    let mut hash = Sha256::new();
    hash.update(b"people-recovery-choices-v1");
    hash.update(id.as_bytes());
    hash.update(cmd.expected_draft_revision.to_be_bytes());
    hash.update(run.get::<i64, _>("recovery_candidate_count").to_be_bytes());
    if let Some(chain) = run.get::<Option<Vec<u8>>, _>("recovery_draft_digest") {
        hash.update(chain);
    }
    let digest_choices = hash.finalize();
    let delta = if run
        .get::<Option<Vec<u8>>, _>("recovery_choices_digest")
        .is_none()
    {
        32
    } else {
        0
    };
    super::super::store::require_admin(&mut tx, ctx).await?;
    let reservation = if delta > 0 {
        Some(
            s::reserve(
                &mut tx,
                ctx.organization_id,
                id,
                "prepare",
                None,
                delta,
                policy,
            )
            .await?,
        )
    } else {
        None
    };
    sqlx::query("UPDATE migration_people_admission_plan SET state='superseded' WHERE admission_id=$1 AND organization_id=$2 AND state='ready'").bind(id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    s::checkpoint(&mut tx, ctx.organization_id, id, "").await?;
    sqlx::query("UPDATE migration_people_admission SET recovery_frozen_revision=$3,recovery_choices_digest=$4,state='preparing',preparation_phase='groups',pause_reason=NULL,lifecycle_revision=lifecycle_revision+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(cmd.expected_draft_revision).bind(digest_choices.as_slice()).execute(&mut *tx).await?;
    if let Some(token) = reservation {
        s::release(&mut tx, ctx.organization_id, id, token, delta).await?;
    }
    let v = json!({"admission_id":id,"state":"preparing"});
    s::receipt(
        &mut tx,
        key,
        ctx,
        "seal_recovery",
        cmd.request_id,
        id,
        &digest,
        &v,
    )
    .await?;
    tx.commit().await?;
    Ok(v)
}

pub(crate) async fn resolve(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &PgRow,
    record: &ExtractedRecord,
) -> Result<crate::domain::migration::people_admission_source::ResolvedMappings, MigrationError> {
    let Entity::People(p) = &record.entity else {
        return Err(MigrationError::SourceNotEligible);
    };
    let stage = match &p.stage_label {
        Some(label) => repair::SourceKey::StageLabel(label.clone()),
        None => repair::SourceKey::MissingStage,
    };
    let mut raw_bytes = qualify_source(conn, key, org, run, &stage).await?;
    if let Some(source) = &p.assignee_key {
        raw_bytes = raw_bytes.saturating_add(
            qualify_source(
                conn,
                key,
                org,
                run,
                &repair::SourceKey::Assignee(source.clone()),
            )
            .await?,
        );
    }
    let stage_choice = selected(conn, key, org, run, &stage).await?;
    let (assignee, assignee_choice) = if let Some(source) = &p.assignee_key {
        let choice = selected(
            conn,
            key,
            org,
            run,
            &repair::SourceKey::Assignee(source.clone()),
        )
        .await?;
        (choice.1, Some(choice.0))
    } else {
        (None, None)
    };
    Ok(
        crate::domain::migration::people_admission_source::ResolvedMappings {
            stage_id: stage_choice.1.ok_or(MigrationError::SourceNotEligible)?,
            assignee_id: assignee,
            stage_mapping_id: stage_choice.0,
            assignee_mapping_id: assignee_choice,
            raw_bytes,
        },
    )
}
async fn selected(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &PgRow,
    source: &repair::SourceKey,
) -> Result<(Uuid, Option<Uuid>), MigrationError> {
    let row=sqlx::query("SELECT * FROM migration_people_recovery_choice WHERE admission_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4 AND revision<=$5 ORDER BY revision DESC LIMIT 1").bind(run.get::<Uuid,_>("id")).bind(org.0).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).bind(run.get::<Option<i64>,_>("recovery_frozen_revision")).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let body: Value = s::open(
        key,
        org,
        run.get("id"),
        row.get("id"),
        "recovery-choice",
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    if matches!(source,repair::SourceKey::Assignee(id) if id.starts_with("pond:"))
        && row.get::<String, _>("disposition") != "unassigned"
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let target: Option<Uuid> = row.get("target_id");
    let disposition: String = row.get("disposition");
    if body["source"] != serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?
        || body["target"]
            != repair::target_snapshot(conn, org, source.kind(), &disposition, target).await?
    {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok((row.get("id"), target))
}

async fn qualify_source(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    run: &PgRow,
    source: &repair::SourceKey,
) -> Result<i64, MigrationError> {
    use crate::domain::migration::{
        crypto, people_admission_source as retained, snapshot_source::Stream,
    };
    let (stream, id, expected) = match source {
        repair::SourceKey::MissingStage => return Ok(0),
        repair::SourceKey::Assignee(id) if id.starts_with("pond:") => return Ok(0),
        repair::SourceKey::Assignee(id) => (Stream::Users, id.clone(), None),
        repair::SourceKey::StageLabel(_) => {
            let rows=sqlx::query("SELECT source_id,semantic_hmac,qualified FROM migration_people_recovery_catalog WHERE admission_id=$1 AND organization_id=$2 AND source_key_hmac=$3 ORDER BY source_id LIMIT 2").bind(run.get::<Uuid,_>("id")).bind(org.0).bind(source.digest(key,org)?.as_slice()).fetch_all(&mut *conn).await?;
            if rows.len() != 1 || !rows[0].get::<bool, _>("qualified") {
                return Err(MigrationError::SourceNotEligible);
            }
            (
                Stream::Stages,
                rows[0].get::<String, _>("source_id"),
                rows[0].get::<Option<Vec<u8>>, _>("semantic_hmac"),
            )
        }
    };
    let retained::Observation::Qualified(observation) = retained::retained_record(
        conn,
        key,
        org,
        run.get("newer_snapshot_id"),
        stream,
        run.get("newer_sequence"),
        &id,
    )
    .await?
    else {
        return Err(MigrationError::SourceNotEligible);
    };
    if !(if matches!(source, repair::SourceKey::StageLabel(_)) {
        super::discovery::stage_mapping_reasons_allowed(&observation.record.reasons)
    } else {
        observation.record.reasons.is_empty()
    }) {
        return Err(MigrationError::SourceNotEligible);
    }
    if let Some(expected) = expected {
        let hash = crypto::snapshot_hmac(
            key,
            org,
            &format!("semantic:{}", stream.representation()),
            &observation.record.canonical,
        );
        if hash.as_slice() != expected {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    match (&observation.record.entity, source) {
        (Entity::Stage(stage), repair::SourceKey::StageLabel(label))
            if stage.label.as_ref() == Some(label) => {}
        (Entity::User(_), repair::SourceKey::Assignee(_)) => {}
        _ => return Err(MigrationError::SourceNotEligible),
    }
    Ok(observation.raw_bytes)
}
