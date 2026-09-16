# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- Current milestone: bounded activity source traversal following persisted
  activity proposals at **`b2cd149`** and mapping conversion at **`4690233`**.
  The combined feature is unfinished. No merge/push/deploy.
- Foundation: comparison policies, source ordering, bounded encrypted evidence,
  exact decimal counts, draft persistence ownership and native/lease guards.
- Accounting now measures family and shared evidence separately. One frozen plan
  pays shared costs; original head storage ownership survives head replacement.
  Core-derived charges reach the selected snapshot and Organization ledgers;
  history plans also charge the selected history capture run. Migration 009 adds
  that source budget to admission, settlement, reclaim and erasure, including
  backfill of existing refresh evidence without duplicating Organization charges.
- Reservation/settlement functions preserve cancellation capacity, reject stale
  unit leases, and atomically charge/refund capacity. Deferred checks reject an
  application commit with unsettled evidence or inconsistent reservations.
- History storage now has three typed correction tables, immutable first-owner
  bootstrap, a current head, encrypted deletable displays, and date-bucket-aware
  counts/erasure. All five tables remain application SELECT-only pending the
  version-aware reader and execution-admission stage.
- The retained-history adapter reuses existing identity/canonical HMAC purposes;
  body-only changes create different semantics with unchanged metadata. Displays
  use a separate, version-bound AEAD purpose and a 4 KiB plaintext limit.
- Native delta planning now builds atomic metadata and note/task update
  proposals. Metadata retains ownership separately from equality, preserves local
  links, checks complete alias absence, reports source gaps, and counts qualified
  clears/removals. Activity preserves immutable fields and native local state,
  validates role mappings, and counts completion/reopen without inventing actors.
  These are pure preparation components; persistence execution is not wired.
- Cohort preparation now has a bounded transactional page runner. It freezes
  original/admitted/recovered identity proofs behind a workspace boundary, records
  exclusions, fences current admin/lease ownership, and settles exact byte charges
  with the checkpoint. Database guards also enforce the frozen identity/terminal
  boundary and reject application cohort/progress writes without a live payer
  claim. Cohort/index dispatch now shares the existing one-second scheduler.
- Core capture indexing now retains authenticated page/cursor evidence and every
  source occurrence before Person filtering, with scoped raw references, lease
  fences and atomic accounting/checkpoints. It reuses the existing metadata and
  activity parsers and performs no source calls. Persisted core classification remains unwired. The immutable core index now resolves across
  family/mapping plans without recopying or reencrypting evidence.
- New positively applied original/admitted activity results retain exact encrypted
  native after-state/revision. A first-result discovery adapter checks frozen
  cohort/identity/manifest/result ownership and current equality/revision; missing
  legacy evidence holds. Legacy bootstrap and refresh execution remain.
- New successful original/admitted metadata Person results retain encrypted full
  tag/typed-field state, metadata revision, exact Organization/import/manifest/Person
  binding and insertion ownership. Already-present cells remain unowned; owned
  tags retain all supporting source-key aliases. Held units publish no baseline.
  The verifier rejects missing legacy proof, binding mismatch, local changes/ABA
  and refresh-head replacement. Original planning budgets now include the existing
  native state; exact ciphertext is charged through each existing result ledger.
  A scoped metadata discovery adapter now selects exact original/admitted results
  and checks terminal first coverage, Person identity, proof binding and current
  revision. Metadata/activity discovery now also requires capture ordering after
  first-family coverage; activity coverage must have terminated before bundle
  creation. Legacy adapters and classification remain pending.
- Core source resolution reconciles all occurrences before cohort filtering,
  rejects conflicting Person links/open-versus-completed task streams, and
  requires note detail. Shared manifest references remain bundle/Org/kind/Person
  scoped; database native guards inspect all source-ID occurrences.
- The bounded history index authenticates retained capture/observation hashes,
  identity, stream totals, cursor continuity and ordering after the frozen core
  anchor. It retains all parsed occurrences, explicit diagnostic page evidence,
  raw references and metadata-only displays, with transaction/lease/ledger fences.
  It does not create native history facts or corrections.
- History selection reconciles the complete occurrence set and authenticates
  original-namespace identity hashes and metadata-only evidence. Baseline discovery
  verifies original/admitted first ownership, current typed correction bindings,
  display authentication/erasure and strictly newer capture ordering. Original/admitted
  first owners and successive typed correction heads have authenticated database
  evidence. Prior corrections now verify the exact cohort, terminal/frozen boundary,
  immediate predecessor and encrypted source/display agreement. Qualified new
  history identities now classify through the source walker.
