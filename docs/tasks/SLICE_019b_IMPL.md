# Slice 019b — Implementation brief

Release follow-up: the user subsequently authorized “commit, deploy, merge
with main and delete loose branches.” That authorization supersedes the
implementation-only release restrictions below. The verification results
remain the evidence for this unchanged source tree.

**Status: APPROVED by the user on 2026-09-10; implement and verify the concrete
specification.** Specification: [SLICE_019b.md](../specs/SLICE_019b.md).
Branch `codex/slice-019b-custom-field-filters`, base `6bad52a`, checkout
`/Users/karrad/projects/crm-worktrees/019b`. One primary writer: Terra, high
reasoning. Astra plans; a separate Astra reviews. Preserve the user's actual
authorization across handoffs; readiness is not approval (D-058 §1, AGENTS §11).

Implementation and verification completed on 2026-09-10 in the uncommitted
worktree. Results: [SLICE_019b_VERIFICATION.md](SLICE_019b_VERIFICATION.md).

## 1. Required reading and implementation order

Read AGENTS, DECISION_LOG, accepted ARCHITECTURE_BASELINE, this specification,
docs/prompts/README and 05-implement; SLICE_011a §4–7, 011b §3–7,
011b_SORT, 011c §3–5, 011d §2–6, 011e §4–9, SLICE_019 §2–7 and UI_STYLE.
Inspect the current paths below; state a short plan and conflicts before edits.

**Part A — backend, acceptance 1–6 and 9–10.**

1. Extend the strict typed model and tests, five-slot params, reference checks,
   deadline variant, descriptions and all closed error mappings exactly as spec.
   Reuse number/text/date validators; checked scaled integers, no new dependency.
2. Extend all fourteen statements / fifteen matrices per spec §4, preserving
   old positions and caps. Add ID-bounded label/reference SQL; do not reuse
   expensive definition counts. Regenerate offline metadata under the gate lock.
3. Add table-driven DB parity/reference/stale/error tests and existing Operator
   saved-list coverage. Freeze only the baseline statements needed for the paired
   D-050 harness at `6bad52a`; implement spec §10's test-only frozen/live query
   wrapper dispatch through the same authenticated request orchestration.
   SQL-only timings do not satisfy request p95. Do not mutate old slice fixtures.
4. Run targeted Rust/DB tests and Rust formatting/lint. Deliver checkpoint with
   exact changed files, outcomes, SQL/binding inventory, risks and commands.
   Coordinator performs review round 1 and releases Part B after required fixes.
   This is a coordination checkpoint, not another human permission question if
   implementation was already approved.

**Part B — Web, acceptance 7–10.**

5. Extend types, filter reader/writer/descriptions and field-ID editor identity;
   wire shared definitions to PeopleView and TodayFeedsView; implement accessible
   custom editors and repairable error messages, preserving existing URL/list
   guards. Update field archive consequence text and cache/settle behavior.
6. Add focused Vitest; run Web lint/typecheck/test/build. Coordinator performs
   review round 2 on Web/integration plus round-1 fixes, then final-tree gates
   and the single performance run. Do not repeatedly benchmark successful code.

## 2. Ownership and exclusions

Terra owns the following implementation files and their focused tests:

- `backend/crates/crm-app/src/domain/person/filter.rs`, `queries.rs`,
  `sql/filtered_summaries*.sql`; `domain/custom_field/{mod.rs,model.rs,queries.rs}`
  only for reusable validation and bounded lookup helpers.
- `domain/saved_list/{error.rs,queries.rs}` and `commands.rs` only where
  necessary for new closed error propagation; `domain/today/{mod.rs,model.rs,
  sources.rs,source_membership.sql,source_candidates.sql}`;
  `domain/today/system_feeds/{mod.rs,error.rs,queries.rs,evaluate.rs,commands.rs}`
  and its three SQL files, only the declared predicate/error/name changes.
- Test-only `domain/person/filter_test_support.rs` (new) and registration in
  `domain/person/mod.rs`; query-wrapper dispatch in the three files named by
  spec §10. `domain/today/test_support.rs` is the existing task-local/HTTP capture
  pattern; a minimal helper addition there is owned only if needed. Existing
  `crm-api/src/lib.rs` app/auth/collector builders and
  `routes/today.rs::router_with_test_clock` are reused without production edits;
  the fixed-arm middleware lives in the new harness fixture. Necessary
  feature-gated result-type visibility belongs to the same query-wrapper files.
