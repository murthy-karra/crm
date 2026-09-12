# Slice 010c — Execution brief

**IMPLEMENTED, VERIFIED, MERGED AND DEPLOYED (D-065 and follow-ups).**
2026-09-11, main baseline `c6c5930`, deployed 010b source `89471f0`.
The user approved the reviewed specification, brief and shared contracts after
accepting three policy choices in D-064. [Spec](../specs/SLICE_010c.md),
[code findings](../research/SLICE_010c_CODE_CONTRACTS.md),
[independent review](SLICE_010c_REVIEW.md).
The complete review returned READY-WITH-FIXES; all five corrections received
targeted READY confirmation. Planning and documentation checks are complete.
Implementation `fcd2480` and merge `c3f6ca9` are published to main; the merged
worktree/branch and disposable QA resources are removed. See
[verification and integration](SLICE_010c_VERIFICATION.md) and
[shared-development release](SLICE_010c_RELEASE.md). The brief below
preserves the approved scope and required checks.

## 1. Outcome and boundaries

Build a frozen, reviewable People/contact/stage/assignment import from retained
010b raw evidence into a new empty Organization, with duplicate-free recovery
and per-record reconciliation. Enter a durable admin review workspace before
the first business write. Separate FUB People remain separate, stage creation
requires explicit approval, and ordinary agent use waits for later activation.

No new FUB request, source mutation, populated-destination merge, synthetic
Inquiry/contact history, note/task/tag/custom-field import, cutover/activation,
destructive reset, native application, generic worker platform or storage vendor
is included. Live FUB validation remains user-deferred. Use synthetic data only;
the review hold does not waive customer-data readiness gates.

## 2. Required reading and approval boundary

Read AGENTS.md; DECISION_LOG D-004/005/007/012/015/019/050 and D-059–064;
architecture baseline; current migration summary; the complete 010c spec and
code record; 010b spec/contract and source qualification. Read the existing
commands/queries, facts, migrations, Operator, workers, Web session handling
and test precedents named in the code record before editing them.

Under AGENTS §11, full 010c approval owns the following changes: Organization
mode/session payload and authoritative read/write permits; bounded Operator
admission; scoped import/stage commands and INSERT grant; deterministic contact
order across shared readers; import facts/reasons and erasable provenance;
import-owned storage accounting; additive import HTTP/Web contracts. All existing
Organizations default operational. Do not change source profiles, intake identity
matching, Inquiry-source filter meaning or general Person visibility policy.

Before coding, freeze concrete DTOs/error/state transitions, cursor/encryption
purposes and counted variable-byte columns within those approved semantics.
Reconcile the gate/table inventory with the implementation head. If that reveals
a policy conflict or requires a contract outside this ownership, report it under
AGENTS §11 before the dependent change; do not discard fidelity to fit existing code.

## 3. Ownership and sequence

After approval, use one primary implementation writer on one short-lived
`codex/slice-010c-people-import` branch/worktree from current main. This slice is
larger than a table-copy operation because review mode crosses existing domains.
Keep the work sequential and review the backend boundary before building Web.
Follow the actual assignment and MODEL_ROUTING; this document does not itself
start agents or change models.

The primary implementation lane owns the sole additive database migration,
generated SQLx metadata and all shared integration files. Two bounded support
assignments refine execution ownership: `source_profile` owns only the new pure
`import_source.rs` extractor, `import_display.rs` pure display/segment helpers,
their inline tests, the shared synthetic fixture `tests/fixtures/import_support.rs`
and `db_import_source.rs`/`db_import_gate.rs`, and the standalone `scripts/migration-release-preflight`
with its synthetic `scripts/tests/test_migration_release_preflight.py`;
`snapshot_harness` owns only
the new `import_qa.rs`, `db_import_contact_perf.rs`, `db_import_plans.rs`,
`db_workspace_background.rs` and
`tests/fixtures/import_contact_c6c5930/` files. The primary owns registrations
and shared test seams. The coordinator owns existing specs/current-state pointers,
private browser scripts, review evidence, the new `db_workspace_http.rs` guard
tests and `db_import_http.rs` HTTP acceptance tests, and final integration; the primary owns
the new concrete backend contract. No writers overlap. Do not overlap DB gate
runs or introduce another database-owning lane. These execution assignments do
not alter approved product or contract scope.

