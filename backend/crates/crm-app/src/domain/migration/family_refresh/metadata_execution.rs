//! Atomic catalog prerequisites and whole-Person metadata execution.
use super::{
    catalog_plan,
    cohort::Claim,
    core_resolution::{self, Resolution},
    core_source::Derived,
    evidence::{Purpose, Scope},
    execution::{self, Progress, ResultUnit},
    metadata_baseline,
    metadata_delta::{self, Catalog, Field},
    metadata_destination,
    metadata_mapping::{self, Conversion},
    metadata_plan,
    model::{Counts, Hold},
    native_baseline::{AfterState, ResultData, State},
    native_write, preparation, source_policy,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::migration::{
        admitted_metadata_worker as registry, snapshot::SnapshotPolicy, MigrationError,
    },
};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

enum Input {
    Final(ResultUnit),
    Catalog(Box<metadata_destination::Evidence>, Derived),
    Person(
        Box<metadata_plan::Evidence>,
        Box<metadata_mapping::Converted>,
    ),
}
async fn input(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    unit: &PgRow,
) -> Result<Input, MigrationError> {
    let disposition = unit.get::<String, _>("disposition");
    if matches!(disposition.as_str(), "held" | "excluded") {
        return Ok(Input::Final(ResultUnit {
            disposition: if disposition == "held" {
                "held"
            } else {
                "excluded"
            },
            reason: None,
            person: unit.get("person_id"),
            target: unit.get("target_id"),
            revision: None,
            counts: Counts {
                units: 1,
                held: u64::from(disposition == "held"),
                excluded: u64::from(disposition == "excluded"),
                ..Counts::default()
            },
            data: ResultData { after_state: None },
            native_bytes: 0,
        }));
    }
    match unit.get::<String, _>("kind").as_str() {
        "catalog" => {
            let saved: catalog_plan::Evidence = scope.open(
                key,
                unit.get("id"),
                Purpose::Manifest,
                unit.get("nonce"),
                unit.get("ciphertext"),
            )?;
            let catalog_plan::Decision::Ready { evidence } = saved.decision else {
                return Err(MigrationError::Crypto);
            };
            let kind = if evidence.mapping.kind == "tag" {
                core_resolution::Kind::Person
            } else {
                core_resolution::Kind::Field
            };
            match core_resolution::resolve(pool, key, claim, kind, &evidence.frozen.source_id)
                .await?
            {
                Resolution::Held(h) => Ok(Input::Final(ResultUnit::held(unit, h))),
                Resolution::Ready(source) => Ok(Input::Catalog(evidence, source.record)),
            }
        }
        "metadata" => {
            let saved: metadata_plan::Evidence = scope.open(
                key,
                unit.get("id"),
                Purpose::Manifest,
                unit.get("nonce"),
                unit.get("ciphertext"),
            )?;
            let metadata_plan::Decision::Ready {
                ownership, source, ..
            } = &saved.decision
            else {
                return Err(MigrationError::Crypto);
            };
            match metadata_mapping::convert(pool, key, claim, &saved.source_id, &ownership.fields)
                .await?
            {
                Conversion::Held(h) => Ok(Input::Final(ResultUnit::held(unit, h))),
                Conversion::Ready(current) => {
                    if serde_json::to_value(&current.evidence)
                        .map_err(|_| MigrationError::Crypto)?
                        != serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?
                    {
                        return Ok(Input::Final(ResultUnit::held(unit, Hold::SourceConflict)));
                    }
                    Ok(Input::Person(Box::new(saved), current))
                }
            }
        }
        _ => Err(MigrationError::Conflict),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn apply(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: &ReleaseReadiness,
    claim: &Claim,
    scope: Scope,
    unit: PgRow,
) -> Result<Progress, MigrationError> {
    let prepared = input(pool, key, claim, scope, &unit).await?;
    let (mut tx, b, p) = execution::begin(pool, claim).await?;
    release
        .require_family_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    if p.get::<i64, _>("apply_position") + 1 != unit.get::<i64, _>("position")
        || p.get::<i64, _>("revision") != scope.revision
    {
        return Err(MigrationError::Conflict);
    }
    let bound = unit
        .get::<i64, _>("added_byte_bound")
        .checked_mul(2)
        .and_then(|n| n.checked_add(16384))
        .filter(|n| *n <= 64 * 1024 * 1024)
        .ok_or(MigrationError::StorageLimit)?;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Progress::Capacity);
    };
    let result_id = Uuid::new_v4();
    let result = match prepared {
        Input::Final(result) => result,
        Input::Catalog(approved, source) => {
            catalog(&mut tx, key, claim, scope, &b, &unit, &approved, &source).await?
        }
        Input::Person(approved, current) => {
            person(
                &mut tx, key, claim, scope, &b, &p, &unit, result_id, &approved, &current,
            )
            .await?
        }
    };
    execution::finish(
        tx,
        key,
        claim,
        scope,
        &p,
        &unit,
        result_id,
        reservation,
        result,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn catalog(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    b: &PgRow,
    unit: &PgRow,
    approved: &metadata_destination::Evidence,
    source: &Derived,
) -> Result<ResultUnit, MigrationError> {
    if let Some(h) = metadata_destination::revalidate_execution(
        conn,
        key,
        scope,
        b.get("source_account_id"),
        approved,
        source,
    )
    .await?
    {
        return Ok(ResultUnit::held(unit, h));
    }
    let proofs = native_write::load(conn, key, scope, unit.get("id")).await?;
    let inserting = unit.get::<String, _>("disposition") == "insert";
    if proofs.len() != usize::from(inserting) {
        return Err(MigrationError::Crypto);
    }
    for proof in &proofs {
        if !native_write::current_matches(conn, claim, proof).await? {
            return Ok(ResultUnit::held(unit, Hold::TargetUnavailable));
        }
    }
    let mut native_bytes = 0;
    for proof in &proofs {
        native_bytes += native_write::growth(conn, proof).await?;
        native_write::apply(conn, claim, proof).await?;
    }
    let current = registry::claim_state(
        conn,
        claim.organization,
        b.get("source_account_id"),
        &approved.mapping.kind,
        &approved.mapping.source_key,
    )
    .await?;
    if current.is_null() {
        let value = scope.seal(key, unit.get("id"), Purpose::CatalogClaim, &approved.frozen)?;
        sqlx::query("INSERT INTO migration_metadata_catalog_claim(organization_id,source_account_id,kind,source_key,target_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id,evidence_nonce,evidence_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(&approved.mapping.kind).bind(&approved.mapping.source_key).bind(approved.target).bind(claim.bundle).bind(claim.plan).bind(unit.get::<Uuid,_>("id")).bind(value.nonce.as_slice()).bind(value.ciphertext).execute(&mut *conn).await?;
    }
    Ok(ResultUnit {
        disposition: if inserting {
            "applied"
        } else {
            "already_current"
        },
        reason: None,
        person: None,
        target: Some(approved.target),
        revision: None,
        counts: serde_json::from_value(unit.get("counts")).map_err(|_| MigrationError::Crypto)?,
        data: ResultData { after_state: None },
        native_bytes,
    })
}
async fn live_catalog(
    conn: &mut PgConnection,
    claim: &Claim,
    fields: &[Uuid],
) -> Result<Catalog, MigrationError> {
    let tags: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM tag WHERE organization_id=$1 ORDER BY id LIMIT 201")
            .bind(claim.organization.0)
            .fetch_all(&mut *conn)
            .await?;
    if tags.len() > 200 || fields.len() > 50 {
        return Err(MigrationError::Conflict);
    }
    let rows=sqlx::query("SELECT f.id,f.field_type,ARRAY(SELECT o.id FROM custom_field_option o WHERE o.field_id=f.id AND o.organization_id=f.organization_id AND o.archived_at IS NULL ORDER BY o.id LIMIT 51) AS options FROM custom_field f WHERE f.organization_id=$1 AND f.id=ANY($2) AND f.archived_at IS NULL ORDER BY f.id LIMIT 51").bind(claim.organization.0).bind(fields).fetch_all(conn).await?;
    let mut live_fields = BTreeMap::new();
    for row in rows {
        let options: Vec<Uuid> = row.get("options");
        if options.len() > 50 {
            return Err(MigrationError::Conflict);
        }
        live_fields.insert(
            row.get("id"),
            Field {
                kind: serde_json::from_value(serde_json::json!(row.get::<String, _>("field_type")))
                    .map_err(|_| MigrationError::Crypto)?,
                live_options: options.into_iter().collect(),
            },
        );
    }
    Ok(Catalog {
        live_tags: tags.into_iter().collect(),
        live_fields,
    })
}
#[allow(clippy::too_many_arguments)]
async fn person(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    b: &PgRow,
    p: &PgRow,
    unit: &PgRow,
    result_id: Uuid,
    approved: &metadata_plan::Evidence,
    current_source: &metadata_mapping::Converted,
) -> Result<ResultUnit, MigrationError> {
    let metadata_plan::Decision::Ready {
        baseline,
        ownership,
        native,
        ..
    } = &approved.decision
    else {
        return Err(MigrationError::Crypto);
    };
    let c=sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(unit.get::<Uuid,_>("cohort_id")).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *conn).await?;
    let target:Option<Uuid>=sqlx::query_scalar("SELECT p.id FROM person p JOIN migration_import_identity i ON i.target_id=p.id AND i.organization_id=p.organization_id AND i.family='people' WHERE p.id=$1 AND p.organization_id=$2 AND i.source_account_id=$3 AND i.source_id=$4 FOR UPDATE OF p").bind(native.person).bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(c.get::<String,_>("source_person_id")).fetch_optional(&mut *conn).await?;
    if target.is_none() {
        return Ok(ResultUnit::held(unit, Hold::TargetErased));
    }
    if let Err(h) = source_policy::qualify_accepted_scan(conn, claim.organization, b, p, &c).await?
    {
        return Ok(ResultUnit::held(unit, h));
    }
    let head:Option<Uuid>=sqlx::query_scalar("SELECT result_id FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind='metadata' AND source_key_hmac=$3 FOR UPDATE").bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(unit.get::<Vec<u8>,_>("source_key_hmac")).fetch_optional(&mut *conn).await?;
    if head != native.expected_head || head != unit.get::<Option<Uuid>, _>("expected_head_id") {
        return Ok(ResultUnit::held(unit, Hold::StaleHead));
    }
    let mut current = metadata_baseline::observe(conn, claim.organization, native.person).await?;
    current.head = head;
    if current.revision != native.expected_revision || !current.state.matches(&baseline.state) {
        return Ok(ResultUnit::held(unit, Hold::LocalChange));
    }
    let dependencies: Vec<Uuid> = current_source
        .evidence
        .catalog_units
        .iter()
        .copied()
        .collect();
    if !dependencies_match(conn, key, scope, &dependencies).await? {
        return Ok(ResultUnit::held(unit, Hold::TargetUnavailable));
    }
    let fields: Vec<Uuid> = current_source
        .evidence
        .source
        .fields
        .iter()
        .map(|(id, _)| *id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let catalog = live_catalog(conn, claim, &fields).await?;
    let recomputed = match metadata_delta::propose(
        Some(baseline),
        Some(&current),
        ownership,
        &current_source.evidence.source,
        &catalog,
        true,
    ) {
        Ok(p) => p,
        Err(h) => return Ok(ResultUnit::held(unit, h)),
    };
    if serde_json::to_value(&recomputed).map_err(|_| MigrationError::Crypto)?
        != serde_json::to_value(native).map_err(|_| MigrationError::Crypto)?
    {
        return Ok(ResultUnit::held(unit, Hold::LocalChange));
    }
    let proofs = native_write::load(conn, key, scope, unit.get("id")).await?;
    if proofs.len() != native.changes.len() {
        return Err(MigrationError::Crypto);
    }
    for proof in &proofs {
        if !native_write::current_matches(conn, claim, proof).await? {
            return Ok(ResultUnit::held(unit, Hold::LocalChange));
        }
    }
    let mut native_bytes = 0;
    for proof in &proofs {
        native_bytes += native_write::growth(conn, proof).await?;
        native_write::apply(conn, claim, proof).await?;
    }
    let mut after = metadata_baseline::observe(conn, claim.organization, native.person).await?;
    if !after.state.matches(&native.after)
        || after.revision
            != native.expected_revision
                + i64::try_from(native.changes.len()).map_err(|_| MigrationError::Crypto)?
    {
        return Err(MigrationError::Crypto);
    }
    after.head = Some(result_id);
    let revision = after.revision;
    Ok(ResultUnit {
        disposition: if native.changes.is_empty() {
            "already_current"
        } else {
            "applied"
        },
        reason: None,
        person: Some(native.person),
        target: Some(native.person),
        revision: Some(revision),
        counts: native.counts.clone(),
        data: ResultData {
            after_state: Some(AfterState {
                version: 1,
                manifest: unit.get("id"),
                source_id: approved.source_id.clone(),
                person: native.person,
                target: native.person,
                state: State::Metadata {
                    snapshot: after,
                    ownership: native.ownership.clone(),
                },
            }),
        },
        native_bytes,
    })
}

/// Read all bounded catalog prerequisites together under the Organization lock.
/// Successful execution alone is insufficient: definitions must still equal
/// their approved snapshots (including timestamps) or exact inserted rows.
async fn dependencies_match(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    dependencies: &[Uuid],
) -> Result<bool, MigrationError> {
    use super::{mapping_inventory::Choice, mapping_selection::Destination};
    let rows = sqlx::query("SELECT u.id,u.nonce,u.ciphertext,n.body,EXISTS(SELECT 1 FROM migration_family_refresh_write_proof w WHERE w.manifest_id=r.manifest_id AND w.plan_id=r.plan_id AND w.organization_id=u.organization_id AND w.operation='INSERT' AND w.target_id=u.target_id AND w.after_hash=crm_family_refresh_native_digest(n.body)) AS inserted_matches FROM migration_family_refresh_manifest u JOIN migration_family_refresh_result r ON r.organization_id=u.organization_id AND ((u.inherited_result_id IS NULL AND r.manifest_id=u.id AND r.plan_id=u.plan_id) OR r.id=u.inherited_result_id) CROSS JOIN LATERAL (SELECT CASE m.kind WHEN 'tag' THEN (SELECT to_jsonb(t) FROM tag t WHERE t.id=u.target_id AND t.organization_id=u.organization_id) WHEN 'field' THEN (SELECT to_jsonb(f) FROM custom_field f WHERE f.id=u.target_id AND f.organization_id=u.organization_id AND f.archived_at IS NULL) WHEN 'option' THEN (SELECT to_jsonb(o) FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.id=u.target_id AND o.organization_id=u.organization_id AND o.archived_at IS NULL AND f.archived_at IS NULL) END AS body FROM migration_family_refresh_mapping m WHERE m.id=u.mapping_id AND m.plan_id=u.plan_id AND m.organization_id=u.organization_id) n WHERE u.plan_id=$1 AND u.bundle_id=$2 AND u.organization_id=$3 AND u.id=ANY($4) AND u.kind='catalog' AND r.disposition IN ('applied','already_current') ORDER BY u.position LIMIT 501")
        .bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).bind(dependencies).fetch_all(conn).await?;
    if rows.len() != dependencies.len() || rows.len() > 500 {
        return Ok(false);
    }
    for row in rows {
        let saved: catalog_plan::Evidence = scope.open(
            key,
            row.get("id"),
            Purpose::Manifest,
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        let catalog_plan::Decision::Ready { evidence } = saved.decision else {
            return Err(MigrationError::Crypto);
        };
        let Some(body) = row.get::<Option<serde_json::Value>, _>("body") else {
            return Ok(false);
        };
        match evidence.mapping.choice {
            Choice::CreateMatching { .. } => {
                if !row.get::<bool, _>("inserted_matches") {
                    return Ok(false);
                }
            }
            Choice::Existing { .. } => {
                let mut destination = body;
                destination["kind"] = serde_json::json!(evidence.mapping.kind);
                if evidence.mapping.kind == "tag" {
                    destination["label"] = destination["name"].clone();
                }
                let current: Destination =
                    serde_json::from_value(destination).map_err(|_| MigrationError::Crypto)?;
                if serde_json::to_value(Some(current)).map_err(|_| MigrationError::Crypto)?
                    != serde_json::to_value(&evidence.mapping.destination)
                        .map_err(|_| MigrationError::Crypto)?
                {
                    return Ok(false);
                }
            }
            _ => return Err(MigrationError::Crypto),
        }
    }
    Ok(true)
}
