# Project history

Archives from PROJECT_STATE.md: 2026-09-11 foundations handoff and 2026-09-13
documentation efficiency pass.
This is historical evidence, not current operational status or implementation
instructions. Dates, branch names, approvals and performance conclusions below
belong to their recorded checkpoints; later decisions (including D-050) govern.
Use [PROJECT_STATE.md](PROJECT_STATE.md) for current work, live residuals and next
actions, and the [decision log](../decisions/DECISION_LOG.md) for authority.
Per-slice verification/release records remain the detailed evidence.

## 010g1 foundation and discovery checkpoints — 2026-09-15

Local staged implementation on `codex/010g1-family-refresh`, through `26323f6`.
These are targeted milestone checks, not final workflow or release acceptance.
See [010g1 implementation status](../tasks/SLICE_010g1_IMPLEMENTATION_STATUS.md)
for remaining work and current verification.

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

## Mobile007 / 010d3 implementation — 2026-09-15

D-086's Find and Save People and admitted-People history tracks are implemented
and verified on `codex/mobile007-history010d3`. Evidence includes all 1,111 original
DB acceptance cases across the documented initial run and targeted recovery,
two additional migration cases, 1,303 Web tests, real desktop/390px migration
journeys, native discovery/offline/process-restart journeys, actual installed
Mobile006 upgrades, and complete D-050 query/paired Person/Today gates.

[Integrated verification](../tasks/MOBILE_007_010d3_FINAL_VERIFICATION.md) is the
authoritative acceptance record, including source/binary identity and retained
failures. No publication or deployment is included in this implementation milestone.

## Mobile006 / 010f4 shared-development release — 2026-09-15

D-084's authorized release merged/pushed main (`d11fccf`), then fixed the new
preparation label's narrow-screen wrapping (`a96b771`) with all Web tests/build
and real browser checks passing. API/worker/admin/migrator artifacts built at
`3c080b5` match final production Rust; Web runs `a96b771`. The actual database
already had the prior pair's 69 migrations. Five additive migrations brought it
to 74, preserving older data fields and verifying new metadata defaults. The
[release record](../tasks/MOBILE_006_010f4_RELEASE.md) owns backup, exact rowset/
artifact hashes, 75 HTTP checks, five revoked smoke sessions, guarded mobile
cleanup, all 73 public assets, tunnel routing and 180.21s final observation.
Three worktrees and five milestone branches were removed after preserving lane
histories and the exact dirty patch in a verified bundle. Native QA runtime/stores,
QA databases, evidence and recovery artifacts remain. No live customer/source,
activation, native distribution, physical phone or production-cluster work occurred.

## Mobile006 / 010f4 local completion — 2026-09-15

D-084 implementation and synthetic acceptance completed at `4bf6053` (last
production Rust repair `22c6c06`, later native fixes/tests and reviewed test-only
corrections). Both final implementation round-2 reviews are READY. All 1,103 DB
checks, 993 ordinary Rust tests, 1,298 Web tests, native store/actual UI/installed
upgrades, 18 Mobile and 26 migration plans pass. The completed paired measurement
passes exact responses and p95 limits for Person detail and Today. The
[final record](../tasks/MOBILE_006_010f4_FINAL_VERIFICATION.md) retains exact runners,
failed/interrupted attempts, corrections and scope limits. D-084's subsequent
release follow-up authorizes main publication, cleanup and shared-development
rollout; [release evidence](../tasks/MOBILE_006_010f4_RELEASE.md) owns that execution.

## Mobile006 / 010f4 implementation checkpoints — 2026-09-14

D-083 drafted the pair; both independent planning reviews returned READY and
D-084 accepted implementation and isolated synthetic verification. The root
integrated the frozen mobile backend before native writers started, then repaired
source qualification, excluded coverage, old-binary handover, native provenance,
sealed metadata qualification and bounded remainder copying within the accepted
contracts. Detailed final evidence and retained failures belong to the
[verification record](../tasks/MOBILE_006_010f4_FINAL_VERIFICATION.md).

### Earlier focused evidence

All logs are under the private QA root, with command/timing sidecars where run
through `run-check.py`:

- `logs/integration/metadata-compatibility-db-1.log`: 7 passed at `ec2ccdd`.
- `logs/integration/metadata-extra-boundaries-db-1.log`: 3 passed, observer failed;
  `metadata-tag-delete-concurrency-db-2.log`: corrected observer test passed.
- `logs/mobile006-focused-2.log`: 4 passed at writer `b744e4c`.
- `logs/mobile006-http-boundaries-2.log`: bounds passed, receipt URL failed;
  `mobile006-http-boundaries-3.log`: corrected authority test passed at `82d081e`.
- `logs/integration/admitted-activity-remainder-db-2.log`: confirmed-cancel and
  successor execution passed (6.202 test seconds) after the pointer/FK order fix.
- Migration writer Web/API 28 tests and preflight 51 tests passed as development
  checkpoints; final-tree gates remain required.


## Mobile005 / 010f3 local completion — 2026-09-14

D-082 implementation completed on `codex/mobile005-010f3-integration`, with final
tested code `ab4a362a224183dd1d51acd51196b71315826fd6` and subsequent documentation
only. Both native clients support offline Person name/contact editing; 010f3 adds
retained-source tags/custom-field imports for admitted People. Both independent
implementation reviews conclude READY within the two allowed review rounds.
The [implementation record](../tasks/MOBILE_005_010f3_IMPLEMENTATION_STATUS.md)
owns delivered behavior; [final verification](../tasks/MOBILE_005_010f3_FINAL_VERIFICATION.md)
owns all passing repository/native/browser/readiness/performance gates, source
attribution, and retained failures with corrections. All 1,076 database tests pass.

All 74 protected artifacts and four shared listeners were preserved. Retained
metadata upgrade proof preserves 15 protected rowsets, both original native cells
and exact import/snapshot/storage row hashes. QA runtimes/databases, native stores
and three completed writer worktrees remain for release handoff; only inactive
private compiler caches were removed. Shared development remains Mobile004/010e4.
No push/deployment, native distribution, live FUB/customer processing, activation
or physical-phone work occurred. Calling remains after the agreed progression.

## Mobile004 / 010e4 shared-development release — 2026-09-13

Following the user's commit/push/cleanup/deploy request, main was published and
all six merged milestone branches removed. Shared development now runs backend
`ffbc9fd` and unchanged production Web `26657c8`. Release smoke exposed and fixed
an early authorization-denial no-store header; nine affected tests and the
corrected HTTP/mobile/browser checks passed. Backup, 56 schema checksums, exact
151→161 rowset preservation and 188s stable observation passed. The
[release record](../tasks/MOBILE_004_010e4_RELEASE.md) owns details and limits.
Native demo/QA APIs, installed stores, evidence and recovery files were preserved.
No native distribution, live FUB, activation or production-cluster work occurred.

## Mobile004 / 010e4 local completion — 2026-09-13

D-080 implementation completed on `codex/mobile004-010e4-integration`, with final
application/test source `7930b83` and subsequent handoff documentation. Mobile004
adds encrypted offline stage proposals, replay and explicit conflict/follow-up
handling. 010e4 adds core refresh for terminal admission cohorts with retained
provenance, local-change holds and cancellation/remainder. Both review rounds,
installed simulator/emulator upgrades, browser acceptance, performance and final
gates passed. The [implementation record](../tasks/MOBILE_004_010e4_IMPLEMENTATION_STATUS.md)
owns detailed evidence and links; it retains failures and corrections.

The three completed clean writer worktrees and migration QA API/Web preview were
closed. Evidence, QA databases, native QA API and installed stores were preserved.
Published main remains `44dcf52`; shared development stays on its D-078 release.
No push/deployment, physical-phone, live FUB or activation claim is made. Dependent
families remain sequential follow-ups; calling remains after the agreed progression.

## Historical progress

The entries below retain the state at each recorded checkpoint. Current status,
authorization and next actions are in PROJECT_STATE.md, not these older notes.

Previously: **SLICE 019b (CUSTOM-FIELD FILTERING) — COMPLETE, MERGED, DEPLOYED AND
CLEANED UP (2026-09-10).** User authorized commit, deployment, merge with main
and loose-branch cleanup. Implementation `a642987`, merge `237639d`, followed
by a release-record commit. Only local main remains; no push was requested or
performed, and `origin/main` is still `6bad52a`.

Five custom clauses now work across People, personal saved lists, Today sources
and admin rules. Astra high planned and independently reviewed; Terra high
implemented backend and Web, with coordinator integration and thin helpers.
Both implementation review/fix rounds completed and all thirteen findings were
fixed. Final `sqlx-prepare`, `check` (831 Rust, 5 doctests, 878 Web, 11 worker)
and `check-db` (785/785) passed. Browser QA passed; the single performance run
passed thirteen paired p95 budgets and eighteen inspected plans. Zero-source
Today serial grew 19.348 ms within its accepted 25 ms allowance.

Deployed the unchanged tested source from main: API PID 67757 on 3000,
Web preview PID 68235 on 5173, exposed through `app.tarams.org`. Public health,
authenticated read/filter checks, six bundle asset comparisons and realtime
WebSocket upgrade passed. No migrations or configuration changes were needed.
Only the exact former API/Web listener PIDs were stopped. Rollback artifacts
are retained privately. The 019b branch and worktree were removed; the synthetic
QA database and the unrelated untracked `notes.txt` remain untouched.

[Implementation verification](../tasks/SLICE_019b_VERIFICATION.md) and
[release record](../tasks/SLICE_019b_RELEASE.md) contain exact revisions,
commands, screenshots, performance data and release limits. API and Web must
ship together. No active implementation lane; Slice 017 live sends remain deferred.

Previously: **SLICE 019a (TYPED CUSTOM FIELDS) — COMPLETE, MERGED, PUSHED
(2026-09-10, with the user's approval).** Merge `7dc4a2f`; source
`slice-019-custom-fields` at `c2214f8` (fifteen lane commits, the
[verification record](../tasks/SLICE_019_VERIFICATION.md) and seven
screenshots under `docs/design/qa/slice-019-2026-09-10/`). One lane
(Claude Sonnet 5), backend then Web with a checkpoint between. Review
round 1 in two halves: backend READY WITH FIXES; Web NOT READY on one
blocking defect (editor drafts seeded once and never re-synced, so with
cached definitions an editor rendered empty and a blur deleted the stored
value), reproduced by both reviewer and tester; twenty-one fixes applied
in one round; round 2 confirmed all, READY. Final-tree gates once on
`7d17ab1`: `sqlx-prepare` clean, `check` green (823 Rust, 822 Web, 11
worker), `check-db` 776 of 776 first run. Walkthrough on a QA runtime,
seven steps: admin creates the four types; a member sets and clears
values, is refused 403 on the admin API and redirected from the Fields
page; a second Organization sees nothing and gets 404; archive hides and
keeps the value; the Operator answers from `custom_fields`; restore
brings the value back. Runtime updated: `crm_dev` migrated
(`20260915000001`), the dev API restarted by exact PID (70740, binary
19:04, `/api/health` 200), `dev-web-prod` rebuilt (preview pid 71190).
Worktree `../crm-worktrees/019`, the branch and `crm_slice019_qa`
deleted. `main` pushed to `origin/main`, carrying the Slice 018 merge and
records as well. Rung 019b (custom-field filter clauses) is scheduled,
unspecified and needs its own approval (D-058 §1). No slice active.

Previously: **SLICE 019a (CUSTOM FIELDS) — APPROVED 2026-09-10, LANE IN
IMPLEMENTATION.** [SLICE_019.md](../specs/SLICE_019.md) and its
[brief](../tasks/SLICE_019_IMPL.md), reviewed READY WITH CORRECTIONS
(fifteen applied: no derived key; rename/archive/restore folded into one
`PUT` per resource, eight routes and seven commands; numbers cross the
Rust boundary as validated decimal strings because no decimal crate is
enabled and `Cargo.*` is unowned; a Person-only realtime arm; the admin
extractor precedence verified; the static `order` route beside the uuid
parameter verified and pinned), approved with one named default:
definition management is admin-only. One lane (Claude Sonnet 5) in
`../crm-worktrees/019` on `slice-019-custom-fields`; Part A backend with
a hard checkpoint (review round 1), then Part B Web (round 2). Pointer
lines added to SLICE_002 §2 and §5, SLICE_003 §6, SLICE_005 §5, the 011
and 010 ladders.

Previously: **SLICE 019 (CUSTOM FIELDS) — PLANNING (2026-09-10).** The user picked
custom fields (the last unbuilt CRM-core model, thesis §7 and §11; the
last FUB destination besides deals, SLICE_010_LADDER 010f+; the reserved
dynamic-SQL fork point in the filter ladder). **D-058 accepted (user,
2026-09-10): filtering deferred to a separate rung 019b; four FUB types;
any active member sets values; archive-only definitions; CRUD per AGENTS
§4.6; import-ready columns.** FUB's custom-field shape was verified from
its API docs (text/date/number/dropdown, choices, isRecurring,
hideIfEmpty, orderWeight). The first planner run stalled at its first
step for 50 minutes and was stopped; the retry with a tool budget
delivered in six minutes. `docs/specs/SLICE_019.md` (rung 019a) drafted
from it and sent for independent review; next the corrections, the
brief, then the implementation gate. `main` is still ahead of `origin/main` (the Slice
018 merge and records); push on the user's word.

Previously: **SLICE 018 (OPERATOR `create_task` / `complete_task`) — COMPLETE AND
MERGED TO LOCAL MAIN** at `d74c493` (2026-09-10, with the user's approval;
not pushed, not deployed). Source `slice-018-operator-tasks` at `6910665`
(thirteen lane commits, the walkthrough fix `20131e9`, the
[verification record](../tasks/SLICE_018_VERIFICATION.md) and six
screenshots under `docs/design/qa/slice-018-2026-09-10/`). One lane
(Claude Sonnet 5), backend then Web with a checkpoint between. Review
round 1 in two halves (no blocking finding; twenty fixes incl. two real
Web defects: receipt state keyed by task id, and a finalized confirm
failure left retryable); round 2 confirmed all, READY. Final-tree gates
run once on `20131e9`: `sqlx-prepare` clean, `check` green (807 Rust,
780 Web, 11 worker, 33 s), `check-db` 741 of 741 first run (254 s).
Walkthrough on a QA runtime, 8 steps: complete with receipt, Undo,
proposal, confirm, bob's confirm 404, confirm with the provider disabled,
an expired proposal 409; one finding fixed on the branch (the model
resolved "Friday" to Sunday because the time line named no weekday; the
card exposed it before anything existed; the line now names the weekday).
Runtime updated with the user's approval: `crm_dev` migrated
(`20260914000001` applied), the dev API restarted by exact PID (30690,
binary 14:51, `/api/health` 200), `dev-web-prod` rebuilt (preview pid
31100). Worktree `../crm-worktrees/018`, the branch and `crm_slice018_qa`
deleted. **`main` is ahead of `origin/main`; push needs the user's
word.** A helper Chrome with remote debugging on port 9222 was launched
for the walkthrough and left running. No slice active.

Previously: **SLICE 018 — APPROVED 2026-09-10, LANE IN IMPLEMENTATION.** [SLICE_018.md](../specs/SLICE_018.md)
and its [brief](../tasks/SLICE_018_IMPL.md) were drafted from the
planner's analysis (which found three things D-057 did not anticipate:
`TaskView` has no id, the PII-free proposal table needs a sidecar for the
proposed title, and the confirm route gates on telephony), independently
reviewed READY WITH CORRECTIONS (thirteen, all applied, none a human
decision; two veto-able defaults named at the gate: unconfirmed titles
retained until Person erasure, no per-row "completed via Operator"
marker), and approved. One lane (Claude Sonnet 5) in
`../crm-worktrees/018` on `slice-018-operator-tasks`; Part A backend with
a hard checkpoint, then Part B Web. Pointer lines added to SLICE_005 §5,
SLICE_006b §4, SLICE_016 §7; D-057 carries the O-013 runbook note. The user picked this rung from the queue. **D-057 accepted
(user, 2026-09-10):** `complete_task` executes at once with a receipt and
Undo, the first AGENTS §5.4 "low-risk and reversible" action;
`create_task` proposes then confirms in the SLICE_006b shape; D-053
authorization unchanged; the mechanism is a planning default (no
`crm-operator -> crm-app` edge, seam methods, `operator_proposal.tool`
widened) that the planner confirms. Coordinator next: audit at the
Part A checkpoint, release Part B, then review and test analysis, the
once-only gates, the walkthrough, and the commit and merge gates.

