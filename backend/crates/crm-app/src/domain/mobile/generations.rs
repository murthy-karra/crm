use super::*;
use crate::domain::{person::visibility::PersonVisibilityScope, today};
use sha2::Digest;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationRequest {
    pub protocol: String,
    pub installation_id: Uuid,
    pub pinned_person_ids: Vec<Uuid>,
}
#[derive(Clone, PartialEq)]
struct Entry {
    person: Uuid,
    revision: i64,
    reasons: Vec<String>,
}
struct Selection {
    entries: Vec<Entry>,
    today: Value,
    digest: Vec<u8>,
    complete: bool,
}
async fn selection(
    conn: &mut PgConnection,
    auth: &AuthContext,
    pins: &[Uuid],
    now: DateTime<Utc>,
    validate_pins: bool,
) -> Result<Selection, MobileError> {
    let list = today::query_in_transaction(
        conn,
        &PersonVisibilityScope::Organization(auth.active_organization_id),
        auth.actor_user_id,
        now,
    )
    .await?;
    let complete = list.sources.status == today::model::TodaySourcesStatus::Complete
        && list.sources.system_feed_issues.is_empty();
    let today_ids: Vec<Uuid> = list.items.iter().map(|i| i.person.id.0).collect();
    let value = serialize(&list)?;
    let digest = Sha256::digest(serde_json::to_vec(&value).map_err(|_| invalid())?).to_vec();
    let rows=sqlx::query("SELECT id,mobile_revision,assigned_user_id=$2 AS assigned,id=ANY($3) AS pinned,id=ANY($4) AS today FROM person WHERE organization_id=$1 AND (assigned_user_id=$2 OR id=ANY($3) OR id=ANY($4)) ORDER BY id LIMIT 25001")
        .bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(pins).bind(today_ids).fetch_all(&mut *conn).await?;
    if rows.len() > MAX_PEOPLE {
        return Err(code(422, "over_limit"));
    }
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let mut reasons = Vec::new();
        if row.get::<Option<bool>, _>("assigned") == Some(true) {
            reasons.push("assigned".into());
        }
        if row.get::<bool, _>("pinned") {
            reasons.push("pinned".into());
        }
        if row.get::<bool, _>("today") {
            reasons.push("today".into());
        }
        entries.push(Entry {
            person: row.get("id"),
            revision: row.get("mobile_revision"),
            reasons,
        });
    }
    if validate_pins
        && pins
            .iter()
            .any(|p| entries.binary_search_by_key(p, |e| e.person).is_err())
    {
        return Err(missing());
    }
    Ok(Selection {
        entries,
        today: value,
        digest,
        complete,
    })
}
async fn cleanup(conn: &mut PgConnection, org: Option<Uuid>) -> Result<(), MobileError> {
    sqlx::query("DELETE FROM mobile_reconciliation WHERE id IN (SELECT id FROM mobile_reconciliation WHERE expires_at<=statement_timestamp() AND ($1::uuid IS NULL OR organization_id=$1) ORDER BY expires_at,id LIMIT 2 FOR UPDATE SKIP LOCKED)").bind(org).execute(conn).await?;
    Ok(())
}
/// Each sweep removes at most two generations (50,000 bounded manifest rows).
/// Receipt markers and contexts are never garbage-collected by this worker.
pub async fn cleanup_once(pool: &PgPool) -> Result<(), MobileError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='2000ms'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='5000ms'")
        .execute(&mut *tx)
        .await?;
    cleanup(&mut tx, None).await?;
    tx.commit().await?;
    Ok(())
}

