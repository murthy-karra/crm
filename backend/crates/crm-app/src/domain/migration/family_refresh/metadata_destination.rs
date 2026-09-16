//! Read-only catalog prerequisites for explicit refresh choices. Prospective IDs
//! remain the IDs frozen by Plan; no native row or registry claim is created.
use super::{
    cohort::Claim,
    core_source::Derived,
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    mapping_selection::{self, Selection},
    metadata_catalog::{self, Qualification},
    model::{Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{
        admitted_metadata::FrozenMapping, admitted_metadata_worker as registry,
        metadata_source::Entity, MigrationError,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub mapping_id: Uuid,
    pub target: Uuid,
    pub field: Option<Uuid>,
    pub(super) mapping: Mapping,
    pub(super) parent: Option<Mapping>,
    pub(super) parent_frozen: Option<FrozenMapping>,
    pub(super) frozen: FrozenMapping,
}
pub enum Inspection {
    Ready(Box<Evidence>),
    Held(Hold),
}

async fn load(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    id: Uuid,
) -> Result<(Mapping, Option<Uuid>), MigrationError> {
    let row=sqlx::query("SELECT m.*,parent.source_key_hmac AS parent_key FROM migration_family_refresh_mapping m LEFT JOIN migration_family_refresh_mapping parent ON parent.id=m.parent_id AND parent.plan_id=m.plan_id AND parent.organization_id=m.organization_id WHERE m.id=$1 AND m.plan_id=$2 AND m.bundle_id=$3 AND m.organization_id=$4")
        .bind(id).bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).fetch_optional(conn).await?.ok_or(MigrationError::NotFound)?;
    let data: Mapping = scope.open(
        key,
        id,
        Purpose::Mapping,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    data.verify(&row)?;
    Ok((data, row.get("parent_id")))
}
fn frozen(data: &Mapping, source: &Derived) -> Result<FrozenMapping, MigrationError> {
    let Derived::Metadata(record) = source else {
        return Err(MigrationError::Crypto);
    };
    let element = data.source.as_ref().ok_or(MigrationError::Crypto)?.element as usize;
    let (definition, raw_choice) = match (&record.entity, data.kind.as_str()) {
        (Entity::Person(person), "tag") => (
            None,
            person
                .tags
                .get(element)
                .ok_or(MigrationError::Crypto)?
                .raw
                .clone(),
        ),
        (Entity::Field(field), "field") if element == 0 => (Some(field.clone()), None),
        (Entity::Field(field), "option") if element > 0 => (
            Some(field.clone()),
            field
                .choices
                .get(element - 1)
                .ok_or(MigrationError::Crypto)?
                .raw
                .clone(),
        ),
        _ => return Err(MigrationError::Crypto),
    };
    Ok(FrozenMapping {
        source_id: record.source_id.clone().unwrap_or_default(),
        label: data.label.clone(),
        machine_name: definition.as_ref().and_then(|f| f.name.clone()),
        field_type: definition.as_ref().and_then(|f| f.field_type.clone()),
        raw_choice,
        definition,
        target_baseline: Value::Null,
        claim_baseline: Value::Null,
        reasons: vec![],
    })
}

pub async fn inspect(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    id: Uuid,
) -> Result<Inspection, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata" || !p.get::<bool, _>("mappings_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: p.get("revision"),
    };
    let owner=sqlx::query("SELECT id,revision,family,phase,source_snapshot_id FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3")
        .bind(b.get::<Uuid,_>("payer_plan_id")).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let owner_family: Family =
        serde_json::from_value(serde_json::json!(owner.get::<String, _>("family")))
            .map_err(|_| MigrationError::Crypto)?;
    let snapshot = b
        .get::<Option<Uuid>, _>("core_snapshot_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    if owner_family == Family::History
        || owner.get::<Option<Uuid>, _>("source_snapshot_id") != Some(snapshot)
        || matches!(
            owner.get::<String, _>("phase").as_str(),
            "cohort" | "capture"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let source_scope = Scope {
        plan: owner.get("id"),
        revision: owner.get("revision"),
        family: owner_family,
        ..scope
    };
    if let Err(hold) =
        super::core_source::qualify_family(&mut tx, key, source_scope, snapshot, Family::Metadata)
            .await?
    {
        return Ok(Inspection::Held(hold));
    }
    let account = b.get("source_account_id");
    let (data, parent_id) = load(&mut tx, key, scope, id).await?;
    if !matches!(data.kind.as_str(), "tag" | "field" | "option") {
        return Err(MigrationError::InvalidInput);
    }
    if data.kind == "tag" && !b.get::<bool, _>("mapping_capture_order") {
        return Ok(Inspection::Held(Hold::SourceUnavailable));
    }
    let source = mapping_selection::source(&mut tx, key, scope, &data).await?;
    let mut frozen = frozen(&data, &source)?;
    drop(tx);
    if data.kind != "tag" {
        if let Qualification::Held(h) =
            metadata_catalog::qualify_field(pool, key, claim, &frozen.source_id).await?
        {
            return Ok(Inspection::Held(h));
        }
    }
    let (mut tx, _, _) = preparation::begin(pool, claim).await?;
    let ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE organization_id=$1 AND state='ready' AND engine_version='fub-admitted-metadata-v1')")
        .bind(claim.organization.0).fetch_one(&mut *tx).await?;
    if !ready {
        return Err(MigrationError::ReleaseNotReady);
    }
    let context = Context {
        key,
        scope,
        account,
    };
    let mut parent_evidence = None;
    let parent = if let Some(parent_id) = parent_id {
        let (parent, _) = load(&mut tx, key, scope, parent_id).await?;
        if parent.kind != "field" || data.parent_key.as_ref() != Some(&parent.source_key) {
            return Err(MigrationError::Crypto);
        }
        let parent_source = mapping_selection::source(&mut tx, key, scope, &parent).await?;
        let mut parent_frozen = self::frozen(&parent, &parent_source)?;
        if parent_frozen.source_id != frozen.source_id {
            return Err(MigrationError::Crypto);
        }
        if let Some(h) = context
            .validate(&mut tx, &parent, &parent_source, &mut parent_frozen, None)
            .await?
        {
            return Ok(Inspection::Held(h));
        }
        parent_evidence = Some(parent_frozen);
        Some(parent)
    } else {
        None
    };
    let field = parent.as_ref().and_then(|p| p.choice.columns().1);
    if data.kind == "option" && field.is_none() {
        return Ok(Inspection::Held(Hold::MappingRequired));
    }
    if let Some(h) = context
        .validate(&mut tx, &data, &source, &mut frozen, field)
        .await?
    {
        return Ok(Inspection::Held(h));
    }
    let target = data.choice.columns().1.ok_or(MigrationError::Crypto)?;
    Ok(Inspection::Ready(Box::new(Evidence {
        mapping_id: id,
        target,
        field,
        mapping: data,
        parent,
        parent_frozen: parent_evidence,
        frozen,
    })))
}
struct Context<'a> {
    key: &'a RawPayloadKey,
    scope: Scope,
    account: i64,
}
impl Context<'_> {
    async fn validate(
        &self,
        conn: &mut PgConnection,
        data: &Mapping,
        source: &Derived,
        frozen: &mut FrozenMapping,
        parent: Option<Uuid>,
    ) -> Result<Option<Hold>, MigrationError> {
        if !data.qualified {
            return Ok(Some(Hold::UnsupportedSource));
        }
        let selection = match data.choice {
            Choice::Existing { target } => Selection::Existing { target_id: target },
            Choice::CreateMatching { .. } => Selection::CreateMatching,
            _ => return Ok(Some(Hold::MappingRequired)),
        };
        let (_, current) = match mapping_selection::validate(
            conn,
            self.scope.organization,
            data,
            source,
            &selection,
            parent,
        )
        .await
        {
            Ok(v) => v,
            Err(MigrationError::InvalidImportChoice) => return Ok(Some(Hold::TargetUnavailable)),
            Err(e) => return Err(e),
        };
        if serde_json::to_value(&current).map_err(|_| MigrationError::Crypto)?
            != serde_json::to_value(&data.destination).map_err(|_| MigrationError::Crypto)?
        {
            return Ok(Some(Hold::TargetUnavailable));
        }
        let target = data.choice.columns().1.ok_or(MigrationError::Crypto)?;
        if matches!(data.kind.as_str(), "field" | "option") {
            let claims:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT 1 FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND target_id=$4 LIMIT 2) claims")
                .bind(self.scope.plan).bind(self.scope.organization.0).bind(&data.kind).bind(target).fetch_one(&mut *conn).await?;
            if claims != 1 {
                return Ok(Some(Hold::SourceConflict));
            }
        }

        frozen.target_baseline =
            registry::target_state(conn, self.scope.organization, &data.kind, target).await?;
        frozen.claim_baseline = registry::claim_state(
            conn,
            self.scope.organization,
            self.account,
            &data.kind,
            &data.source_key,
        )
        .await?;
        if !frozen.claim_baseline.is_null()
            && !registry::claim_equal(
                conn,
                self.key,
                self.scope.organization,
                self.account,
                &data.kind,
                &data.source_key,
                frozen,
                target,
            )
            .await?
        {
            return Ok(Some(Hold::SourceConflict));
        }
        let previous:Option<Uuid>=sqlx::query_scalar("SELECT target_id FROM migration_metadata_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key=$4")
            .bind(self.scope.organization.0).bind(self.account).bind(&data.kind).bind(&data.source_key).fetch_optional(&mut *conn).await?;
        if previous.is_some_and(|id| id != target) {
            return Ok(Some(Hold::IdentityMismatch));
        }
        if matches!(data.choice, Choice::CreateMatching { .. }) {
            if !frozen.target_baseline.is_null()
                || !frozen.claim_baseline.is_null()
                || previous.is_some()
            {
                return Ok(Some(Hold::TargetUnavailable));
            }
            if !self.creation_available(conn, data, frozen, parent).await? {
                return Ok(Some(Hold::TargetUnavailable));
            }
        }
        Ok(None)
    }
    async fn creation_available(
        &self,
        conn: &mut PgConnection,
        data: &Mapping,
        frozen: &FrozenMapping,
        parent: Option<Uuid>,
    ) -> Result<bool, MigrationError> {
        let limit = if data.kind == "tag" { 200_i64 } else { 50_i64 };
        // At most native capacity + 1 candidates, including choices sharing a
        // destination field through different source definitions.
        let rows=sqlx::query("SELECT m.*,p.source_key_hmac AS parent_key FROM migration_family_refresh_mapping m LEFT JOIN migration_family_refresh_mapping p ON p.id=m.parent_id AND p.plan_id=m.plan_id AND p.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.organization_id=$2 AND m.kind=$3 AND m.disposition='create_matching' AND ($3<>'option' OR p.target_id=$4) ORDER BY m.id LIMIT $5")
            .bind(self.scope.plan).bind(self.scope.organization.0).bind(&data.kind).bind(parent).bind(limit+1).fetch_all(&mut *conn).await?;
        if rows.len() as i64 > limit {
            return Ok(false);
        }
        let mut labels = Vec::with_capacity(rows.len());
        for row in rows {
            let mapping: Mapping = self.scope.open(
                self.key,
                row.get("id"),
                Purpose::Mapping,
                row.get("nonce"),
                row.get("ciphertext"),
            )?;
            mapping.verify(&row)?;
            labels.push(mapping.label.ok_or(MigrationError::Crypto)?);
        }
        let collision: bool = sqlx::query_scalar(
            "SELECT count(*)>1 FROM unnest($1::text[]) labels(label) WHERE lower(label)=lower($2)",
        )
        .bind(&labels)
        .bind(&frozen.label)
        .fetch_one(&mut *conn)
        .await?;
        if collision {
            return Ok(false);
        }
        let (count_sql,collision_sql)=match data.kind.as_str(){
            "tag"=>("SELECT count(*) FROM tag WHERE organization_id=$1", "SELECT EXISTS(SELECT 1 FROM tag WHERE organization_id=$1 AND lower(name)=lower($2))"),
            "field"=>("SELECT count(*) FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL", "SELECT EXISTS(SELECT 1 FROM custom_field WHERE organization_id=$1 AND archived_at IS NULL AND (lower(label)=lower($2) OR (source='fub' AND external_key=$3)))"),
            "option"=>("SELECT count(*) FROM custom_field_option WHERE organization_id=$1 AND field_id=$2 AND archived_at IS NULL", "SELECT EXISTS(SELECT 1 FROM custom_field_option WHERE organization_id=$1 AND field_id=$3 AND archived_at IS NULL AND lower(label)=lower($2))"),
            _=>return Err(MigrationError::Crypto),
        };
        let count = sqlx::query_scalar::<_, i64>(count_sql).bind(self.scope.organization.0);
        let count = if data.kind == "option" {
            count.bind(parent).fetch_one(&mut *conn).await?
        } else {
            count.fetch_one(&mut *conn).await?
        };
        if count + labels.len() as i64 > limit {
            return Ok(false);
        }
        let collision = sqlx::query_scalar::<_, bool>(collision_sql)
            .bind(self.scope.organization.0)
            .bind(&frozen.label);
        let collision = match data.kind.as_str() {
            "field" => {
                collision
                    .bind(&frozen.machine_name)
                    .fetch_one(&mut *conn)
                    .await?
            }
            "option" => collision.bind(parent).fetch_one(&mut *conn).await?,
            _ => collision.fetch_one(&mut *conn).await?,
        };
        if collision {
            return Ok(false);
        }
        if data.kind == "field" && frozen.field_type.as_deref() == Some("choice") {
            let expected = frozen
                .definition
                .as_ref()
                .ok_or(MigrationError::Crypto)?
                .choices
                .len();
            let children:i64=sqlx::query_scalar("SELECT count(*) FROM migration_family_refresh_mapping c JOIN migration_family_refresh_mapping p ON p.id=c.parent_id AND p.plan_id=c.plan_id AND p.organization_id=c.organization_id WHERE c.plan_id=$1 AND c.organization_id=$2 AND p.source_key_hmac=$3 AND c.kind='option' AND c.disposition='create_matching' AND c.qualified")
                .bind(self.scope.plan).bind(self.scope.organization.0).bind(&data.source_key).fetch_one(&mut *conn).await?;
            if usize::try_from(children).ok() != Some(expected) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
