# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** P1–P4 and shared contracts were accepted
through Lavish Send & End. Do not reopen that session. Deployment is deferred.

## Current work

- Branch `codex/010g1-family-refresh`, base `dd140b0`; one primary writer and the
  previously authorized single reviewer. Planning review READY, round 1.
- User requested “commit and proceed”; foundation checkpoint **`8b5959e`** is
  committed; accounting checkpoint **`0ef822f`** is also committed. History
  correction storage is committed as **`047e3e5`**; native delta planning is
  committed as **`cbae4c6`**. Cohort preparation is committed as **`b949ffe`**. The combined feature is unfinished. No merge/push/deploy.
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
  claim. Dispatcher integration remains pending.
- Core capture indexing now retains authenticated page/cursor evidence and every
  source occurrence before Person filtering, with scoped raw references, lease
  fences and atomic accounting/checkpoints. It reuses the existing metadata and
  activity parsers and performs no source calls. Dispatcher/classification and
  classification remain unwired. The immutable core index now resolves across
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
- No refresh HTTP commands, family dispatcher or Web workflow are exposed yet.

## Evidence and isolation

- Foundation: 12 focused Rust tests and 2 database regressions passed; fresh schema,
  direct retained-size check and formatting passed. Initial syntax/hash failures
  were corrected. This is targeted foundation evidence, not final package gates.
- Accounting: all 3 strengthened database regressions passed, including the
  deferred application-commit guard, expired-lease takeover, exact-once refunds,
  snapshot/Organization balances and catalog-driven byte inventory. Fresh schema
  application and formatting also passed. Logs:
  `/private/tmp/010g1-accounting-db-tests.log` and
  `/private/tmp/010g1-accounting-schema.log`.
- History: all 14 focused Rust tests and 6 database regressions passed; fresh
  schema, formatting and diff checks passed. The Rust tests cover namespace
  continuity, body-only semantic
  changes, metadata privacy and version-bound encryption. Database storage tests
  exercise two successive corrections for all three fact types, preserved first
  ownership, invalid head/Person writes, immutable facts, known/unknown counts,
  suppression, and exact erasure refunds across completed plans. Fixture failures
  (reinstalling the permanent marker and settling before terminal-state changes)
  were corrected; they are not ignored checks. Final logs:
  `/private/tmp/010g1-history-policy-tests.log`,
  `/private/tmp/010g1-history-db-tests.log`,
  `/private/tmp/010g1-history-legacy-tests.log` and
  `/private/tmp/010g1-history-schema.log`. These are targeted storage/adapter
  checks, not evidence of a working refresh HTTP/worker/Web flow.
- Native delta planning: all 28 focused family tests passed, including 8 new
  metadata and 6 new activity tests. Checks cover ownership, alias conflicts,
  missing/null/empty data, serialization, local edits/ABA, capacity, role mappings,
  task completion/reopen, and immutable fields. Clippy (`crm-app --lib`, warnings
  denied), formatting, and diff checks passed. The existing database 20-tag limit
  and idempotent-reapplication regression passed. An intermediate test-helper
  signature compile failure was corrected and rerun successfully. Logs:
  `/private/tmp/010g1-native-delta-tests.log`,
  `/private/tmp/010g1-native-delta-clippy.log` and
  `/private/tmp/010g1-native-tag-regression.log`. No refresh execution or UI claim.
- Cohort preparation: all 6 family database regressions passed, including
  application-role paging, admission/recovery proof selection, terminal cutoff,
  wrong Organization/token rejection, capacity rollback, injected checkpoint
  failure rollback, replay and exact metering. The initial fixture-count assertion
  was corrected (one original plus one admitted Person). Migration application,
  `crm-app` Clippy with warnings denied, formatting and diff checks passed. Logs:
  `/private/tmp/010g1-cohort-db-tests.log`,
  `/private/tmp/010g1-cohort-clippy.log`, and
  `/private/tmp/010g1-cohort-schema.log`. Full workflow gates remain outstanding.
- Cohort database fences: the strengthened 6-test family suite passed, including
  direct application inserts and progress updates without lease context. Migration
  application, formatting and diff checks passed. Logs:
  `/private/tmp/010g1-cohort-fences-db-tests.log` and
  `/private/tmp/010g1-cohort-fences-schema.log`.
- Core indexing/activity baseline checkpoint: all 9 family database tests, the
  existing original activity fidelity regression (plain/HTML long notes and task
  date/role policy), and all 29 focused Rust tests passed. Tests cover authenticated
  pagination, conflicting Person occurrences, inaccessible detail, corrupted source
  HMAC, injected checkpoint rollback, replay, exact byte inventory/settlement,
  original/admitted baseline discovery and wrong-cohort rejection. Fresh schema,
  `crm-app --lib` Clippy with warnings denied, formatting and diff checks passed.
  Initial request serialization/trigger-record-shape errors and synthetic fixture
  assumptions were corrected and rerun. Logs:
  `/private/tmp/010g1-core-index-db-tests.log`,
  `/private/tmp/010g1-after-state-original-regression.log`,
  `/private/tmp/010g1-index-baseline-unit.log`,
  `/private/tmp/010g1-index-baseline-clippy.log`, and
  `/private/tmp/010g1-core-index-schema.log`. No end-to-end refresh claim.
