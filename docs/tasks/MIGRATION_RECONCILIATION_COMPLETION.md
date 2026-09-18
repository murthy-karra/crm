# Migration reconciliation summary — implementation brief

## Accepted boundary

This scope adds one read-only, administrator-only endpoint:

    GET /api/migrations/fub/imports/{original_import_id}/reconciliation

It reads existing authoritative migration ledgers through one repeatable-read
PostgreSQL snapshot. It creates no persistence, worker, receipt, source request,
activation mechanism, cutoff policy, or database migration.

The response is migration-review evidence only. It must never say migration
complete, cutover ready, or operationally active. D-064/D-065's review hold
remains in force. Live FUB/customer work and activation remain deferred.

## Authoritative inputs

The route is rooted at one completed original People import and server-derived
Organization/workspace context. The reader uses only:

- original People import result and identity records;
- ordinary admission and recovery result/identity records;
- original/admitted metadata, activity, and history result ledgers and retained
  source/capture coverage;
- core-change reports and history captures as evidence identifiers; and
- D-092 family-refresh bundle, plan, cohort, manifest, and immutable result
  records.

It imports the coverage lane's static public types from
crm_app::domain::migration::coverage_inventory:

    CoverageDescriptor, CoverageFamily, CoveragePath, SourceEvidence,
    DestinationSurface, inventory()

The static descriptor is a family constraint only. Reconciliation copies its
family, path, source evidence, destination surface, and blocker codes verbatim;
it does not interpret it as proof that dynamic source coverage is complete.

Mutable Person, tag, custom-field, note, task, and timeline state is never an
outcome source. The reader never decrypts or returns raw source/customer content.

## Exact DTO proposal

Every count is a canonical decimal string.

    ReconciliationSummary {
      original_import_id: Uuid,
      workspace_revision: String,
      generated_at: DateTime<Utc>,
      review_hold: true,
      families: Vec<FamilySummary>,
      blockers: Vec<ReconciliationBlocker>
    }

    FamilySummary {
      coverage: CoverageDescriptor,
      unit: String,
      cohorts: Vec<CohortSummary>,
      blockers: Vec<ReconciliationBlocker>
    }

    CohortSummary {
      cohort_id: Uuid,
      cohort_origin: Original | Admission | Recovery,
      result_totals: OutcomeTotals,
      latest_bundle: Option<LatestBundle>,
      evidence_ids: Vec<Uuid>,
      blockers: Vec<ReconciliationBlocker>
    }

    OutcomeTotals {
      applied: String,
      already_current: String,
      held: String,
      excluded: String,
      unprocessed: String
    }

    LatestBundle {
      bundle_id: Uuid,
      plan_id: Uuid,
      state: Preparing | Ready | Queued | Running | Paused | Cancelled | Completed,
      outcome_totals: OutcomeTotals,
      evidence_ids: Vec<Uuid>
    }

    ReconciliationBlocker {
      code: String,
      evidence_id: Option<Uuid>
    }

FamilySummary contains one dynamic cell per qualified cohort for the applicable
coverage family. The current implementation mapping is:

| Dynamic lineage | Static CoverageFamily entries |
| --- | --- |
| people core | people_contacts |
| metadata | embedded_person_tags, custom_fields |
| activity | notes, tasks |
| history | historical_events, calls, texts |

unit names the actual counted entity: people, person_atomic_unit, catalog_row,
activity_record, or history_fact_identity. Values from different units are never
added or presented as interchangeable People/source identities.

Every remaining taxonomy entry is represented with its static descriptor and
blockers, but has no invented cohort/result totals. This includes addresses,
relationships, appointments, deals, emails, recordings, external files,
automation settings, source deletion semantics, workspace activation, and privacy
erasure.

result_totals describes terminal initial lineage results for the exact cell:
(original import, original/admitted/recovery cohort, family). latest_bundle is
separate, per family, and points only to the newest applicable D-092 refresh plan
and its own terminal/unfinished outcomes. It is never summed into result_totals.
This means a completed metadata or history result remains displayed when a newer
activity-only remainder exists; no bundle may obscure another family.

