//! At most 50 mapping elements from one authenticated source occurrence. Shared
//! source rows retain their original encryption scope; mappings default to hold.
use super::{
    cohort::Claim,
    core_source::Derived,
    evidence::{Purpose, Scope},
    model::Family,
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        migration::{
            activity_store, metadata_source::Entity, metadata_store, snapshot::SnapshotPolicy,
            MigrationError,
        },
        task::TaskKind,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Choice {
    Hold,
    Existing {
        #[serde(rename = "target_id")]
        target: Uuid,
    },
    CreateMatching {
        #[serde(rename = "target_id")]
        target: Uuid,
    },
    Unassigned,
    Kind {
        kind: TaskKind,
    },
    Timezone {
        zone: String,
    },
}
impl Choice {
    pub(super) fn columns(&self) -> (&'static str, Option<Uuid>) {
        match self {
            Self::Hold => ("hold", None),
            Self::Existing { target } => ("existing", Some(*target)),
            Self::CreateMatching { target } => ("create_matching", Some(*target)),
            Self::Unassigned => ("unassigned", None),
            Self::Kind { .. } => ("kind", None),
            Self::Timezone { .. } => ("timezone", None),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Reference {
    pub row: Uuid,
    pub element: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub kind: String,
    pub source_key: Vec<u8>,
    pub parent_key: Option<Vec<u8>>,
    /// Full values stay in this exact source reference, including malformed or
    /// oversized labels. The summary label is never a replacement for evidence.
    pub source: Option<Reference>,
    pub label: Option<String>,
    /// Intrinsic value validity only. Full source-group qualification and native
    /// authorization are independently rechecked when preparing/applying a unit.
    pub qualified: bool,
    pub creation_allowed: bool,
    pub choice: Choice,
    #[serde(default)]
    pub(super) destination: Option<super::mapping_selection::Destination>,
}
impl Mapping {
    pub(super) fn verify(&self, row: &sqlx::postgres::PgRow) -> Result<(), MigrationError> {
        let (disposition, target) = self.choice.columns();
        let source_bound = match &self.source {
            Some(reference) => {
                Some(reference.row) == row.get::<Option<Uuid>, _>("source_row_id")
                    && i32::try_from(reference.element).ok()
                        == row.get::<Option<i32>, _>("source_element")
            }
            None => {
                self.kind == "timezone"
                    && row.get::<Option<Uuid>, _>("source_row_id").is_none()
                    && row.get::<Option<i32>, _>("source_element").is_none()
            }
        };
        let destination_bound = match (&self.choice, &self.destination) {
            (Choice::Existing { target }, Some(destination)) => *target == destination.id(),
            (Choice::Existing { .. }, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        if !source_bound
            || !destination_bound
            || self.kind != row.get::<String, _>("kind")
            || self.source_key != row.get::<Vec<u8>, _>("source_key_hmac")
            || self.parent_key != row.get::<Option<Vec<u8>>, _>("parent_key")
            || self.qualified != row.get::<bool, _>("qualified")
            || disposition != row.get::<String, _>("disposition")
            || target != row.get::<Option<Uuid>, _>("target_id")
        {
            return Err(MigrationError::Crypto);
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Finished,
    Capacity,
}
struct Seed {
    kind: &'static str,
    purpose: &'static str,
    raw: Vec<u8>,
    parent: Option<Vec<u8>>,
    label: Option<String>,
    qualified: bool,
    creation_allowed: bool,
}
fn slots(source: &Derived) -> usize {
    match source {
        Derived::Metadata(r) => match &r.entity {
            Entity::Person(p) => p.tags.len(),
            Entity::Field(f) => 1 + f.choices.len(),
            Entity::Invalid => 0,
        },
        Derived::Activity(r)
            if r.stream == crate::domain::migration::snapshot_source::Stream::Notes =>
        {
            0
        }
        Derived::Activity(r) => {
            r.roles.len()
                + usize::from(
                    matches!(r.stream.as_str(), "tasks_open" | "tasks_completed")
                        && r.source_type.is_some(),
                )
        }
        Derived::Oversized { .. } => 0,
    }
}
fn seed(source: &Derived, index: usize) -> Result<Option<Seed>, MigrationError> {
    let out = match source {
        Derived::Metadata(r) => match &r.entity {
            Entity::Person(p) => {
                let Some(tag) = p.tags.get(index) else {
                    return Ok(None);
                };
                let Some(raw) = tag.raw.as_ref() else {
                    return Ok(None);
                };
                Seed {
                    kind: "tag",
                    purpose: if tag.label.is_some() {
                        "tag-group"
                    } else {
                        "tag-raw"
                    },
                    raw: raw.as_bytes().to_vec(),
                    parent: None,
                    label: tag.label.clone(),
                    qualified: tag.reasons.is_empty(),
                    creation_allowed: tag.label.is_some() && tag.reasons.is_empty(),
                }
            }
            Entity::Field(f) => {
                let Some(id) = r.source_id.as_ref() else {
                    return Ok(None);
                };
                if index == 0 {
                    Seed {
                        kind: "field",
                        purpose: "field",
                        raw: id.as_bytes().to_vec(),
                        parent: None,
                        label: f.label.clone(),
                        qualified: f.reasons.is_empty(),
                        creation_allowed: f.creation_reasons.is_empty() && f.reasons.is_empty(),
                    }
                } else {
                    let Some(choice) = f.choices.get(index - 1) else {
                        return Ok(None);
                    };
                    let Some(raw) = choice.raw.as_ref() else {
                        return Ok(None);
                    };
                    Seed {
                        kind: "option",
                        purpose: "option",
                        raw: serde_json::to_vec(&(id, raw)).map_err(|_| MigrationError::Crypto)?,
                        parent: Some(id.as_bytes().to_vec()),
                        label: choice.label.clone(),
                        qualified: f.reasons.is_empty() && choice.reasons.is_empty(),
                        creation_allowed: choice.label.is_some() && choice.reasons.is_empty(),
                    }
                }
            }
            Entity::Invalid => return Ok(None),
        },
        Derived::Activity(r) => {
            let (kind, value) = if let Some(role) = r.roles.get(index) {
                let kind = match role.role.as_str() {
                    "note_author" => "note_author",
                    "task_creator" => "task_creator",
                    "task_assignee" => "task_assignee",
                    _ => return Err(MigrationError::Crypto),
                };
                (kind, role.source_id.as_str())
            } else if index == r.roles.len()
                && matches!(r.stream.as_str(), "tasks_open" | "tasks_completed")
            {
                let Some(value) = r.source_type.as_deref() else {
                    return Ok(None);
                };
                ("task_kind", value)
            } else {
                return Ok(None);
            };
            Seed {
                kind,
                purpose: kind,
                raw: value.as_bytes().to_vec(),
                parent: None,
                label: Some(value.into()),
                qualified: true,
                creation_allowed: false,
            }
        }
        Derived::Oversized { .. } => return Ok(None),
    };
    Ok(Some(out))
}
struct Pending {
    id: Uuid,
    parent: Option<Uuid>,
    element: i32,
    data: Mapping,
    sealed: crate::domain::migration::crypto::Sealed,
}
struct Input<'a> {
    key: &'a RawPayloadKey,
    scope: Scope,
    account: i64,
    source: Uuid,
}
async fn mappings(
    conn: &mut PgConnection,
    input: Input<'_>,
    record: &Derived,
    offset: usize,
    end: usize,
) -> Result<Vec<Pending>, MigrationError> {
    let mut ids = BTreeMap::<(&'static str, Vec<u8>), Uuid>::new();
    let mut pending = Vec::new();
    for element in offset..end {
        let Some(mut seed) = seed(record, element)? else {
            continue;
        };
        // Match the database's existing tag-group folding, including its locale.
        if seed.purpose == "tag-group" {
            let group: String = sqlx::query_scalar("SELECT lower($1::text)")
                .bind(seed.label.as_deref())
                .fetch_one(&mut *conn)
                .await?;
            seed.raw = group.into_bytes();
        }
        let hash = if input.scope.family == Family::Metadata {
            metadata_store::source_key(
                input.key,
                input.scope.organization,
                input.account,
                seed.purpose,
                &seed.raw,
            )
        } else {
            activity_store::source_key(
                input.key,
                input.scope.organization,
                input.account,
                seed.purpose,
                &seed.raw,
            )
        };
        let parent_key = seed.parent.as_ref().map(|v| {
            metadata_store::source_key(
                input.key,
                input.scope.organization,
                input.account,
                "field",
                v,
            )
        });
        if ids.contains_key(&(seed.kind, hash.clone())) {
            continue;
        }
        let parent = if let Some(key) = parent_key.as_ref() {
            if let Some(id) = ids.get(&("field", key.clone())) {
                Some(*id)
            } else {
                Some(sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND source_key_hmac=$3")
                    .bind(input.scope.plan).bind(input.scope.organization.0).bind(key).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Crypto)?)
            }
        } else {
            None
        };
        if let Some(existing)=sqlx::query("SELECT id,parent_id,source_row_id,source_element,qualified,nonce,ciphertext FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4")
            .bind(input.scope.plan).bind(input.scope.organization.0).bind(seed.kind).bind(&hash).fetch_optional(&mut *conn).await? {
            let id:Uuid=existing.get("id");
            let data:Mapping=input.scope.open(input.key,id,Purpose::Mapping,existing.get("nonce"),existing.get("ciphertext"))?;
            let bound_source = data.source.as_ref().is_some_and(|reference| {
                Some(reference.row) == existing.get::<Option<Uuid>, _>("source_row_id")
                    && i32::try_from(reference.element).ok() == existing.get::<Option<i32>, _>("source_element")
            });
            if !bound_source || data.qualified != existing.get::<bool, _>("qualified") || data.kind!=seed.kind || data.source_key!=hash || data.parent_key!=parent_key || existing.get::<Option<Uuid>,_>("parent_id")!=parent {return Err(MigrationError::Crypto);}
            ids.insert((seed.kind,hash),id);
            continue;
        }
        let id = Uuid::new_v4();
        let mut data = Mapping {
            kind: seed.kind.into(),
            source_key: hash.clone(),
            parent_key,
            source: Some(Reference {
                row: input.source,
                element: element as u32,
            }),
            label: seed.label.filter(|s| s.len() <= 2048),
            qualified: seed.qualified,
            creation_allowed: seed.qualified && seed.creation_allowed,
            choice: Choice::Hold,
            destination: None,
        };
        super::mapping_inheritance::apply(conn, input.key, input.scope, &mut data).await?;
        let sealed = input.scope.seal(input.key, id, Purpose::Mapping, &data)?;
        pending.push(Pending {
            id,
            parent,
            element: i32::try_from(element).map_err(|_| MigrationError::Crypto)?,
            data,
            sealed,
        });
        ids.insert((seed.kind, hash), id);
    }
    Ok(pending)
}

pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    let family: Family = serde_json::from_value(serde_json::json!(p.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    if family == Family::History || p.get::<String, _>("phase") != "mappings" {
        return Err(MigrationError::Conflict);
    }
    if p.get::<bool, _>("mappings_complete") {
        return Ok(Progress::Finished);
    }
    let after: Option<Uuid> = p.get("mapping_after");
    let next: Option<Uuid> =
        sqlx::query_scalar("SELECT crm_family_refresh_next_mapping_source($1,$2,$3,$4)")
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(family.as_str())
            .bind(after)
            .fetch_one(&mut *tx)
            .await?;
    let Some(source) = next else {
        sqlx::query("UPDATE migration_family_refresh_plan SET mappings_complete=true WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    if p.get::<Option<Uuid>, _>("mapping_current")
        .is_some_and(|id| id != source)
    {
        return Err(MigrationError::Crypto);
    }
    let row=sqlx::query("SELECT s.*,owner.family AS owner_family,owner.revision AS owner_revision,owner.phase AS owner_phase FROM migration_family_refresh_source s JOIN migration_family_refresh_plan owner ON owner.id=s.plan_id AND owner.bundle_id=s.bundle_id AND owner.organization_id=s.organization_id WHERE s.id=$1 AND s.bundle_id=$2 AND s.organization_id=$3")
        .bind(source).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let owner_family: Family =
        serde_json::from_value(serde_json::json!(row.get::<String, _>("owner_family")))
            .map_err(|_| MigrationError::Crypto)?;
    if owner_family == Family::History
        || matches!(
            row.get::<String, _>("owner_phase").as_str(),
            "cohort" | "capture"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let source_scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: row.get("plan_id"),
        family: owner_family,
        revision: row.get("owner_revision"),
    };
    let record: Derived = source_scope.open(
        key,
        source,
        Purpose::Source,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let source_id = row.get::<Option<String>, _>("source_id");
    let person_id = row.get::<Option<String>, _>("source_person_id");
    let kind = row.get::<String, _>("kind");
    let consistent = match &record {
        Derived::Metadata(record) => {
            family == Family::Metadata
                && matches!(kind.as_str(), "person" | "field")
                && record.source_id == source_id
                && (kind != "person" || person_id == source_id)
        }
        Derived::Activity(record) => {
            family == Family::Activity
                && record.source_id == source_id
                && record.person_id == person_id
                && matches!(
                    (kind.as_str(), record.stream.as_str()),
                    ("note", "notes" | "note_detail") | ("task", "tasks_open" | "tasks_completed")
                )
        }
        Derived::Oversized { .. } => true,
    };
    if !consistent {
        return Err(MigrationError::Crypto);
    }
    let eligible = if row.get::<String, _>("kind") == "field" {
        true
    } else {
        sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND organization_id=$2 AND source_person_id=$3)")
            .bind(claim.bundle).bind(claim.organization.0).bind(row.get::<Option<String>,_>("source_person_id")).fetch_one(&mut *tx).await?
    };
    let total = if eligible { slots(&record) } else { 0 };
    let offset =
        usize::try_from(p.get::<i32, _>("mapping_offset")).map_err(|_| MigrationError::Crypto)?;
    if offset > total {
        return Err(MigrationError::Crypto);
    }
    let end = (offset + 50).min(total);
    let scope = Scope {
        plan: claim.plan,
        family,
        revision: p.get("revision"),
        ..source_scope
    };
    let pending = mappings(
        &mut tx,
        Input {
            key,
            scope,
            account: b.get("source_account_id"),
            source,
        },
        &record,
        offset,
        end,
    )
    .await?;
    let reservation = if pending.is_empty() {
        None
    } else {
        let bound = pending
            .iter()
            .try_fold(4096_i64, |sum, row| {
                i64::try_from(row.sealed.ciphertext.len())
                    .ok()
                    .and_then(|size| sum.checked_add(size + 2048))
            })
            .ok_or(MigrationError::StorageLimit)?;
        let Some(token) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
            return Ok(Progress::Capacity);
        };
        Some(token)
    };
    let added = pending.len();
    for pending in pending {
        sqlx::query("INSERT INTO migration_family_refresh_mapping(id,bundle_id,plan_id,organization_id,kind,source_key_hmac,parent_id,disposition,nonce,ciphertext,source_row_id,source_element,qualified,target_id) VALUES($1,$2,$3,$4,$5,$6,$7,$13,$8,$9,$10,$11,$12,$14)")
            .bind(pending.id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(&pending.data.kind).bind(&pending.data.source_key).bind(pending.parent)
            .bind(pending.sealed.nonce.as_slice()).bind(pending.sealed.ciphertext).bind(source).bind(pending.element).bind(pending.data.qualified).bind(pending.data.choice.columns().0).bind(pending.data.choice.columns().1).execute(&mut *tx).await?;
    }
    if end == total {
        sqlx::query("UPDATE migration_family_refresh_plan SET mapping_after=$3,mapping_current=NULL,mapping_offset=0 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(source).execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE migration_family_refresh_plan SET mapping_current=$3,mapping_offset=$4 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(source).bind(i32::try_from(end).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    }
    if let Some(reservation) = reservation {
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(reservation)
            .bind(claim.epoch)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,source_id=%source,added,"Family refresh mappings discovered");
    Ok(Progress::Advanced)
}