- `backend/crates/crm-api/src/error.rs`, `operator/backend.rs` and route modules
  only where needed to map the two codes (no new route); `backend/.sqlx/**`.
- `backend/crates/crm-api/tests/db_custom_field_filters.rs` (new, named prefix
  `db_custom_field_filters`), `tests/all.rs`, related `db_people_filter`,
  `db_saved_lists`, `db_today_source*`, `db_today_system_feed*`,
  `db_operator_filter` tests; a new bounded `fixtures/statements_6bad52a/**` and
  `db_custom_field_filter_perf.rs` harness for spec §10, registered under
  `#[cfg(feature = "perf-harness")]` in `tests/all.rs`. Keep fixtures synthetic.
- `web/src/api/{types.ts,queries.ts}` and tests; `web/src/lib/filter.ts` and test;
  `web/src/components/FilterBar.vue` and test; small dedicated custom editor
  component/test if needed; `web/src/views/{PeopleView,TodayFeedsView,FieldsView}.vue`
  and their tests; existing saved-list/Today issue rendering files where required;
  `web/src/realtime/events.ts` and tests.

Coordinator owns all `docs/**` amendments and verification records, state and
integration. During Part B, the coordinator assigned disjoint files in the same
checkout: primary Terra owns FilterBar and its dedicated custom editor/tests;
second Terra owns query/realtime, People/TodayFeeds/Fields integration and their
tests; coordinator owns filter helpers/tests and saved-count error tests. No
files have concurrent writers; no extra worktree or contract is introduced.
Planner owns only this brief and the spec until handoff. No
concurrent shared-file editing. One lane is enough; no new worktree/delegation
without coordinator assignment. No migration is presently needed; coordinator
must explicitly assign the lane database ownership before an evidence-backed
index migration. Do not edit the 019a migration.

Excluded: Cargo manifests/lock, new dependencies, infrastructure, PersonSummary,
legacy list_summaries, Operator tools/snapshot/new construction vocabulary,
custom sorting, unrelated refactors/cleanup, existing frozen perf fixtures,
commits/merge/push/deployment unless separately authorized.

## 3. Required checks and prerequisites

Use repository scripts, not inferred commands. Fresh worktree requires a
gitignored `.env` populated from the authorized local configuration without
printing secrets; Node/pnpm selected via `source ~/.nvm/nvm.sh`; run
`pnpm install --frozen-lockfile` from `web/` if dependencies are absent.
`./scripts/bootstrap` documents Rust, cargo-nextest and sqlx-cli prerequisites;
sqlx-cli must match the locked 0.8 minor. `./scripts/dev-services up` supplies
PostgreSQL and Centrifugo; `check-db` requires its health endpoint and the
configured MIGRATION_DATABASE_URL. No production or dev-data migration.

Serialize **all DB-backed jobs across all worktrees** using the established
atomic directory lock `/private/tmp/claude-501/crm-gate.lock` (parent directory
must exist). Acquire with `mkdir`, wait/poll without deleting another owner's
lock, release with `rmdir` in a trap on every exit. A stale lock requires
checking its owner/process before recovery. This includes sqlx-prepare, targeted
DB tests, final check-db, fixtures and performance: scripts share fixed
`crm_sqlx_prepare` / `crm_sqlx_prepare_check` database names. Never overlap them.

Targeted backend commands, from `backend/` unless specified:

```sh
cargo fmt --all --check
cargo nextest run -p crm-app --locked -E 'test(domain::person::filter)'
cargo clippy --workspace --all-targets --locked -- -D warnings
```

After query edits, repository root `./scripts/sqlx-prepare` under the shared
lock. For targeted DB tests, mirror `scripts/check-db` step 2: source the root
`.env` without echoing it, export `DATABASE_URL="$MIGRATION_DATABASE_URL"` and
`SQLX_OFFLINE=true`, then from `backend/`:

```sh
cargo nextest run -p crm-api --test all --locked --run-ignored only -E 'test(db_custom_field_filters)'
```

Also run changed existing suites using a single filter expression covering
`db_people_filter`, `db_saved_lists`, `db_today_source`,
`db_today_system_feed`, and `db_operator_filter`; do not silently exclude a
new test module whose name differs. Explicitly list the selected tests before
running if the filter yields zero.