The response has at most three bounded lineage cohorts: original, ordinary
admission, and recovery. The original cohort ID is the original import ID; an
admission/recovery cohort ID is the stable minimum run ID in that category.
Counts inside a grouped category use distinct durable source identities or
stable family-unit keys, never attempt rows. `multiple_lineage_runs_grouped`
marks grouped evidence. Recovery remains Recovery, and historical held results
remain historical outcomes even when a later typed recovery succeeds.

## Accounting and safety rules

- Unknown is not zero. Null inaccessible totals and unavailable/incomplete retained
  evidence are unknown blockers, never decimal zero.
- Absent is not deleted. A missing source property, absent Person, source 404,
  inaccessible record, or later-capture absence yields no deletion, removal,
  success, or inferred refresh action. Only an existing qualified plan/result may
  report explicit clear/removal/task-state action.
- A terminal result replaces its manifest in totals. A manifest without terminal
  result is unprocessed. An exact remainder owns only its unfinished predecessor
  set. Settled holds remain blockers unless a distinct typed repair/recovery
  result is present.
- Initial and refresh result records are evidence links, not multiplicity. The
  reader never produces a cross-family grand total or records-migrated count.
- Held, excluded, unprocessed, missing qualified coverage, inaccessible source,
  invalid/incomplete capture, taxonomy block, stale/erased/identity/mapping
  reason, and ledger mismatch remain explicit blockers.
- Expose evidence IDs/interval references only. There is no freshness threshold,
  activation indicator, or cutover conclusion.

## Route behavior

The endpoint accepts no query parameters. It is limited to 512 KiB. If the fixed
family/cohort matrix cannot fit, return the existing payload-limit error rather
than an unmarked truncation.

Use the existing server-derived Organization-admin context and migration review
workspace guard. Enforce Organization, workspace, original import, and current
membership for every query. Emit Cache-Control: no-store, including errors.
Existing malformed, foreign, inactive-member, non-admin, not-found, and
review-ineligible migration errors apply.

Open one REPEATABLE READ transaction before resolving the original import. The
endpoint is application-level read-only, but the transaction is not PostgreSQL
READ ONLY because the existing authorization guards may lock membership state. The
root projection itself takes no row lock; REPEATABLE READ supplies the consistent
snapshot without conflicting with migration lifecycle lock order. All source,
cohort, outcome, manifest, and taxonomy-derived cells use that transaction.
workspace_revision is the workspace revision observed in it. Concurrent
import, repair, remainder, or refresh activity cannot mix a response; a later
request can observe a later whole snapshot.

## Exact file ownership

Root owns shared wiring only:

- backend/crates/crm-api/src/routes/mod.rs
- backend/crates/crm-api/src/lib.rs
- backend/crates/crm-app/src/domain/migration/mod.rs
- global test registry, E2E registration, migrations (none expected), and serial
  DB test scheduling.

Reconciliation lane owns:

- backend/crates/crm-app/src/domain/migration/reconciliation.rs
- backend/crates/crm-api/src/routes/migration_reconciliation.rs
- backend/crates/crm-api/tests/db_migration_reconciliation.rs

Root inserts the completed panel into web/src/views/MigrationView.vue to avoid
concurrent edits. The coverage lane owns coverage_inventory and its tests.

## Query shape and D-050 evidence