- Core resolution independently checks each family's exhausted streams and the
  final authenticated cursor, including settled note-detail work. Shared indexing
  does not make an unfinished activity stream a metadata prerequisite.
- Prior-refresh metadata/note/task discovery now authenticates the current
  successful result in its original AEAD scope, verifies the exact frozen cohort,
  terminal predecessor and source ordering, and checks complete native state and
  revision. Metadata retains separate ownership/aliases; activity requires explicit
  positive ownership. Invalid heads never fall back to first-import evidence.
  These read adapters do not yet produce results through a refresh executor.
- All three baseline discovery paths now separately enforce prior accepted scan
  boundaries for the exact family/cohort. Held work and zero-write cancellation
  cannot make the same capture newly eligible, even with no applied refresh head.
  A genuinely newer capture can still use the unchanged older baseline. This
  reuses confirmed plans/cohorts; exact remainder execution remains unwired.
- New-identity prerequisite discovery now combines authenticated complete-source
  resolution with frozen/live Person checks, first-family successful-result and
  capture boundaries, accepted scans, and global/native collision rejection.
  Original and admitted/recovered cohorts use separate owner proofs. This is a
  read-only candidate adapter; metadata/activity mapping conversion and persisted
  classification remain outstanding. History proposals freeze prospective IDs.
- Qualified history units can now persist encrypted addition/current/correction
  proposals, including exact prior-version evidence and stable prospective IDs.
  Replay keeps IDs/counts/charges unchanged; manifest, count/position and byte
  settlement share one transaction. History source diagnostics and out-of-cohort
  exclusions now persist through a bounded keyset walk. A second bounded pass
  holds original/admitted identities absent from the new source; complete-plan
  sealing remains pending. Qualified identities with ineligible/unproven
  baselines now persist encrypted, counted holds atomically without prospective
  native IDs or heads; replay preserves the held outcome even if prerequisites
  later become eligible.
- Native activity planning also builds initial note/task rows for qualified new
  identities, with content/role validation and source completion attribution.
  Retained mapping conversion, persisted activity units and execution remain.
- Typed bundle preparation now validates retained selections, freezes encrypted
  bindings and plan IDs, meters admission/cancellation capacity and records an
  authorized replay receipt atomically. History-only indexing no longer treats
  the latest cohort's creation boundary as a global blocker; existing and new
  identity eligibility enforce each cohort's own boundary.
- Preparation dispatch now authenticates frozen bindings, claims 60-second leases,
  freezes the shared cohort and indexes retained core/history sources in bounded
  steps. Core plans reuse the payer's index. Exact token/epoch release protects a
  successor; capacity and integrity failures pause with metered control capacity.
  The existing one-second scheduler gives this adapter a finite turn without a
  new polling loop. It now drives observed and missing history classification plus core mapping
  inventory. Core classification, execution, revoked-executor pause/resume and release-readiness
  integration remain.
- Item-summary reads now use scoped keyset pages with an immutable upper bound,
  25/default and 50/max rows, decimal counters, 4 KiB summary and 512 KiB response
  limits. Queries avoid manifest ciphertext; cursors bind actor, Organization,
  workspace, bundle/plan revisions, family, cohort/outcome filters, size and order.
  Current admin/workspace checks and ordered bundle/plan locks apply to every page.
  Bundle list/detail and current-family summaries now project bounded state and
  exact counts without encrypted payloads. List and item cursors bind the current
  workspace revision. Remaining readers and full field/proof review are pending.
- Migration 010 adds a fixed-width source-walk completion flag and database
  fences against skipped occurrences, missing outcomes, stale leases and cursor
  regression. Each history outcome/checkpoint/charge commits together; equal
  occurrences reuse the identity unit. Source exhaustion remains preparation,
  with no ready digest or native writes.
- Migration 011 adds a separate owned-history cursor and completion flag. Its
  database selection shares exact frozen cohort/parent/account/terminal-owner
  bounds with the cursor fence. Original/admitted identities absent from the
  capture persist encrypted holds; observed identities reuse existing outcomes.
  Absence never deletes native history, reconstructs erased facts or advances a
  baseline. Refresh-owned initial identities must extend this selection when
  their ownership model lands.
