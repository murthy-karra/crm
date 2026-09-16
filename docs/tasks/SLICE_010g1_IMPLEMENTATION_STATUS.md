# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- Current milestone: bounded item summaries following preparation dispatch at
  **`f760907`**, typed admission at **`108678c`** and qualified-unit planning at
  **`d9d2a43`**.
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
  immediate predecessor and encrypted source/display agreement. Persisted new-identity
  classification remains outstanding.
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
  read-only candidate adapter; mapping/native validation, frozen target allocation
  and persisted classification remain outstanding.
- Qualified history units can now persist encrypted addition/current/correction
  proposals, including exact prior-version evidence and stable prospective IDs.
  Replay keeps IDs/counts/charges unchanged; manifest, count/position and byte
  settlement share one transaction. Dispatcher-owned held/excluded/source-gap
  settlement and complete-plan sealing are still pending.
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
  new polling loop. It stops at mappings; classification/execution dispatch,
  revoked-executor pause/resume and release-readiness integration remain.
- Item-summary reads now use scoped keyset pages with an immutable upper bound,
  25/default and 50/max rows, decimal counters, 4 KiB summary and 512 KiB response
  limits. Queries avoid manifest ciphertext; cursors bind actor, Organization,
  workspace, bundle/plan revisions, family, cohort/outcome filters, size and order.
  Current admin/workspace checks and ordered bundle/plan locks apply to every page.
  Other readers and full field/proof review remain pending.
- No refresh HTTP commands or Web workflow are exposed yet.

## Evidence and isolation

Earlier checkpoint evidence, including new-identity qualification at `26323f6`,
is archived in [Project history](../plans/PROJECT_HISTORY.md#010g1-foundation-and-discovery-checkpoints--2026-09-15).

- Qualified history preparation/native insertion planning: all 24 serial family
  database regressions and 40 focused Rust tests passed, plus `crm-app --lib`
  Clippy with warnings denied, formatting and diff checks. Real retained history
  fixtures freeze new/current/body-only-correction units, decrypt their exact
  proposals, verify stable target/version IDs and replay without changed counts
  or charges. Capacity rejection and an injected final checkpoint failure leave
  no partial manifest, position, count or ledger mutation. Native insertion
  policy checks ownership/coverage, content/roles, fixed IDs and source completion
  with no invented native actor. These are preparation checks, not execution.
  Final logs: `/private/tmp/010g1-preparation-family-db.log`,
  `/private/tmp/010g1-preparation-unit.log`,
  `/private/tmp/010g1-preparation-clippy.log`.
  Runners: `cargo test -p crm-api --features test-support --test all
  family_refresh --locked -- --ignored --test-threads=1`; `cargo test -p crm-app
  --lib family_refresh --locked`; `cargo clippy -p crm-app --lib --locked --
  -D warnings`. All used the isolated target/database below.
- Typed preparation/per-cohort history ordering: all 27 serial family database
  regressions and 40 focused Rust tests passed, plus `crm-app --lib` Clippy with
  warnings denied, formatting and diff checks. The three new command scenarios
  cover combined/history-only payers, encrypted frozen bindings, exact retained
  and reserved bytes, whole-admission rollback, same-request replay despite a
  newly restrictive budget, changed-body/active-bundle conflicts, invalid fields,
  unauthorized callers and another Organization's real source report. The history
  fixture proves indexing still succeeds when an admitted cohort's creation
  interval overlaps, while both its new and existing identities stay held until
  the boundary is valid. No native-refresh capability is installed by preparation.
  Logs: `/private/tmp/010g1-command-family-db.log`,
  `/private/tmp/010g1-command-unit.log`, `/private/tmp/010g1-command-clippy.log`.
  Runners match the previous milestone: serial `all family_refresh` database
  tests, focused library tests and library Clippy in the isolated target/database.
- Bounded preparation dispatcher: all 29 serial family database regressions,
  40 focused library tests and `crm-api --lib` Clippy with warnings denied passed.
  Combined preparation reaches mappings for every selected family through real
  claims. Tests cover active-owner exclusion, expired takeover, stale release,
  exact settlement, storage-limit pause and wrong-key integrity pause before any
  cohort work. Formatting and diff checks passed. Logs:
  `/private/tmp/010g1-worker-family-db.log`, `/private/tmp/010g1-worker-unit.log`,
  `/private/tmp/010g1-worker-clippy.log`. Runners match the prior checkpoint with
  Clippy widened to the API library containing scheduler integration.
- Bounded item summaries: the real original-history preparation scenario passes
  on the final tree with three persisted action classes, stable growing-plan
  pagination, valid cohort/outcome filters and rejection of altered size/filter,
  tampered cursors, a second authorized actor, changed bundle revision, unknown
  cohort and another Organization's current admin. Non-admin reads are denied.
  Summaries contain no proof ciphertext or source-body sentinel. API-library
  Clippy, formatting and diff checks passed. Logs:
  `/private/tmp/010g1-items-final-db.log`, `/private/tmp/010g1-items-clippy.log`.
  Database runner: serial `all history_baseline_authenticates_original_owner`;
  the full 29-test family baseline is retained at `f760907` above.
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
