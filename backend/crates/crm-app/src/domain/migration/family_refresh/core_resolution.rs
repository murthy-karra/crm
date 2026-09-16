//! Reconcile the complete shared core index before choosing one authenticated
//! representation. No cohort filter may hide an occurrence or a Person conflict.
use super::{
    cohort::Claim,
    core_source::Derived,
    evidence::{Purpose, Scope},
    model::{Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{snapshot_source::Stream, MigrationError},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Kind {
    Person,
    Field,
    User,
    Note,
    Task,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Field => "field",
            Self::User => "user",
            Self::Note => "note",
            Self::Task => "task",
        }
    }
    fn stream(self) -> Stream {
        match self {
            Self::Person => Stream::People,
            Self::Field => Stream::CustomFields,
            Self::User => Stream::Users,
            Self::Note => Stream::NoteDetail,
            Self::Task => Stream::TasksOpen,
        }
    }
}
pub struct Resolved {
    pub row: Uuid,
    pub observations: i64,
    pub source_person: Option<String>,
    pub semantic: Vec<u8>,
    pub(super) record: Derived,
}
impl Resolved {
    /// Parser diagnostics remain reviewable; resolution authenticates the source
    /// representation, while family classification decides native eligibility.
    pub fn reasons(&self) -> &[String] {
        match &self.record {
            Derived::Metadata(r) => &r.reasons,
            Derived::Activity(r) => &r.reasons,
            Derived::Oversized { .. } => unreachable!("oversized observations resolve to a hold"),
        }
    }
}
pub enum Resolution {
    Ready(Box<Resolved>),
    Held(Hold),
}
pub async fn resolve(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    kind: Kind,
    source_id: &str,
) -> Result<Resolution, MigrationError> {
    if source_id.is_empty()
        || source_id.len() > 128
        || source_id.starts_with('0')
        || !source_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(MigrationError::InvalidInput);
    }
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    let family = p.get::<String, _>("family");
    if !matches!(
        p.get::<String, _>("phase").as_str(),
        "mappings" | "classify"
    ) || !matches!(
        (family.as_str(), kind),
        ("metadata", Kind::Person | Kind::Field)
            | ("activity", Kind::User | Kind::Note | Kind::Task)
    ) {
        return Err(MigrationError::Conflict);
    }
    let payer: Uuid = b
        .get::<Option<Uuid>, _>("payer_plan_id")
        .ok_or(MigrationError::Conflict)?;
    let owner=sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(payer).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    if !matches!(
        owner.get::<String, _>("phase").as_str(),
        "mappings" | "classify" | "apply" | "finished"
    ) || owner.get::<Option<Uuid>, _>("source_snapshot_id")
        != b.get::<Option<Uuid>, _>("core_snapshot_id")
    {
        return Err(MigrationError::Conflict);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: payer,
        family: match owner.get::<String, _>("family").as_str() {
            "metadata" => Family::Metadata,
            "activity" => Family::Activity,
            _ => return Err(MigrationError::Conflict),
        },
        revision: owner.get("revision"),
    };
    let groups=sqlx::query("SELECT s.representation,count(*) AS observations,count(DISTINCT s.semantic_hmac) AS variants,count(DISTINCT s.source_person_id)+(bool_or(s.source_person_id IS NULL))::int AS people,min(s.source_person_id) AS person,bool_and(s.qualified) AS qualified,bool_or(s.reason='unit_too_large') AS oversized,count(DISTINCT page.stream) AS streams FROM migration_family_refresh_source s JOIN migration_family_refresh_core_page page ON page.id=s.core_page_id AND page.bundle_id=s.bundle_id AND page.organization_id=s.organization_id WHERE s.bundle_id=$1 AND s.organization_id=$2 AND s.plan_id=$3 AND s.kind=$4 AND s.source_id=$5 GROUP BY s.representation ORDER BY s.representation LIMIT 3")
        .bind(claim.bundle).bind(claim.organization.0).bind(payer).bind(kind.name()).bind(source_id).fetch_all(&mut *tx).await?;
    if groups.is_empty() {
        return Ok(Resolution::Held(Hold::SourceNotObserved));
    }
    let mut person = None;
    let mut seen_person = false;
    let mut count = 0_i64;
    let mut desired = false;
    for g in &groups {
        let representation: String = g.get("representation");
        if representation != kind.stream().representation()
            && !(matches!(kind, Kind::Note) && representation == Stream::Notes.representation())
        {
            return Ok(Resolution::Held(Hold::UnsupportedSource));
        }
        if g.get::<i64, _>("variants") != 1
            || g.get::<i64, _>("people") != 1
            || matches!(kind, Kind::Task) && g.get::<i64, _>("streams") > 1
        {
            return Ok(Resolution::Held(Hold::SourceConflict));
        }
        if !g.get::<bool, _>("qualified") {
            return Ok(Resolution::Held(
                if g.get::<Option<bool>, _>("oversized") == Some(true) {
                    Hold::UnitTooLarge
                } else {
                    Hold::UnsupportedSource
                },
            ));
        }
        let linked: Option<String> = g.get("person");
        if seen_person && linked != person {
            return Ok(Resolution::Held(Hold::SourceConflict));
        }
        person = linked;
        seen_person = true;
        desired |= representation == kind.stream().representation();
        count = count
            .checked_add(g.get::<i64, _>("observations"))
            .ok_or(MigrationError::StorageLimit)?;
    }
    if !desired {
        return Ok(Resolution::Held(Hold::SourceUnavailable));
    }
    if matches!(kind, Kind::Person | Kind::Note | Kind::Task) && person.is_none() {
        return Ok(Resolution::Held(Hold::UnsupportedSource));
    }
    let r=sqlx::query("SELECT * FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND plan_id=$3 AND kind=$4 AND source_id=$5 AND representation=$6 ORDER BY id LIMIT 1")
        .bind(claim.bundle).bind(claim.organization.0).bind(payer).bind(kind.name()).bind(source_id).bind(kind.stream().representation()).fetch_one(&mut *tx).await?;
    let record: Derived = scope.open(
        key,
        r.get("id"),
        Purpose::Source,
        r.get("nonce"),
        r.get("ciphertext"),
    )?;
    let matches = match &record {
        Derived::Metadata(data) => {
            data.source_id.as_deref() == Some(source_id)
                && matches!(kind, Kind::Person | Kind::Field)
        }
        Derived::Activity(data) => {
            data.source_id.as_deref() == Some(source_id)
                && data.person_id == person
                && data.stream.representation() == kind.stream().representation()
                && matches!(kind, Kind::User | Kind::Note | Kind::Task)
        }
        Derived::Oversized { .. } => return Ok(Resolution::Held(Hold::UnitTooLarge)),
    };
    if !matches {
        return Err(MigrationError::Crypto);
    }
    Ok(Resolution::Ready(Box::new(Resolved {
        row: r.get("id"),
        observations: count,
        source_person: person,
        semantic: r.get("semantic_hmac"),
        record,
    })))
}