After backend review/fix confirmation, the [Web execution boundary](SLICE_010c_WEB.md)
assigns new import API/components/tests to one support writer while the primary
retains all shared session/navigation/Person integration. Its concrete props,
scope keys and checks preserve this sequence and avoid overlapping writers.

| Step | Work and checkpoint | Required proof |
|---|---|---|
| 1. Freeze source/contract | Qualified accepted raw item extractor, exact ID/HMAC verification, contact/primary normalization fixtures, mappings and held reasons, field paging, immutable plan/HTTP/schema inventory | No projection-derived values, no fabricated Inquiry, all source variants considered; D-064 decisions reflected |
| 2. Workspace foundation | Mode/binding and shared/exclusive permits; concrete table emptiness checks; guard ordinary commands, raw ingress and query assembly; Operator admission/absolute deadline; terminal cleanup allowlist | Deterministic entry-vs-read/write/intake/call/Operator races, unchanged operational behavior, no DB transaction across network, no role/cache/CLI bypass |
| 3. Plan and import | Bounded encrypted manifest revisions, explicit mappings/stage creation, source-ID identity map, per-Person atomic commits/facts/provenance, leased/fenced worker and import reservations | Confirm/replay, byte admission, missing keys, active-member revalidation, rollback/lost response, cancel/explicit resume and source-cancel independence |
| 4. Backend checkpoint | Add routes, closed DTOs/errors and bounded query/field pagination; finish targeted backend/DB tests; first bounded implementation review/fix round | Complete gate inventory, exported contract for Web, no unresolved authority/fidelity/atomicity defect |
| 5. Web | Plan/choices/counts, explicit subset confirmation, held reasons and coverage, durable progress and provenance; mode-aware shell/router/caches/Operator/call owner; admin review/member waiting screen | Real API fixtures, stale responses, actor/mode changes, no operational controls while held, desktop and 390px Web |
| 6. Final verification | Second bounded review/fix round, final full gates, indexed query plans, one paired hot-reader regression, synthetic production-build browser walkthrough | Exact final-tree evidence, gaps retained and implementation approval separated from release authorization |

Step 1 qualifies stage/user supporting evidence under the same exact raw rules
as People, establishes native text/index eligibility and proves each item's
bounded added retained bytes before readiness. Step 2 allows terminal IDs-only
Operator audit while excluding active admissions/actionable work. Step 6 also
verifies the server/worker/CLI compatibility and rollback boundary in spec §7.

## 4. File ownership and concrete implementation seams

- `backend/crates/crm-app/src/domain/migration/`: new import plan/extractor/worker/
  commands/queries, reuse existing lossless parser and retained-data crypto;
  necessary reservation/budget integration without changing 010b source authority.