Performance is opt-in and is absent from ordinary `check-db`. Under the same
shared DB lock and exported database variables above, after preparing the
production SQL metadata, run from `backend/` exactly:

```sh
CRM_019B_PERF_OUTPUT="$(mktemp -d /private/tmp/crm-019b-perf.XXXXXX)" \
  cargo test -p crm-api --test all --features perf-harness --locked \
  db_custom_field_filter_perf::slice_019b_authenticated_request_performance \
  -- --ignored --exact --nocapture --test-threads=1
```

`CRM_019B_PERF_OUTPUT` is read only by that test to write its sanitized protocol,
raw timings, comparisons, SQL plans and fixture/hash report; it cannot select a
runtime application query path. The harness creates/cleans an isolated database,
starts its own loopback API and authenticates synthetic users. It reuses local
PostgreSQL/Centrifugo prerequisites; no Vite, external inference or user database
is required. The test name must match the module registration exactly and report
one executed test, not zero. Implement and run spec §10's 11-case matrix (eight
sorts, count, two Today modes; separate serial/concurrency-5 series), including
the plans in the same invocation. Do not launch the historical 011c/011d load
matrix. Record the returned artifact directory and exact result in the handoff.

Web (from `web/`): `pnpm run lint`, `pnpm run typecheck`,
`pnpm run test -- src/lib/filter.test.ts src/components/FilterBar.test.ts`
plus modified query/realtime/view test paths, and `pnpm run build`.

Coordinator's final delivered-tree gates, root, once after final fixes:
`./scripts/sqlx-prepare`, `./scripts/check`, `./scripts/check-db`, and
`git diff --check`. Full check includes Rust fmt/clippy/build/crate fences,
unit tests/doctests, Web lint/typecheck/Vitest/build and email-worker tests.
Run long commands with yielding tools; on macOS use
`perl -e 'alarm shift; exec @ARGV' 1800 ./scripts/check` if a bound is needed,
not unavailable GNU timeout. Never call a skipped or aborted gate passed.

## 4. Walkthrough and handoff

One synthetic browser session per role: admin creates/uses Budget, Anniversary,
Referrer and Lead temperature (or existing synthetic 019a fields); member
filters two text fields independently, combines a budget/date range and choice,
saves/opens a list and adds it to Today. Change/clear a value and verify the
list/count/Today refresh. Archive a held option: matching survives; archive the
field: saved-list repair/Today issue appears; restore recovers. Admin previews
a rule with its anchor intact. Check keyboard-only Apply/Escape/focus, a narrow
viewport, and Organization switch showing no prior criteria/labels. Operator
runs the saved list and treats a synthetic instruction-like description as data.

Handoff: base/current commit; exact tracked/untracked file list; criterion →
test mapping/results/tree; all fourteen statement bindings; scripts run and
counts; perf archive path and pass/fail; browser evidence; review round consumed;
unresolved limits; actual model/effort and unavailable cost measurements clearly
marked. Coordinator updates source-spec amendment pointers from spec §8 only
after approval, records final status and asks only for any truly remaining gate.

## 5. Planning review dispositions

Round 1: **READY WITH FIXES**, applied to this draft; not user approval.
Targeted round 2 (2026-09-10, independent Astra high): **READY**. Both
blocking corrections and the documentation correction below are resolved;
no remaining blocking finding or additional product decision. Review inspected
the specification and code; compilation, tests and performance remain future
implementation checks. The two planning review rounds are complete.

- **R019B-01 (P2 CONTRACT):** corrected preview precedence to structure →
  subject membership → references/rules from the current implementation;
  acceptance 3 pins combined-invalid 404/422 ordering. Stale helper-comment
  correction is explicitly allowed when implementing that file.
- **R019B-02 (P2 EXECUTION):** replaced SQL-only comparator ambiguity with
  test-feature task-local frozen/live statement dispatch in current authenticated
  orchestration; specified hook ownership, perf-only registration, warmups,
  sample/case matrix, request p95 calculation and exact invocation. No production
  switch, old product clone or dependency change.
- **N019B-01 (documentation):** removed nonexistent `count_people` tool and
  `saved_list/model.rs` ownership; identified `list_saved_lists` as a domain
  metadata query, with actual Operator tools `filter_people` and `run_saved_list`.

Targeted planning round 2 verified these corrections. The user then approved
implementation and verification. Implementation review has the two-round budget
described above; no further planning approval is pending.