Previously: **LATER BATCH (2026-09-10) — COMPLETE AND MERGED TO LOCAL MAIN** at
`753d685` (2026-09-10, with the user's approval; not pushed, not deployed).
Source `chore/later-batch-2026-09-10` at `bb58e35` (eight item commits,
three round-1 fix commits and the
[verification record](../tasks/LATER_BATCH_2026-09-10_VERIFICATION.md)).
One lane (Claude Sonnet 5), no migration, no wire change, 12 code files.
Review round 1 of two: reviewer READY WITH FIXES, tester no blocking
finding; three fixes applied (a real validator gap: ignorables around a
space passed as a task title; the seven note CHECK assertions to SQLSTATE
23514; a deterministic tie-break test); round 2 not needed. Final-tree
gates run once by the coordinator on `9671d65`: `check` green (782 Rust,
751 Web, 11 worker, 13 s), `check-db` 720 of 720 first run (261 s). Item 8
trend: peak RSS of the 20 MiB intake test 331 MB → 289 MB. Runtime updated:
the dev API stopped by exact PID (41716) and relaunched from `main` (pid
83913, binary 12:03, `/api/health` 200); the production web server
rebuilt and relaunched (preview pid 84325, `5173` 200). Worktree
`../crm-worktrees/later-2` and the branch deleted. LATER items carried in
the record (unreachable focus fallback without a ring, fractional-seconds
case, wildcard task-kind arm, two-entry preview fixture, a pre-existing
task-edit Vitest flake). **Pushed to `origin/main` at `ae9d154` on 2026-09-10 with the user's
approval.** No slice active.

Previously: **SLICE 017 (INBOUND MAIL SIZE CAP) — MERGED, PUSHED, DEV API UPDATED;
WORKER DEPLOY AND WALKTHROUGH PENDING (2026-09-09).** D-056: the relay's
threshold is Cloudflare's own 25 MiB inbound ceiling and the relay streams a
chunked base64 JSON body; the endpoint's body limit is a derived 34 MiB;
the frozen `{"recipient","raw"}` envelope is untouched;
`scripts/inbound-email` keeps the message and the bearer off every argv.
One lane (Sonnet 5), four commits, two review rounds (round 1 READY WITH
FIXES, six small items applied; round 2 a read-only confirmation), final-tree
gates once on `33f8284`: `check` green (778 Rust, 747 Web, 11 worker tests,
15 s), `check-db` 718 of 718 (266 s, no flake). Merged `--no-ff` at
`f06eba3` with the user's approval, pushed with the records; the dev API
restarted by exact PID (pid 41716, binary 21:34) and proven on the new limit
(3 MiB + bad bearer → 401, 35 MiB → 413); worktree and branch deleted.
Evidence: [SLICE_017_VERIFICATION.md](../tasks/SLICE_017_VERIFICATION.md).
**Walkthrough stopped at §6 step 2: the account is on the Workers Free
plan (user, 2026-09-09).** Per D-056 §3 the streaming relay is not deployed;
the production relay is still the pre-017 build and bounces above 1.4 MiB.
**Decided (user, 2026-09-09): upgrade the account to Workers Paid** (no
code change; the pass-through fallback stays recorded, not built). The
user upgraded and approved the deploy from the coordinator's machine:
`wrangler deploy` on 2026-09-09 23:25 local put version `63c168c5` at 100 %
(the secret and the Email Routing route survived). **The real sends (a
small message, then a 15–20 MB attachment, to a capture address and the
intake address) are deferred by the user**; the evidence goes into the
verification record when they happen (the dev API log and the Workers
dashboard keep the outcomes). Deployment of the application is not
authorized.

Previously: **SLICE 016 (TASKS) — COMPLETE: BOTH RUNGS MERGED, RUNTIME UPDATED, PUSHED,
CLEANED UP (2026-09-09, each with the user's approval).** `main` at the
016b merge `faa2878` plus records; pushed to `origin/main`. No migration in
016b; the dev API and `dev-web-prod` were restarted by exact PID and
relaunched from `main` (API on `127.0.0.1:3000`, health 200, the tasks
route answering; preview on `5173`; logs under `/private/tmp/claude-501/`).
Branch `slice-016b-today`, the worktree and `crm_slice016b_qa` deleted.
Today observed live on the dev runtime: the Tasks panel and the task reason
rendered for a task due today and Complete from the panel removed it (QA
record addendum). Evidence: [SLICE_016a_VERIFICATION.md](../tasks/SLICE_016a_VERIFICATION.md),
[SLICE_016b_VERIFICATION.md](../tasks/SLICE_016b_VERIFICATION.md).
Deployment is not authorized. No slice active.

Previously: **LATER BATCH (2026-09-08) — COMPLETE AND MERGED TO LOCAL MAIN** at `3ff6c5f`
(with the user's approval; not pushed, not deployed). Source
`chore/later-batch-2026-09-08` at `5fe4231` (seven commits incl. the
round-1 fixes `28a3bf1` and the
[verification record](../tasks/LATER_BATCH_2026-09-08_VERIFICATION.md)).
Final-tree gates run once by the coordinator: `sqlx-prepare` clean, `check`
green (757 Rust, 656 Web), `check-db` 622 of 622 first run. Review round 1
of two: reviewer READY WITH FIXES, tester one blocking regression (fixed),
all applied; round 2 not needed. Runtime updated with approval: `crm_dev`
migrated (`20260911000001` applied; triggers only, the API needed no
restart), the production web server rebuilt and relaunched from `main`
(preview pid 28850, 20:09), branch and worktree `../crm-worktrees/later-1`
deleted. Open flakes carried forward: the `db_calls` correction-ordering
test (strict assertion kept, unreproduced in 21 runs) and the
`db_today_system_feed_evaluation` stage-clause test (one load failure, passes
isolated). `main` was pushed to `origin/main` on 2026-09-08 at `88df7f5`
with the user's approval; deployment is not authorized.

Previously: **LATER BATCH (2026-09-08) — LANE IN IMPLEMENTATION.** The user chose the
"worth a small batch soon" group from the LATER lists: `inquiry` append-only
triggers (one migration), the `db_calls` timing flake, splitting the three
largest test files, field-only `onSuccess` writes in the optimistic
mutations, and `isMutating` guards on the settle-invalidate and the realtime
invalidation. Brief: [LATER_BATCH_2026-09-08.md](../tasks/LATER_BATCH_2026-09-08.md);
no spec (recorded LATER items; no contract or behaviour decision). One lane
(Claude Sonnet 5) in `../crm-worktrees/later-1` on
`chore/later-batch-2026-09-08`. Progress: item 2 done (`2482b4c`, the
`db_calls` flake was a test asserting strict order on `recorded_at` alone
while the query already tie-breaks on `id`; test-only fix, gates green
757 / 650 / 617 of 617). Item 1 hit its checkpoint: a plain append-only
trigger blocks the `person` → `inquiry` cascade that D-015 §5 erasure relies
on; coordinator decision: a cascade-aware `reject_direct_mutation()`
(updates and direct deletes rejected, cascaded deletes allowed via
`pg_trigger_depth()`), recorded in the brief. **All five items complete**
(`c18d7e6`/`1b488a9` item 1 with five tests; `b4a4a04` item 3: the three
files split into nine, fixtures moved to `tests/common/`, test-name sets
identical per group and 729 = 729 overall; `cbdcbc7` items 4 and 5). The
lane caught two of its own bugs before landing: the trigger must check
`pg_trigger_depth() > 1` (a direct statement's own trigger already runs at
depth 1) and the settle guard must check `isMutating > 1` (TanStack v5 runs
`onSettled` before the success state change, so the calling mutation counts
itself). Lane final gates: `check` green (757 Rust, 653 Vitest); `check-db`
622 of 622 on the second run after one load-dependent failure in a moved
`db_today_system_feed_evaluation` test that passes 6 of 6 in isolation
(pre-existing shape, not in this batch's scope; classified in review).
Coordinator audit passed (22 files, all under tests, migrations and
`web/src`). **Review round 1 (of two) complete** on `cbdcbc7`: reviewer READY
WITH FIXES, tester one BLOCKING regression — two sibling mutations for the
same Person settling in the same tick both skip the invalidation (each sees
the other pending), so nothing refetches; fix: decide after the mutation's
own state flips (deferred check, invalidate when the count is zero, and
release the realtime hold by refetching active stale queries under the
Organization prefix from all four mutations). Also: the item 2 "fix" was
reverted to the spec-backed strict assertion (the flake was never reproduced
and remains open); `TRUNCATE inquiry` gains a test; the cascade guard also
requires the parent Person to be gone; a duplicated fixture removed.
Consolidated fix round dispatched. LATER: an in-trigger `DELETE FROM
inquiry` would pass the depth check (none exists; the header forbids one);
same-field rapid pairs show the earlier response until the later settles;
the hold is Organization-blind (one org active at a time);
`useLogContactMutation` still writes the whole `person` and has no
mutation key; the `db_today_system_feed_evaluation` load flake (moved test,
unchanged body, not reproduced in 5 isolated runs plus the trio).

Previously: **Slice 014 — COMPLETE AND MERGED TO LOCAL MAIN** at `ac270fb` (2026-09-08,
with the user's approval; not pushed, not deployed). Source
`slice-014-perceived-latency` at `3495f71` (six commits incl. the round-1
fixes `fa9bcab`, the walkthrough archive and the
[verification record](../tasks/SLICE_014_VERIFICATION.md)). Final gate run
once by the coordinator: `check` green (757 Rust, 650 Web); no `check-db`
needed (Web-only). Review round 1 of two: reviewer READY WITH FIXES, tester
no blocking finding, all fixes applied; round 2 not needed. Measured over
the tunnel in production mode: cold login DOMContentLoaded 0.22–0.58 s (was
1.3–3.1 s), warm Today data 0.17–1.36 s (was 2.2–2.5 s), 3 scripts before
DOMContentLoaded (was 50–60); walkthrough 9 of 9. **The tunnel now serves the
merged production build from the main checkout** (`./scripts/dev-web-prod`,
preview pid 62908, started 17:36); the slice-tree preview was stopped by
exact PID; the branch and worktree `../crm-worktrees/014` were deleted with
approval. Standing note: after a merge or pull, re-run
`./scripts/dev-web-prod` and reload; `./scripts/dev-web` is for HMR on
loopback when 5173 is free. `main` was pushed to `origin/main` on 2026-09-08
at `57dbde1` with the user's approval (carrying Slice 014 and its records);
deployment is not authorized.

Previously: **Slice 014 — APPROVED 2026-09-08, LANE IN IMPLEMENTATION (Web-only).**
[SLICE_014.md](../specs/SLICE_014.md) and its
[brief](../tasks/SLICE_014_IMPL.md) were drafted from the planner's analysis
(which corrected the investigation on one point: through the tunnel the
browser calls `api.tarams.org` directly, so production serving needs only
Vite's preview server on 5173 and no tunnel change; and found three of the
four 2026-08-29 FilterBar gaps already fixed by the 2026-09-06 pass),
independently reviewed READY WITH CORRECTIONS (the request-count gate
restated as before-DOMContentLoaded so the Today preload does not defeat it;
the caching claim corrected to what `vite preview` actually emits; tag
mutations write `data.tags` not `data.person`; a held-response Vitest for
the single-request claim; a shared `preloadTodayView` export), all applied,
and approved with both workflow confirmations: the tunnel is normally served
from the production bundle (`scripts/dev-web-prod`; dev mode stays for
loopback), and the running dev server (`pnpm run dev` pid 24542, Vite pid
24563, up since 2026-09-06) is stopped by exact PID at verification so the
production server can take port 5173. Four parts in order: A production
serving, B optimistic stage/assignment/tag mutations, C Today chunk preload
and data prefetch plus hover prefetch of Person detail, D FilterBar residue.
One lane (Claude Sonnet 5) in `../crm-worktrees/014` on
`slice-014-perceived-latency`; checkpoint after part A. Coordinator runs the
tunnel probe and walkthrough at the end.

Progress (2026-09-08): **part A complete** (`5301b5b`): `preview: { port }`
in `vite.config.ts` (Vite 8.2.1's preview resolver verified in source to
inherit host, strictPort, allowedHosts and proxy from `server`),
`scripts/dev-web-prod`, README and `.env.example` notes. Scratch-port
measurement of the production bundle: **3 script requests before
DOMContentLoaded** (was 50–60), DCL 50 ms on loopback, the login page renders
and `/api/me` reaches the API through the preview proxy. Web gate green (614
Vitest). Coordinator audit passed; parts B–D released. **Parts B–D complete**
(`1401008` optimistic stage/assignment/tag mutations with snapshot rollback
and settle-invalidate; `3e4fd5d` `preload.ts` shared by the router and
LoginView, `prefetchTodayData` from the guard, `onRowIntent` hover/focus
prefetch with a 150 ms dwell; `9f1ce13` FilterBar selected triggers, chip
chevron, Clear all only with a non-locked clause). Two real problems found
and fixed by the lane: a cached stage object leaking `position` into the
optimistic row, and the new prefetch reaching the live dev API from
`router.test.ts` until mocked. Web gate green, 643 Vitest. Coordinator audit
passed (15 files, all under `web/`). **Review round 1 (of two) complete** on
`9f1ce13`: reviewer READY WITH FIXES, tester no blocking finding; prefix
isolation, clean reference shapes, guard placement, the shared Today chunk
and the network-leak fix all verified. Consolidated fix round dispatched:
tag mutations invalidate the person key on any error; a transparent border
on the selected FilterBar trigger; one shared `fetchPerson`; test hardenings
(a vacuous other-row assertion, the assignment row write and rollback, a
real racing-invalidation test with a held stale GET, server tag order on
success, the 149/150 ms boundary and unmount, client mocks in the new test
files). LATER: field-only `onSuccess` writes for rapid mutation pairs,
insertion-order tag sorting vs collation, pointerleave timer clearing, the
unrelated-person invalidation transient. **Fix round complete** (`fa9bcab`:
all nine items; the stricter integration test caught a spurious extra
`GET /me` from a missing test-client `staleTime`; the racing-invalidation
test now holds a stale GET open past the mutation and passes
deterministically). Coordinator audit passed (19 files against `main`, all
under `web/` plus the four part-A files); **final gate run once on
`fa9bcab`: `check` green, 757 Rust / 650 Vitest.** The pre-approved tunnel
switch was performed on 2026-09-08 (pre-approved): the dev server (pids
24542/24563, up since 2026-09-06) stopped by exact PID; `scripts/dev-web-prod`
from the slice tree serves the production build on 5173 (preview pid 55779);
through the tunnel `/` answers `cache-control: no-cache` (`cf-cache-status:
DYNAMIC`) and hashed assets `max-age=14400`. **Probe re-run in production
mode (3 runs):** cold `/login` DCL 0.22–0.58 s (one jittery run 2.0 s) from
1.3–3.1 s; warm Today data 0.17–1.36 s from 2.2–2.5 s; login → Today
0.97–1.6 s of which the login POST is 0.76–1.1 s; filter change still
flash-free; 3 scripts before DOMContentLoaded (gate ≤ 10). **Walkthrough
9 of 9** in production mode over the tunnel (hover prefetch removes the
preview's Loading; optimistic stage/assignee hold through a 1.5 s delayed
response; tag apply/remove; locked chips; selected trigger "Stage · 2",
chevron, Clear all; popover 7 px under its chip when wrapped); archive
`docs/design/qa/slice-014-2026-09-08/`; the lane stalled on the walkthrough
script twice and the coordinator wrote and ran it. Verification record on
the branch: `docs/tasks/SLICE_014_VERIFICATION.md`. Next: the merge gate. **Standing note:** the tunnel is now served from the
production bundle; after a merge or pull, re-run `./scripts/dev-web-prod`
(from the main checkout once Slice 014 merges) and reload; use
`./scripts/dev-web` for HMR on loopback only when 5173 is free.

Previously: **PLANNING Slice 014 — perceived-latency chunk plus FilterBar UX polish
(Web-only).** The user chose the coordinator's suggestions 1 and 2 on
2026-09-08: `main` was pushed to `origin/main` at `32b36de` (carrying Slices
012 and 013, D-052 and the state records), and planning started for the held
perceived-latency chunk together with lane C (the four FilterBar UX gaps
recorded since 011a), so the FilterBar is touched once. Planner analysis
dispatched; a spec, one review round and one implementation gate follow.
Push and deployment beyond this are not authorized.

Previously: **Slices 012 and 013 — BOTH COMPLETE AND MERGED TO LOCAL MAIN.** Slice 013 merged at `9af47c1` (2026-09-08, with the user's
approval; not pushed, not deployed) from `slice-013-operator-filter` at
`151d38a` (ten commits after the rebase, incl. the
[verification record](../tasks/SLICE_013_VERIFICATION.md)). Final-tree gates
run once by the coordinator on the rebased tree: `sqlx-prepare` clean (no
new statements), `check` green (757 Rust, 614 Web), `check-db` 617 of 617
first run. Review round 1 of two: reviewer READY WITH FIXES, tester no
blocking finding, all fixes applied; round 2 not needed. No migration; the
old dev API (pid 12421) stopped by exact PID, the merged binary built and
`./scripts/dev-api` relaunched (pid 17478, binary of 13:25; health 200, the
Operator route answers 401 unauthenticated). Branch and worktree
`../crm-worktrees/013` deleted with approval. The Operator now has eight
tools. Lane C (FilterBar UX polish) remains held for the perceived-latency
chunk. `main` pushed 2026-09-08 (`32b36de`); deployment is not authorized.

**Slice 012 — COMPLETE AND MERGED TO LOCAL MAIN** at `26ddab7` (2026-09-08,
with the user's approval; not pushed, not deployed). Source
`slice-012-activity-columns` at `e32ffd7` (six commits incl. the round-1
fixes `77a51c8` and the [verification record](../tasks/SLICE_012_VERIFICATION.md)).
Final-tree gates run once by the coordinator under the shared lock:
`sqlx-prepare` clean, `check` green (718 Rust, 614 Web), `check-db` 603 of 603
first run. Review round 1 of two: reviewer READY WITH FIXES, tester no
blocking finding, all fixes applied; round 2 not needed. The shared
development runtime was updated with approval: `crm_dev` migrated
(`20260910000001` applied), the old API (pid 16112) stopped by exact PID, the
merged binary built and `./scripts/dev-api` relaunched. The branch and the
worktree `../crm-worktrees/012` were deleted. **Slice 013** rebased cleanly
onto `26ddab7` (`3d68a7c`; the three `tests/all.rs` registrations merged
without conflict); its round-1 fixes (`fe3321b`, `3d68a7c`: trait defaults
removed, alias dedup, empty-item drop, context-mismatch guard, six test
hardenings; lane gates 756 / 601 of 601) passed the coordinator audit; the
coordinator's once-only final-tree gates are running on the rebased tree.

Previously: **Slices 012 and 013 — APPROVED 2026-09-08, TWO PARALLEL LANES IN
IMPLEMENTATION.** After the ladder closed, the user chose to run the
"worth a small chunk soon" items in parallel worktrees ("Go"). Planner
analyses, then [SLICE_012.md](../specs/SLICE_012.md) (denormalized
last-activity columns on `person`; trigger-maintained per **D-052**; the
fourteen statements switch to the columns behind a frozen-text equivalence
gate; one migration; size M) and [SLICE_013.md](../specs/SLICE_013.md)
(Operator `filter_people` and `run_saved_list`, name-based, read-only,
D-046-faithful; size S), each with a brief, were drafted and independently
reviewed the same day: both READY WITH CORRECTIONS, all applied (012: create
the triggers before the backfill so no deploy-window row is lost; one index
not three; probe-gate placement; a testable backfill block. 013: a missed
closed code set in the count scheduler analogue, `get_today` drawer side
effect of `MAX_REFERENCES` 25, `validate_references` cut as redundant).
Lane A (012) in `../crm-worktrees/012` on `slice-012-activity-columns`; lane
B (013) in `../crm-worktrees/013` on `slice-013-operator-filter`; both from
`main` after the planning commit; one Claude Sonnet 5 writer each; Claude
Fable 5.1 coordinates. Ownership per the specs' §10; only `tests/all.rs` is
shared (alphabetical insertion, "keep both"). Merge order A then B. Lane C
(FilterBar UX polish) stays held for the perceived-latency chunk. Gate runs
across the two lanes are serialized by a directory lock because
`sqlx-prepare`/`check-db` use fixed throwaway database names.

Progress (2026-09-08): **lane A steps 1–3 complete** (`a503640`: migration in
the reviewed order, three triggers, marked backfill block, one NULLS FIRST
index, 11 invariant tests incl. real two-transaction concurrency, 3 schema
tests; `805699b`: the fourteen pre-switch statements frozen as a Rust module
under `tests/fixtures/statements_b45b04f/` and `db_statement_equivalence.rs`
green against the live text; lane gates `check` 717, `check-db` 602 of 602;
coordinator audit passed; one accepted implementation detail: `pub`
visibility on the Today source/feed statement functions so the equivalence
test can call them). Steps 4–5 (the read-side switch behind the equivalence
test, then performance evidence) released. **Lane B step 1 complete**
(`ab1657f`: input types, two trait methods with bridging defaults to be
removed in step 3, `FilterOutcome` views, both tool schemas and parsing with
`limit` defaulting to 10, regenerated snapshot, `kind_label` mirror test in
crm-api, fake backends; `check` 734 Rust / 614 Vitest; coordinator audit
passed). Steps 2–6 released with decisions: `RefBucket::Search` for both
tools; `validate()` failures after resolution are `invalid_arguments`, unknown
names are clarifications; names clipped and control-stripped at parse time.
**Lane A steps 4–5 complete** (`9f02ead` the read-side switch of all fourteen
statements with the equivalence test green throughout and the §8.6 pins
added; `df72926` the performance archive
`docs/design/perf/slice-012-2026-09-08/` with the harness committed behind
the `perf-harness` feature). Lane gates `check` 717, `check-db` 603 of 603
(three `db_calls` timing flakes under load, clean on re-run). Measured: seed
of 25k People with triggers 2.1 s; backfill 323 ms; paired regression all
seven rows within gate with payloads equal, `person_state` 353 → 22 ms,
`source_candidates` 46 → 2 ms, the `never` filter 22 → 5 ms; the gated
`waiting` probe runs once per gated candidate (20,333) with the index
descended 5,293 times. One fixture fix in `db_operator.rs` (a direct
`UPDATE inquiry SET received_at` backdate now keeps the column in step, the
same action the erasure runbook would take). Disclosed, not touched: an
011e-era `perf-harness` test binds 25 parameters to a 27-parameter
statement, broken at the branch point. Coordinator audit passed; review
round 1 (reviewer and tester) launched on `df72926`. **Lane B steps 2–6
complete** (`b0511e3` pure name resolver; `2a8279a` the adapter with the
request's `AuthContext` threaded into the backend constructor, bridging trait
defaults removed; `b815e04` dispatch, ledger names, `MAX_REFERENCES` 25,
declared span fields, prompt; `163b876` thirteen database tests;
`7da1b21` a real bug check-db caught: duplicate-named saved lists resolved
to the first match instead of a clarification, plus two fixture fixes).
Lane gates `sqlx-prepare` no new entries, `check` 751 Rust / 614 Vitest,
`check-db` 600 of 600. Two test files outside the boundary
(`db_today_source_operator.rs`, `db_today_source_settings.rs`) received
mechanical placeholder `AuthContext` fixtures because the constructor
signature changed; accepted. Coordinator audit passed; review round 1
(reviewer and tester) launched on `7da1b21`. **Review round 1 results:**
Slice 012 reviewer READY WITH FIXES and tester no blocking finding (byte-identity
of all fourteen switched statements verified against the base commit; fixes:
exercise the `last_inquiry`/`last_inbound` axes and the new `$5/$27` source
guard in the equivalence test, a cross-Organization backfill correlation
case, a blocking assertion in the concurrency test, a migration-order pin,
`ELSIF` in the correspondence trigger, an exact `db_operator.rs` fix-up, and
gate-2 plans re-captured under `plan_cache_mode = force_generic_plan`);
Slice 013 reviewer READY WITH FIXES (one required fix: the bridging trait
default bodies the lane reported removed are still present; coordinator
confirmed on the tree) with D-046, §5.2, telemetry and injection containment
verified; Slice 013 tester no blocking finding (fixes: deduplicate aliases
that resolve to one id instead of a strike — a revised coordinator decision;
drop empty name items; a fail-closed context-mismatch guard in
`run_saved_list`; cross-Organization same-name test with People on both
sides; admin-by-`list_id` test; a service unit for the counter reset; exact
span-field assertions incl. absent uuids; same-display-name members test).
Both fix rounds dispatched (lane A for 012, lane B for 013).
LATER recorded: `inquiry` lacks a `reject_mutation` trigger (migrator can
update it; `crm_app` cannot); every history insert now takes the Person row
lock (BEYOND_ENVELOPE; a future importer must insert in stable Person order);
the 011e-era perf-harness "25 vs 27 parameters" claim could not be reproduced
by the tester and awaits the lane's exact error or retraction.

Previous phase: **Slice 011e (tags) — COMPLETE. RUNG e2 MERGED TO LOCAL MAIN** at `b6dc49b`
(2026-09-07, with the user's approval; not pushed, not deployed). Source
`slice-011e-tag-clauses` at `1796e85` (seven commits: vocabulary `db2be0a`,
fourteen statements `2cecee3`, `invalid_tag` `23340d1`, Web `147ef64`,
performance `ad6f10a`, round-1 fixes `3d33ff7`, verification record
`1796e85`). Final-tree gates run once by the coordinator: `sqlx-prepare`
clean, `check` green (717 Rust, 614 Web tests), `check-db` 587 of 587 first
run. Review round 1 of two: reviewer READY WITH FIXES, tester no blocking
finding, all fixes applied; round 2 not needed. Performance (D-050): paired
regression unchanged with the clause absent; plan shape an index-only scan
on `person_tag_org_tag_person_idx` after the §4 binding amendment. Evidence
in the [verification record](../tasks/SLICE_011e_VERIFICATION.md) and
`docs/design/perf/slice-011e-2026-09-08/`. The branch and the worktree
`../crm-worktrees/011e-e2` were deleted with the user's approval; no 011e
branch remains and none existed on the remote. The shared development runtime
was updated the same evening (no migration in e2): the old API (pid 37778)
stopped by exact PID, the merged binary built and `./scripts/dev-api`
relaunched (pid 16112, binary of 22:06; health 200, the new filter kind
reaches authentication). **This completes the Slice 011 ladder** (011a, 011b,
011b-sort, 011c, 011d, 011e). `main` was pushed to `origin/main` on 2026-09-07 at
`aef151b` with the user's approval (the push carried 011e e1/e2, D-051, the
perceived-latency investigation and the state records); deployment is not
authorized.

Implementation history: the user passed the e2 Phase 6 gate on 2026-09-07;
one lane (Claude Sonnet 5) followed brief steps 1–5 with a coordinator audit
after step 2 and after step 5; Claude Fable 5.1 coordinated, took the
predicate-binding decision in review round 1, and amended spec §4/§8/§9.14
by pointer.

Previous rung: **RUNG e1 COMPLETE AND MERGED TO LOCAL MAIN** at
`51331e9` (2026-09-07, with the user's approval; not pushed, not deployed).
Source `slice-011e-tags` at `4af2e13` (five commits: backend `502f418`, Web
`e7530d2`, walkthrough `a563cce`, round-1 fixes `8d7c748`, verification
record `4af2e13`). The branch and the worktree `../crm-worktrees/011e-e1`
were deleted on 2026-09-07 with the user's approval; no 011e branch remains
locally and none ever existed on the remote. Final-tree gates run
once by the coordinator: `sqlx-prepare` clean, `check` green (704 Rust, 598
Web tests), `check-db` 566 of 566 first run. Review round 1 of two: reviewer
READY WITH FIXES, tester no blocking finding, all fixes applied; round 2 not
needed. Walkthrough 11 of 11. Full evidence in the
[verification record](../tasks/SLICE_011e_VERIFICATION.md). The shared
development runtime was updated the same evening with the user's approval:
`crm_dev` migrated (`20260909000001` applied), the old API (pid 89707,
binary of 15:51) stopped by exact PID, the merged binary built and
`./scripts/dev-api` relaunched. **Next rung: e2** (the `tags`/`not_tags`
clauses across the fourteen statements) per the brief; its Phase 6 gate is
the next approval. Specification approved and committed as `97b889f`.

Implementation history: the user passed the e1 Phase 6 gate on 2026-09-07
("proceed in a worktree"); one lane, one writer (Claude Sonnet 5, `implement`
profile) followed brief steps 1–6 with a coordinator audit after the backend
half and after each later commit; Claude Fable 5.1 coordinated.

Previous phase: **Slice 011d (tweakable built-in Today rules) — COMPLETE AND MERGED TO
LOCAL MAIN** at `b8b53e2` (2026-09-07, with the user's approval; not pushed,
not deployed). The two lane branches, all three worktrees and, on 2026-09-07 at the
user's request, the merged integration branch
`slice-011d-today-system-feeds` (was `77a8963`) were deleted; no 011d
branch remains locally or on the remote. Verification summary: 144 files against the previous
`main`. Final-tree gates run once by the coordinator:
`sqlx-prepare` clean, `check` green (699 Rust, 572 Web tests), `check-db`
541 of 541 on the second run after a pre-existing `db_calls` timing flake
that also fails on `main`. Review round 2: READY. Full evidence in the
[verification record](../tasks/SLICE_011d_VERIFICATION.md). Seven production
defects were found and fixed before merge (listed there). Implementation
history follows.
The user said "start 011d" on 2026-09-07. Integration branch
`slice-011d-today-system-feeds` from `main` at `66b44ff`; Lane B (Claude
Sonnet 5) in `../crm-worktrees/011d-lane-b` on `slice-011d-lane-b` doing brief
steps 1–3 (vocabulary, persistence, feed path behind the `Legacy | Feeds`
seam plus the equivalence suite), then stopping for the coordinator; Lane W
(Claude Sonnet 5) in `../crm-worktrees/011d-lane-w` on `slice-011d-lane-w`
doing steps 1–4 (types, chips, Today rules page, Today markers) with Vitest.
Claude Fable 5.1 coordinates.

Progress so far (2026-09-07):

- **Lane W steps 1–4 complete** on `slice-011d-lane-w` (`ab5f7b3`, `ef31a9b`):
  types and hooks mirroring spec §6, three boolean chips plus a locked-clause
  mode, the `/manage/today-feeds` page with preview/revert/typed-off
  confirmation and 409 reload, the Today Rules section and notices. Web gate
  green on the final lane tree (lint, typecheck, 560 Vitest tests, build).
  Coordinator audited the 20 changed files: all under `web/`, matching the
  report. Step 5 (browser walkthrough) waits for Lane B's routes. Two
  ten-minute agent stalls occurred; work was checkpointed and resumed.
- **Lane B steps 1–3 complete** on `slice-011d-lane-b` (`99e8d99`,
  `bd631c2`, `0280e1f`): the three clause kinds across all eleven statements
  with regenerated SQLx metadata; migration `20260908000001` (feed table,
  `today_feed_changed` fact, backfill) and org seeding; the `Legacy | Feeds`
  provider seam with `person_state.sql`, `call_membership.sql`,
  `call_only.sql`, and `system_feed_issues` on every `TodaySources` site;
  `db_today_feed_equivalence.rs` (5 tests, byte-identical `TodayList` JSON
  across a rich mixed fixture, two tenants, deactivated caller, a list
  source enabled, call feed disabled/enabled) plus every existing Today
  suite passing under `Feeds` by default. Gates on the lane tree: `check`
  (699 tests), `check-db` (471 of 471). Coordinator audited the 56 changed
  files: all under `backend/`. A machine-sleep interruption was resumed
  without loss. The `Legacy` provider stays until step 6.
- **Lane B corrections and step 4 complete** (`5ffeb2e`, `a4dc96b`,
  `c0f9bd6`): the three corrections verified by the coordinator (no issue for
  a disabled feed; per-axis parity tests in `db_people_filter.rs`; call-feed
  connection recovery with three failure-injection tests in
  `db_today_system_feed_call_failures.rs`); commands, preview, the six routes
  in `routes/today_feeds.rs`, the Operator field, telemetry, and
  `db_today_system_feed_commands.rs` (15 tests). Lane B found and fixed a
  real preview bug (read-only set before the `FOR SHARE` membership lock,
  which PostgreSQL rejects). Gates on the lane tree: `check` green,
  `check-db` 493 of 493.
- **Coordinator decision (2026-09-07):** the call feed's two statements
  bound no filter matrix, so extra clauses on that feed were ignored, which
  contradicts spec §1 rule 4. Decision: extend both call statements with the
  full matrix (spec §5 "feed C matrix params"), not restrict validation.
  Assigned to Lane B with the remaining §9 coverage gaps (deleted-stage and
  unsupported-JSON fallback evaluation tests, preview timeout 503, Operator
  parity under customized/disabled/fallback feeds) and then step 5
  performance evidence paired against `Legacy`.

- **Lane B round 3 complete** (`401c18e`, `549243a`, `145335a`, `97cdbec`):
  call feed bound to the full filter matrix; fallback, preview-timeout and
  Operator-parity coverage; step 5 evidence at
  `docs/design/perf/slice-011d-2026-09-07/` (paired Legacy vs Feeds serial p95
  204 ms vs 177 ms, payload-identical apart from `system_feed_issues`; the
  011c matrix all complete; person-state EXPLAIN a nested-loop anti join with
  index use with and without the merge-join toggle). Gates on the lane tree:
  `check` green, `check-db` 504 of 504.
- **Lanes merged** into `slice-011d-today-system-feeds` at `3496d71` via the
  third worktree `../crm-worktrees/011d-integration` (113 files, no
  conflicts).
- **D-050 applied** (committed on main as `1d951a6` by a peer session; spec
  §8 pointer `ae449ad`): step 5 gates only on the paired regression and the
  person-state plan shape, both already met; the 1/10/20 matrix and pool wait
  are trend data; the merge-join toggle question is closed as keep both
  settings; at most two review-then-fix rounds.
- **Review round 1 (of two) complete** on the merged tree. Reviewer:
  equivalence gate CONFIRMED by SQL analysis and both suites; READY WITH
  FIXES. One BLOCKING defect: `person_state.sql` projects a nullable boolean
  into a non-null decode, so a customized feed with `unassigned` plus one
  unassigned replied Person would 503 the whole Organization's Today. Plus:
  call-feed statements use `now()` instead of the bound clock; Update
  validates references before the revision check (422 before 409); no
  HTTP-level route tests; five §9.2 cases missing. Tester: the same clock and
  precedence defects, the 199/200/201 × call-only cap case, a mislabeled
  failure test, a weak revert-fact test, a vacuous telemetry test, and
  several cheap boundary/idempotency tests; Operator parity had no gap.
  Beyond-envelope items (concurrency 20, pool wait) recorded as trend only.
- **Lane B fix round assigned** with every in-envelope finding, the
  call-statement EXPLAINs D-050 asks for, and then step 6: delete the
  `Legacy` provider and freeze its SQL under `tests/fixtures/today_f51bff8/`.
- **Lane W step 5 assigned**: browser walkthrough on the merged tree in a
  scratch QA runtime (011c pattern, scratch database, the user's dev
  processes untouched). Two Web items from the tester wait for its report:
  a session-identity fence on the preview dialog, and the 409 flow's draft
  handling (coordinator choice: keep the reload but say so explicitly in the
  notice, and keep the editor open if the refetch fails).

Planning history follows. On 2026-09-06 the user asked to look at 011d. The read-only planner
analysed the rung against the code and found that the ladder's pre-declared
d1/d2 seam does not exist (the three Today arms are one statement with one
precedence rule and one cap). Offered three cuts, the user chose **one L rung
with parallel backend and web lanes** (D-049). Claude Fable 5.1 then wrote
[SLICE_011d.md](../specs/SLICE_011d.md), its
[companion](../specs/SLICE_011d_EXPLAINED.md) and the two-lane
[brief](../tasks/SLICE_011d_IMPL.md); the independent reviewer returned READY
WITH CORRECTIONS with no blocking decision, and all eight corrections were
applied (migration version, rule 7 on the call feed, the call-only sentinel
gating, the invalid-definition example, 403-before-400 precedence, `Feed.filter`
semantics under `filter_error`, a `Legacy | Feeds` provider seam for the
equivalence and paired-perf gates, `unavailable` precedence over `partial`).
The user approved the specification with its seven §1 safe defaults on
2026-09-07, held implementation briefly, then started it the same day.

Previous phase, for context:

**Slice 011c (saved lists feed Today) — COMPLETE, MERGED AND PUSHED.**
The user approved the §8 planner amendment, accepted the Phase B pairing
limitation and approved the local commit and merge on 2026-09-06 (late
evening). Both 011b and 011c were then pushed to `origin/main` at the user's
request; deployment was not authorized.

How it got here: the Codex lanes (Astra / Terra, `xhigh`) specified, built,
reviewed and browser-walked the slice, then ran out of usage on the evening of
2026-09-06 with the tree uncommitted and three items open (Phase B HTTP
performance, final-tree gates, independent acceptance review). The user asked
Claude to finish. Claude Fable 5.1 took the coordinator, sole-gate-runner and
acceptance-reviewer roles; Claude Sonnet 5 lanes made the bounded corrections;
the lane ledger is in the [implementation brief](../tasks/SLICE_011c_IMPL.md#takeover-on-2026-09-06-evening-codex-usage-exhausted)
and every result in the [verification record](../tasks/SLICE_011c_VERIFICATION.md).

What the takeover found and did:

- The tree as left passed `check-db` (422 of 422) and the Web gate but failed
  clippy on three test-file lints; a telemetry test was never registered; the
  Phase B harness did not compile. All fixed; the harness lints clean.
- Acceptance review found no blocking backend or Operator defect. The Web
  session-privacy review found a P1 availability lockout (an auth attempt that
  never settles left every tab paused for ever with no exit) plus three small
  router/copy defects; all fixed with tests (C011C-I17–I20) and the lockout
  recovery verified live. One pre-existing telephony gap is a residual (R1).
- **Phase B exposed a pre-existing Today planner hazard:** once autovacuum
  fills the visibility map, PostgreSQL 18 replans the built-in query's
  per-Person effective-contact probe into a Merge Anti Join that scans the
  whole corrections index per Person (~2.2 s instead of ~0.2 s for a 30k-Person
  book; the frozen original query shows 2.6 s). Run 1 failed on it. A
  transaction-local `SET LOCAL enable_mergejoin = off` beside the approved JIT
  setting pins the fast plan with no result change (frozen-fixture parity
  tests pass) and no effect on the source statements; run 2 with it passed
  every final-arm criterion (522/522 complete, all p95 caps met, five-source
  concurrency-20 p95 2,710 ms against 4,500, pool headroom ≥ 414 ms). It is
  recorded in [spec §8](../specs/SLICE_011c.md) as the fourth planning change,
  approved by the user after the evidence was in. Evidence, both runs retained:
  [Phase B archive](../design/perf/slice-011c-http-2026-09-06/README.md).
- Final-tree gates were run once by the coordinator after run 2 and passed
  (`sqlx-prepare`; `check` with 675 Rust and 446 Web tests; `check-db` 423 of
  423); see the verification record for the actual results.

QA setup incident (Codex phase): a bootstrap command used the shared migration
URL and rewrote the existing development owner's local credential hash and
timestamp. The previous password's equivalence is unknown and its hash cannot
be restored. Exact effects are recorded in
[the verification record](../tasks/SLICE_011c_VERIFICATION.md#shared-development-credential-incident).
Do not describe shared development data as untouched for this slice. The QA
runtime and its generated databases were cleaned up at the takeover's end.

The user approved **five Today sources per agent** and **available work with
an explicit notice when one source fails** (D-047). 011b-sort remains separately
queued; it is not a functional prerequisite for 011c. The approved specification
preserves built-in Today work, private-list visibility and deterministic order,
and addresses the measured cost of evaluating filters against a large history.

## Historical checkpoint: Slice 011b-sort

Started 2026-09-06 (late evening) at the user's request. The planner's
recommendation is reconciled into a draft specification
[SLICE_011b_SORT.md](../specs/SLICE_011b_SORT.md) and brief
[SLICE_011b_SORT_IMPL.md](../tasks/SLICE_011b_SORT_IMPL.md); independent
review returned READY-WITH-FIXES and the eight corrections are applied. The
user took the one genuine decision, **D-048**: sort is part of the list
definition, with clickable headers plus an "Added" column as the accepted
control. The user approved implementation the same evening. Branch
`slice-011b-sort` from `main` at `31c9980`: two Claude Sonnet 5 lanes built the
backend and Web halves in parallel, the coordinator passed the 100k
performance gate (sorted p95 20–132 ms against the same-run 343.6 ms
four-clause baseline; custom plans retained; no lever needed), independent
review and adversarial analysis found no P1/P2 defect and their test and
hardening items were applied by two fix lanes, and the final gates passed
once on the final tree (Rust 689 + 5 doctests, Web 518, database 459 of 459).
Full evidence: [SLICE_011b_SORT_VERIFICATION.md](../tasks/SLICE_011b_SORT_VERIFICATION.md).
Implementation commit `bd23f42`, merged to `main` as `d52a0ad` and pushed to
`origin/main` on 2026-09-06 at the user's request; the slice branch was deleted
locally and never existed on the remote. Deployment was not authorized.

## Historical checkpoint: earlier slices

**Slice 011b (saved lists) — COMPLETE AND MERGED TO LOCAL MAIN.**
The user authorized starting the slice with **Astra / ultra** for
specification, coordination, and independent review, and **Terra / ultra**
for implementation, tests, and fixes. This supersedes the older
Sonnet/Fable assignment for this slice. The spec and implementation brief
were drafted against `1635fc4`, which includes the 011a filter UX
fixes (`0117b87`) and D-045 workspace/Person-preview changes. Independent Astra/ultra specification review is **READY** after corrections.
The user approved the slice after reading the plain-language companion;
implementation, tests and fixes are authorized.

The implementation now passes the full repository and database gates, the
synthetic browser walkthrough and independent source/performance review.
Personal/shared lists, counts, copy/edit/delete flows, privacy and recovery are
implemented in `2af023c` and merged to local `main`. The user approved this
commit and merge on 2026-09-06; no push or deployment was performed. See
[verification evidence](../tasks/SLICE_011b_VERIFICATION.md) for actual results
and limits, including the 50k-Person query plans.

Official FUB guidance was rechecked on 2026-09-06: creating lists from
People filters, explicit save/update, admin management of shared lists,
independent duplication, and deletion of only the definition all remain
supported patterns. D-046, not an inferred FUB policy, is authoritative
for personal-list privacy and the separate limits.

Decisions taken this phase (user, 2026-08-29):

- **Per-list sort stays OUT of 011b** and becomes its own small
  follow-up rung immediately after 011b (v1 restricted to non-derived
  columns). Ladder amendment recorded at 011b spec approval as **011b-sort**.
  Rationale recorded: variable ORDER BY vs the fixed-matrix
  static-SQL discipline, and sort determines WHICH 500 rows survive
  truncation on >500-match lists.

Decisions accepted at restart (user, 2026-09-06; **D-046**):

- Personal list names and criteria are creator-only, including from admins.
  Organization-wide Person visibility is unchanged.
- Shared lists ≤200/Organization plus personal lists ≤50/creator/Organization;
  no combined cap. These count definitions, not People matching a list.

Also 2026-08-29: a three-agent docs-freshness audit ran over the whole
docs tree; the spec supersession-pointer chain verified fully intact
(zero missing pointers). All findings were fixed with per-item user
approval: README rewritten to current state (feature summary, real
directory list, gate-speedup check steps, Email intake section, 4 new
env-table rows, runtime-neutral Docker wording); D-013 amended to
bless .env.example's non-credential defaults; O-005 deduped and O-005/
O-007 marked resolved, O-014 annotated with shipped status, D-023 §4
supersession note added, accepted-decisions-continue-below pointer
added; thesis §8 (D-043) and §16 (achieved) annotated; ARCHITECTURE_
BASELINE gained an "Amendments since baseline" section + the contracts/
correction; ZITADEL dev-vs-prod parentheticals added (AGENTS §3/§4.2,
baseline); status headers fixed on SLICE_007_LADDER / SLICE_011_LADDER /
SLICE_006c_PLAN / type-safety-hardening; orphan CRM_ENVIRONMENT deleted
from .env.example. Uncommitted, awaiting the commit gate.

Also this session (2026-08-29): the user's "011a filters don't work"
report was root-caused to the KNOWN orphaned-dev-api hazard — the
running crm-api predated the 011a merge and silently ignored
`?filter=`. Killed by PID, relaunched via ./scripts/dev-api, filter
path verified live (garbage filter 400s; assigned_to narrows
correctly). 011a FilterBar UX intuitiveness gaps noted for a later
polish pass: draft chips look active while filtering nothing,
detached editor panel, undiscoverable chip-click-to-edit, no
clear-all.

## Earlier accepted-decision summary

D-061 (2026-09-11) — 010b captures People, users, stages, custom fields, notes and
tasks first, explicitly tracking other data as remaining snapshot work. This
accepts sequencing, not reduced cutover fidelity or implementation contracts.
D-060's latest follow-up authorizes the completed 010a shared-development
release and preserves the user's deferred live-account validation.

D-060 (2026-09-10) — API-first bounded FUB assessment with encrypted saved
credentials; implementation and synthetic verification complete. Its
2026-09-11 follow-up authorizes source cleanup, commit, merge and push.
D-059 establishes a new, empty destination Organization first; later import,
recovery and cutover policies remain open. D-058 delivered custom fields and
019b filtering.

D-056 (2026-09-09) — the inbound mail size cap is Cloudflare's 25 MiB
inbound ceiling (endpoint 34 MiB); the relay streams; the Workers plan is
a verified precondition; the raw pass-through body is the recorded
fallback, not adopted. Resolves O-015 question 1.
D-055 (2026-09-09) — the development telephony host is an EC2 `c6i.xlarge`
with a dormant LiveKit Egress; production hosting is still decided at the
deployment slice.
D-054 (2026-09-09) — due tasks reach Today through a fixed built-in axis
(a recorded temporary exception to D-043) plus a task panel.
D-053 (2026-09-08) — notes ship as plaintext erasable CRUD; O-012 amended.
D-052 (2026-09-08) — trigger-maintained derived columns are a read-model
mechanism. D-051 (tags), D-050 (operating envelope, two review rounds,
relative-only performance gates), D-049–D-043 (the 011 ladder), D-045/D-044
(design and identity) stand as recorded in the log.

## Slice ledger

Source through 010f1 is merged to main and pushed. Live/deployment residuals
remain in Current state and the per-slice verification records; source
publication does not close them.

| Slice | What | Merge |
|---|---|---|
| 000 | Foundation (workspace, health/ready, compose, scripts) | `e5182d1` |
| 001 | Identity/sessions (Argon2id, HMAC tokens, role split) | `587a087` |
| — | Tunnel + CORS (app./api.tarams.org; later D-024/D-025) | `3b6df76` |
| 002 | Intake + People/history + web stack (D-017) | 2026-08-21 |
| 003 | Realtime (Centrifugo, D-023) + Today | 2026-08-22 |
| 004 | Administration (platform admin, invitations, D-026/27) | 2026-08-22 |
| 005 | Read-only AI Operator (crm-operator, 5 tools, D-028/29) | 2026-08-22 |
| 006 | Calling (LiveKit/Telnyx) | `332e78a` |
| 006a | crm-app extraction | `a17aed3` |
| 006b | Operator start_call (propose→confirm, D-034) | `3f36d25` |
| 006c | Call outcome (D-032/D-033, low tier) | `58ecad8` |
| 007a | Intake address | `81af77f` |
| 007b | Inbound email endpoint | `4b3462a` |
| 007c | System actor + unattended routing (D-035) | `fe0b99b` |
| 007d | First pinned email format (D-036) | `a75b9a8` |
| 007e | Unresolved workbench (D-037) | 2026-08-25 |
| 007f | LLM extraction via Groq (D-038) | 2026-08-25 |
| 007g | Real receiving: Cloudflare Email Routing + worker (D-039) | `9604f76` |
| 007h1 | Forwarded-wrapper unwrap, Gmail inline (D-040) | `105f730` |
| — | Type-safety hardening ladder, 8/8 chunks (S1…S2) | `069f55a` |
| 008 | Intake routing modes / round-robin (D-041) | `defdab1` |
| 009 | Correspondence capture v1 (D-042; largest slice, 78 files) | `807d7c2` |
| 011a | Filter vocabulary + ad-hoc People filtering (D-043) | `4aee12d` |
| 011b | Personal and shared saved People lists (D-046) | `9d62e86` (implementation `2af023c`) |
| 011c | Saved lists feed Today (D-047; §8 planner amendment approved) | `929b6ab` (implementation `6117b4a`) |
| 011b-sort | Per-list sorting for saved People lists (D-048) | `d52a0ad` (implementation `bd23f42`) |
| 011d | Tweakable built-in Today rules: system feeds, three derived clauses, admin surface, change fact (D-049, D-050) | `b8b53e2` (integration `77a8963`), pushed 2026-09-07 |
| 011e-e1 | Tags model, commands, six routes, Person page and Tags page, Operator field (D-051) | `51331e9` (branch head `4af2e13`), pushed |
| 011e-e2 | `tags`/`not_tags` clauses across the fourteen statements, `invalid_tag` paths, FilterBar chips, performance evidence | `b6dc49b` (branch head `1796e85`), pushed 2026-09-07 |
| 012 | Denormalized last-activity columns on Person, trigger-maintained (D-052); fourteen statements read the columns; equivalence gate; perf archive | `26ddab7` (branch head `e32ffd7`), pushed |
| 013 | Operator `filter_people` and `run_saved_list` read-only tools, name-based, D-046-faithful, `MAX_REFERENCES` 25 | `9af47c1` (branch head `151d38a`), pushed 2026-09-08 |
| 014 | Production bundle through the tunnel (`dev-web-prod`), optimistic stage/assignment/tag mutations, Today chunk preload and data prefetch, hover prefetch of Person detail, FilterBar residue | `ac270fb` (branch head `3495f71`), pushed 2026-09-08 |
| — | LATER batch: `inquiry` append-only triggers (cascade-aware), three largest test files split into nine, field-only success writes and `isMutating` guards on the Person mutations | `3ff6c5f` (branch head `5fe4231`), pushed 2026-09-08 |
| 015 | Notes: `note` table (tombstone delete, import-ready), three commands and routes, `note` timeline kind, `note_changed`, Operator `PersonDetail.notes` (untrusted, history filtered), Person page composer with inline edit/delete (D-053) | `fd5a184` (branch head `00e2e67`), pushed 2026-09-09 |
| 016a | Tasks model: `task` table (tombstone, import-ready, Today index), six commands and routes, `tasks[]` on the detail, `task_completed` timeline kind, `task_changed`, Operator `PersonDetail.tasks` (untrusted, history filtered), Person page Tasks card (D-054) | `f106afc` (branch head `1d6c3bf`), pushed 2026-09-09 |
| 016b | Tasks on Today: the fixed built-in task axis (`task_due`/`task_overdue`, D-054 exception), `GET /api/tasks?scope=mine`, Operator explanations, the Today badge, Complete button and Tasks panel with Snooze | `faa2878` (branch head `e68b51d`), pushed 2026-09-09 |
| 017 | Inbound mail size cap: relay threshold at Cloudflare's 25 MiB ceiling with a streaming chunked base64 body, endpoint 34 MiB, `scripts/inbound-email` off argv (D-056; O-015 question 1 resolved) | `f06eba3` (branch head `33f8284`), pushed 2026-09-09 |
| 018 | Operator create-task proposals and complete-task receipts with Undo (D-057) | `d74c493`, pushed with 019a |
| 019a | Typed custom-field definitions and Person values (D-058) | `7dc4a2f`, pushed 2026-09-10 |
| 019b | Custom-field filtering across People, lists and Today | `237639d`, deployed 2026-09-10; pushed 2026-09-11 |
| 010a | Bounded FUB assessment, encrypted credentials/evidence and recovery (D-060); live validation deferred | `cd224fe` (implementation `e4e0658`), pushed and deployed 2026-09-11 |
| 010b | Encrypted core FUB capture, deterministic preview, budget controls and recovery (D-063); live validation deferred | `89471f0` (implementation `b71a854`), pushed and [deployed](../tasks/SLICE_010b_RELEASE.md) 2026-09-11; worktree/QA cleanup complete |
| 010c | Retained People/contact/stage/assignment import, provenance/reconciliation and admin review hold (D-064/065); activation and live validation deferred | `c3f6ca9` (implementation `fcd2480`), pushed 2026-09-11; [verification and integration](../tasks/SLICE_010c_VERIFICATION.md) complete, merged worktree/QA resources removed; [deployed and verified](../tasks/SLICE_010c_RELEASE.md) as source `f01c2e3` |
| 010f1 | Retained tags/custom-field child for completed 010c People, explicit mappings and immutable review hold (D-066) | `e36ce36` (implementation `f37ddd1`), pushed and [deployed/verified](../tasks/SLICE_010f1_RELEASE.md) 2026-09-11 under the explicit follow-up; all 975 source hashes match, merged worktree/branch removed and 45 pending main docs preserved. Synthetic [evidence](../design/qa/slice-010f1-2026-09-11/README.md): 900 Rust / 1,054 Web / 881 DB, 90 plans / 246 checks, 51 browser checkpoints. Public [release evidence](../design/qa/slice-010f1-2026-09-11-release/README.md): 43 HTTP/auth/asset checks, eight browser workflows and eight inspected screenshots; unchanged business counts/workspaces, no source import. Live validation/customer data/activation remain deferred. |
| — | Gate-speedup chunk (check 35m→79s, check-db 37m→~2m) | 2026-08-28 |
| — | Test-binary consolidation (40 files → 1 binary) | `6427ee8` |

Closing-state documents: docs/design/type-safety-hardening.md (ladder
closing state + residuals), docs/tasks/GATE_SPEEDUP.md (gate-speedup
resume artifact), docs/design/intake-throughput.md (intake capacity
notes).

## Historical perceived-latency analysis

**Historical perceived-latency analysis (2026-08-29–09-07).** The associated
read-model and Web work was subsequently delivered by Slices 012 and 014;
the measurements below are retained as historical evidence, not a current queue. *Re-measured over the public tunnel on 2026-09-07 at the
user's request, in a real browser: see
[perceived-latency-2026-09-07.md](../design/perceived-latency-2026-09-07.md).
Headline: the per-request edge floor is now 90–250 ms (two edge hops, jitter;
not app code) and POST bodies pay 300–700 ms more; dev-mode Vite ships 50–60
module requests per load and a 9-module route chunk before People's first
data request (rows at 0.4–0.8 s); login → Today data 1.4–1.8 s over four
sequential stages; the filter path is already flash-free
(`keepPreviousData` landed). Ranked levers: production build through the
tunnel first, optimistic mutations second, chunk preload and detail
hover-prefetch third. The earlier notes below stand.* Scale baselines measured 2026-08-29 against a
"Perf Test Realty" org seeded via the live API (dev DB only; wiped by
the next dev-bootstrap), first at 5k people, then at **100k people +
66,589 contact attempts + ~5k repeat inquiries** (the mature-FUB-team
case; write path held 104–107 leads/s across the whole 15-minute
seed, no degradation). At 100k: core filters stay FLAT (people
unfiltered 23 ms, assigned_to 21 ms, source 25 ms, last_contact-
within-7d 22 ms — the fixed matrix + indexes hold); ABSENCE-proving
filters degrade (has_phone-false 43 ms; last_contact-never 234 ms;
4-clause combo with a never clause 318 ms); **Today = 966 ms admin /
590 ms member** (linear in org size × history — ~97 ms at 5k). The
ladder's "fine to ~50k" holds for filters but NOT for Today (~500 ms
at 50k extrapolated). Consequence: the recorded denormalized
last-activity-columns lever (person.last_contact_at /
last_inbound_at / last_inquiry_at maintained at write time) now has a
measured trigger and should be its own small chunk BEFORE or WITH
011c (which multiplies Today's cost); it also collapses the
never-filters to indexed column tests. Tunnel-path measurements
(2026-08-29): ~60 ms edge floor per request; browser-realistic
People ≈ 90–140 ms; payloads edge-compressed 229 KB→27 KB (origin
CompressionLayer would shrink only the Mac→edge leg). Since the
backend is flat and fast, perceived speed work is web-side:
`placeholderData: keepPreviousData` on people/filter queries (kills
the Loading… flash on every chip edit), optimistic updates on
stage/assignment mutations, hover prefetch of person detail, collapse
the me→org-queries waterfall (2 sequential RTTs over the tunnel), and
prod-build web serving for the tunnel (dev Vite ships hundreds of
unbundled modules through it). Bundles naturally with the FilterBar
UX polish items (draft-chip affordance, anchored editors, clear-all).
100-AGENT CONCURRENCY TEST (same org, 2026-08-29): STRESS (no think
time) saturates the sqlx-default 10-connection pool — every request
queues to ~2.3–2.8 s and 9% 503 via the 2 s acquire timeout, Today the
biggest consumer; REALISTIC (2–5 s think time, ~24 req/s) is healthy
at p50 (people/filters 23–60 ms) but Today p50 612 ms / p95 1.8 s and
a 1.2% 503 rate — 100 active agents in one 100k-person org is past
comfort TODAY. Root cause is capacity = pool(10) / Today(~1 s);
the denormalization chunk multiplies capacity ~40x and is the fix;
explicit pool sizing (max_connections currently sqlx default 10,
state.rs) is the cheap secondary lever. Login (Argon2id) 236 ms avg
sequential — by design, fine. THE HARNESS IS COMMITTED: ./scripts/perf
(seed | bench | agents) + docs/design/PERF_BASELINE.md (full tables,
EXPLAIN anatomy of Today, method caveats: debug build, skewed books,
Python client) — re-run bench after any query/index/pool change and
compare against the baseline doc.

## Earlier verification checkpoints

- 2026-09-06, 011b final implementation tree on
  `codex/slice-011b-saved-lists`, base `1635fc4`: Terra ran
  `./scripts/check` (30s; 651 Rust tests, 5 doctests, 388 Web tests,
  9 email-worker tests, lint/type checking/build) and then
  `./scripts/check-db` (139s; SQLx prepare check and 379 DB tests), all passing.
  Astra source/performance review found no remaining actionable finding.
  Coordinator completed the isolated browser walkthrough and verified both
  existing People queries plus all 147 prior SQLx files unchanged. Full
  [evidence and criterion mapping](../tasks/SLICE_011b_VERIFICATION.md) includes
  the 50k dense/sparse plans and the native-confirm automation limitation.
- 2026-09-06, 011b documentation phase at `1635fc4`: independent
  Astra/ultra review READY after A1/A2 and R1–R3 corrections (typed version
  validation, membership freshness, dirty-copy preservation, uncertain
  create handling and count concurrency). Coordinator verified all nine
  relative Markdown file links in the five changed documents and
  `git diff --check`. No application/DB tests were run for this docs-only
  change; implementation and performance evidence remain future work.
- 2026-08-28, test-binary consolidation on `main`: coordinator's own
  final-tree run — check 14s warm; check-db 2:11 (363/363). Test
  reconciliation keyed on (file, test-name): 439/363 before = after,
  exact.
- 2026-08-28, 011a on `slice-011a-filter-vocabulary` before merge:
  lane gates green post-fix (check; check-db 49 blocks 0 failed;
  filter unit 65; db_people_filter 31; web 300); coordinator
  final-tree check + check-db green (own run).
- 2026-08-29, live against the running dev stack: post-011a filter
  path verified (garbage `?filter=` → 400; assigned_to filter → 4/16
  people, single assignee).

## Archived project-state snapshot — 2026-09-13

Preserved during the documentation-only efficiency pass from commit `6e89776`.
The following snapshot is verbatim, including stale branch, next-action and
approval paragraphs. Those paragraphs are historical, not current instructions;
use [current state](PROJECT_STATE.md) and the decision log for current scope.

<!-- BEGIN PROJECT_STATE snapshot 6e89776 -->
# Project state

Last updated: 2026-09-13 (Mobile 003 / 010e3 published and deployed to shared development).
This file holds current operational status, active work and live residuals.
[PROJECT_HISTORY.md](PROJECT_HISTORY.md) preserves earlier progress, the slice
ledger and historical measurements; its old instructions are not current work.

## Current state

**Mobile 003 and 010e3 are published and deployed under D-078 and its release
follow-up.** Runtime source is `b37a480c5bc4cd5c711dceba287cc5bed70a71d0`.
The [release record](../tasks/MOBILE_003_010e3_RELEASE.md) verifies all 45 schema
checksums, 142 prior/151 upgraded tenant rowsets, exact artifacts, public HTTP,
mobile synchronization and desktop/390px browser behavior. Only main and its
worktree remain; the original native demo is preserved. Native distribution,
physical phones, live FUB/customer work and activation remain separate.

Terra high completed the
shared backend, migration, iOS and Android lanes within the three-worktree limit.
Both native clients passed actual offline/restart/replay and installed-store
upgrade checks. Migration passed desktop/390px browser acceptance with exact
preservation of original records. All 1,009 regular database cases have passing
evidence; final affected admission checks, SQLx and Clippy also passed. The final
25k-Person admission query-plan run passed bounded page and worker-claim checks;
paired Today and ordinary Person read regressions passed. Release reused these
implementation gates and needed no application-source correction.
See the
[coordinated plan](MOBILE_003_010e3_PARALLEL_LAUNCH.md) and
[implementation status](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md).
Shared development and native-demo stores remain preserved.

**Mobile 002 and 010e2 were the preceding published/shared-development release
under D-076 and its explicit release follow-up.**
Both native clients passed encrypted-store upgrade,
offline/restart/replay and real-API conflict/revised-receipt checks. The migration
Web passed synthetic preview/confirmation/reload with exact preserved-record
reconciliation; the 25k bounded-query fix and paired Today read passed. Combined
`scripts/check` and final SQLx/Clippy checks passed. All 990 current DB cases have
passing evidence after the full run and 121-case affected rerun; the final fix
prevents old import permits from inheriting contact UPDATE/DELETE permissions.
All implementation worktrees are closed and isolated QA servers are stopped.
See the
[implementation record](../tasks/MOBILE_002_010e2_IMPLEMENTATION_STATUS.md) for
actual commits, checks and evidence limits. Runtime source is
`9d04755e405848cb06a8e4c135793d9994176648`. The
[release record](../tasks/MOBILE_002_010e2_RELEASE.md) verifies all 43 schema
checksums, 134 prior/142 upgraded tenant rowsets, exact deployed artifacts,
124 HTTP checks, 43 mobile requests and actual desktop/390px browser behavior.
The milestone integration branch is removed. Native distribution and
physical-phone/cellular testing remain separate.

**Mobile 001 is implemented and verified for the approved synthetic scope under D-074.** The reviewed
[specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md) and
[briefs](../tasks/MOBILE_001_IMPL.md) cover both native platforms, protected SQLite,
seven-day access and durable synchronization. [Toolchain preparation](../tasks/MOBILE_NATIVE_TOOLCHAIN_SETUP.md)
records installed SDKs and successful iPhone/Android emulator boots. Backend
implementation and both native apps are merged and published on main. Their
synthetic verification is complete; backend/Web are deployed in shared development. iOS and Android passed offline work preservation, restart and actual
API synchronization checks; Android also passed emulator reboot/reauthorization.
All implementation worktrees are closed. Combined repository/SQLx gates passed,
and all 966 DB cases have passing evidence after two test-fixture corrections.
A repeated-sync defect found by the native apps was corrected and all ten affected
backend tests passed; iOS and Android then passed repeated downloads against the
patched private API. See the [implementation status](../tasks/MOBILE_MIGRATION_IMPLEMENTATION_STATUS.md).

Approved [010e1](../specs/SLICE_010e1.md) compares a newer core capture with the
original import baseline and reports source changes without updating CRM data.
Its reviewed contracts, implementation and release are complete under D-075 and
its follow-up. The [coordinated launch](MOBILE_MIGRATION_PARALLEL_LAUNCH.md) is the
completed milestone's ownership record, not a pending launch instruction.

After trying the iOS app, the user reported that the app and sync seem to work
and asked to continue. Mobile design/system-information cleanup is deferred.
The user separately selected **"Leave physical-phone testing for later"** after
read-only discovery found no connected physical phones. The user then clarified
that iOS/Android feature development should continue in parallel, and requested
preparation/review of [Mobile 002](../specs/MOBILE_002_OFFLINE_EDITS.md): offline
note/task editing with explicit conflict recovery. It accompanies the
[010e2 specification](../specs/SLICE_010e2.md) for previewed updates to already imported
People. D-076 accepted both contracts and Terra high implementation; the
[coordinated plan](MOBILE_002_010e2_PARALLEL_LAUNCH.md) records the completed backend,
migration and iOS/Android sequence within three worktrees.
That implementation used isolated synthetic resources; the native demo remains intact.
The [implementation status](../tasks/MOBILE_002_010e2_IMPLEMENTATION_STATUS.md)
records actual lane progress and assigned resources separately from planning.

**The superseded Mobile 001 / 010e1 shared-development runtime was**
`08cb42057013cb8766ae61acb23458b0cf38c416` at
[app.tarams.org](https://app.tarams.org/manage/migration). The four additive
migrations through `20260924000001` are applied; all 41 source checksums match.
The release preserves all prior tenant columns and adds initial Person/task
revision values. Public HTTP/browser checks, exact served assets and all five
migration capabilities passed. The distinct mobile receipt key is configured.
[Milestone release evidence](../tasks/MOBILE_001_010e1_RELEASE.md) owns final
mobile proof, data reconciliation, backup and current report expiry. Native
distribution, physical devices, real cellular and live customer/FUB work remain
separate. The private native-demo API3101 is preserved.

**010d2 is the previous verified shared-development release.** Its runtime
source was `5924f097a7d7475e7e9742bb08e102d7c7ca98d2`, incorporating implementation
`f6f74262e0cd019cb8fd49a114529aafea4f2aca`, at
[app.tarams.org](https://app.tarams.org/manage/migration). Additive migration
`20260922000001` is applied to `crm_dev`. Actual API/admin/migrator/Web hashes,
independent timeline preflight, backup and exact preservation of 108 prior tenant
rowsets are recorded in the [010d2 release](../tasks/SLICE_010d2_RELEASE.md).
The derived review state exactly matches 100,077 existing People; import/fact
tables remain empty. Public HTTP/browser checks passed. Source was committed,
merged and pushed, and the implementation branch/worktree removed at that release. No live source/customer processing or activation occurred.
Confirmation still requires a fresh five-minute
`CRM_MIGRATION_RELEASE_REPORT`; consult the current release record for its
observed expiry and actual running artifact identity before an operational step.

Before 010d1 integration, the
[2026-09-12 completion audit](../tasks/SLICE_010_COMPLETION_AUDIT_2026-09-12.md)
independently matched then-current GitHub/main, all 1,016 source hashes, the running release
artifacts and all 35 applied migration checksums. Shared business counts/workspace
states remain unchanged. Fresh service-free checks and a complete 908-test DB
rerun passed; two earlier runs reproduced the pre-existing call-history ordering
failure, and nextest LEAK annotations have passing isolated follow-ups. Two extra
A6 audit tests passed note/task local-state preservation cases absent from the
original focused coverage. An overnight database-warning episode strongly
correlates with host sleep/background wake and stopped on full wake; its exact
mechanism is unconfirmed. No product source/runtime change or live FUB operation
was made during that audit. Its documentation/evidence was preserved and
committed with the 010d1 integration.

The following 010f2 measurements describe the superseded release baseline;
current operational identity is recorded above and in the milestone release record.
**010f2 was deployed and verified in shared development**, including assessment,
core snapshot/preview, People, tags/custom fields and retained notes/tasks.
Its source was `fc5a8757bcb2591a43976544e282722b3616039f`
(implementation `9f457bc8db8707fa4ce361aac485fd6bf928bf34`) at
[app.tarams.org](https://app.tarams.org/manage/migration).
Migration `20260920000001` is applied on `crm_dev`; API PID 25757 on port 3000
and Web PID 25780 on port 5173 were observed at 22:18:29 PDT on 2026-09-11.
Verify live identity again before a later operational action.

The [release record](../tasks/SLICE_010f2_RELEASE.md) and
[sanitized evidence](../design/qa/slice-010f2-2026-09-11-release/README.md)
record locked API/admin/migrator and production Web builds (36.89/1.42 seconds),
the additive migration, backup catalog, actual-DB compatibility preflight,
50 HTTP/auth/asset checks and eight completed public browser checks with ten
inspected desktop/390px screenshots. Activity refresh/reload returned 200;
member access was denied and temporary sessions were revoked. The browser's
final telemetry assertion was qualified offline after all functional checks:
four deliberately blocked Cloudflare Insights GETs were the only unexpected-origin
entries. The raw attempt is retained; the UI flow was not repeated.

All 45 business counts and three operational workspace revisions are unchanged;
all 56 migration tables (including 13 activity tables) and operation admissions
are empty. The 110,063,438-byte backup's 932-entry catalog was validated;
restoration was not exercised. Tunnel routing passed 200/200/101. A bounded
214.61-second UTC observation found stable listeners and zero API/Web WARN/ERROR
entries; three backend, 73 Web and 1,016 source hashes matched. Implementation
gates remain applicable because production source did not change during release.
The merged worktree/branch and disposable QA files were removed after preserving
249 original evidence files and the eight pending main planning files.

Import confirmation requires a fresh operator compatibility report at
`CRM_MIGRATION_RELEASE_REPORT`. Actual-DB launch and confirmation checks passed
with workspace, metadata and activity readiness at that release. Its recorded
five-minute report expired at **2026-09-12 05:18:49 UTC**
(2026-09-11 22:18:49 PDT).
Renew from an actual workload inventory before a later confirmation. Expiry
leaves ordinary CRM access available and import confirmation unavailable.
No automatic renewal or recurring monitor was established.

No live FUB connection, assessment, snapshot or import was created. Registered
system configuration remains unset, so upstream reads fail closed. Live
validation remains user-deferred. Source authorization does not replace
customer-data prerequisites, including D-015's erasure runbook. No activation
or production-cluster deployment occurred. The bounded SQLx cancellation
follow-up remains open in production readiness.

## Current slice

- **Mobile 003 / 010e3:** implemented, verified, merged/pushed and deployed to
  shared development under D-078 and its release follow-up. The
  [release record](../tasks/MOBILE_003_010e3_RELEASE.md) owns current runtime and
  preservation proof; the [implementation status](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md)
  retains native/backend/browser and performance evidence.
- **Mobile 002:** implementation, verification and source publication are complete.
  Terra high implemented backend/iOS/Android; both native clients passed protected
  upgrade, offline restart/replay and explicit conflict/revised-receipt checks.
  The backend adapter is deployed with the shared milestone. Native distribution
  and physical devices remain deferred.
- **010e2:** implementation, source publication and backend/Web deployment are
  complete. Reviewed existing-People refresh preserves local changes, provenance
  and the review hold. The [release record](../tasks/MOBILE_002_010e2_RELEASE.md)
  owns the completed deployment and preservation checks. Live source processing,
  new-Person admission and workspace activation remain separate.
- **010e1 implementation and release:** merged, pushed and deployed under D-075
  and its release follow-up. Retained core-change
  reports preserve exact source evidence, representative metadata and the review
  hold. Final focused/process/query-plan checks, real-API desktop/390px workflow
  and the combined backend gates passed. See [verification](../tasks/SLICE_010e1_VERIFICATION.md)
  and [combined evidence](../tasks/MOBILE_MIGRATION_COMBINED_VERIFICATION.md).
  The merged migration worktree and temporary QA listeners are closed. Git
  publication and shared-development release are complete. Live FUB work, delta
  application and activation remain separate.
- **Mobile 001:** backend integration, shared regression gates and both native
  offline/sync workflows have passing evidence. iOS (`92abb916`, `6e3f715`) and
  Android (`83379df`) are locally integrated; their worktrees are closed with native
  build/test artifacts preserved. One bounded native review's corrections are resolved.
  Physical-device/cellular validation and app distribution remain later gates.

- **010d2 implementation:** [Implemented and verified](../tasks/SLICE_010d2_VERIFICATION.md)
  under D-072 with user-selected metadata-first exposure. Separate external facts,
  resumable retained-capture import and paged review preserve native truth/Today;
  both complete-reader fences and the confirmation race are verified. One bounded
  implementation review's four findings were corrected. Full gates passed:
  963 Rust, 1,164 Web and 953 DB tests, plus supporting suites, paired/query-plan
  checks and a real-API desktop/390px walkthrough. The explicitly authorized
  [release](../tasks/SLICE_010d2_RELEASE.md) completed Git integration, push,
  cleanup and shared-development deployment, with 65 public HTTP checks and
  desktop/390px verification. Original implementation evidence keeps its attribution.
- **010d1 implementation and release:** complete under D-070 and its follow-up;
  source is committed, merged, pushed and deployed in shared development, with
  the implementation worktree/branch removed. Both bounded reviews are READY.
  Original implementation evidence includes SQLx preparation, full checks
  (949 Rust/1,131 Web plus supporting suites), 934 DB tests, the
  75,000-observation collector and desktop/390px production-Web walkthrough.
  See [verification](../tasks/SLICE_010d1_VERIFICATION.md) for the original hashes,
  failure/recovery evidence, query-work limits and owned cleanup, and
  [release](../tasks/SLICE_010d1_RELEASE.md) for final runtime identity and the
  separately verified release-time denial-header correction. D-069's
  [split](../specs/SLICE_010d.md) is preserved: capture stores encrypted source
  evidence and bounded admin coverage without native timeline/Today changes.
  Public text pagination remains provisional and live-unqualified. Native fact
  promotion, email/media, deltas, activation and customer-data readiness
  remain separate work.
- **010f2 implementation:** implemented under D-068; both bounded reviews,
  measured SQL/paired-reader checks, all 18 production-Web phases and exact
  native/business/storage reconciliation have passed. All final gates and owned
  cleanup passed; see [verification](../tasks/SLICE_010f2_VERIFICATION.md).
  Retained notes/open/completed tasks use explicit mappings, preserved originals,
  confirmed timezone and bounded admin review under the unchanged hold. Source
  is committed, merged, pushed and deployed under the D-068 follow-up; the merged
  worktree/branch was removed. See [release evidence](../tasks/SLICE_010f2_RELEASE.md).
  SQLx cancellation notices remain tracked with attribution limits in production
  readiness. Live FUB/customer-data work and activation remain separate scopes.
- **010f1 implementation and release:** complete under D-066 and its follow-up. The
  [specification](../specs/SLICE_010f1.md),
  [brief](../tasks/SLICE_010f1_IMPL.md) and
  [code evidence](../research/SLICE_010f1_CODE_CONTRACTS.md) cover tags and
  custom-field definitions/options/values for the already imported People in
  the existing review workspace. Notes/tasks are now delivered by 010f2; full
  standalone-tag capture, deltas and activation remain following work. [Independent plan review](../tasks/SLICE_010f1_REVIEW.md)
  returned READY with its one finding resolved. The accepted scope preserves current limits,
  including 20 tags per Person, 200 tags per Organization and 50 live custom
  fields; incompatible/ambiguous data and differing destination values are held.
  Implementation used an isolated worktree with one backend/database owner,
  Web authoring after the frozen contract and two bounded review/fix rounds.
  [Verification record](../tasks/SLICE_010f1_VERIFICATION.md)
  records 900 Rust, 1,054 Web and 881 DB test passes, 90 query plans / 246 checks,
  51 real-API/production-Web browser checkpoints and 18 inspected screenshots.
  Both bounded implementation reviews are READY. Exact native values, unchanged
  review bindings, storage pause/resume and partial cancellation are verified.
  Implementation `f37ddd1` and merge `e36ce36` are published; the merged worktree/
  branch and disposable QA services/databases are removed. The
  [shared-development release](../tasks/SLICE_010f1_RELEASE.md) passed its separate
  HTTP, browser, compatibility and tunnel checks. No live FUB validation occurred.
- **010c implementation:** approved under D-065, committed as `fcd2480`,
  merged as `c3f6ca9` and pushed to main under its follow-up authorization.
  The [spec](../specs/SLICE_010c.md) and [brief](../tasks/SLICE_010c_IMPL.md) cover
  People/contact/stage/assignment import from retained 010b evidence, separate
  People with contact overlap flags, explicit mappings, atomic empty-Organization
  entry and a durable administrator review hold. Both bounded implementation
  reviews and their corrections are complete; executed query-plan and browser
  findings received targeted fix confirmation. The final sequential gates passed:
  877 Rust tests, five doctests, 1,002 Web tests, 845 DB tests, 14 release-preflight
  tests and 11 email-worker tests. All 59 query plans/258 assertions and the single
  paired reader benchmark passed. Production-Web browser verification passed all 50 recorded checkpoints,
  including six states at six viewport widths. [Evidence summary](../design/qa/slice-010c-2026-09-11/README.md) and
  [verification chronology](../tasks/SLICE_010c_VERIFICATION.md) retain the exact
  source, checks and limits. Git integration, publication and cleanup are complete;
  the subsequent [shared-development deployment](../tasks/SLICE_010c_RELEASE.md)
  is verified. No live FUB operation or activation occurred.
- **Foundations documentation:** user authorized the three deliverables on
  2026-09-11: refresh the system map/state, draft foundations, define readiness.
  [System map](../architecture/ARCHITECTURE_BASELINE.md),
  [foundations proposal](FOUNDATIONS.md) and
  [readiness checklist](PRODUCTION_READINESS.md) are the handoff artifacts.
  New policies are proposals, not accepted decisions or implemented capabilities.
- **010b:** [spec](../specs/SLICE_010b.md), [brief](../tasks/SLICE_010b_IMPL.md)
  and [source qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) are
  approved under D-063 after independent review returned READY. Core-first
  sequencing remains D-061. The approved contract includes representation-aware
  capture, retained-read authorization, frozen preview inputs, bounded overlap
  groups, durable logical-byte reservations and revisioned budget increases.
  Backend and Web are complete, including explicit budget review, separate source
  and preview recovery, partial/previous reports and session fencing. Both bounded
  review/fix rounds are complete. Final sequential `sqlx-prepare`, `check` and
  `check-db` passed: 860 Rust tests, 5 doctests, 933 Web tests, 11 email-worker tests
  and 809 DB tests. The 25,000-People group case and real-API/production-build Chrome
  workflow, storage recovery and desktop/390px verification passed. See the
  [verification record](../tasks/SLICE_010b_VERIFICATION.md) and its sanitized evidence.
  The subsequent D-063 follow-up authorized integration, shared-development
  deployment and cleanup; all are complete. Live FUB validation stays deferred.

## Current branch

`codex/mobile003-010e3-integration` starts from published main closeout
`619c1b33b15dcd8f331561e16178b3e107984a3b` plus the approved planning checkpoint.
The coordinated plan allocates two backend worktrees, then both native lanes.

Main contains the published milestone source
`9d04755e405848cb06a8e4c135793d9994176648` plus documentation-only closeout.
All implementation worktrees and the integration branch are removed. Native
source/evidence are published; protected build/test artifacts remain outside the
checkout. The synthetic API3101 and the installed demo stores are preserved.

The shared backend/Web run exact isolated release builds from that source.
The [release record](../tasks/MOBILE_002_010e2_RELEASE.md) distinguishes runtime
source from closeout commits. Continue to isolate Cargo targets and Web outputs
from artifacts used by running shared services. The prior implementation output
collisions and restoration remain historical evidence.

## Last accepted decision

**D-078:** implement both reviewed slices, including occurrence-time mobile
contact logging, using Terra high and the coordinated three-worktree sequence.
Local checkpoints and synthetic verification are authorized; publication and
deployment remain later release scope. This supersedes D-077 pending acceptance.

**D-077:** prepare/review Mobile 003 and 010e3 and their implementation sequence.
This is planning authorization; proposed contract changes are not accepted yet.
The mobile occurrence-time choice remains pending. See the two specifications
and coordinated plan above for the concrete reviewable result.

**D-076:** the user explicitly approves Mobile 002 and 010e2 implementation,
including their reviewed contracts and parallel plan, and confirms Terra for
implementation. Local integration and isolated synthetic verification are in
scope; no repeat approval is needed. Its explicit 2026-09-13 release follow-up
also authorizes commit/main merge/push/cleanup and shared backend/Web deployment;
that release is complete. Native distribution and live customer work remain separate.

**D-075 and release follow-up:** reviewed implementation is complete; the user
authorized releasing the completed milestone. Git integration/publication and the
shared backend/Web deployment are performed under this follow-up. Native app
distribution, physical-device/cellular checks and live customer work remain separate.

**D-074:** the user approves the reviewed Mobile 001 contracts and implementation
for backend, iOS and Android, and confirms parallel migration development.
Prepare the next migration specification and ownership plan before launch. No
repeat mobile approval is pending; a new migration contract needs its own scope.

**D-073:** initial offline record selection, SQLite and seven-day access were
accepted during planning. D-074 supersedes its pending Mobile 001 approval.
Native SDK licenses were separately authorized and setup is verified.

**D-072:** reviewed 010d2 implementation and the subsequent explicitly authorized
commit/merge/push/cleanup/shared-development deployment are complete. See current
release evidence above; this does not authorize future releases or activation.

**D-070:** the user said “Ok go for it” after the complete 010d1 plan and READY
review were presented. The full spec/contracts, implementation and isolated
synthetic verification are approved. Native history/timeline, live FUB/customer
work, activation, Git publication and deployment remain separate scope.

**D-069:** the user authorizes 010d planning and selects capture/coverage in
010d1 followed by verified historical timeline import in 010d2. This accepts
the sequence only; complete proposed contracts and implementation still require
review/approval. Live FUB validation remains deferred.

**D-068:** the user approved the complete reviewed 010f2 specification, policies,
shared contracts and implementation with isolated synthetic verification. This
includes the bounded native review/body-read amendment and activity-capable
recovery contract. The subsequent follow-up authorized Git integration, cleanup
and shared-development deployment, now complete. Live FUB/customer processing
and activation remain separate scope.

**D-067:** next notes/tasks import planning and independent review are authorized.
The user accepts readable plain-text HTML notes with preserved originals and
date-only deadlines at the end of the day in an explicitly confirmed source
timezone. D-068 subsequently accepts the complete reviewed 010f2 specification and
implementation; its follow-up completes the shared-development release. Source
access remains separate scope.

**D-066:** the complete independently reviewed 010f1 specification, execution
brief, policies and declared shared contracts are approved for implementation
and isolated synthetic verification. Completed-parent-only, one metadata child,
explicit creation/mapping, held-item subset acknowledgement, unchanged limits
and no replacement of local values are accepted. The explicit follow-up
authorized commit, merge, push, cleanup and shared-development deployment; those
steps are complete. Live FUB/customer-data work, activation and production-cluster
deployment remain outside this authorization.

**D-065:** the complete reviewed 010c specification, shared contracts and synthetic
implementation/check work are approved. Its follow-up authorizes commit, merge,
push and cleanup; the subsequent follow-up authorizes shared-development
deployment and next-import planning. Deployment is verified; D-066 separately
approved 010f1. Further import families, activation, real-data processing and
live-source validation retain their own scope and readiness boundaries.

**D-064:** 010c preserves distinct source People despite shared contacts, allows
explicitly approved matching stages, and keeps imported records in an admin
review workspace until later activation. D-065 subsequently accepts the complete
010c contracts after independent READY review.

**D-063:** the reviewed 010b specification, contracts, implementation and synthetic
verification are approved. Initial synthetic-development allowances are 2 GiB/run
and 4 GiB retained/Organization; current admins may explicitly raise them within
operator ceilings, with a separate resume action. These are logical payload
allowances, not production quotas or physical disk limits. **D-062** places future
email bulk content outside PostgreSQL; implementation/provider remain open.
Foundations F-01/F-02/F-03 remain proposals except where separately accepted.

## Operational entry points

| Need | Repository entry point / limitation |
|---|---|
| Start or inspect development | [README](../../README.md#development); local processes differ from Docker services; dev-bootstrap wipes data |
| Understand boundaries | [Architecture map](../architecture/ARCHITECTURE_BASELINE.md); actual state here, policy in the decision log |
| Release or recover a release | [010f1 release/evidence](../tasks/SLICE_010f1_RELEASE.md), [compatibility runbook](../tasks/SLICE_010c_RELEASE_PREPARATION.md), [release procedure](../prompts/07-deploy.md); fresh metadata-capable workload inventory is required, and private temporary backups are not a production backup system |
| Prepare real data or production | [Readiness](PRODUCTION_READINESS.md); decisions, accountable roles and required proof remain visible |
| Find a previous merge/checkpoint | [History and slice ledger](PROJECT_HISTORY.md#slice-ledger); detailed evidence in per-slice records |

## Parked / queued tracks

- **Mobile design:** user-deferred after the simulator walkthrough. Review the
  amount and placement of system/diagnostic information later; keep essential
  saved-work, access-lock and sync-recovery feedback. No redesign was made here.
- **Physical iPhone/Android testing:** explicitly user-deferred. Existing proof
  remains simulator/emulator only. [Device follow-up](../tasks/MOBILE_001_PHYSICAL_DEVICE_FOLLOWUP.md)
  records discovered build/environment prerequisites and the later test sequence.
- **Slice 010b (FUB migration): DEPLOYED AND VERIFIED.** Live validation
  remains deferred. The approved 010b implementation passed its synthetic/full gates.
  [Current summary](SLICE_010_MIGRATION_SUMMARY.md); historical survey retained
  in [the ladder](SLICE_010_LADDER.md). New, empty Organization first and
  010a's source/credential contracts are accepted. Full inventory, mapping,
  later imports and cutover need their own specifications and approvals. 010c and
  010f1 are implemented, integrated and deployed. The next notes/tasks scope is
  implemented in [010f2](../specs/SLICE_010f2.md), with synthetic verification recorded separately from release.
  Live FUB/customer-data work and activation stay deferred.
- **Remote gates (gate-speedup phase 2): DEFERRED** pending local
  phase-1 results (now in: local gates are ~3 min — pressure is low).
  Survey recorded so it is not re-litigated: first choice GitHub
  Actions + self-hosted runner on the user's 64-core machine (origin
  github.com/murthy-karra/crm, gh authenticated; hosted default
  runners too small; GitHub larger runners need a paid Team org;
  Depot/Blacksmith-class vendors are the cheap escape hatch with no
  workflow rewrite; Buildkite the only non-Actions product seriously
  weighed). Re-verify vendor pricing at spec time.

## Live residuals and follow-ups

Carried forward; everything else previously listed here was resolved
and now lives only in git history.

**For the 011b spec (recorded 2026-08-28, not blocking):**
- M7 positive span pin (`filter_kinds` values) was skipped in 011a.
- Resolved in the 2026-09-06 filter UX pass (`0117b87`): dedicated
  Back/Forward, empty-filter URL normalization, and invalid fractional-day
  regressions. Fractions are rejected, not truncated, under amended 011a §6.
- "20 clauses accepted" ceiling is unconstructible (10 kinds ×
  one-per-kind) — record as closed/wontfix; the two reachable
  ceilings are pinned.

**Deferred live walkthroughs (user's choice, test-pinned meanwhile):**
- 011a §8 functional walkthrough completed in the 2026-09-06 filter UX
  pass before D-045's visual refresh: combined Stage + Me + Source,
  reload, Clear all, navigation, and invalid-day dismissal verified live.
- 009 walkthrough steps 3–5 (reply-all→client_replied, retroactive
  forwards, rotation) deferred 2026-08-28; one stray held row was
  left in the capture queue deliberately, for the user to dismiss as
  the dismiss-path exercise.

**Watch items:**
- Three plain-language docs of UNKNOWN authorship appeared
  mid-011a-implementation (SLICE_011_LADDER "In plain language",
  SLICE_011a.md preamble, SLICE_011a_EXPLAINED.md). Kept by user
  decision; the implementation lane denied authorship twice. Watch
  for a recurrence in the next implementation cycle.
- Always diff a subagent's reported file list against real `git
  status` (standing rule since the Slice 002 undisclosed
  PII-dump-tool incident).

**Gate/tooling residuals (gate-speedup + consolidation, 2026-08-28):**
- 4th 501-INSERT test left unbatched (db_people.rs unresolved-queue).
- Stray sqlx ephemeral test databases from killed runs — drop at
  leisure.
- The doctest step (73s) now dominates ./scripts/check — future
  micro-lever: scope `--doc`.
- lld linker experiment failed cleanly on this Xcode/clang and was
  reverted (don't retry without a new toolchain reason).
- Never overlap two db-backed test runs in one checkout (self-
  inflicted collision during 008 verification).

**Telephony (006x, all pre-existing):**
- Rotate the Telnyx SIP password; update the trunk (user action,
  still pending).
- Busy/ring-out outcomes never proven live; `placing` sweep horizon
  vs slow mic prompts; orphaned "outcome needed" calls when a caller
  is deactivated (O-004 territory). LiveKit hostname:
  `livekit1.tarams.org`.
- Known ordering issue: `db_calls_corrections::a_second_correction_chains_onto_the_first_
  with_strictly_increasing_recorded_at` failed twice in the 2026-09-12 audit,
  including in isolation, before passing in the complete rerun. The actual
  failure was timeline placement around `call_completed`; strict timestamp,
  chain and complete-row assertions passed. Host/DB clock comparison is the
  supported mechanism, not proven timestamp ties. See the completion audit.

**Known accepted edges / small gaps:**
- 007h1: a forwarder's trailing signature is part of the inner body
  in plain text (spec §5); HTML gmail_quote separation is a later
  rung.
- Pre-existing D-027/O-004 gap: `is_organization_member` lacks a
  status filter on the manual explicit-assign path (flagged in 007c
  exclusions, deliberately not fixed there).
- `set_local_password` has no test coverage; no test executes the
  `crm-admin` binary (recorded at the dev-seed rework).
- Early-slice (000/001) deferred review minors — dev-only or latent
  library-internal edge cases (cookie-parsing pins, `[::1]` bind,
  `x-request-id` trust, empty `DATABASE_URL`, migrate/seed error
  `Debug` propagation) — full text in this file's git history at
  2026-08-28; revisit only if the affected surface changes.

**Environment (standing):**
- **LiveKit is back (2026-09-09) on a new host:** `livekit1.tarams.org`
  is an EC2 `c6i.xlarge` in us-west-1 with the Slice 006 stack plus a
  dormant LiveKit Egress (D-055); `./scripts/check-telephony` passes
  against it with `CRM_TEST_LIVEKIT_API_URL=https://livekit1.tarams.org`
  (the variable is empty in `.env`, so the script refuses unless it is
  exported). The Telnyx SIP password rotation is still pending.
- Dev tunnel routing lives ONLY in the Cloudflare dashboard (D-025);
  `config.yml`'s ingress section is documentation. A fresh clone or
  recreated tunnel needs the three dashboard routes set by hand per
  README.
- The orphaned-dev-api hazard remains structural: "restart services"
  restarts only Docker; crm-api keeps running the old binary. Compare
  process start time vs binary mtime; kill by exact PID only. Bit us
  again 2026-08-29 (011a filters). Run ./scripts/db-migrate after
  checking out a branch with a new migration.
- The current shared-development release is 010f2 from `fc5a875`, deployed
  2026-09-11; see [the release record](../tasks/SLICE_010f2_RELEASE.md). Source
  validation remains deferred; registered FUB system configuration is unset.

## Backlog (deferred product tracks — full notes in the decision log)

- **O-014 email epic**: remaining products — send (O-006),
  transactional, migration reconstruction. Gmail restricted-scope
  CASA assessment is the schedule-driver — start paperwork early.
- **O-013 "Delete my data"**: Person erasure on O-012 crypto-shred;
  must be addressed before the first external customer holds real
  consumer data.

- **O-015 blob storage / retention:** the size-cap question was resolved by
  D-056 and implemented/deployed in Slice 017 (25 MiB relay threshold,
  34 MiB endpoint envelope limit); its live sends remain user-deferred.
  D-062 accepts email bulk content outside PostgreSQL, using object storage
  or dedicated storage servers; provider/layout and implementation are still
  open. Whole-message relocation and retention remain later work, coordinated
  with recordings and O-013. Raw MIME still lives encrypted in Postgres BYTEA.
- **O-008 AI next-step suggestions**: after every communication and
  daily; reminder only; no work before the communication slices.
- O-006 (outbound messaging consent) blocks the SMS slice; O-002
  (recording consent) blocks recording features.

## Latest verification

- 2026-09-12 010d2: isolated implementation verification passed SQLx preparation,
  `check` (963 Rust, five doctests, 1,164 Web, 31 preflight, 11 email-worker),
  and `check-db` (953 DB tests). Targeted import/reader/authority/race/old-artifact
  cases, corrected query plans and the 112-record production-Web walkthrough
  passed. [Evidence](../tasks/SLICE_010d2_VERIFICATION.md) preserves original
  failures, runner annotations, source hashes and measurement limits. No release
  or live-source/customer processing occurred.

- 2026-09-11 010f2 planning: complete specification, implementation brief and
  source/code evidence received independent review. The all-held confirmation
  finding is corrected with a minimum eligible-unit check and explicit tests;
  [the review record](../tasks/SLICE_010f2_REVIEW.md) retains the disposition and
  reviewed hashes. Planning uses public documentation and local code only; no
  application tests/builds, DB operations, runtime changes or live FUB calls.

- 2026-09-11 010f1 release: implementation `f37ddd1`, merge `e36ce36`, published
  and all 975 source hashes verified. Locked backend/Web builds and additive
  migration `20260919000001` passed; the 109,990,155-byte backup catalog was
  validated without a restore. Actual-DB launch/confirmation preflight passed
  with metadata capability and a five-minute report expiry. All 43 HTTP/auth/
  asset checks, eight public browser workflows, eight inspected desktop/390px
  screenshots and tunnel 200/200/101 passed. All 45 business counts are unchanged,
  43 migration tables remain empty and three Organizations stay operational at
  revision 1. The 72-second runtime observation found zero WARN/ERROR entries;
  binary/Web/source hashes match, and sessions were revoked. Main-only cleanup
  preserved the 45 pending main docs and all recovery material. See
  [release evidence](../tasks/SLICE_010f1_RELEASE.md). No live source operation,
  customer import or activation occurred; later confirmation needs fresh evidence.

- 2026-09-11 010f1 implementation: both bounded reviews are READY. Final gates
  passed 900 Rust, 1,054 Web and 881 DB tests; 90 plans / 59 distinct SQL hashes /
  246 checks and 51 synthetic real-API/production-Web browser checkpoints passed.
  Eighteen screenshots were inspected. Exact native reconciliation, role/tenant
  fencing, retry/cancel and unchanged parent/review bindings are recorded in the
  [verification record](../tasks/SLICE_010f1_VERIFICATION.md). These synthetic
  workflows are distinct from the public empty-state release check above.

- 2026-09-11 010f1 planning: the complete specification, execution brief and code
  evidence received independent READY review after the declared-choice correction.
  [Review record](../tasks/SLICE_010f1_REVIEW.md) preserves the finding, disposition,
  reviewed hashes and then-remaining human decisions. At that planning checkpoint,
  no 010f1 implementation or application/DB/browser test had been performed;
  the later implementation and release results are recorded above.

- 2026-09-11 010c release: locked API/admin/migrator and staged Web builds passed;
  database backup catalog read, migration applied, actual-DB compatibility
  launch/confirmation preflights passed. Retired 23 old executable paths and
  observed the new runtime/workers/admin launch inventory. All 38 HTTP/auth/asset
  checks, seven public browser workflows and tunnel 200/200/101 passed. All 45
  business counts unchanged; 30 migration tables remain empty; zero runtime
  WARN/ERROR entries in bounded observation. Six screenshots visually inspected.
  [Release evidence](../tasks/SLICE_010c_RELEASE.md) distinguishes operational
  release checks from prior isolated synthetic review-hold proof. No restore,
  live source operation or activation.

- 2026-09-11 010c implementation: both bounded reviews and targeted corrections
  are complete. Final gates passed: 877 Rust, 5 doctests, 1,002 Web, 845 DB,
  14 preflight and 11 email-worker tests. All 59 plans/258 assertions and the
  single paired reader benchmark passed. Production Web/real synthetic API
  passed import/replay/provenance, role and late-response fencing, isolation,
  storage resume/cancellation and six-state/six-width browser checks. Source and
  runtime manifests, 32 screenshots and logs are in the
  [evidence summary](../design/qa/slice-010c-2026-09-11/README.md).
  Follow-up Git integration and cleanup passed: implementation `fcd2480`, merge
  `c3f6ca9`, pushed to origin/main; identical implementation/merge trees and all
  236 source entries verified. The merged worktree/branch and disposable QA
  resources are removed. The later release is verified separately above; live
  FUB validation remains deferred.

- 2026-09-11 010c planning: independent full review returned READY-WITH-FIXES;
  five corrections received targeted READY confirmation. Documentation paths,
  Markdown anchors, whitespace and documentation-only scope passed: 111 local
  paths and three anchors across eight changed Markdown files. No application/DB/browser/performance test, source call,
  commit/push, restore or runtime operation was performed. See the
  [plan review](../tasks/SLICE_010c_REVIEW.md).

- 2026-09-11 010b release: API/migrator and staged Web builds passed; database
  backup catalog read; additive migration applied. All 29 HTTP/auth/asset checks,
  tunnel 200/200/101 and public admin/member/reload/desktop/390px browser checks
  passed. All 45 business counts unchanged; 12 snapshot tables present and empty;
  zero API/Web WARN/ERROR entries during bounded observation. All 37 source hashes
  match the verified implementation and merge tree. Branch/worktree and disposable
  QA database cleaned up. No full gate rerun or live source operation. See
  [release evidence](../tasks/SLICE_010b_RELEASE.md).

- 2026-09-11 010b implementation: both bounded review/fix rounds complete;
  sequential `sqlx-prepare`, `check` (860 Rust, 5 doctests, 933 Web, 11 worker tests)
  and `check-db` (809 DB tests) passed. Synthetic real-API/Chrome flows, storage
  exhaustion/recovery, tenant/session boundaries, 25k indexed pagination and six
  final desktop/390px screenshots passed. All 37 final source hashes match.
  Temporary QA servers were stopped; shared-development 010a was not redeployed.
  See [verification](../tasks/SLICE_010b_VERIFICATION.md). Live FUB validation remains deferred.

- 2026-09-11 revised 010b plan: independent review returned READY-WITH-FIXES;
  two findings were corrected and targeted confirmation returned READY. All
  47 local document paths/anchors, whitespace and documentation-only scope
  checks passed. See the review record for dispositions. During that planning
  phase, no 010b code, tests, browser walkthrough, live FUB call or deployment was performed.

- 2026-09-11 documentation publication: staged whitespace and scope checks
  passed for 26 documentation/evidence paths; six JSON files parsed and the
  bounded credential-pattern check found no matches. Commit `a498a2c` pushed
  to origin/main successfully; local/remote-tracking revisions matched and
  the working tree was clean before 010b revisions began.

- 2026-09-11 foundations documentation: the system map, README/index, current
  state/history, foundations proposal and readiness checklist were reviewed.
  Independent review found no actionable issue. Local Markdown paths/anchors
  and whitespace checks passed; historical sections were moved mechanically
  with preservation assertions, and live residuals/backlog were retained.
  No application tests, restore exercise, source call or runtime operation was
  performed. New policies remain proposals; the existing working-tree decision
  log and 010b draft were not changed by this milestone.

- 2026-09-11 010a deployment: API/migrate and Web builds passed; private database
  backup catalog checked; additive migration applied; 21 HTTP/auth/asset checks,
  tunnel/realtime checks and public desktop/mobile/member-denial browser smoke
  passed. Business counts unchanged, no FUB connection/assessment created,
  zero API/Web WARN/ERROR entries in the bounded observation. All 25 source
  hashes match prior verification. No full test or benchmark rerun.

- 2026-09-11 source integration: all 25 final code/config hashes and 17 backend
  checkpoint hashes match the verified 010a tree; implementation/merge tree
  comparison is empty. Documentation links and Git whitespace checks passed.
  The remote main ref matched merge `cd224fe` after the source push.
- 2026-09-10 010a: `sqlx-prepare`, `check` (839 Rust, 5 doctests, 889 Web,
  11 worker tests), `check-db` (796 DB tests), query plans and synthetic
  API/browser walkthroughs passed. See [verification](../tasks/SLICE_010a_VERIFICATION.md).
  No live FUB validation or repeat full gate was performed during integration.

## Next recommended action

The [Mobile 002 / 010e2 milestone](../tasks/MOBILE_002_010e2_RELEASE.md) is
implemented, verified, published and deployed. Do not restart its completed lanes.

1. Implement the accepted [Mobile 003](../specs/MOBILE_003_OFFLINE_CONTACT_LOGGING.md)
   and [010e3](../specs/SLICE_010e3.md) contracts under D-078. Use the
   [coordinated plan](MOBILE_003_010e3_PARALLEL_LAUNCH.md) with Terra high and at most
   three worktrees; complete synthetic acceptance and combined gates. Dependent
   families for newly admitted People are following scope. Preserve deferred
   design/physical-phone work and the released runtime.
2. Resume authorized FUB qualification when the user is ready, against an agreed
   dataset and applicable readiness gates. Later data families, mapping repair,
   deltas and activation need their own approved specification.
3. Use readiness C gates to prepare for first real customer data; use V gates for
   the user's later authorized FUB validation. Identify dataset type and satisfy
   applicable prerequisites before connecting. No live call is scheduled here.
4. Review F-01/F-02/F-03 proposals as their triggers approach. First production
   planning owns recovery/service targets, deployment, identity/secrets and
   worker roles. Native support windows and tenant relocation come at their
   respective capabilities; do not expand D-050 now.
5. Existing user-deferred work remains: Slice 017 live sends/walkthrough, 009
   walkthrough steps 3–5, and Telnyx SIP password rotation. See live residuals
   and the owning verification records before acting.

## Approval currently required

- D-078 accepts Mobile 003 / 010e3 implementation and reported contact time.
  No repeat scope/contract approval is pending. Git publication/deployment,
  native distribution, live customer/FUB work and activation remain later scopes.
- D-076 and its explicit release follow-up accept Mobile 002/010e2 implementation
  and the completed commit/main merge/push/cleanup/shared-development deployment.
  No repeat release approval is pending. Native distribution, live FUB/customer
  processing and activation remain separate scopes.
- D-075 approves the 010e1 source-change report, its HTTP/persistence and
  compatibility contracts and implementation. No repeat scope approval is pending. Mobile 001 is already approved under D-074, including its
  operation/reconciliation, origin and local-lifecycle contracts; do not ask again.
  Native SDK license acceptance also has existing authorization.

- D-072 approves 010d2 contracts, implementation and isolated synthetic checks.
  Its explicit follow-up authorized the completed commit, merge, push, cleanup
  and shared-development deployment. Live source/customer processing, body/media
  access, activation and production-cluster deployment remain separate scopes.
- D-070 approves full 010d1 source, lifetime, storage, HTTP and capability
  contracts, implementation and isolated synthetic checks; its follow-up
  authorized the completed commit, merge, push, cleanup and shared-development
  deployment. No repeat authorization is needed for that completed release.
  Live source/customer work, activation and
  production-cluster deployment remain separate scopes.
- D-068 approves the complete 010f2 specification, shared contracts, implementation
  and isolated synthetic checks; its follow-up authorized the now-completed
  Git integration, cleanup and shared-development deployment. Live validation,
  customer-data processing and activation remain separate scopes.
- D-066's complete 010f1 implementation and synthetic verification, followed by
  explicitly authorized Git integration, cleanup and shared-development deployment,
  are finished; no repeat release approval is needed. Notes/tasks implementation
  and release are complete under D-068; activation and live source/customer-data
  operations remain separate scopes.
- D-065 approves 010c's complete workspace/session/import/history/persistence
  contracts and implementation. No repeat specification approval is needed for
  owned implementation detail. Its authorized Git integration, publication and
  cleanup and subsequently authorized deployment are complete. Next-import
  planning is authorized; its specification/implementation, activation and
  source operations retain their own approval boundaries.
- The foundations documentation assignment is authorized; its new architecture
  and policy proposals are not automatically accepted. Their owning specs must
  identify contract changes and acceptance under AGENTS §11/§16.
- 010a cleanup/integration and shared-development deployment are complete under
  D-060. Live FUB validation remains deferred by the user. 010b contracts, implementation and synthetic verification are approved under
  D-063. Its authorized commit, merge/publication, shared-development deployment
  and cleanup are complete. Customer-data processing, live source validation and
  later import/cutover remain outside scope. No repeated release authorization is needed.
- Slice 017 live sends remain the user's deferred action, not a new approval gate.
- Recovery targets, retention/erasure policy details and support-access policy
  remain open in the readiness plan and decision log. No values were invented.
- R1 (auto-hangup of a live call on identity change) remains a later product
  choice, not a blocker for this documentation milestone.
<!-- END PROJECT_STATE snapshot 6e89776 -->

## 010g1 qualified proposals and typed admission — 2026-09-15

Verified implementation checkpoints `d9d2a43` and `108678c`; the combined feature
remains unfinished. Current residuals live in the implementation status.

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

## 010g1 preparation and source classification checkpoints — 2026-09-16

Verified local commits: dispatcher `f760907`, item summaries `bcab991`,
qualified holds `18170d4`, source walk `c882de1`. These were intermediate
checkpoints; the combined family refresh feature remained unfinished.

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
- Qualified history holds: all 29 serial family database regressions, 40 focused
  library tests, API-library Clippy with warnings denied, formatting and diff
  checks passed. The admitted cohort fixture freezes a qualified source with an
  overlapping creation boundary as a counted hold, verifies its encrypted reason
  and absence of native target/head IDs, then proves replay preserves that hold
  after eligibility changes. Capacity rejection and an injected checkpoint fault
  leave no partial unit or charge. This full family run also rechecks item cursor
  isolation and preparation dispatch. Logs: `/private/tmp/010g1-held-family-db.log`,
  `/private/tmp/010g1-held-unit.log`, `/private/tmp/010g1-held-clippy.log`.
  Runners: serial `all family_refresh`, library `family_refresh` tests and
  `cargo clippy -p crm-api --lib --locked -- -D warnings`, using isolation below.
- Bounded history source walk: all 30 serial family database regressions and
  API-library Clippy with warnings denied passed on the final implementation.
  The 40 focused library tests also passed during this milestone. Tests cover
  duplicate identity accounting, source diagnostics, cohort exclusions, storage
  limits, injected transactional failure, forbidden cursor skips and replacement
  plans reusing the bundle index under original encryption scopes. The replacement
  fixture now uses authentic typed preparation after its placeholder ciphertext
  caused an initial test failure. Logs: `/private/tmp/010g1-walk-final-family-db.log`,
  `/private/tmp/010g1-walk-unit.log`, `/private/tmp/010g1-walk-revision-clippy.log`.
  Runners match the preceding milestone; source exhaustion does not seal readiness.


## 010g1 mapping and activity preparation checkpoints — 2026-09-16

Verified milestones through activity traversal at `aaccd81` on
`codex/010g1-family-refresh`; no merge, push or deployment. These checkpoints
are preparation evidence, not complete 010g1 execution readiness.

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


## 010g1 preparation checkpoint details through fad4761 — 2026-09-16

Historical implementation detail through `fad4761`, retained when the current
status was condensed. Later status and accepted contracts take precedence.
Milestones: `c7e456b` missing activity, `fbbb0aa` complete field names, `8c76d5f`
catalog destinations, `fad4761` deterministic mapping representatives.

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
- Catalog destination inspection now authenticates mapping/parent evidence,
  rechecks native snapshots and exact original/admitted registry claims, rejects
  competing field/option targets, preserves prospective IDs, checks native label
  and capacity limits, and requires complete option choices for new fields. It
  requires an already-ready shared catalog registry; typed refresh admission
  still needs to wire its handover. Migration 018 indexes target collision probes.
  This adapter grants no native permission and persists no catalog unit.
- Mapping discovery now preserves first retained tag representative order across
  People for new bundles. Migration 019 freezes capture/item/element traversal
  while preserving UUID traversal for older in-progress bundles; old tag choices
  cannot qualify for catalog execution until a new bundle is prepared. Existing
  immutable evidence and choices are not rewritten.
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

- Catalog destination inspection: all **39** family-refresh database tests
  passed serially (`/private/tmp/010g1-metadata-destination-family-db.log`),
  including real admitted registry reuse/conflict, missing readiness, complete
  option creation, unrelated label-collision isolation, duplicate destinations,
  stable IDs, changed snapshots, native capacity and lease/tenant boundaries.
  Library Clippy passed with warnings denied
  (`/private/tmp/010g1-metadata-destination-final-clippy.log`); formatting and diff
  checks passed. Native execution and persistence of these proposals remain.

- Deterministic mapping order: focused inventory (including both compatibility
  modes), typed Plan/old-tag hold, and combined preparation database tests passed
  serially (`/private/tmp/010g1-mapping-order-db.log`,
  `/private/tmp/010g1-mapping-order-plan-db.log`,
  `/private/tmp/010g1-mapping-order-combined-db.log`). Library Clippy passed with
  warnings denied (`/private/tmp/010g1-mapping-order-clippy.log`); formatting and
  diff checks passed. The preceding full 39-test suite remains the broader
  catalog checkpoint; final-tree verification is still required.

## 010g1 catalog preparation checkpoints — 2026-09-16

- `d1252f2` persisted atomic encrypted catalog outcomes, mapping-owner fences,
  parent-option holds, counts and byte settlement. The isolated serial family
  suite passed 39 tests (`/private/tmp/010g1-catalog-plan-family-db.log`), with
  subsequent focused owner-forgery and dependency cases passing in
  `/private/tmp/010g1-catalog-plan-final-db.log`. Library Clippy passed with
  warnings denied (`/private/tmp/010g1-catalog-plan-final-clippy.log`).
- `a2cf04f` added bounded catalog worker traversal, fixed capture-order
  projections, atomic exact-next cursor advancement and exhaustion fencing.
  The 21 affected database tests passed serially
  (`/private/tmp/010g1-catalog-walk-family-db.log`), including read-only
  native behavior, fault rollback and replay under exhausted capacity.
  Library Clippy passed (`/private/tmp/010g1-catalog-walk-final-clippy.log`).

Both used the isolated `crm_010g1_schema_20260915` database and
`/private/tmp/crm-010g1-target-20260915` Cargo target. Neither checkpoint exposed
HTTP/Web routes, performed native refresh execution, or completed the slice.

## 010g1 typed catalog admission checkpoint — 2026-09-16

`a554871` reused the existing shared catalog handover inside typed refresh
admission, preserving original claim owners and original-payer byte charges.
Failed admission rolls back handover/bundle/receipt together; authorized replay
does not repeat work. Existing admitted metadata preserves its prior transaction
boundary. The family run passed 38/39 before correcting an obsolete inventory
drain bound; the corrected scenario, exact-owner/charge/fault/replay scenario
and two admitted metadata regressions passed in
`/private/tmp/010g1-catalog-admission-focused-db.log`. Library Clippy passed in
`/private/tmp/010g1-catalog-admission-clippy.log`. No HTTP or native refresh
execution was exposed.

## 010g1 Person metadata preparation — c217df7 — 2026-09-16

Person metadata conversion, atomic proposals and bounded source/cohort traversal
are wired into the existing worker. Supplied values require explicit catalog
outcomes; missing/null values retain gap semantics and cannot invent clears.
Owned tag removals preserve alias ownership, numeric comparison uses exact native
scale, and changed native state/revisions hold the Person. Missing-source Persons
receive holds and outside-cohort records receive explicit outcomes. Migrations
022/023 fence typed Person units and exact-next traversal. No native refresh
writes, sealing or confirmation are enabled by this stage.

The family suite ran **41 tests**: 40 passed; one old shared-source fixture was
correctly rejected for inventing an already-current Person without a baseline
(`/private/tmp/010g1-person-metadata-family-db.log`). Correcting it to a held
review unit passed (`/private/tmp/010g1-person-metadata-shared-db.log`). The new
Person scenario passed with rollback, replay, tag removal/addition, a field
update, null/missing gaps, outside/missing Persons and unchanged numeric values
(`/private/tmp/010g1-person-metadata-number-db.log`). Library Clippy passed with
warnings denied (`/private/tmp/010g1-person-metadata-final-clippy.log`); formatting
and diff checks passed. Final slice-wide gates remain outstanding.

## 010g1 sealing and lifecycle — 9796b2c — 2026-09-16

Encrypted native row recipes and bounded keyed sealing established exact ready
counts, digests and ten-minute expiry. Typed Confirm queues exact selected plans
with a permanent capability requirement and replayable receipt. Cancel preserves
completed units and shared payer preparation; Resume explicitly adopts the admin
and atomically replenishes control capacity. Revoked executors pause durably.
The integrated run passed 41/42; the corrected revocation fixture and settlement
path passed its focused rerun, and partial-payer cancellation passed separately.
Evidence: `/private/tmp/010g1-lifecycle-family-db.log`,
`/private/tmp/010g1-family_refresh_revoked_executor-db.log`,
`/private/tmp/010g1-family_refresh_partial_cancel-db.log`, and
`/private/tmp/010g1-lifecycle-final-clippy.log`. No HTTP or native executor was
connected to the scheduler in this milestone.

## 010g1 metadata execution — 1796ea3 — 2026-09-16

Metadata execution now applies closed encrypted native recipes, refresh-owned
catalog claims and atomic whole-Person deltas. Each commit revalidates source,
identity, native state/revision, head and catalog snapshots; native writes,
authenticated after-state, result, head CAS, counts and measured bytes settle
atomically. The bounded execution runner releases each successful lease and can
pause for storage, integrity or release readiness. It currently dispatches only
metadata and is not connected to the application scheduler.

Focused tests verify capacity, native/result rollback and retry, result pagination
and tenant isolation, running cancellation/replay, full completion/count
reconciliation and changed-catalog holds. Evidence:
`/private/tmp/010g1-metadata-execution-db.log` and
`/private/tmp/010g1-metadata-execution-terminal-db.log`.
The integrated family run passed all 45 tests
(`/private/tmp/010g1-metadata-execution-family-db.log`).
Final focused execution checks also pass, including durable release-readiness
pause, explicit Resume and completion
(`/private/tmp/010g1-metadata-execution-final-db.log`).
Library Clippy passes with warnings denied
(`/private/tmp/010g1-metadata-execution-final-clippy.log`); 55 release-preflight
checks pass (`/private/tmp/010g1-metadata-preflight-tests.log`). Formatting/diff
checks pass. HTTP/Web exposure and the final slice-wide gates remain pending.

## 010g1 activity execution — 2ec476a — 2026-09-16

Metadata and activity now execute through the same bounded queue. Notes/tasks
revalidate frozen source conversion, mapped memberships, accepted scans, native
rows/revisions and baseline heads. New activity identities use an exclusive
refresh owner and are discoverable by later bundles. Native writes, identity,
result, after-state, head and accounting commit atomically. Scheduling remains
disconnected while history and the public workflow are completed.

The focused activity checks pass, including capacity, update/addition result-fault
rollback and retry, local-change holds, note conversion and fresh-bundle ownership
recovery (`/private/tmp/010g1-activity-execution-final-db.log`). The integrated run
passed 45/47; the two new tests failed only at their later snapshot setup because
the common reader capability was absent. After adding that capability, both pass.
The corrected-history reader temporarily fails closed until its current-version
projection is installed. Existing imported history review compatibility passes
(`/private/tmp/010g1-activity-reader-compat.log`). Library Clippy passes with
warnings denied (`/private/tmp/010g1-activity-final-clippy.log`); 55 preflight checks
pass (`/private/tmp/010g1-activity-preflight-tests.log`). Final slice-wide gates
remain pending.

## 010g1 common HTTP and history reader checkpoint — 2026-09-16

Metadata and activity execute through the same bounded queue with atomic
native/result/head/accounting settlement. Scheduling remains disconnected.

History review now projects exactly one current version per identity, preserving
its stable entry ID and using the corrected source date and metadata. Missing
current ciphertext fails closed. Bounded immutable version-list/detail readers
and HTTP routes authenticate displays and scope cursors to the actor, workspace,
identity, kind, revision and page size. Capture provenance is available in detail;
message bodies remain outside this display contract. The temporary blanket
corrected-history read block is removed.

The focused synthetic correction regression passes for all three families,
including versions 3/2/1, date-bucket movement, tenant isolation, cursor scope,
missing ciphertext, HTTP validation and no-store responses
(`/private/tmp/010g1-history-versions-final-db.log`). Existing imported-family
review compatibility passes (`/private/tmp/010g1-history-projection-compat-db.log`).
Clippy passes with warnings denied (`/private/tmp/010g1-family-http-final-clippy.log`).
Web transport tests pass 3/3 and typecheck passes
(`/private/tmp/010g1-history-transport-web.log`,
`/private/tmp/010g1-history-versions-web-typecheck.log`). These are reader checks;
history native execution and the final performance gate remain outstanding.

Common HTTP routes now expose Prepare, immutable Plan revisions, Confirm, Resume,
Cancel, bundle/family/mapping/item summaries and results. Typed receipt replay
precedes fresh readiness lookup validation. Strict DTOs and body/page bounds are
preserved; outer workspace middleware also stamps no-store on rejected requests.
The combined admission/API regression passes, including missing-readiness replay,
member rejection and malformed controls (`/private/tmp/010g1-family-http-final-db.log`).
History version member/unauthenticated response checks pass
(`/private/tmp/010g1-history-version-auth-db.log`); Clippy passes with warnings
denied (`/private/tmp/010g1-family-http-verified-clippy.log`). Field/detail and
Remainder routes remain pending with their typed implementations.


## 010g1 native history execution checkpoint — 2026-09-16

All three families now execute through the common bounded queue. History supports
exclusive refresh-owned initial facts and immutable correction versions, with
source/baseline reauthentication inside the final transaction. Result, typed fact,
current head, maintained counts, accounting and cursor settle atomically. Completed
and cancelled bundles release remaining control reservations. Scheduling remains
disconnected pending complete readiness inventory and final verification.

The history Web review shows current corrections and bounded prior-version pages
and metadata details. Authority changes clear private state; late replies are
ignored. Message bodies remain outside the history display contract.

Verified: 49 family-refresh database tests pass, including native history creation,
correction, later-bundle ownership, injected rollback/retry, erasure and exact byte
accounting (`/private/tmp/010g1-history-execution-family-db.log`). Preflight tests
pass 55/55 (`/private/tmp/010g1-history-preflight-tests.log`). Clippy with warnings
denied, formatting and diff checks pass. Web tests pass 18/18, typecheck and focused
lint pass (`/private/tmp/010g1-history-versions-ui-{tests,typecheck,lint}.log`).
Browser checks at 320, 640, 768, 1024, 1280 and 1536 pixels pass without overflow or
page errors (`/private/tmp/010g1-history-browser/checks.json`); mobile and desktop
screenshots were visually inspected. Temporary synthetic browser fixtures removed.


## 010g1 field and common Web checkpoints through af19664 — 2026-09-16


Bounded field inventory and UTF-8 fragment routes now review frozen source,
before/after values, changes and mapping choices without exposing proof envelopes.
Source numeric values retain lossless canonical JSON (including integers beyond
JavaScript's safe range). Fragments are at most 16 KiB, summaries at most 4 KiB;
cursors bind the actor, workspace, bundle, plan, revision, item, endpoint and size.
Current scoped mapping-target suggestions remain subject to typed Plan validation.
Family summaries now include settled counts and retained/reserved/limit bytes.

Native activity review exposes separate refresh provenance for first refresh
owners and later updates. Its page revision includes refresh execution positions,
so new committed work invalidates old page series. Source-only coverage survives
late native holds. The bounded field Web viewer opens from native activity review
and clears private state on authority/revision changes.

Verified: all 49 family-refresh database tests pass
(`/private/tmp/010g1-review-apis-final-db.log`), including field/API/tenant/cursor
checks, exact long Unicode reconstruction, large canonical numbers, mapping
suggestions, activity provenance and source-only reporting. Family unit tests pass
42/42 (`/private/tmp/010g1-fields-unit-final.log`); an existing test-only readiness
initializer missing the family flag was fixed. Clippy with warnings denied passes
(`/private/tmp/010g1-review-apis-clippy.log`). Field/native activity Web tests pass
20/20; focused lint/typecheck pass. Six viewport checks and mobile/desktop visual
inspection pass (`/private/tmp/010g1-fields-browser/checks.json`). The common Web
panel now selects retained sources and families, edits scoped mappings, reviews
exact counts and fields, confirms a frozen request, tracks results, cancels and
resumes individual families. Uncertain responses replay the same request ID/body;
authority changes clear private state. Remainder, scheduler admission and final
gates remain.

Web verification: all 1,325 tests pass across 103 files
(`/private/tmp/010g1-workflow-all-web.log`); the final mapping/progress adjustment
passes all five focused workflow tests. Typecheck, focused lint and isolated
production build pass (`/private/tmp/010g1-workflow-{final-typecheck,final-lint,build}.log`).
The build reports the existing large-chunk warning (MigrationView is now 505 kB).
Combined preview and scrollable confirmation pass six viewport checks with no
page errors (`/private/tmp/010g1-workflow-browser/checks.json`); mobile confirmation
and desktop preview were visually inspected. Synthetic fixture files removed.

