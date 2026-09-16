//! Convert reconciled retained activity through this plan's explicit mappings.
//! This is preparation evidence, never permission to mutate a native record.
use super::{
    activity_delta::{Content, Members, Source},
    cohort::Claim,
    core_resolution::{self, Resolution},
    core_source::Derived,
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    mapping_selection::{self, Selection},
    model::{Family, Hold, Kind},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{
        activity_source::{self, NativeActivity},
        activity_store, MigrationError,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub source_row: Uuid,
    pub source_person: String,
    pub semantic: Vec<u8>,
    pub source: Source,
    pub transformations: Vec<String>,
    pub source_only: BTreeMap<String, u64>,
    /// Choices and destination snapshots remain bound to their retained source
    /// keys. Execution revalidates them; these are not client-supplied targets.
    pub(super) mappings: Vec<Mapping>,
}
pub struct Converted {
    pub evidence: Evidence,
    pub members: Members,
}
pub enum Conversion {
    Ready(Box<Converted>),
    Held(Hold),
}

async fn mapping(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    account: i64,
    kind: &str,
    raw: &str,
) -> Result<Option<Mapping>, MigrationError> {
    let hash = activity_store::source_key(key, scope.organization, account, kind, raw.as_bytes());
    let row=sqlx::query("SELECT *,NULL::bytea AS parent_key FROM migration_family_refresh_mapping WHERE plan_id=$1 AND bundle_id=$2 AND organization_id=$3 AND kind=$4 AND source_key_hmac=$5")
        .bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).bind(kind).bind(hash).fetch_optional(conn).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let data: Mapping = scope.open(
        key,
        row.get("id"),
        Purpose::Mapping,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    data.verify(&row)?;
    Ok(Some(data))
}

pub async fn convert(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    kind: Kind,
    source_id: &str,
) -> Result<Conversion, MigrationError> {
    let source_kind = match kind {
        Kind::Note => core_resolution::Kind::Note,
        Kind::Task => core_resolution::Kind::Task,
        _ => return Err(MigrationError::InvalidInput),
    };
    let selected = match core_resolution::resolve(pool, key, claim, source_kind, source_id).await? {
        Resolution::Ready(r) => r,
        Resolution::Held(h) => return Ok(Conversion::Held(h)),
    };
    let Derived::Activity(record) = &selected.record else {
        return Err(MigrationError::Crypto);
    };
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "activity" || !p.get::<bool, _>("mappings_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Activity,
        revision: p.get("revision"),
    };
    let account = b.get("source_account_id");
    let timezone = mapping(&mut tx, key, scope, account, "timezone", "source_timezone").await?;
    let zone = match timezone.as_ref().map(|m| &m.choice) {
        Some(Choice::Timezone { zone }) if activity_source::valid_timezone(zone) => {
            Some(zone.as_str())
        }
        None | Some(Choice::Hold) => None,
        _ => return Err(MigrationError::Crypto),
    };
    let preview = activity_source::preview(record, zone);
    if !preview.reasons.is_empty() {
        return Ok(Conversion::Held(
            if preview
                .reasons
                .iter()
                .all(|r| r == "source_timezone_confirmation_required")
            {
                Hold::MappingRequired
            } else {
                Hold::UnsupportedSource
            },
        ));
    }
    let Some(native) = preview.native else {
        return Ok(Conversion::Held(Hold::UnsupportedSource));
    };
    // Reconcile source-user evidence separately, before taking this plan's lock
    // again. Missing user rows do not invent a timezone; conflicting evidence
    // cannot be hidden by picking one occurrence.
    drop(tx);
    if kind == Kind::Task
        && zone.is_some()
        && record
            .provenance
            .get("dueDate")
            .is_some_and(|v| v != "null")
    {
        if let Some(assignee) = record.roles.iter().find(|r| r.role == "task_assignee") {
            match core_resolution::resolve(
                pool,
                key,
                claim,
                core_resolution::Kind::User,
                &assignee.source_id,
            )
            .await?
            {
                Resolution::Held(Hold::SourceNotObserved) => {}
                Resolution::Held(h) => return Ok(Conversion::Held(h)),
                Resolution::Ready(user) => {
                    let Derived::Activity(user) = &user.record else {
                        return Err(MigrationError::Crypto);
                    };
                    for field in ["timezone", "timeZone"] {
                        if let Some(raw) = user.provenance.get(field) {
                            if serde_json::from_str::<String>(raw)
                                .ok()
                                .as_deref()
                                .is_some_and(|actual| Some(actual) != zone)
                            {
                                return Ok(Conversion::Held(Hold::SourceConflict));
                            }
                        }
                    }
                }
            }
        }
    }
    let (mut tx, _, _) = preparation::begin(pool, claim).await?;
    let mut uses = timezone.into_iter().collect::<Vec<_>>();
    let mut roles = BTreeMap::<String, Option<Uuid>>::new();
    let mut members = Members {
        all: Default::default(),
        active: Default::default(),
    };
    for role in &record.roles {
        let Some(data) = mapping(&mut tx, key, scope, account, &role.role, &role.source_id).await?
        else {
            return Ok(Conversion::Held(Hold::MappingRequired));
        };
        let selection = match data.choice {
            Choice::Existing { target } => Selection::Existing { target_id: target },
            Choice::Unassigned => Selection::Unassigned,
            _ => return Ok(Conversion::Held(Hold::MappingRequired)),
        };
        let (_, current) = match mapping_selection::validate(
            &mut tx,
            scope.organization,
            &data,
            &selected.record,
            &selection,
            None,
        )
        .await
        {
            Ok(value) => value,
            Err(MigrationError::InvalidImportChoice) => {
                return Ok(Conversion::Held(Hold::TargetUnavailable))
            }
            Err(e) => return Err(e),
        };
        if serde_json::to_value(&data.destination).map_err(|_| MigrationError::Crypto)?
            != serde_json::to_value(&current).map_err(|_| MigrationError::Crypto)?
        {
            return Ok(Conversion::Held(Hold::TargetUnavailable));
        }
        let target = data.choice.columns().1;
        if let Some(target) = target {
            members.all.insert(target);
            if current.as_ref().is_some_and(|d|matches!(d,mapping_selection::Destination::Member{status,..} if status=="active")) {members.active.insert(target);}
        }
        roles.insert(role.role.clone(), target);
        uses.push(data);
    }
    let role = |kind: &str, source: &Option<String>| -> Result<Option<Uuid>, MigrationError> {
        if source.is_none() {
            Ok(None)
        } else {
            roles.get(kind).copied().ok_or(MigrationError::Crypto)
        }
    };
    let source = match native {
        NativeActivity::Note {
            body,
            author_source_id,
            created_at,
            updated_at,
        } => Source {
            created: created_at,
            updated: updated_at,
            content: Content::Note {
                body,
                author: role("note_author", &author_source_id)?,
            },
        },
        NativeActivity::Task {
            title,
            source_type,
            creator_source_id,
            assignee_source_id,
            created_at,
            updated_at,
            due_at,
            completed_at,
        } => {
            let Some(data) =
                mapping(&mut tx, key, scope, account, "task_kind", &source_type).await?
            else {
                return Ok(Conversion::Held(Hold::MappingRequired));
            };
            let Choice::Kind { kind } = data.choice else {
                return Ok(Conversion::Held(Hold::MappingRequired));
            };
            let result = Source {
                created: created_at,
                updated: updated_at,
                content: Content::Task {
                    title,
                    kind,
                    creator: role("task_creator", &creator_source_id)?,
                    assignee: role("task_assignee", &assignee_source_id)?,
                    due: due_at,
                    completed: completed_at,
                },
            };
            uses.push(data);
            result
        }
    };
    Ok(Conversion::Ready(Box::new(Converted {
        evidence: Evidence {
            source_row: selected.row,
            source_person: selected.source_person.ok_or(MigrationError::Crypto)?,
            semantic: selected.semantic,
            source,
            transformations: preview.transformations,
            source_only: preview.source_only,
            mappings: uses,
        },
        members,
    })))
}