- Metadata after-state checkpoint: all 33 focused family Rust tests and 11
  database regressions passed (8 original metadata source/fidelity scenarios,
  admitted typed metadata, and original/admitted result-failure rollback). The
  actual executors produce decrypted proofs matching native revisions, tags and
  all four field types; tests also reject foreign bindings and edit/revert,
  preserve local ownership and supporting aliases, and verify exact ledger/retry
  behavior. `crm-app --lib` Clippy with warnings denied, formatting and diff
  checks passed. Logs: `/private/tmp/010g1-metadata-baseline-unit.log`,
  `/private/tmp/010g1-metadata-baseline-db.log`,
  `/private/tmp/010g1-metadata-baseline-clippy.log`, and
  `/private/tmp/010g1-{admitted_metadata_typed_units_preserve_types_and_settle_together,admitted_metadata_result_failure_rolls_back_native_claim_checkpoint_and_bytes,metadata_concurrency_person_failure_rolls_back_all_cells_result_cursor_and_bytes}.log`.
  Runners: `cargo test -p crm-app --lib family_refresh --locked`,
  `cargo test -p crm-api --features test-support --test all metadata_source_
  --locked -- --ignored --test-threads=1`, then three exact tests using that
  freshly compiled `debug/deps/all-07593333edaca6c2` binary, serially against the
  owned synthetic database. No refresh discovery/execution or final-gate claim.
- Discovery/source-index checkpoint: 13 serial family database regressions and
  33 focused Rust tests passed. These include original/admitted metadata discovery,
  edit/revert and wrong-cohort/token holds; complete source occurrence conflicts,
  negative note detail, shared cross-family source reuse without duplicate storage;
  and 103 history records over four pages with privacy, corruption rollback,
  checkpoint-failure rollback, replay and exact accounting. Fresh migrations,
  `crm-app --lib` Clippy with warnings denied, formatting and diff checks passed.
  Initial SQL CASE syntax and Rust visibility checks failed during implementation,
  were corrected, and passed subsequent schema/build/tests. Logs:
  `/private/tmp/010g1-metadata-discovery-db.log`,
  `/private/tmp/010g1-resolution-db.log`,
  `/private/tmp/010g1-history-index-db.log`,
  `/private/tmp/010g1-evidence-discovery-unit.log`,
  `/private/tmp/010g1-history-index-clippy.log`,
  `/private/tmp/010g1-source-reuse-schema.log`,
  `/private/tmp/010g1-history-index-schema.log`.
  Runners: `cargo test -p crm-api --features test-support --test all family_refresh
  --locked -- --ignored --test-threads=1`; `cargo test -p crm-app --lib
  family_refresh --locked`; isolated target/database as below. No final workflow gate claim.
- Baseline/source-boundary and history-budget checkpoint: 33 focused Rust tests
  and all 15 family database regressions have passing evidence. The final suite
  passed 14; the new takeover fixture initially attempted replacement before lease
  expiry and was correctly rejected. After explicitly expiring that lease, the
  affected regression passed on the final tree. Checks cover original/admitted
  history ownership, body-only corrections, new-identity holds, overlapping source
  intervals, late first-coverage completion, independent family stream exhaustion,
  exact history capture charges, source budget rejection, one-time takeover
  refunds and erasure refunds to both original captures. Clippy (`crm-app --lib`,
  warnings denied), migration application, formatting and diff checks passed.
  Logs: `/private/tmp/010g1-final-baseline-db.log`,
  `/private/tmp/010g1-history-budget-takeover-db.log`,
  `/private/tmp/010g1-baseline-boundaries-unit.log`,
  `/private/tmp/010g1-baseline-budget-clippy.log`, and
  `/private/tmp/010g1-history-budget-schema.log`.
  Runners: serial `cargo test -p crm-api --features test-support --test all
  family_refresh --locked -- --ignored --test-threads=1`, then the exact affected
  `history_index_preserves_pages_privacy_and_atomic_accounting` test; focused
  `cargo test -p crm-app --lib family_refresh --locked`, all in the isolated
  target/database. Prior-correction discovery still needs authenticated fixture
  evidence. This is not a final workflow/release gate.
