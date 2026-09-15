//! Admin-only pages of immutable repair keys and the current draft choices.
use super::{
    people_mapping_repair::{Owner, SourceKey},
    people_refresh_store, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    actor: Uuid,
    revision: i64,
    limit: u16,
    last: Uuid,
}

pub async fn mappings(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    owner: Owner,
    id: Uuid,
    page: Page,
) -> Result<Value, MigrationError> {
    let limit = page.limit.unwrap_or(20);
    if limit == 0 || limit > 50 || page.cursor.as_ref().is_some_and(|c| c.len() > 4096) {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = people_refresh_store::begin(pool, ctx).await?;
    let org = ctx.organization_id;
    let p = owner.prefix();
    let root=sqlx::query(&format!("SELECT repair_draft_revision,repair_candidates_complete,repair_source_refresh_id FROM {p} WHERE id=$1 AND organization_id=$2"))
        .bind(id).bind(org.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if root
        .get::<Option<Uuid>, _>("repair_source_refresh_id")
        .is_none()
    {
        return Err(MigrationError::Conflict);
    }
    if !root.get::<bool, _>("repair_candidates_complete") {
        return Err(MigrationError::Conflict);
    }
    let revision = root.get::<i64, _>("repair_draft_revision");
    let last = if let Some(cursor) = page.cursor {
        let bytes = URL_SAFE_NO_PAD
            .decode(cursor)
            .map_err(|_| MigrationError::InvalidInput)?;
        if bytes.len() < 40 {
            return Err(MigrationError::InvalidInput);
        }
        let c: Cursor = owner
            .open(
                key,
                org,
                id,
                Uuid::nil(),
                "repair-mapping-cursor",
                &bytes[..24],
                &bytes[24..],
            )
            .map_err(|_| MigrationError::InvalidInput)?;
        if c.actor != ctx.actor_user_id.0 || c.revision != revision || c.limit != limit {
            return Err(MigrationError::Conflict);
        }
        Some(c.last)
    } else {
        None
    };
    let descriptors=sqlx::query(&format!("SELECT id FROM {p}_repair_key WHERE refresh_id=$1 AND organization_id=$2 AND ($3::uuid IS NULL OR id>$3) ORDER BY id LIMIT $4"))
        .bind(id).bind(org.0).bind(last).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
    let mut items = Vec::new();
    let mut position = last;
    let mut used = 0;
    let mut more = false;
    for descriptor in descriptors {
        if items.len() == usize::from(limit) {
            more = true;
            break;
        }
        let entry = sqlx::query(&format!(
            "SELECT * FROM {p}_repair_key WHERE id=$1 AND refresh_id=$2 AND organization_id=$3"
        ))
        .bind(descriptor.get::<Uuid, _>("id"))
        .bind(id)
        .bind(org.0)
        .fetch_one(&mut *tx)
        .await?;
        let source: SourceKey = owner.open(
            key,
            org,
            id,
            entry.get("id"),
            "repair-key",
            &entry.get::<Vec<u8>, _>("nonce"),
            &entry.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let hmac = entry.get::<Vec<u8>, _>("source_key_hmac");
        let choice=sqlx::query(&format!("SELECT * FROM {p}_repair_choice WHERE refresh_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4 AND revision<=$5 ORDER BY revision DESC LIMIT 1"))
            .bind(id).bind(org.0).bind(source.kind()).bind(&hmac).bind(revision).fetch_optional(&mut *tx).await?;
        let current = if let Some(choice) = choice {
            let body: Value = owner.open(
                key,
                org,
                id,
                choice.get("id"),
                "repair-choice",
                &choice.get::<Vec<u8>, _>("nonce"),
                &choice.get::<Vec<u8>, _>("ciphertext"),
            )?;
            json!({"disposition":choice.get::<String,_>("disposition"),"target_id":choice.get::<Option<Uuid>,_>("target_id"),"target":body["target"]})
        } else {
            Value::Null
        };
        let column = if source.kind() == "stage" {
            "stage_source_hmac"
        } else {
            "assignee_source_hmac"
        };
        let affected:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {p}_repair_candidate WHERE refresh_id=$1 AND organization_id=$2 AND {column}=$3"))
            .bind(id).bind(org.0).bind(hmac).fetch_one(&mut *tx).await?;
        let value = json!({"id":entry.get::<Uuid,_>("id"),"kind":source.kind(),"source":source,"affected_count":affected.to_string(),"choice":current});
        let bytes = serde_json::to_vec(&value)
            .map_err(|_| MigrationError::Crypto)?
            .len();
        if used + bytes > 192 * 1024 {
            if items.is_empty() {
                return Err(MigrationError::StorageLimit);
            }
            more = true;
            break;
        }
        used += bytes;
        position = Some(entry.get("id"));
        items.push(value);
    }
    let next = if more {
        let c = Cursor {
            actor: ctx.actor_user_id.0,
            revision,
            limit,
            last: position.ok_or(MigrationError::Crypto)?,
        };
        let sealed = owner.seal(key, org, id, Uuid::nil(), "repair-mapping-cursor", &c)?;
        Some(
            URL_SAFE_NO_PAD
                .encode([sealed.nonce.as_slice(), sealed.ciphertext.as_slice()].concat()),
        )
    } else {
        None
    };
    tx.commit().await?;
    Ok(json!({"items":items,"next_cursor":next,"draft_revision":revision.to_string()}))
}