- Core mapping inventory now walks at most 50 elements per transaction, with
  original source encryption scopes, frozen-cohort filtering, exact original
  field/option/role keys and database-folded tag groups. Full source values remain
  in retained references; new mappings default to hold. Native field-creation
  limits remain distinct from intrinsic mapping-value validity and complete
  record qualification. Note authors come from enriched detail, never list
  fallback. Migration 012 binds mapping references, family/parent relationships,
  current admin/lease and bounded element progress. Bounded mapping summaries now
  authenticate kind/key/parent/source/value/choice bindings and expose only small
  labels and explicit choices. Their cursors bind inventory progress as well as
  actor/Org/workspace/bundle/plan/filter/size, so partial discovery cannot silently
  change a page. Migration 013 adds the filtered review index. Typed Plan now
  admits up to 50 explicit choices into an immutable successor, authenticates
  frozen conversion bindings, invalidates the combined digest and settles old/new
  control capacity atomically. Migration 014 retains encrypted predecessor-bound
  patches; inventory inherits untouched choices and stable prospective IDs over
  the same shared sources. Existing destinations are scoped snapshots, option
  targets bind the effective field, and inactive assignees are rejected. Activity
  timezone omission inherits; explicit null clears. Native conversion and
  classification remain pending; mapping intent creates no native rows.
- Activity conversion now resolves complete source groups before applying the
  current plan's authenticated mappings. It reuses original note HTML/content and
  task-time conversion, requires note detail, checks role destination snapshots,
  validates explicit kind/timezone choices and reconciles source-user timezone
  evidence. Missing timezone and conflicting evidence hold date-only tasks. It
  returns native proposal inputs and exact mapping/source references without
  source calls or native writes. A typed activity-unit runner now persists
  encrypted insert/update/current or held outcomes with exact first-coverage,
  baseline/head/revision and mapping evidence. Replay preserves earlier holds and
  prospective IDs before mutable checks. Manifest/count/position/byte settlement
  is atomic; native rows/identities/heads remain unchanged. The scheduler now
  traverses one activity occurrence at a time after mappings complete, with
  duplicate identity reuse, full conflict resolution before cohort filtering,
  encrypted diagnostics and explicit exclusions. Migration 015 fences live
  leases, no-skip cursors, required outcomes and source exhaustion. Unit outcome,
  cursor and charges commit together. Absent-owned classification and complete-
  plan sealing remain pending; source exhaustion remains preparation.
- No refresh HTTP commands or Web workflow are exposed yet.

## Evidence and isolation