#[tracing::instrument(name="mobile.create_generation",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id))]
pub async fn create_generation(
    pool: &PgPool,
    keys: &ReceiptKeys,
    auth: &AuthContext,
    context_id: Uuid,
    mut request: ReconciliationRequest,
) -> Result<Value, MobileError> {
    protocol(&request.protocol)?;
    if request.pinned_person_ids.len() > MAX_PEOPLE {
        return Err(code(422, "over_limit"));
    }
    request.pinned_person_ids.sort();
    request.pinned_person_ids.dedup();
    let mut tx = begin(pool, auth, true).await?;
    admission(&mut tx, auth).await?;
    if context(&mut tx, auth, context_id, false).await? != request.installation_id {
        return Err(code(401, "unauthenticated"));
    }
    let (role, workspace_revision) = authority(&mut tx, auth, false).await?;
    cleanup(&mut tx, Some(auth.active_organization_id.0)).await?;
    let row=sqlx::query("SELECT count(*) AS org_count,count(*) FILTER(WHERE actor_user_id=$2) AS actor_count,count(*) FILTER(WHERE context_id=$3) AS context_count FROM mobile_reconciliation WHERE organization_id=$1")
        .bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(context_id).fetch_one(&mut *tx).await?;
    if row.get::<i64, _>("org_count") >= 20
        || row.get::<i64, _>("actor_count") >= 4
        || row.get::<i64, _>("context_count") >= 2
    {
        return Err(code(429, "mobile_capacity"));
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let selected = selection(&mut tx, auth, &request.pinned_person_ids, now, true).await?;
    let id = Uuid::new_v4();
    let expiry = now + chrono::Duration::minutes(30);
    sqlx::query("INSERT INTO mobile_reconciliation(id,context_id,organization_id,actor_user_id,role,workspace_revision,evaluated_at,expires_at,complete,selected_count,pinned_person_ids,today_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(id).bind(context_id).bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(role).bind(workspace_revision).bind(now).bind(expiry).bind(selected.complete).bind(selected.entries.len() as i32).bind(&request.pinned_person_ids).bind(selected.digest).execute(&mut *tx).await?;
    if !selected.entries.is_empty() {
        let ids: Vec<_> = selected.entries.iter().map(|e| e.person).collect();
        let versions: Vec<_> = selected.entries.iter().map(|e| e.revision).collect();
        let reasons: Vec<_> = selected
            .entries
            .iter()
            .map(|e| serde_json::to_value(&e.reasons).expect("strings serialize"))
            .collect();
        sqlx::query("INSERT INTO mobile_reconciliation_person(generation_id,person_id,revision,reasons) SELECT $1,u.id,u.revision,ARRAY(SELECT jsonb_array_elements_text(u.reasons)) FROM UNNEST($2::uuid[],$3::bigint[],$4::jsonb[]) AS u(id,revision,reasons)")
            .bind(id).bind(ids).bind(versions).bind(reasons).execute(&mut *tx).await?;
    }
    let page = manifest_page(&mut tx, keys, context_id, id, None).await?;
    tx.commit().await?;
    Ok(
        json!({"generation_id":id,"context_id":context_id,"evaluated_at":now,"expires_at":expiry,"complete":selected.complete,"selected_count":selected.entries.len(),"manifest":page}),
    )
}
struct Generation {
    evaluated: DateTime<Utc>,
    complete: bool,
    count: i32,
    pins: Vec<Uuid>,
    digest: Vec<u8>,
}
async fn generation(
    conn: &mut PgConnection,
    auth: &AuthContext,
    context_id: Uuid,
    id: Uuid,
) -> Result<Generation, MobileError> {
    context(conn, auth, context_id, false).await?;
    let (role, workspace) = authority(conn, auth, false).await?;
    let row=sqlx::query("SELECT *,expires_at>statement_timestamp() AS live FROM mobile_reconciliation WHERE id=$1 AND context_id=$2 AND organization_id=$3 AND actor_user_id=$4")
        .bind(id).bind(context_id).bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).fetch_optional(&mut *conn).await?.ok_or_else(missing)?;
    if !row.get::<bool, _>("live") {
        return Err(code(409, "generation_expired"));
    }
    if row.get::<String, _>("role") != role || row.get::<i64, _>("workspace_revision") != workspace
    {
        return Err(code(409, "generation_changed"));
    }
    Ok(Generation {
        evaluated: row.get("evaluated_at"),
        complete: row.get("complete"),
        count: row.get("selected_count"),
        pins: row.get("pinned_person_ids"),
        digest: row.get("today_digest"),
    })
}
async fn download_slot(conn: &mut PgConnection, context: Uuid) -> Result<(), MobileError> {
    for slot in 0..2 {
        let acquired:bool=sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended('crm-mobile-download:'||$1::text||':'||$2::text,0))").bind(context).bind(slot.to_string()).fetch_one(&mut *conn).await?;
        if acquired {
            return Ok(());
        }
    }
    Err(code(429, "mobile_capacity"))
}
fn after(
    keys: &ReceiptKeys,
    raw: Option<&str>,
    expected: Cursor,
) -> Result<Option<Uuid>, MobileError> {
    raw.map(|r| {
        let parsed = keys.decode_cursor(r)?;
        let position = parsed.after;
        let mut compare = expected;
        compare.after = position;
        if compare != parsed {
            return Err(invalid());
        }
        Ok(position)
    })
    .transpose()
}
async fn manifest_page(
    conn: &mut PgConnection,
    keys: &ReceiptKeys,
    context: Uuid,
    id: Uuid,
    cursor: Option<&str>,
) -> Result<Value, MobileError> {
    let binding = Cursor {
        context,
        generation: id,
        person: None,
        section: "manifest".into(),
        revision: None,
        after: Uuid::nil(),
    };
    let position = after(keys, cursor, binding)?;
    let mut rows=sqlx::query("SELECT person_id,revision,reasons FROM mobile_reconciliation_person WHERE generation_id=$1 AND ($2::uuid IS NULL OR person_id>$2) ORDER BY person_id LIMIT 251").bind(id).bind(position).fetch_all(conn).await?;
    let more = rows.len() > 250;
    rows.truncate(250);
    let next = if more {
        Some(keys.cursor(&Cursor {
            context,
            generation: id,
            person: None,
            section: "manifest".into(),
            revision: None,
            after: rows.last().ok_or_else(invalid)?.get("person_id"),
        })?)
    } else {
        None
    };
    let items:Vec<Value>=rows.into_iter().map(|r|json!({"person_id":r.get::<Uuid,_>("person_id"),"revision":r.get::<i64,_>("revision").to_string(),"reasons":r.get::<Vec<String>,_>("reasons")})).collect();
    Ok(json!({"items":items,"next_cursor":next,"complete":!more}))
}
#[tracing::instrument(name="mobile.manifest",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id))]
pub async fn manifest(
    pool: &PgPool,
    keys: &ReceiptKeys,
    auth: &AuthContext,
    context_id: Uuid,
    id: Uuid,
    cursor: Option<&str>,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    download_slot(&mut tx, context_id).await?;
    generation(&mut tx, auth, context_id, id).await?;
    let page = manifest_page(&mut tx, keys, context_id, id, cursor).await?;
    tx.commit().await?;
    Ok(page)
}
#[tracing::instrument(name="mobile.seal",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id))]
pub async fn seal(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    id: Uuid,
) -> Result<Value, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    download_slot(&mut tx, context_id).await?;
    let gen = generation(&mut tx, auth, context_id, id).await?;
    if !gen.complete {
        return Err(code(409, "generation_changed"));
    }
    let fresh = selection(&mut tx, auth, &gen.pins, gen.evaluated, false).await?;
    if !fresh.complete {
        return Err(code(409, "generation_changed"));
    }
    let rows=sqlx::query("SELECT person_id,revision,reasons FROM mobile_reconciliation_person WHERE generation_id=$1 ORDER BY person_id").bind(id).fetch_all(&mut *tx).await?;
    let old: Vec<_> = rows
        .into_iter()
        .map(|r| Entry {
            person: r.get("person_id"),
            revision: r.get("revision"),
            reasons: r.get("reasons"),
        })
        .collect();
    if old != fresh.entries || fresh.digest != gen.digest {
        let removed = old
            .iter()
            .filter(|e| {
                fresh
                    .entries
                    .binary_search_by_key(&e.person, |v| v.person)
                    .is_err()
            })
            .count();
        let added = fresh
            .entries
            .iter()
            .filter(|e| old.binary_search_by_key(&e.person, |v| v.person).is_err())
            .count();
        let changed = fresh
            .entries
            .iter()
            .filter(|e| {
                old.binary_search_by_key(&e.person, |v| v.person)
                    .ok()
                    .is_some_and(|i| old[i] != **e)
            })
            .count();
        return Err(MobileError::ProjectionChanged {
            changed,
            added,
            removed,
            today_changed: fresh.digest != gen.digest,
        });
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(
        json!({"generation_id":id,"context_id":context_id,"sealed_at":now,"evaluated_at":gen.evaluated,"selected_count":gen.count,"today":fresh.today}),
    )
}

