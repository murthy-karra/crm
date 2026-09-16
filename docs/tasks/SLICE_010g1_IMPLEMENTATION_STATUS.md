# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- Current milestone: metadata catalog qualification following missing-activity
  ownership traversal at **`c7e456b`**.
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
  activity parsers and performs no source calls. Metadata/catalog classification remains pending; activity dispatch is wired below. The immutable core index now resolves across
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
  read-only candidate adapter; metadata/catalog conversion and classification remain outstanding; activity
  proposals now persist and traverse as described below. History proposals freeze prospective IDs.
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
  Retained mapping conversion and persisted units are wired below; execution remains.
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
  inventory. Metadata/catalog classification, execution, revoked-executor pause/resume and release-readiness
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
  timezone omission inherits; explicit null clears. Metadata conversion and
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
  cursor and charges commit together. A second pass now walks exact original/
  admitted activity owners within the frozen cohort. It reuses observed outcomes
  and persists encrypted missing-source holds with authenticated baseline evidence
  when available. Migration 016 fences the composite identity cursor, charges its
  variable-width state, and checks missing-source absence and owner scope. A moved
  owned source ID remains an identity-mismatch hold even when its new Person is
  outside the cohort. Native rows and heads do not change. Refresh-owned initial
  identities must extend this selector when their owner shape lands. Complete-
  plan sealing remains pending; exhausted preparation is not confirmation.
- Complete metadata field-name qualification now indexes every retained field
  occurrence, including conflicting second definitions, under the existing exact
  source-name namespace. Migration 017 meters those tokens and adds bounded name
  and incomplete-index probes. A read-only adapter resolves the complete source
  group, detects another source ID claiming the same name, authenticates the
  selected name token, and checks choice labels with native database folding.
  Native creation limits remain separate from field eligibility. Older source
  indices hold until a new bundle is prepared. Catalog destination validation,
  persisted catalog/Person proposals and metadata dispatch remain pending.
- No refresh HTTP commands or Web workflow are exposed yet.

## Evidence and isolation

Earlier checkpoint evidence through typed admission at `108678c`, including
qualified-unit planning at `d9d2a43` and identity qualification at `26323f6`,
is archived in [Project history](../plans/PROJECT_HISTORY.md#010g1-foundation-and-discovery-checkpoints--2026-09-15).

Earlier dispatcher, item-summary, qualified-hold and source-walk verification is
archived in [Project history](../plans/PROJECT_HISTORY.md#010g1-preparation-and-source-classification-checkpoints--2026-09-16).

Mapping, bounded review, typed Plan, activity conversion/proposals and source
traversal evidence through `aaccd81` is archived in
[Project history](../plans/PROJECT_HISTORY.md#010g1-mapping-and-activity-preparation-checkpoints--2026-09-16).

- Missing-activity ownership traversal: all **36** family-refresh database tests
  passed serially (`/private/tmp/010g1-activity-missing-family-db.log`). Focused
  absence, moved-Person, original/admitted owner scope, capacity rollback, lease
  fencing, replay and byte-inventory checks passed
  (`/private/tmp/010g1-activity-missing-db.log`). Library Clippy passed with
  warnings denied (`/private/tmp/010g1-activity-missing-clippy.log`).

- Complete field-name qualification: all **37** family-refresh database tests
  passed serially (`/private/tmp/010g1-metadata-catalog-family-db.log`), including
  conflicting second names, exact-case distinctions, database-folded option
  collisions, older-index holds, tenant boundaries and byte accounting. Library
  Clippy passed with warnings denied (`/private/tmp/010g1-metadata-catalog-clippy.log`);
  formatting and diff checks passed. The first focused run found a stale fixture
  count after expanding the occurrence set; corrected before the passing suite.

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
