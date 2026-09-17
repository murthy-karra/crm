//! Private, bounded review of frozen values. Never serialize proof envelopes or
//! reparse retained canonical numbers through floating-point JSON values.
use super::{
    core_source::Derived,
    evidence::{Purpose, Scope},
    model::{Family, RESPONSE_BYTES},
    queries,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{crypto, imports::hex, snapshot, MigrationError},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const FRAGMENT_BYTES: usize = 16 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentQuery {
    pub cursor: Option<String>,
}
#[derive(Serialize)]
pub struct Descriptor {
    pub id: String,
    pub section: String,
    pub label: String,
    pub total_bytes: String,
}
#[derive(Serialize)]
pub struct Fields {
    pub item_id: Uuid,
    pub items: Vec<Descriptor>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize)]
pub struct Fragment {
    pub item_id: Uuid,
    pub field_id: String,
    pub total_bytes: String,
    pub offset: String,
    pub text: String,
    pub next_cursor: Option<String>,
}
struct Field {
    id: String,
    section: String,
    label: String,
    text: String,
}
struct Review {
    fields: Vec<Field>,
    scope: Value,
}
fn add(
    fields: &mut Vec<Field>,
    section: &str,
    label: &str,
    text: String,
) -> Result<(), MigrationError> {
    // Source names are untrusted and may be long. Their complete spelling is
    // retained in the field value; the descriptor is only a small preview.
    let identity = serde_json::to_vec(&(section, label)).map_err(|_| MigrationError::Crypto)?;
    use sha2::{Digest, Sha256};
    fields.push(Field {
        id: hex(&Sha256::digest(identity)),
        section: section.into(),
        label: label.chars().take(256).collect(),
        text,
    });
    Ok(())
}
fn value(
    fields: &mut Vec<Field>,
    section: &str,
    label: &str,
    v: &Value,
) -> Result<(), MigrationError> {
    add(
        fields,
        section,
        label,
        serde_json::to_string(v).map_err(|_| MigrationError::Crypto)?,
    )
}
fn project(
    fields: &mut Vec<Field>,
    section: &str,
    v: Option<&Value>,
    names: &[&str],
) -> Result<(), MigrationError> {
    if let Some(v) = v {
        for name in names {
            if let Some(v) = v.get(*name) {
                value(fields, section, name, v)?;
            }
        }
    }
    Ok(())
}
async fn load(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    item: Uuid,
) -> Result<Review, MigrationError> {
    let mut tx = queries::begin(pool, ctx).await?;
    let b=sqlx::query("SELECT b.revision,b.parent_import_id,b.parent_plan_id,o.workspace_revision FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id JOIN organization o ON o.id=b.organization_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF b")
        .bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let u=sqlx::query("SELECT m.*,p.revision AS plan_revision,p.family FROM migration_family_refresh_manifest m JOIN migration_family_refresh_plan p ON p.id=m.plan_id AND p.bundle_id=m.bundle_id AND p.organization_id=m.organization_id WHERE m.id=$1 AND m.bundle_id=$2 AND m.organization_id=$3 FOR SHARE OF p")
        .bind(item).bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let family: Family = serde_json::from_value(json!(u.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    let scope = Scope {
        organization: ctx.organization_id,
        bundle,
        plan: u.get("plan_id"),
        family,
        revision: u.get("plan_revision"),
    };
    let evidence: Value = scope.open(
        key,
        item,
        Purpose::Manifest,
        u.get("nonce"),
        u.get("ciphertext"),
    )?;
    let mut fields = Vec::new();
    if family != Family::History {
        if let Some(source) = u.get::<Option<Uuid>, _>("source_row_id") {
            let row=sqlx::query("SELECT s.*,p.family,p.revision FROM migration_family_refresh_source s JOIN migration_family_refresh_plan p ON p.id=s.plan_id AND p.bundle_id=s.bundle_id AND p.organization_id=s.organization_id WHERE s.id=$1 AND s.bundle_id=$2 AND s.organization_id=$3")
                .bind(source).bind(bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Crypto)?;
            let owner = Scope {
                plan: row.get("plan_id"),
                family: serde_json::from_value(json!(row.get::<String, _>("family")))
                    .map_err(|_| MigrationError::Crypto)?,
                revision: row.get("revision"),
                ..scope
            };
            // Only record rows have a Derived payload. Page-level diagnostics
            // remain represented by their frozen reason/counts.
            if matches!(
                row.get::<String, _>("kind").as_str(),
                "person" | "field" | "user" | "note" | "task"
            ) {
                let derived: Derived = owner.open(
                    key,
                    source,
                    Purpose::Source,
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                let provenance = match derived {
                    Derived::Metadata(r) => r.provenance,
                    Derived::Activity(r) => r.provenance,
                    Derived::Oversized { .. } => Default::default(),
                };
                for (name, canonical) in provenance {
                    // canonical is already validated lossless JSON. Embed it
                    // directly instead of parsing/re-serializing its number.
                    let label = serde_json::to_string(&name).map_err(|_| MigrationError::Crypto)?;
                    add(
                        &mut fields,
                        "source",
                        &name,
                        format!("{{{label}:{canonical}}}"),
                    )?;
                }
            }
        }
    }
    let decision = evidence.get("decision");
    match u.get::<String, _>("kind").as_str() {
        "metadata" => {
            project(
                &mut fields,
                "before",
                decision.and_then(|v| v.pointer("/baseline/state")),
                &["tags", "fields"],
            )?;
            project(
                &mut fields,
                "after",
                decision.and_then(|v| v.pointer("/native/after")),
                &["tags", "fields"],
            )?;
            project(
                &mut fields,
                "changes",
                decision.and_then(|v| v.get("native")),
                &["changes", "gaps"],
            )?;
        }
        "note" | "task" => {
            let names = [
                "body",
                "title",
                "kind",
                "author_user_id",
                "created_by_user_id",
                "assignee_user_id",
                "completed_by_user_id",
                "due_at",
                "completed_at",
                "created_at",
                "updated_at",
            ];
            project(
                &mut fields,
                "before",
                decision.and_then(|v| v.pointer("/baseline/native")),
                &names,
            )?;
            project(
                &mut fields,
                "after",
                decision.and_then(|v| v.pointer("/native/after")),
                &names,
            )?;
        }
        "event" | "call" | "text" => {
            if let Some(display) = evidence.pointer("/source/display") {
                value(&mut fields, "source", "Historical metadata", display)?;
            }
            project(&mut fields, "changes", Some(&evidence), &["change"])?;
        }
        "catalog" => {
            let mapping =
                decision.and_then(|v| v.pointer("/evidence/mapping").or_else(|| v.get("mapping")));
            project(
                &mut fields,
                "mapping",
                mapping,
                &["kind", "label", "choice", "target"],
            )?;
        }
        _ => return Err(MigrationError::Crypto),
    }
    if let Some(reason) = u.get::<Option<String>, _>("reason") {
        value(&mut fields, "review", "Reason", &json!(reason))?;
    }
    fields.sort_by(|a, b| a.id.cmp(&b.id));
    let binding = json!({"engine":super::model::ENGINE,"actor":ctx.actor_user_id.0,"organization":ctx.organization_id.0,"bundle":bundle,"bundle_revision":b.get::<i64,_>("revision"),"parent":b.get::<Uuid,_>("parent_import_id"),"workspace":b.get::<Uuid,_>("parent_plan_id"),"workspace_revision":b.get::<i64,_>("workspace_revision"),"plan":scope.plan,"plan_revision":scope.revision,"family":family,"item":item,"order":"field_id_asc"});
    tx.commit().await?;
    Ok(Review {
        fields,
        scope: binding,
    })
}
fn binding(
    mut scope: Value,
    endpoint: &str,
    limit: usize,
    field: Option<&str>,
    digest: Option<String>,
) -> Result<String, MigrationError> {
    scope["endpoint"] = json!(endpoint);
    scope["limit"] = json!(limit);
    scope["field"] = json!(field);
    scope["digest"] = json!(digest);
    serde_json::to_string(&scope).map_err(|_| MigrationError::Crypto)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%bundle,item_id=%item))]
pub async fn fields(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    item: Uuid,
    q: Page,
) -> Result<Fields, MigrationError> {
    let limit = usize::from(q.limit.unwrap_or(25));
    if !(1..=50).contains(&limit) {
        return Err(MigrationError::InvalidInput);
    }
    let review = load(pool, key, ctx, bundle, item).await?;
    let scope = binding(review.scope, "fields", limit, None, None)?;
    let after: Option<String> = snapshot::decode_cursor(
        key,
        ctx.organization_id,
        bundle,
        &scope,
        q.cursor.as_deref(),
    )?
    .map(serde_json::from_value)
    .transpose()
    .map_err(|_| MigrationError::InvalidInput)?;
    let mut iter = review
        .fields
        .into_iter()
        .filter(|f| after.as_ref().is_none_or(|a| f.id > *a));
    let items: Vec<_> = iter
        .by_ref()
        .take(limit)
        .map(|f| Descriptor {
            id: f.id,
            section: f.section,
            label: f.label,
            total_bytes: f.text.len().to_string(),
        })
        .collect();
    let next_cursor = if iter.next().is_some() {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            bundle,
            &scope,
            &json!(items.last().ok_or(MigrationError::Crypto)?.id),
        )?)
    } else {
        None
    };
    let result = Fields {
        item_id: item,
        items,
        next_cursor,
    };
    queries::bounded(&result, RESPONSE_BYTES)?;
    Ok(result)
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%bundle,item_id=%item))]
pub async fn fragment(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    bundle: Uuid,
    item: Uuid,
    field: &str,
    q: FragmentQuery,
) -> Result<Fragment, MigrationError> {
    if field.len() != 64
        || !field
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MigrationError::InvalidInput);
    }
    let review = load(pool, key, ctx, bundle, item).await?;
    let selected = review
        .fields
        .into_iter()
        .find(|f| f.id == field)
        .ok_or(MigrationError::NotFound)?;
    let digest = hex(&crypto::snapshot_hmac(
        key,
        ctx.organization_id,
        "family-review-field",
        selected.text.as_bytes(),
    ));
    let scope = binding(
        review.scope,
        "field_fragment",
        FRAGMENT_BYTES,
        Some(field),
        Some(digest),
    )?;
    let offset: usize = snapshot::decode_cursor(
        key,
        ctx.organization_id,
        bundle,
        &scope,
        q.cursor.as_deref(),
    )?
    .map(serde_json::from_value)
    .transpose()
    .map_err(|_| MigrationError::InvalidInput)?
    .unwrap_or(0);
    let (text, end) = part(&selected.text, offset)?;
    let next_cursor = if end < selected.text.len() {
        Some(snapshot::encode_cursor(
            key,
            ctx.organization_id,
            bundle,
            &scope,
            &json!(end),
        )?)
    } else {
        None
    };
    let result = Fragment {
        item_id: item,
        field_id: field.into(),
        total_bytes: selected.text.len().to_string(),
        offset: offset.to_string(),
        text: text.into(),
        next_cursor,
    };
    queries::bounded(&result, RESPONSE_BYTES)?;
    Ok(result)
}
fn part(text: &str, offset: usize) -> Result<(&str, usize), MigrationError> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(MigrationError::InvalidInput);
    }
    let mut end = offset.saturating_add(FRAGMENT_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((&text[offset..end], end))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_fragments_are_lossless_and_bounded() {
        let text = "a🙂é".repeat(10000);
        let mut offset = 0;
        let mut joined = String::new();
        while offset < text.len() {
            let (p, end) = part(&text, offset).unwrap();
            assert!(p.len() <= FRAGMENT_BYTES);
            joined.push_str(p);
            offset = end;
        }
        assert_eq!(text, joined);
        assert!(part("🙂", 1).is_err());
        assert!(part("x", 2).is_err());
    }
}