- Prior-refresh native discovery: the three new database regressions passed
  under the application role with scoped preparation claims. They cover metadata
  ownership/aliases, authenticated note/task after-states at revision 2, exact
  previous capture ordering, edit/revert, foreign Organization/cohort/token,
  corrupted result AEAD, unowned-head rejection without older-result fallback,
  and repeated discovery without new results. Successful refresh results are
  migrator-built fixtures; this is not refresh-executor evidence. All 36 focused
  Rust tests, all 18 serial family database regressions, `crm-app --lib` Clippy
  with warnings denied, formatting and diff checks passed. Initial
  fixture omissions (no captured note, control settlement using its old rather
  than current lease epoch) and two Clippy findings were corrected. A proposed
  lookup index duplicated the existing index and was removed; no schema change
  is included. Logs: `/private/tmp/010g1-prior-baseline-db.log`,
  `/private/tmp/010g1-prior-baseline-family-db.log`,
  `/private/tmp/010g1-prior-baseline-unit.log`, and
  `/private/tmp/010g1-prior-baseline-clippy.log`.
  Runners: `cargo test -p crm-api --features test-support --test all prior_
  --locked -- --ignored --test-threads=1`; `cargo test -p crm-app --lib
  family_refresh --locked`. The full family database rerun used the freshly
  compiled `debug/deps/all-07593333edaca6c2 family_refresh --ignored
  --test-threads=1` binary. Same isolated target/database as below.
- Authenticated prior-history correction discovery: all 21 serial family database
  regressions and 37 focused Rust tests passed, plus `crm-app --lib` Clippy with
  warnings denied, formatting and diff checks. Three new database scenarios use
  genuine retained indexes and scoped encrypted displays: two successive event/
  call/text corrections for original and admitted owners, wrong version scope,
  validly encrypted but source-mismatched metadata, late predecessor boundaries,
  replay without read-model changes, first-owner preservation, exact settlement
  and erasure without original-version fallback. Correction inserts remain
  migrator-only fixtures; no execution or public timeline claim. Logs:
  `/private/tmp/010g1-correction-baseline-db.log`,
  `/private/tmp/010g1-correction-family-db.log`,
  `/private/tmp/010g1-correction-baseline-unit.log`,
  `/private/tmp/010g1-correction-baseline-clippy.log`.
  Runners: `cargo test -p crm-api --features test-support --test all
  db_family_refresh_history_baseline --locked -- --ignored --test-threads=1`,
  then the freshly compiled `debug/deps/all-07593333edaca6c2 family_refresh
  --ignored --test-threads=1`; `cargo test -p crm-app --lib family_refresh --locked`.
- Accepted scan boundaries: all 22 serial family database regressions and 37
  focused Rust tests passed, plus `crm-app --lib` Clippy with warnings denied,
  formatting and diff checks. The new cancellation regression proves that an
  accepted zero-write scan does not advance the applied baseline, while fresh
  preparation rejects reuse or overlap and accepts a later capture. Accepted/
  cancelled plans are migrator-built fixtures; typed confirmation and execution
  remain outstanding. Logs: `/private/tmp/010g1-scan-boundary-db.log`,
  `/private/tmp/010g1-scan-family-db.log`,
  `/private/tmp/010g1-scan-boundary-unit.log`, and
  `/private/tmp/010g1-scan-boundary-clippy.log`.
  Runners: `cargo test -p crm-api --features test-support --test all
  accepted_scan_survives --locked -- --ignored --test-threads=1`, then the freshly
  compiled `debug/deps/all-07593333edaca6c2 family_refresh --ignored
  --test-threads=1`; `cargo test -p crm-app --lib family_refresh --locked`.
- New-identity prerequisites: all 24 serial family database regressions, 37
  focused Rust tests and `crm-app --lib` Clippy with warnings denied passed.
  Original and admitted activity/history fixtures qualify later new identities;
  missing first coverage, late completion, existing identities, foreign scope,
  legacy/canonical native source collisions and native tombstones are rejected.
  Repeated discovery allocates no identity or head. The strengthened two-test
  activity collision rerun also passed. A test-only attempt to clone a non-Clone
  lease claim was corrected before the successful full run. Logs:
  `/private/tmp/010g1-new-identity-family-db.log`,
  `/private/tmp/010g1-new-identity-collision-db.log`,
  `/private/tmp/010g1-new-identity-unit.log`, and
  `/private/tmp/010g1-new-identity-clippy.log`.
  Runners: `cargo test -p crm-api --features test-support --test all
  family_refresh --locked -- --ignored --test-threads=1`, then the
  `family_refresh_new_activity` filter; `cargo test -p crm-app --lib
  family_refresh --locked`. No refresh executor or workflow completion claim.
- Rust target `/private/tmp/crm-010g1-target-20260915`; intended Web output
  `/private/tmp/crm-010g1-web-dist-20260915`; synthetic DB
  `crm_010g1_schema_20260915`. Database tests run serially.
- Shared 010e5 runtime, native stores, local 010e6 and release artifacts preserved.
  No live FUB/customer processing. Linker reports the large `__eh_frame` warning.

Remaining: final capability inventories and refresh-owned initial identities;
new-identity first-coverage classification, legacy baselines and refresh-executor after-state production; cohort/index dispatcher
integration; metadata/activity/history execution; typed commands and bounded
readers; common Web workflow; independent implementation
review; full database/browser/performance/final gates. Do not report 010g1 done.
