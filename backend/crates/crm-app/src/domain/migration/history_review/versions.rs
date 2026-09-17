//! Metadata-only immutable version pages under the same current admin review
//! gate. Cursors additionally bind the actor, kind and global history identity.
use super::*;

fn history_kind(name: &str) -> Result<Kind, ReviewError> {
    KINDS
        .iter()
        .find(|k| k.name == name && k.family != Family::Native)
        .copied()
        .ok_or(ReviewError::Malformed)
}
fn endpoint(auth: &AuthContext, kind: Kind, identity: Uuid) -> String {
    format!(
        "history-versions:{}:{}:{identity}",
        auth.actor_user_id.0, kind.name
    )
}
async fn first(
    tx: &mut Transaction<'_, Postgres>,
    scope: &Scope,
    kind: Kind,
    identity: Uuid,
) -> Result<PgRow, ReviewError> {
    sqlx::query(&candidate_branch(
        kind,
        Dated::Known,
        true,
        None,
        Projection::FirstVersion,
    ))
    .bind(scope.org.0)
    .bind(scope.person.0)
    .bind(identity)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ReviewError::NotFound)
}
async fn version_value(
    tx: &mut Transaction<'_, Postgres>,
    key: &RawPayloadKey,
    scope: &Scope,
    row: &PgRow,
    kind: Kind,
    detail: bool,
) -> Result<Value, ReviewError> {
    let mut item = value(tx, key, scope, row, kind, detail).await?;
    let version: i64 = row.try_get("version")?;
    let identity: Uuid = row.try_get("identity_id")?;
    item["version"] = json!(version.to_string());
    item["detail_url"] = json!(format!(
        "/api/people/{}/history/{}/{identity}/versions/{version}",
        scope.person.0, kind.name
    ));
    bounded(&item, if detail { DETAIL_BYTES } else { SUMMARY_BYTES })?;
    Ok(item)
}
pub async fn versions(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    kind: &str,
    identity: Uuid,
    query: &PageQuery,
) -> Result<Page, ReviewError> {
    let limit = query.limit()?;
    let kind = history_kind(kind)?;
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let endpoint = endpoint(auth, kind, identity);
    let after = decode(key, &scope, query, &endpoint, kind.family, Dated::Unknown)?;
    let before = after.as_ref().and_then(|k| k.position).unwrap_or(i64::MAX);
    if before <= 1 {
        return Err(ReviewError::Malformed);
    }
    // Establish first-owner visibility even if the requested correction page is
    // empty; an erased or foreign identity never produces a plausible empty page.
    let initial = first(&mut tx, &scope, kind, identity).await?;
    let sql = format!(
        "{} AND v.version<$4 ORDER BY v.version DESC LIMIT $5",
        candidate_branch(kind, Dated::Known, true, None, Projection::PriorVersions)
    );
    let mut rows = sqlx::query(&sql)
        .bind(scope.org.0)
        .bind(person.0)
        .bind(identity)
        .bind(before)
        .bind((limit + 1) as i64)
        .fetch_all(&mut *tx)
        .await?;
    if rows.len() <= limit {
        rows.push(initial);
    }
    let more = rows.len() > limit;
    rows.truncate(limit);
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(version_value(&mut tx, key, &scope, row, kind, false).await?);
    }
    let next_cursor = if more {
        let row = rows.last().ok_or(ReviewError::Unavailable)?;
        Some(encode(
            key,
            &scope,
            &endpoint,
            kind.family,
            Dated::Unknown,
            limit,
            Key {
                time: None,
                position: Some(row.try_get("version")?),
                recorded: row.try_get("recorded_at")?,
                rank: kind.rank,
                id: identity,
                kind: kind.name.into(),
            },
        )?)
    } else {
        None
    };
    let page = Page {
        items,
        next_cursor,
        read_revision: scope.revision.to_string(),
    };
    bounded(
        &serde_json::to_value(&page).map_err(|_| ReviewError::Unavailable)?,
        PAGE_BYTES,
    )?;
    Ok(page)
}
pub async fn version(
    pool: &PgPool,
    key: &RawPayloadKey,
    auth: &AuthContext,
    person: PersonId,
    kind: &str,
    identity: Uuid,
    version: i64,
) -> Result<Value, ReviewError> {
    if version < 1 {
        return Err(ReviewError::Malformed);
    }
    let kind = history_kind(kind)?;
    let (mut tx, scope) = begin(pool, auth, person).await?;
    let row = if version == 1 {
        first(&mut tx, &scope, kind, identity).await?
    } else {
        let sql = format!(
            "{} AND v.version=$4 LIMIT 1",
            candidate_branch(kind, Dated::Known, true, None, Projection::PriorVersions)
        );
        sqlx::query(&sql)
            .bind(scope.org.0)
            .bind(person.0)
            .bind(identity)
            .bind(version)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ReviewError::NotFound)?
    };
    version_value(&mut tx, key, &scope, &row, kind, true).await
}