Use a single fixed source query list, each constrained by organization_id and
original_import_id (or the original import's trusted parent key):

The exact statements are exported as public constants in `reconciliation.rs`:
`ROOT_SQL`, `COHORTS_SQL`, `METADATA_TOTALS_SQL`, `ACTIVITY_TOTALS_SQL`,
`HISTORY_TOTALS_SQL`, `LATEST_REFRESH_SQL`, and `SOURCE_WARNINGS_SQL`.
Static `coverage_inventory` is the eighth in-process input and performs no SQL.
Latest refresh selection ranks by dynamic manifest kind before choosing a plan,
so task-only or calls-only remainders cannot erase earlier note/event evidence.
Admission totals collapse terminal attempts by stable source key: any settled
identity in the grouped origin is applied once; otherwise the latest item is
categorized once as already-current, held, excluded, or unprocessed. Metadata
and activity retain the latest stable source unit per grouped origin. History
HMAC identities retain only the latest terminal outcome, while identityless
facts use their retained observation/ordinal as the honest lineage key. Source
warnings cover original, confirmed admission/recovery, and family-refresh
snapshots and retain one representative evidence ID per fixed family/code
category.

The one-shot D-050 fixture explains all seven exported statements at 25,000
People and 50 members. Each selected original ledger has 25,000 manifests. The
refresh shape has 25,000 distinct Person cohorts and 25,000 manifests/results
for each of metadata, activity, and history. The source-warning shape has 25,000
transaction-local inaccessible captures. The evidence records exact SQL, SHA-256,
typed bindings, returned-row ceilings, and `EXPLAIN (ANALYZE, BUFFERS, FORMAT
JSON)`. It reads capture metadata but never selects or decrypts retained payloads.

Inert cardinality copies require the existing development PostgreSQL superuser
so the rolled-back transaction can suppress row/FK triggers. The fixture restores
`session_replication_role=origin` before ANALYZE and every EXPLAIN. This grants
nothing to `crm_app` or `crm_migrator` and is not runtime mutation evidence. Run
the check only in the root-granted serial database slot.

## Synthetic acceptance

1. Original, ordinary admission, and recovery each appear once; sibling,
   unsettled, and original-held results never leak in.
2. Initial metadata/activity/history and a newer activity-only refresh prove
   per-family latest-bundle isolation: completed metadata/history stay visible.
3. Result/manifest and prior-refresh records cannot double count. Exact
   remainder and typed repair preserve predecessor evidence.
4. Held, excluded, unprocessed, static unsupported/unqualified, and unknown are
   visible. Null inaccessible count and incomplete capture never render as zero.
5. Source omission, inaccessible record, and 404 never create deletion/removal
   or success; explicit qualified removal remains separate.
6. Cross-Org/root/account/workspace, inactive membership, non-admin, malformed
   path, and foreign evidence never disclose summary/evidence IDs.
7. Concurrent outcome commit between family reads yields one repeatable snapshot.
8. No raw customer data appears in JSON, browser cache, logs, or telemetry;
   payload overflow fails closed.
9. Desktop and 390px component tests show matrix, unknown versus zero, blockers,
   evidence IDs, and review hold without completion/activation/cutover claims.

Focused Rust/Web tests run first. DB integration waits for root's serial slot and
uses a dedicated CARGO_TARGET_DIR without touching shared-development artifacts.

## Verification status

Five focused PostgreSQL reconciliation cases passed on 2026-09-17 using
`CARGO_TARGET_DIR=/private/tmp/crm-reconciliation-target`; the retained final
correctness log is `/private/tmp/crm-reconciliation-correctness-final.log`. It
includes positive ledgers, authorization/tenant bounds, recovery grouping, and
two-terminal admission/history deduplication. The realistic plan evidence is
retained at `/private/tmp/crm-reconciliation-plans-final.log`, with the seven
raw JSON plans in `/private/tmp/crm-reconciliation-plan-evidence/`. The one-shot
runner passed in 66.18 seconds. Statement execution times were 0.066 ms for the
root, 76.392 ms for cohorts, 46.972 ms for metadata, 99.947 ms for activity,
154.114 ms for history, 1,301.142 ms for latest refresh, and 34.571 ms for
source warnings. The plans use linear ledger scans with hash/aggregate joins or
indexed selected-result lookups; no statement repeatedly scans a full ledger
per Person. History spilled 636/637 temporary blocks and latest refresh spilled
1,509/1,512 temporary blocks at this envelope. Those spills are a trend to
monitor rather than an absolute developer-machine latency gate.