- Application auth/workspace permit module and all existing command/worker/query
  entry points in the [gate inventory](../research/SLICE_010c_CODE_CONTRACTS.md#3-review-mode-inventory-and-entry-check).
  This expressly includes autocommit routing settings, capture-address GET,
  extraction lock order, direct call dialing and Operator single-use confirmation.
- Person/contact and stage persistence, facts/envelopes, all primary-contact
  projections including Today and Operator; additive import provenance queries.
  No general stage CRUD or refactoring of Today ranking is authorized.
- One unused-version migration under `backend/crates/crm-api/migrations/` and
  generated `backend/.sqlx/` metadata; composite tenant FKs, source identity,
  append-only receipt rules, minimum grants and claim/keyset indexes.
- API session/extractors/error mapping, migration/Person routes, state/startup,
  Operator admission and terminal audit integration, realtime issuance and tests.
  Do not retrofit the existing append-only Operator ledger as a pending queue.
- Web API types/queries, migration review components, Person provenance section,
  session/router/AppShell/call/Operator lifecycle and focused tests. Reuse existing
  UI conventions; responsive Web does not implement native SwiftUI/Kotlin clients.
- Focused Rust/DB/HTTP/Web tests and disposable synthetic walkthrough harness.
  No production route or environment flag may activate a fake FUB source reader.
- Release/admin launch preflight and its tests for the spec's compatibility
  boundary: inspect persistent review bindings, require known compatible artifacts
  and retire all pre-gate API/worker/CLI processes before enabling confirmation.
  Amend owning release guidance with the rollback restriction. No runtime rollout
  is authorized during implementation; use synthetic state/artifact fixtures.

List actual changed files at each checkpoint; compare that list to Git status.
No unrelated cleanup/dependencies/services. Preserve source/preview compatibility
and historical raw rows. Schema preparation is additive; never use bootstrap/reset
against shared `crm_dev` or perform a release from this implementation brief.

## 5. Verification contract

Build a checked matrix linking each concrete read/write/background seam to its
held and operational test, including direct-domain invocation. Tests must prove
the behavior, not merely that a guard helper was called. Exercise alternate
mutation variants (Undo/archive/retry/discard) and narrowly allowed governance/
terminal cleanup. Use deterministic barriers to control concurrent entry, member
read assembly, intake raw commit, call admission and Operator scheduling; include
pool acquisition/lock timeouts and expired/crashed admission recovery.

Source/import tests cover rejected/truncated/foreign captures, exact large IDs,
ordinal/HMAC mismatch, all comparable variants, invalid values, named contactless
People, contact dedup/order, shared household/office numbers, Trash, unknown flags,
stage collision/capacity, active-member changes, changed confirmed plans and
unavailable targets. Interrupt before and after each per-Person atomic commit;
prove unique identity/facts and accounting on retry. Confirm/cancel races retain
already committed data and the review mode. Missing keys/budgets pause before
writing; increases do not auto-resume. Check terminal states and expiry/replan.

Use small synthetic limits for exhaustion tests. D-050 data is 25,000 People,
50 members and dense shared-contact groups: indexed preparation/claim/source-map/
mapping/result/field pages with bounded memory and no all-pairs overlap storage.
Inspect `EXPLAIN (ANALYZE, BUFFERS)` for relevant query shapes. Contact ordering
changes hot People/Today readers, so one paired baseline/final regression check
under D-050 is required, including five concurrent Today loads. Do not expand
the supported envelope or repeatedly rerun benchmarks without a changed concern.

During development run focused library/Web tests and filtered DB/HTTP cases.
After both bounded review/fix rounds, run on the final tree sequentially:

```sh
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

Use current repository scripts/runtime. No simultaneous DB runs and no secrets
in command arguments/logs/fixtures. Record exact commands/results, failures and
counts rather than reusing earlier slice totals.

Browser evidence uses the production Web build and real API with synthetic
retained FUB evidence. Exercise initial plan and partial mapping patches, explicit
confirmation/replay, import progress/reload, held records, long-field inspection,
pause/resume/cancel, budget recovery, cross-Org denial and role/mode changes on
desktop and 390px. Prove no new FUB request or outbound call/model operation is
made by the import. Verify member waiting/logout and admin read-only inspection.

Create `docs/tasks/SLICE_010c_VERIFICATION.md` during implementation with actual
changed-file hashes, migration/grant review, gate matrix, checks/query plans,
browser screenshots, unresolved issues and separate live-validation status.
No such implementation test result exists during planning.

## 6. Handoff

Independent review corrections add explicit negative cases: conflicting/truncated
stage/user support, embedded NUL and poorly compressible near-4 KiB indexed values,
successful >2 MiB provenance admission within its proven bound, intrinsic over-unit
holds before confirmation, terminal-audit-vs-active-Operator eligibility, and
rollback rejection for old/unknown artifacts once a review binding exists.
Preserve accepted source data and ordinary normalizers. The fixed 2 MiB execution
reservation is not part of the revised plan; admission is per item with exact
settlement. See the review record for C010-R1–R5.

The [review record](SLICE_010c_REVIEW.md) owns the actual independent plan verdict
and finding dispositions. D-065 now approves the full spec and implementation.
Commit/merge/push/deployment, live FUB calls and real customer-data processing
remain outside this implementation assignment. Later
activation must resolve communication restrictions, privacy/audience, remaining
data/history, Today behavior, deltas and reconciliation before releasing the hold.