#[tracing::instrument(name="mobile.component",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id))]
// Explicit scope parameters keep every cursor/authority binding visible.
#[allow(clippy::too_many_arguments)]
pub async fn component(
    pool: &PgPool,
    keys: &ReceiptKeys,
    auth: &AuthContext,
    context_id: Uuid,
    id: Uuid,
    person: Uuid,
    section: &str,
    cursor: Option<&str>,
) -> Result<Value, MobileError> {
    if !matches!(section, "summary" | "notes" | "tasks") {
        return Err(missing());
    }
    let mut tx = begin(pool, auth, true).await?;
    download_slot(&mut tx, context_id).await?;
    generation(&mut tx, auth, context_id, id).await?;
    let expected: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM mobile_reconciliation_person WHERE generation_id=$1 AND person_id=$2",
    )
    .bind(id)
    .bind(person)
    .fetch_optional(&mut *tx)
    .await?;
    let expected = expected.ok_or_else(missing)?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT mobile_revision FROM person WHERE organization_id=$1 AND id=$2")
            .bind(auth.active_organization_id.0)
            .bind(person)
            .fetch_optional(&mut *tx)
            .await?;
    if current != Some(expected) {
        return Err(MobileError::ProjectionChanged {
            changed: usize::from(current.is_some()),
            added: 0,
            removed: usize::from(current.is_none()),
            today_changed: false,
        });
    }
    let position = after(
        keys,
        cursor,
        Cursor {
            context: context_id,
            generation: id,
            person: Some(person),
            section: section.into(),
            revision: Some(expected),
            after: Uuid::nil(),
        },
    )?;
    let (role, _) = authority(&mut tx, auth, false).await?;
    let summary = if section == "summary" {
        Some(summary(&mut tx, auth, person).await?)
    } else {
        None
    };
    let sql=match section {
        "summary"=>"SELECT id,jsonb_build_object('id',id,'kind',kind,'value',value) AS data FROM contact_method WHERE organization_id=$1 AND person_id=$2 AND ($3::uuid IS NULL OR id>$3) ORDER BY id LIMIT 101",
        "notes"=>"SELECT n.id,jsonb_build_object('id',n.id,'person_id',n.person_id,'body',n.body,'author',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('id',u.id,'display_name',u.display_name) END,'created_at',n.created_at,'updated_at',n.updated_at,'edited',n.updated_at>n.created_at,'can_manage',COALESCE(($4='admin' OR n.author_user_id=$5),false)) AS data FROM note n LEFT JOIN app_user u ON u.id=n.author_user_id WHERE n.organization_id=$1 AND n.person_id=$2 AND ($3::uuid IS NULL OR n.id>$3) AND n.deleted_at IS NULL ORDER BY n.id LIMIT 101",
        _=>"SELECT t.id,jsonb_build_object('id',t.id,'person_id',t.person_id,'title',t.title,'kind',t.kind,'due_at',t.due_at,'assignee',CASE WHEN a.id IS NULL THEN NULL ELSE jsonb_build_object('id',a.id,'display_name',a.display_name) END,'created_by',CASE WHEN c.id IS NULL THEN NULL ELSE jsonb_build_object('id',c.id,'display_name',c.display_name) END,'completed_at',t.completed_at,'completed_by',CASE WHEN k.id IS NULL THEN NULL ELSE jsonb_build_object('id',k.id,'display_name',k.display_name) END,'created_at',t.created_at,'updated_at',t.updated_at,'can_manage',COALESCE(($4='admin' OR t.assignee_user_id=$5 OR t.created_by_user_id=$5),false),'revision',t.revision::text) AS data FROM task t LEFT JOIN app_user a ON a.id=t.assignee_user_id LEFT JOIN app_user c ON c.id=t.created_by_user_id LEFT JOIN app_user k ON k.id=t.completed_by_user_id WHERE t.organization_id=$1 AND t.person_id=$2 AND ($3::uuid IS NULL OR t.id>$3) AND t.deleted_at IS NULL ORDER BY t.id LIMIT 101",
    };
    let query = sqlx::query(sql)
        .bind(auth.active_organization_id.0)
        .bind(person)
        .bind(position);
    let rows = if section == "summary" {
        query.fetch_all(&mut *tx).await?
    } else {
        query
            .bind(role)
            .bind(auth.actor_user_id.0)
            .fetch_all(&mut *tx)
            .await?
    };
    let total = rows.len();
    let mut items = Vec::new();
    let mut last = None;
    // Reserve ample framing/cursor space, then check exact final serialization.
    let mut bytes = serde_json::to_vec(&summary).map_err(|_| invalid())?.len() + 4096;
    if bytes >= PAGE_BYTES {
        return Err(code(422, "over_limit"));
    }
    for row in rows {
        let value: Value = row.get("data");
        let size = serde_json::to_vec(&value).map_err(|_| invalid())?.len() + 1;
        if items.len() == 100 || bytes + size > PAGE_BYTES {
            break;
        }
        bytes += size;
        last = Some(row.get::<Uuid, _>("id"));
        items.push(value);
    }
    if total > 0 && items.is_empty() {
        return Err(code(422, "over_limit"));
    }
    let more = items.len() < total;
    let next = if more {
        Some(keys.cursor(&Cursor {
            context: context_id,
            generation: id,
            person: Some(person),
            section: section.into(),
            revision: Some(expected),
            after: last.ok_or_else(invalid)?,
        })?)
    } else {
        None
    };
    let page = json!({"generation_id":id,"person_id":person,"revision":expected.to_string(),"section":section,"summary":summary,"items":items,"next_cursor":next,"complete":!more});
    if serde_json::to_vec(&page).map_err(|_| invalid())?.len() > PAGE_BYTES {
        return Err(code(422, "over_limit"));
    }
    tx.commit().await?;
    Ok(page)
}
async fn summary(
    conn: &mut PgConnection,
    auth: &AuthContext,
    person: Uuid,
) -> Result<Value, MobileError> {
    let row=sqlx::query("SELECT p.id,p.first_name,p.last_name,p.created_at,s.id AS stage_id,s.name AS stage_name,u.id AS user_id,u.display_name,(SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='email' ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 1) AS email,(SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='phone' ORDER BY import_order ASC NULLS LAST,created_at,id LIMIT 1) AS phone FROM person p JOIN stage s ON s.id=p.stage_id AND s.organization_id=p.organization_id LEFT JOIN app_user u ON u.id=p.assigned_user_id WHERE p.organization_id=$1 AND p.id=$2")
        .bind(auth.active_organization_id.0).bind(person).fetch_optional(conn).await?.ok_or_else(missing)?;
    let first: Option<String> = row.get("first_name");
    let last: Option<String> = row.get("last_name");
    let email: Option<String> = row.get("email");
    let phone: Option<String> = row.get("phone");
    let name = crate::domain::person::model::compute_display_name(
        first.as_deref(),
        last.as_deref(),
        email.as_deref(),
        phone.as_deref(),
    );
    let user: Option<Uuid> = row.get("user_id");
    let display: Option<String> = row.get("display_name");
    Ok(
        json!({"id":person,"first_name":first,"last_name":last,"display_name":name,"stage":{"id":row.get::<Uuid,_>("stage_id"),"name":row.get::<String,_>("stage_name")},"assigned_user":user.map(|id|json!({"id":id,"display_name":display})),"created_at":row.get::<DateTime<Utc>,_>("created_at")}),
    )
}