Earlier checkpoint evidence through typed admission at `108678c`, including
qualified-unit planning at `d9d2a43` and identity qualification at `26323f6`,
is archived in [Project history](../plans/PROJECT_HISTORY.md#010g1-foundation-and-discovery-checkpoints--2026-09-15).

Earlier dispatcher, item-summary, qualified-hold and source-walk verification is
archived in [Project history](../plans/PROJECT_HISTORY.md#010g1-preparation-and-source-classification-checkpoints--2026-09-16).

- Missing-history classification: all 31 serial family database regressions,
  API-library Clippy with warnings denied, formatting and diff checks passed.
  The empty-capture scenario verifies scoped original ownership, encrypted
  source-not-observed holds, unchanged native identities/heads, capacity and
  injected-failure rollback, forbidden cursor skips and premature completion.
  Admitted ownership and observed-identity reuse are also covered. Logs:
  `/private/tmp/010g1-missing-family-db.log`, `/private/tmp/010g1-missing-clippy.log`.
  Runner: serial `all family_refresh`; Clippy as above. The pure comparison
  policies were unchanged; their 40-test evidence remains the source-walk run.
- Bundle/family summaries and cursor revision binding: all 32 serial family
  database regressions, API-library Clippy with warnings denied, formatting and
  diff checks passed. Coverage includes three-family ordering, zero-count wire
  precision, stable list pagination as new bundles appear, size/tamper/actor/
  tenant rejection, revoked membership, detached workspace and workspace-revision
  cursor invalidation. Summary projections fetch no encrypted evidence bodies.
  Initial compilation caught an unavailable hex helper; the implementation now
  reuses the existing import encoder without a new dependency. Logs:
  `/private/tmp/010g1-summaries-family-db.log`,
  `/private/tmp/010g1-summaries-final-clippy.log`. Runners match the prior milestone.
- Bounded mapping inventory: the full family run passed 32 of 33 tests; its
  remaining legacy accounting probe used an unfenced synthetic insert. That
  fixture now supplies a valid preparation lease/phase and still verifies that
  unsettled evidence cannot commit; its focused rerun passed. The new inventory
  test covers a 70-option field across the 50-element boundary, folded tag aliases,
  note-detail-only authors, repeated task roles, unsupported-record isolation,
  source reuse across families, capacity/fault rollback and completion guards.
  API-library Clippy with warnings denied, formatting and diff checks passed.
  Logs: `/private/tmp/010g1-mapping-family-db.log`,
  `/private/tmp/010g1-mapping-accounting-db.log`, `/private/tmp/010g1-mapping-clippy.log`.
  Runners: serial `all family_refresh`, then the corrected accounting test only;
  Clippy as above. This is preparation evidence, not native execution readiness.
- Mapping review: the expanded inventory/review database scenario passed on the
  final tree, including an explicit check that migration 013's index is installed.
  It pages all 70 options without repeats, rejects altered filter/size/actor/
  workspace cursors, denies foreign Organizations and revoked readers, fails
  closed on wrong encryption keys, and invalidates partial-inventory cursors after
  progress. API-library Clippy with warnings denied, formatting and diff checks
  passed. Logs: `/private/tmp/010g1-mapping-read-index-db.log`,
  `/private/tmp/010g1-mapping-read-clippy.log`. Database runner: serial
  `all family_refresh_mapping_inventory`; preceding family evidence is retained above.
- Typed mapping revisions: 41 family unit tests and API-library Clippy with
  warnings denied passed. The full serial family database run passed 33 of 34;
  the remaining admitted-history fixture incorrectly assumed its first UUID-
  ordered identity was always an event. It now checks the selected identity's
  actual family/person/HMAC, and its focused rerun passed. The new Plan scenario
  verifies immutable old choices, stable proposed IDs through inheritance,
  shared-source reuse, exact settlement, capacity/injected-failure rollback,
  replay/stale-revision/tenant rejection, field-option binding, inactive-assignee
  rejection and timezone inheritance/clearing. Its initial activity assertion
  exposed missing fixture captures; the completed scenario passed both focused
  and full runs. Formatting and diff checks passed. Logs:
  `/private/tmp/010g1-plan-family-db.log`,
  `/private/tmp/010g1-plan-admitted-fix-db.log`,
  `/private/tmp/010g1-plan-db-final.log`, `/private/tmp/010g1-plan-unit.log`,
  `/private/tmp/010g1-plan-clippy.log`. Runners: serial `all family_refresh`,
  corrected admitted-owner test only, `crm-app --lib family_refresh`, and the
  established Clippy command. Native conversion/execution remain unverified work.
- Activity conversion: the expanded typed-plan database scenario and final
  API-library Clippy passed, along with formatting/diff checks. It verifies detail
  content instead of list fallback, explicit task-kind/role mapping, date-only
  conversion in the selected zone, timezone clearing and source-user conflict,
  changed destination snapshots, foreign Organizations and released leases.
  Initial test-helper compilation errors were corrected before the passing run.
  Logs: `/private/tmp/010g1-activity-mapping-conflict-final-db.log` and
  `/private/tmp/010g1-activity-mapping-final-clippy.log`. Runner: serial
  `all family_refresh_plan_choices`; preceding regression evidence remains above.
- Activity-unit persistence: the new serial database scenario and API-library
  Clippy passed, with formatting/diff checks. It verifies an imported task update,
  new-task prospective ID, inactive-assignee hold, exact encrypted/relational
  counts, capacity/injected-failure rollback and replay after mapping revocation.
  Native task snapshots and refresh heads remain unchanged. Initial Clippy enum
  size findings were resolved with boxed evidence before the passing run. Logs:
  `/private/tmp/010g1-activity-plan-db.log` and
  `/private/tmp/010g1-activity-plan-final-clippy.log`. Runner: serial
  `all family_refresh_activity_proposals`; this new adapter is not dispatched yet.
- Activity traversal: all 35 serial family database regressions and API-library
  Clippy passed, plus formatting/diff checks. Coverage includes complete-group
  conflicts before cohort filtering, exclusions, note list/detail identity reuse,
  failed cursor/count/charge rollback, forbidden cursor skips and premature
  completion, and replay of already-frozen unit outcomes. The initial command
  run passed 8/9; the remaining old phase expectation was updated from mappings
  to classify for activity, then the full family run passed. Logs:
  `/private/tmp/010g1-activity-walk-family-db.log` and
  `/private/tmp/010g1-activity-walk-clippy.log`. Runner: serial `all family_refresh`.
- Rust target `/private/tmp/crm-010g1-target-20260915`; intended Web output
  `/private/tmp/crm-010g1-web-dist-20260915`; synthetic DB
  `crm_010g1_schema_20260915`. Database tests run serially.
- Shared 010e5 runtime, native stores, local 010e6 and release artifacts preserved.
  No live FUB/customer processing. Linker reports the large `__eh_frame` warning.

Remaining: final capability inventories and refresh-owned initial identities;
new-identity first-coverage classification, legacy baselines and refresh-executor after-state production; classification/execution dispatcher
integration; metadata/activity/history execution; typed commands and bounded
readers; common Web workflow; independent implementation
review; full database/browser/performance/final gates. Do not report 010g1 done.
