//! Bounded migration-review reconciliation over immutable result ledgers.
//!
//! This projection is run/cohort scoped. It never counts worker attempts as
//! People, reads mutable CRM rows as outcomes, or claims source completeness,
//! activation readiness, or cutover readiness.
use super::{
    coverage_inventory::{inventory, CoverageDescriptor, CoverageFamily},
    store, MigrationError,
};
use crate::{auth::workspace, domain::envelope::CommandContext};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CohortOrigin {
    Original,
    Admission,
    Recovery,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OutcomeTotals {
    pub applied: String,
    pub already_current: String,
    pub held: String,
    pub excluded: String,
    pub unprocessed: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct LatestBundle {
    pub bundle_id: Uuid,
    pub plan_id: Uuid,
    pub state: String,
    pub outcome_totals: OutcomeTotals,
    pub evidence_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReconciliationBlocker {
    pub code: String,
    pub evidence_id: Option<Uuid>,
}
#[derive(Clone, Debug, Serialize)]
pub struct CohortSummary {
    pub cohort_id: Uuid,
    pub cohort_origin: CohortOrigin,
    pub result_totals: OutcomeTotals,
    pub latest_bundle: Option<LatestBundle>,
    pub evidence_ids: Vec<Uuid>,
    pub blockers: Vec<ReconciliationBlocker>,
}
#[derive(Serialize)]
pub struct FamilySummary {
    pub coverage: CoverageDescriptor,
    pub unit: String,
    pub cohorts: Vec<CohortSummary>,
    pub blockers: Vec<ReconciliationBlocker>,
}
#[derive(Serialize)]
pub struct ReconciliationSummary {
    pub original_import_id: Uuid,
    pub workspace_revision: String,
    pub generated_at: DateTime<Utc>,
    pub review_hold: bool,
    pub families: Vec<FamilySummary>,
    pub blockers: Vec<ReconciliationBlocker>,
}

/// Query 1: trusted root and source-lineage evidence.
pub const ROOT_SQL: &str = r#"
SELECT i.id,i.snapshot_id,i.confirmed_plan_id,o.workspace_revision,o.workspace_mode
FROM migration_import i
JOIN migration_workspace w ON w.organization_id=i.organization_id AND w.import_id=i.id
JOIN organization o ON o.id=i.organization_id
WHERE i.id=$1 AND i.organization_id=$2 AND i.state='completed'
  AND i.confirmed_plan_id=w.plan_id
"#;

/// Query 2: one bounded row per lineage run. Successful People are fenced
/// through the unique source identity ledger, so retries cannot become People.
pub const COHORTS_SQL: &str = r#"
WITH admission_groups AS (
 SELECT DISTINCT ON (a.mode) a.mode,a.id AS cohort_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 ORDER BY a.mode,a.created_at,a.id
), original AS (
 SELECT i.id AS cohort_id,'original'::text AS origin,i.id AS evidence_id,
        i.confirmed_plan_id AS plan_id,i.snapshot_id AS source_id,1::bigint AS run_count,
        count(DISTINCT mi.source_id) FILTER (WHERE r.disposition='imported')::bigint AS applied,
        count(DISTINCT mi.source_id) FILTER (WHERE r.disposition='already_imported')::bigint AS already_current,
        count(*) FILTER (WHERE r.disposition='held')::bigint AS held,
        0::bigint AS excluded,count(m.id) FILTER (WHERE r.id IS NULL)::bigint AS unprocessed
 FROM migration_import i
 LEFT JOIN migration_import_manifest m ON m.import_id=i.id AND m.plan_id=i.confirmed_plan_id AND m.organization_id=i.organization_id
 LEFT JOIN migration_import_result r ON r.manifest_id=m.id AND r.import_id=i.id AND r.organization_id=i.organization_id
 LEFT JOIN migration_import_identity mi ON mi.organization_id=i.organization_id
   AND mi.source_account_id=i.source_account_id AND mi.family='people'
   AND mi.source_id=m.source_id AND mi.import_id=i.id AND mi.plan_id=i.confirmed_plan_id
 WHERE i.id=$2 AND i.organization_id=$1
 GROUP BY i.id,i.confirmed_plan_id,i.snapshot_id
), admission_runs AS (
 SELECT groups.cohort_id,a.mode,groups.cohort_id AS evidence_id,
        (array_agg(a.confirmed_admission_plan_id ORDER BY a.created_at,a.id))[1] AS plan_id,
        (array_agg(a.newer_snapshot_id ORDER BY a.created_at,a.id))[1] AS source_id,
        count(DISTINCT a.id)::bigint AS run_count
 FROM migration_people_admission a
 JOIN admission_groups groups ON groups.mode=a.mode
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 GROUP BY groups.cohort_id,a.mode
), admission_items AS (
 SELECT groups.cohort_id,item.source_key,item.disposition AS item_disposition,r.disposition AS result_disposition,r.id AS result_id,
        bool_or(mi.source_id IS NOT NULL OR COALESCE(r.disposition='settled',false)) OVER (PARTITION BY groups.cohort_id,item.source_key) AS succeeded,
        row_number() OVER (PARTITION BY groups.cohort_id,item.source_key ORDER BY a.created_at DESC,a.id DESC) AS rank
 FROM migration_people_admission a
 JOIN admission_groups groups ON groups.mode=a.mode
 JOIN migration_people_admission_item item ON item.admission_id=a.id
   AND item.plan_id=a.confirmed_admission_plan_id AND item.organization_id=a.organization_id
 LEFT JOIN migration_people_admission_result r ON r.item_id=item.id AND r.admission_id=a.id AND r.organization_id=a.organization_id
 LEFT JOIN migration_import_identity mi ON mi.organization_id=a.organization_id
   AND mi.source_account_id=a.source_account_id AND mi.family='people'
   AND mi.admission_id=a.id AND mi.admission_result_id=r.id AND mi.source_id=r.source_id
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
), admissions AS (
 SELECT runs.cohort_id,CASE WHEN runs.mode='mapping_recovery' THEN 'recovery' ELSE 'admission' END AS origin,
        runs.evidence_id,runs.plan_id,runs.source_id,runs.run_count,
        count(*) FILTER (WHERE item.rank=1 AND item.succeeded)::bigint AS applied,
        count(*) FILTER (WHERE item.rank=1 AND NOT item.succeeded
          AND item.item_disposition IN ('already_imported','already_admitted'))::bigint AS already_current,
        count(*) FILTER (WHERE item.rank=1 AND NOT item.succeeded
          AND (item.result_disposition LIKE 'held_%' OR item.item_disposition LIKE 'held_%'))::bigint AS held,
        count(*) FILTER (WHERE item.rank=1 AND NOT item.succeeded
          AND item.item_disposition='excluded_original')::bigint AS excluded,
        count(*) FILTER (WHERE item.rank=1 AND NOT item.succeeded
          AND item.item_disposition='eligible' AND item.result_id IS NULL)::bigint AS unprocessed
 FROM admission_runs runs
 LEFT JOIN admission_items item ON item.cohort_id=runs.cohort_id AND item.rank=1
 GROUP BY runs.cohort_id,runs.mode,runs.evidence_id,runs.plan_id,runs.source_id,runs.run_count
)
SELECT * FROM original UNION ALL SELECT * FROM admissions ORDER BY origin,cohort_id
"#;

/// Query 3: metadata person-atomic units. Ranking by stable source-person key
/// collapses retried copies while retaining settled predecessors.
pub const METADATA_TOTALS_SQL: &str = r#"
WITH admission_groups AS (
 SELECT DISTINCT ON (a.mode) a.mode,a.id AS cohort_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 ORDER BY a.mode,a.created_at,a.id
), original_cells AS (
 SELECT root.parent_import_id AS cohort_id,m.source_id,m.id AS manifest_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.confirmed_plan_id AS plan_id,root.snapshot_id AS source_evidence
 FROM migration_metadata_import root
 LEFT JOIN migration_metadata_manifest m ON m.import_id=root.id AND m.plan_id=root.confirmed_plan_id AND m.organization_id=root.organization_id
 LEFT JOIN migration_metadata_result r ON r.manifest_id=m.id AND r.import_id=root.id AND r.organization_id=root.organization_id AND r.kind='people'
 WHERE root.parent_import_id=$2 AND root.organization_id=$1 AND root.confirmed_plan_id IS NOT NULL
), admitted_ranked AS (
 SELECT groups.cohort_id,m.source_person_id AS source_id,m.id AS manifest_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.confirmed_plan_id AS plan_id,root.source_report_id AS source_evidence,
        row_number() OVER (PARTITION BY groups.cohort_id,m.source_person_id ORDER BY root.created_at DESC,root.id DESC) AS rank
 FROM migration_admitted_metadata_import root
 JOIN migration_people_admission admission ON admission.id=root.admission_id AND admission.organization_id=root.organization_id
 JOIN admission_groups groups ON groups.mode=admission.mode
 LEFT JOIN migration_admitted_metadata_manifest m ON m.import_id=root.id AND m.plan_id=root.confirmed_plan_id AND m.organization_id=root.organization_id
 LEFT JOIN migration_admitted_metadata_result r ON r.manifest_id=m.id AND r.import_id=root.id AND r.organization_id=root.organization_id AND r.kind='people'
 WHERE root.parent_import_id=$2 AND root.organization_id=$1 AND root.confirmed_plan_id IS NOT NULL
), cells AS (
 SELECT cohort_id,source_id,manifest_id,disposition,result_id,run_id,plan_id,source_evidence FROM original_cells
 UNION ALL SELECT cohort_id,source_id,manifest_id,disposition,result_id,run_id,plan_id,source_evidence FROM admitted_ranked WHERE rank=1
)
SELECT cohort_id,'metadata'::text AS family,
 count(*) FILTER (WHERE disposition='applied')::bigint AS applied,
 count(*) FILTER (WHERE disposition='already_present')::bigint AS already_current,
 count(*) FILTER (WHERE disposition='held')::bigint AS held,
 count(*) FILTER (WHERE disposition IN ('not_supplied','source_null'))::bigint AS excluded,
 count(manifest_id) FILTER (WHERE result_id IS NULL)::bigint AS unprocessed,
 min(run_id::text)::uuid AS evidence_id,min(plan_id::text)::uuid AS plan_id,min(source_evidence::text)::uuid AS source_id
FROM cells GROUP BY cohort_id
"#;

/// Query 4: notes and tasks, using stable source IDs to collapse retried copies.
pub const ACTIVITY_TOTALS_SQL: &str = r#"
WITH admission_groups AS (
 SELECT DISTINCT ON (a.mode) a.mode,a.id AS cohort_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 ORDER BY a.mode,a.created_at,a.id
), original_cells AS (
 SELECT root.parent_import_id AS cohort_id,k.kind,m.source_id,m.source_row_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.confirmed_plan_id AS plan_id,root.snapshot_id AS source_evidence
 FROM migration_activity_import root
 CROSS JOIN (VALUES ('note'::text),('task'::text)) k(kind)
 LEFT JOIN migration_activity_manifest m ON m.import_id=root.id AND m.plan_id=root.confirmed_plan_id AND m.organization_id=root.organization_id AND m.kind=k.kind
 LEFT JOIN migration_activity_result r ON r.manifest_id=m.id AND r.import_id=root.id AND r.organization_id=root.organization_id
 WHERE root.parent_import_id=$2 AND root.organization_id=$1 AND root.confirmed_plan_id IS NOT NULL
), admitted_ranked AS (
 SELECT groups.cohort_id,k.kind,m.source_id,m.source_row_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.confirmed_plan_id AS plan_id,root.source_report_id AS source_evidence,
        row_number() OVER (PARTITION BY groups.cohort_id,k.kind,COALESCE(m.source_id,m.source_row_id::text)
                           ORDER BY root.created_at DESC,root.id DESC) AS rank
 FROM migration_admitted_activity_import root
 JOIN migration_people_admission admission ON admission.id=root.admission_id AND admission.organization_id=root.organization_id
 JOIN admission_groups groups ON groups.mode=admission.mode
 CROSS JOIN (VALUES ('note'::text),('task'::text)) k(kind)
 LEFT JOIN migration_admitted_activity_manifest m ON m.import_id=root.id AND m.plan_id=root.confirmed_plan_id AND m.organization_id=root.organization_id AND m.kind=k.kind
 LEFT JOIN migration_admitted_activity_result r ON r.manifest_id=m.id AND r.import_id=root.id AND r.organization_id=root.organization_id
 WHERE root.parent_import_id=$2 AND root.organization_id=$1 AND root.confirmed_plan_id IS NOT NULL
), cells AS (
 SELECT cohort_id,kind,source_id,source_row_id,disposition,result_id,run_id,plan_id,source_evidence FROM original_cells
 UNION ALL SELECT cohort_id,kind,source_id,source_row_id,disposition,result_id,run_id,plan_id,source_evidence FROM admitted_ranked WHERE rank=1
)
SELECT cohort_id,CASE kind WHEN 'note' THEN 'notes' ELSE 'tasks' END AS family,
 count(*) FILTER (WHERE disposition='applied')::bigint AS applied,
 count(*) FILTER (WHERE disposition='already_present')::bigint AS already_current,
 count(*) FILTER (WHERE disposition='held')::bigint AS held,0::bigint AS excluded,
 count(source_row_id) FILTER (WHERE result_id IS NULL)::bigint AS unprocessed,
 min(run_id::text)::uuid AS evidence_id,min(plan_id::text)::uuid AS plan_id,min(source_evidence::text)::uuid AS source_id
FROM cells GROUP BY cohort_id,kind
"#;

/// Query 5: history fact identities. Stable identity HMACs collapse across terminal
/// roots; identityless records retain their observation/ordinal lineage identity.
pub const HISTORY_TOTALS_SQL: &str = r#"
WITH admission_groups AS (
 SELECT DISTINCT ON (a.mode) a.mode,a.id AS cohort_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 ORDER BY a.mode,a.created_at,a.id
), original_cells AS (
 SELECT root.parent_import_id AS cohort_id,k.family,m.id AS manifest_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.plan_id,root.id AS source_evidence,root.created_at,
        CASE WHEN m.id IS NULL THEN 'empty'
             WHEN m.identity_hmac IS NOT NULL THEN 'identity:'||encode(m.identity_hmac,'hex')
             ELSE 'observation:'||m.observation_id::text||':'||m.ordinal::text END AS stable_identity
 FROM migration_history_import_run root
 CROSS JOIN (VALUES ('events'::text),('calls'::text),('text_messages'::text)) k(family)
 LEFT JOIN migration_history_import_manifest m ON m.owner_run_id=root.id AND m.plan_id=root.plan_id AND m.organization_id=root.organization_id AND m.family=k.family
 LEFT JOIN migration_history_import_result r ON r.manifest_id=m.id AND r.owner_run_id=root.id AND r.organization_id=root.organization_id
 WHERE root.parent_import_id=$2 AND root.organization_id=$1
), admitted_cells AS (
 SELECT groups.cohort_id,k.family,m.id AS manifest_id,r.disposition,r.id AS result_id,
        root.id AS run_id,root.confirmed_plan_id AS plan_id,root.history_capture_id AS source_evidence,root.created_at,
        CASE WHEN m.id IS NULL THEN 'empty'
             WHEN m.identity_hmac IS NOT NULL THEN 'identity:'||encode(m.identity_hmac,'hex')
             ELSE 'observation:'||m.observation_id::text||':'||m.ordinal::text END AS stable_identity
 FROM migration_admitted_history_root root
 JOIN migration_people_admission admission ON admission.id=root.admission_id AND admission.organization_id=root.organization_id
 JOIN admission_groups groups ON groups.mode=admission.mode
 CROSS JOIN (VALUES ('events'::text),('calls'::text),('text_messages'::text)) k(family)
 LEFT JOIN migration_admitted_history_manifest m ON m.root_id=root.id AND m.plan_id=root.confirmed_plan_id AND m.organization_id=root.organization_id AND m.family=k.family
 LEFT JOIN LATERAL (
   SELECT x.id,x.disposition FROM migration_admitted_history_result x
   WHERE x.manifest_id=m.id AND x.root_id=root.id AND x.organization_id=root.organization_id
   ORDER BY x.committed_at DESC,x.id DESC LIMIT 1
 ) r ON true
 WHERE root.parent_import_id=$2 AND root.organization_id=$1 AND root.confirmed_plan_id IS NOT NULL
), ranked AS (
 SELECT *,row_number() OVER (PARTITION BY cohort_id,family,stable_identity ORDER BY created_at DESC,run_id DESC) AS rank
 FROM (SELECT * FROM original_cells UNION ALL SELECT * FROM admitted_cells) cells
)
SELECT cohort_id,CASE family WHEN 'events' THEN 'historical_events' WHEN 'calls' THEN 'calls' ELSE 'texts' END AS family,
 count(*) FILTER (WHERE disposition='imported')::bigint AS applied,
 count(*) FILTER (WHERE disposition IN ('already_imported','already_present','equal_repeat'))::bigint AS already_current,
 count(*) FILTER (WHERE disposition='held')::bigint AS held,
 count(*) FILTER (WHERE disposition='excluded')::bigint AS excluded,
 count(manifest_id) FILTER (WHERE result_id IS NULL)::bigint AS unprocessed,
 min(run_id::text)::uuid AS evidence_id,min(plan_id::text)::uuid AS plan_id,min(source_evidence::text)::uuid AS source_id
FROM ranked WHERE rank=1 GROUP BY cohort_id,family
"#;

/// Queries 6-7: rank independently by family and lineage cohort. This is what
/// prevents an activity-only remainder from hiding metadata/history evidence.
pub const LATEST_REFRESH_SQL: &str = r#"
WITH admission_groups AS (
 SELECT DISTINCT ON (a.mode) a.mode,a.id AS cohort_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 ORDER BY a.mode,a.created_at,a.id
), applicable AS (
 SELECT DISTINCT b.id AS bundle_id,p.id AS plan_id,p.family,
        CASE manifest.kind WHEN 'note' THEN 'notes' WHEN 'task' THEN 'tasks' WHEN 'event' THEN 'historical_events'
             WHEN 'call' THEN 'calls' WHEN 'text' THEN 'texts' ELSE 'metadata' END AS dynamic_family,
        p.state,b.created_at,b.revision,p.revision AS plan_revision,
        b.core_report_id,b.core_snapshot_id,b.history_capture_id,
        CASE WHEN c.original_result_id IS NOT NULL THEN b.parent_import_id ELSE groups.cohort_id END AS cohort_id
 FROM migration_family_refresh_bundle b
 JOIN migration_family_refresh_plan p ON p.bundle_id=b.id AND p.organization_id=b.organization_id AND p.state<>'superseded'
 JOIN migration_family_refresh_cohort c ON c.bundle_id=b.id AND c.organization_id=b.organization_id
 JOIN migration_family_refresh_manifest manifest ON manifest.plan_id=p.id AND manifest.bundle_id=b.id
  AND manifest.organization_id=b.organization_id AND manifest.cohort_id=c.id
 LEFT JOIN migration_people_admission admission ON admission.id=c.admission_id AND admission.organization_id=c.organization_id
 LEFT JOIN admission_groups groups ON groups.mode=admission.mode
 WHERE b.parent_import_id=$2 AND b.organization_id=$1
), latest AS (
 SELECT *,row_number() OVER (PARTITION BY cohort_id,dynamic_family ORDER BY created_at DESC,revision DESC,plan_revision DESC,bundle_id DESC) AS rank
 FROM applicable
)
SELECT l.cohort_id,l.dynamic_family AS family,
 l.bundle_id,l.plan_id,l.state,l.core_report_id,l.core_snapshot_id,l.history_capture_id,
 count(*) FILTER (WHERE r.disposition='applied')::bigint AS applied,
 count(*) FILTER (WHERE r.disposition='already_current')::bigint AS already_current,
 count(*) FILTER (WHERE r.disposition='held')::bigint AS held,
 count(*) FILTER (WHERE r.disposition='excluded')::bigint AS excluded,
 count(*) FILTER (WHERE r.id IS NULL AND m.inherited_result_id IS NULL)::bigint AS unprocessed
FROM latest l
JOIN migration_family_refresh_cohort c ON c.bundle_id=l.bundle_id AND c.organization_id=$1
LEFT JOIN migration_people_admission admission ON admission.id=c.admission_id AND admission.organization_id=c.organization_id
LEFT JOIN admission_groups groups ON groups.mode=admission.mode
JOIN migration_family_refresh_manifest m ON m.plan_id=l.plan_id AND m.bundle_id=l.bundle_id
 AND m.organization_id=$1 AND m.cohort_id=c.id
 AND (CASE m.kind WHEN 'note' THEN 'notes' WHEN 'task' THEN 'tasks' WHEN 'event' THEN 'historical_events'
             WHEN 'call' THEN 'calls' WHEN 'text' THEN 'texts' ELSE 'metadata' END)=l.dynamic_family
LEFT JOIN migration_family_refresh_result r ON r.manifest_id=m.id AND r.plan_id=m.plan_id AND r.organization_id=m.organization_id
WHERE l.rank=1 AND (CASE WHEN c.original_result_id IS NOT NULL THEN $2 ELSE groups.cohort_id END)=l.cohort_id
GROUP BY l.cohort_id,l.dynamic_family,
 l.bundle_id,l.plan_id,l.state,l.core_report_id,l.core_snapshot_id,l.history_capture_id
ORDER BY l.cohort_id,l.dynamic_family
"#;

/// Query 8: retained source/capture warnings. Unknown source coverage is a
/// blocker and never a decimal zero.
pub const SOURCE_WARNINGS_SQL: &str = r#"
WITH lineage_snapshots AS (
 SELECT i.snapshot_id
 FROM migration_import i WHERE i.id=$2 AND i.organization_id=$1
 UNION
 SELECT a.newer_snapshot_id
 FROM migration_people_admission a
 WHERE a.parent_import_id=$2 AND a.organization_id=$1 AND a.confirmed_admission_plan_id IS NOT NULL
 UNION
 SELECT b.core_snapshot_id
 FROM migration_family_refresh_bundle b
 WHERE b.parent_import_id=$2 AND b.organization_id=$1 AND b.core_snapshot_id IS NOT NULL
 UNION
 SELECT p.source_snapshot_id
 FROM migration_family_refresh_plan p
 JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id
 WHERE b.parent_import_id=$2 AND b.organization_id=$1 AND p.state<>'superseded' AND p.source_snapshot_id IS NOT NULL
), warnings AS (
 SELECT CASE c.stream
  WHEN 'people' THEN 'people_contacts' WHEN 'users' THEN 'source_users' WHEN 'stages' THEN 'stages'
  WHEN 'custom_fields' THEN 'custom_fields' WHEN 'notes' THEN 'notes' WHEN 'tasks' THEN 'tasks'
  ELSE NULL END AS family,
  CASE WHEN c.truncated THEN 'retained_source_truncated'
       WHEN NOT c.accepted THEN 'retained_source_inaccessible'
       WHEN c.classification IN ('failed','incomplete') THEN 'retained_capture_incomplete'
       ELSE NULL END AS code,c.id AS evidence_id,c.captured_at
 FROM lineage_snapshots lineage
 JOIN migration_snapshot_capture c ON c.snapshot_id=lineage.snapshot_id AND c.organization_id=$1
 WHERE c.truncated OR NOT c.accepted OR c.classification IN ('failed','incomplete')
)
SELECT family,code,(array_agg(evidence_id ORDER BY captured_at,evidence_id))[1] AS evidence_id
FROM warnings GROUP BY family,code ORDER BY family,code
"#;

#[derive(Clone)]
struct CohortBase {
    id: Uuid,
    origin: CohortOrigin,
    run_count: i64,
    people: OutcomeTotals,
    evidence_ids: Vec<Uuid>,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum DynamicFamily {
    People,
    Metadata,
    Notes,
    Tasks,
    HistoricalEvents,
    Calls,
    Texts,
}
impl DynamicFamily {
    fn parse(value: &str) -> Result<Self, MigrationError> {
        match value {
            "people" => Ok(Self::People),
            "metadata" => Ok(Self::Metadata),
            "notes" => Ok(Self::Notes),
            "tasks" => Ok(Self::Tasks),
            "historical_events" => Ok(Self::HistoricalEvents),
            "calls" => Ok(Self::Calls),
            "texts" => Ok(Self::Texts),
            _ => Err(MigrationError::Crypto),
        }
    }
}
fn origin(value: &str) -> Result<CohortOrigin, MigrationError> {
    match value {
        "original" => Ok(CohortOrigin::Original),
        "admission" => Ok(CohortOrigin::Admission),
        "recovery" => Ok(CohortOrigin::Recovery),
        _ => Err(MigrationError::Crypto),
    }
}
fn totals(row: &PgRow) -> OutcomeTotals {
    OutcomeTotals {
        applied: row.get::<i64, _>("applied").to_string(),
        already_current: row.get::<i64, _>("already_current").to_string(),
        held: row.get::<i64, _>("held").to_string(),
        excluded: row.get::<i64, _>("excluded").to_string(),
        unprocessed: row.get::<i64, _>("unprocessed").to_string(),
    }
}
fn add_evidence(ids: &mut Vec<Uuid>, id: Option<Uuid>) {
    if let Some(id) = id {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
}
fn outcome_blockers(value: &OutcomeTotals, evidence_id: Uuid) -> Vec<ReconciliationBlocker> {
    [
        (value.held.as_str(), "held"),
        (value.excluded.as_str(), "excluded"),
        (value.unprocessed.as_str(), "unprocessed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count != "0")
    .map(|(_, code)| ReconciliationBlocker {
        code: code.into(),
        evidence_id: Some(evidence_id),
    })
    .collect()
}
fn dynamic_family(family: CoverageFamily) -> Option<DynamicFamily> {
    match family {
        CoverageFamily::PeopleContacts => Some(DynamicFamily::People),
        CoverageFamily::EmbeddedPersonTags | CoverageFamily::CustomFields => {
            Some(DynamicFamily::Metadata)
        }
        CoverageFamily::Notes => Some(DynamicFamily::Notes),
        CoverageFamily::Tasks => Some(DynamicFamily::Tasks),
        CoverageFamily::HistoricalEvents => Some(DynamicFamily::HistoricalEvents),
        CoverageFamily::Calls => Some(DynamicFamily::Calls),
        CoverageFamily::Texts => Some(DynamicFamily::Texts),
        _ => None,
    }
}
fn unit(family: CoverageFamily) -> &'static str {
    match family {
        CoverageFamily::PeopleContacts => "people",
        CoverageFamily::Stages | CoverageFamily::SourceUsers => "taxonomy_mapping",
        CoverageFamily::EmbeddedPersonTags | CoverageFamily::CustomFields => "person_atomic_unit",
        CoverageFamily::StandaloneTagCatalog => "catalog_row",
        CoverageFamily::Notes | CoverageFamily::Tasks => "activity_record",
        CoverageFamily::HistoricalEvents | CoverageFamily::Calls | CoverageFamily::Texts => {
            "history_fact_identity"
        }
        _ => "not_counted",
    }
}
#[tracing::instrument(skip_all, fields(organization_id=%ctx.organization_id.0, original_import_id=%original_import_id))]
pub async fn summary(
    pool: &PgPool,
    ctx: &CommandContext,
    original_import_id: Uuid,
) -> Result<ReconciliationSummary, MigrationError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    store::require_admin(&mut tx, ctx).await?;
    let root = sqlx::query(ROOT_SQL)
        .bind(original_import_id)
        .bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::NotFound)?;
    if root.get::<String, _>("workspace_mode") != "migration_review" {
        return Err(MigrationError::Forbidden);
    }

    let mut cohorts = Vec::new();
    for row in sqlx::query(COHORTS_SQL)
        .bind(ctx.organization_id.0)
        .bind(original_import_id)
        .fetch_all(&mut *tx)
        .await?
    {
        let mut evidence_ids = Vec::with_capacity(3);
        add_evidence(&mut evidence_ids, row.get("evidence_id"));
        add_evidence(&mut evidence_ids, row.get("plan_id"));
        add_evidence(&mut evidence_ids, row.get("source_id"));
        cohorts.push(CohortBase {
            id: row.get("cohort_id"),
            origin: origin(row.get::<String, _>("origin").as_str())?,
            run_count: row.get("run_count"),
            people: totals(&row),
            evidence_ids,
        });
    }

    let mut initial: BTreeMap<(Uuid, DynamicFamily), (OutcomeTotals, Vec<Uuid>)> = BTreeMap::new();
    for cohort in &cohorts {
        initial.insert(
            (cohort.id, DynamicFamily::People),
            (cohort.people.clone(), cohort.evidence_ids.clone()),
        );
    }
    for sql in [METADATA_TOTALS_SQL, ACTIVITY_TOTALS_SQL, HISTORY_TOTALS_SQL] {
        for row in sqlx::query(sql)
            .bind(ctx.organization_id.0)
            .bind(original_import_id)
            .fetch_all(&mut *tx)
            .await?
        {
            let mut evidence_ids = Vec::with_capacity(3);
            add_evidence(&mut evidence_ids, row.get("evidence_id"));
            add_evidence(&mut evidence_ids, row.get("plan_id"));
            add_evidence(&mut evidence_ids, row.get("source_id"));
            initial.insert(
                (
                    row.get("cohort_id"),
                    DynamicFamily::parse(row.get::<String, _>("family").as_str())?,
                ),
                (totals(&row), evidence_ids),
            );
        }
    }

    let mut refreshes: BTreeMap<(Uuid, DynamicFamily), LatestBundle> = BTreeMap::new();
    for row in sqlx::query(LATEST_REFRESH_SQL)
        .bind(ctx.organization_id.0)
        .bind(original_import_id)
        .fetch_all(&mut *tx)
        .await?
    {
        let bundle_id: Uuid = row.get("bundle_id");
        let plan_id: Uuid = row.get("plan_id");
        let mut evidence_ids = vec![bundle_id, plan_id];
        add_evidence(&mut evidence_ids, row.get("core_report_id"));
        add_evidence(&mut evidence_ids, row.get("core_snapshot_id"));
        add_evidence(&mut evidence_ids, row.get("history_capture_id"));
        let latest = LatestBundle {
            bundle_id,
            plan_id,
            state: row.get("state"),
            outcome_totals: totals(&row),
            evidence_ids,
        };
        refreshes.insert(
            (
                row.get("cohort_id"),
                DynamicFamily::parse(row.get::<String, _>("family").as_str())?,
            ),
            latest,
        );
    }

    let mut warnings: Vec<(CoverageFamily, Vec<ReconciliationBlocker>)> = Vec::new();
    for row in sqlx::query(SOURCE_WARNINGS_SQL)
        .bind(ctx.organization_id.0)
        .bind(original_import_id)
        .fetch_all(&mut *tx)
        .await?
    {
        let (Some(family), Some(code)) = (
            row.get::<Option<String>, _>("family"),
            row.get::<Option<String>, _>("code"),
        ) else {
            continue;
        };
        let family = match family.as_str() {
            "people_contacts" => CoverageFamily::PeopleContacts,
            "stages" => CoverageFamily::Stages,
            "source_users" => CoverageFamily::SourceUsers,
            "custom_fields" => CoverageFamily::CustomFields,
            "notes" => CoverageFamily::Notes,
            "tasks" => CoverageFamily::Tasks,
            _ => continue,
        };
        let blocker = ReconciliationBlocker {
            code,
            evidence_id: Some(row.get("evidence_id")),
        };
        if let Some((_, family_warnings)) = warnings.iter_mut().find(|(key, _)| *key == family) {
            family_warnings.push(blocker);
        } else {
            warnings.push((family, vec![blocker]));
        }
    }

    let cohort_ids: BTreeSet<_> = cohorts.iter().map(|cohort| cohort.id).collect();
    if initial.keys().any(|(id, _)| !cohort_ids.contains(id))
        || refreshes.keys().any(|(id, _)| !cohort_ids.contains(id))
    {
        return Err(MigrationError::Crypto);
    }
    let mut families = Vec::with_capacity(inventory().len());
    for coverage in inventory().iter().copied() {
        let mut family_blockers = coverage
            .blocker_codes
            .iter()
            .map(|code| ReconciliationBlocker {
                code: (*code).into(),
                evidence_id: None,
            })
            .collect::<Vec<_>>();
        if let Some(position) = warnings
            .iter()
            .position(|(family, _)| *family == coverage.family)
        {
            family_blockers.extend(warnings.swap_remove(position).1);
        }
        let Some(dynamic) = dynamic_family(coverage.family) else {
            families.push(FamilySummary {
                coverage,
                unit: unit(coverage.family).into(),
                cohorts: Vec::new(),
                blockers: family_blockers,
            });
            continue;
        };
        let family_cohorts = cohorts
            .iter()
            .map(|cohort| {
                let (result_totals, evidence_ids, missing) =
                    match initial.get(&(cohort.id, dynamic)) {
                        Some((totals, ids)) => (totals.clone(), ids.clone(), false),
                        None => (
                            OutcomeTotals {
                                applied: "0".into(),
                                already_current: "0".into(),
                                held: "0".into(),
                                excluded: "0".into(),
                                unprocessed: "0".into(),
                            },
                            cohort.evidence_ids.clone(),
                            true,
                        ),
                    };
                let mut blockers = outcome_blockers(
                    &result_totals,
                    evidence_ids.first().copied().unwrap_or(cohort.id),
                );
                if missing {
                    blockers.push(ReconciliationBlocker {
                        code: "qualified_initial_coverage_missing".into(),
                        evidence_id: Some(cohort.id),
                    });
                }
                if cohort.run_count > 1 {
                    blockers.push(ReconciliationBlocker {
                        code: "multiple_lineage_runs_grouped".into(),
                        evidence_id: Some(cohort.id),
                    });
                }
                let latest_bundle = refreshes.get(&(cohort.id, dynamic)).cloned();
                if let Some(latest) = &latest_bundle {
                    blockers.extend(outcome_blockers(&latest.outcome_totals, latest.bundle_id));
                }
                CohortSummary {
                    cohort_id: cohort.id,
                    cohort_origin: cohort.origin,
                    result_totals,
                    latest_bundle,
                    evidence_ids,
                    blockers,
                }
            })
            .collect();
        families.push(FamilySummary {
            coverage,
            unit: unit(coverage.family).into(),
            cohorts: family_cohorts,
            blockers: family_blockers,
        });
    }
    let generated_at = sqlx::query_scalar("SELECT transaction_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let response = ReconciliationSummary {
        original_import_id,
        workspace_revision: root.get::<i64, _>("workspace_revision").to_string(),
        generated_at,
        review_hold: true,
        families,
        blockers: Vec::new(),
    };
    tx.commit().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_manifest_families_remain_isolated() {
        assert_eq!(
            DynamicFamily::parse("metadata").unwrap(),
            DynamicFamily::Metadata
        );
        assert_eq!(DynamicFamily::parse("notes").unwrap(), DynamicFamily::Notes);
        assert_eq!(DynamicFamily::parse("tasks").unwrap(), DynamicFamily::Tasks);
        assert_eq!(
            DynamicFamily::parse("historical_events").unwrap(),
            DynamicFamily::HistoricalEvents
        );
        assert_eq!(DynamicFamily::parse("calls").unwrap(), DynamicFamily::Calls);
        assert_eq!(DynamicFamily::parse("texts").unwrap(), DynamicFamily::Texts);
    }
    #[test]
    fn explicit_zero_has_no_unknown_blocker() {
        let totals = OutcomeTotals {
            applied: "0".into(),
            already_current: "0".into(),
            held: "0".into(),
            excluded: "0".into(),
            unprocessed: "0".into(),
        };
        assert!(outcome_blockers(&totals, Uuid::nil()).is_empty());
    }
}
