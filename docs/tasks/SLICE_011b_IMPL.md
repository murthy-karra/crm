# Slice 011b — Saved lists implementation brief

**IMPLEMENTED AND VERIFIED LOCALLY — approved by the user on 2026-09-06.**
The user approved the reviewed specification after reading its plain-language
companion. D-046's privacy and limits remain binding. Independent Astra/ultra
re-review returned READY; A1/A2 and coordinator R1–R3 are resolved.
Implementation branch: `codex/slice-011b-saved-lists`, based on `1635fc4`.
Configured implementation launch: `gpt-5.6-terra` / `ultra`, task
`/root/slice_011b_implement`. Independent reviewer: `gpt-6-astra` / `ultra`.

Review dispositions: A1 adds command-local filter-version validation; A2 forces
working People/count refresh despite fresh unchanged cache. R1 preserves dirty
drafts after Duplicate; R2 defines uncertain-create abandonment/reload limits;
R3 applies the four-count bound to every retry/refetch path. Each has required
acceptance coverage in spec §9; no finding remains open.

## Outcome and authority

Implement the approved [SLICE_011b.md](../specs/SLICE_011b.md): an agent saves
current People criteria, returns to dynamic matches, privately maintains their
lists, and copies administrator-curated shared lists. Match all numbered
acceptance criteria in spec §9. Read AGENTS.md, DECISION_LOG (D-043/045/046),
ARCHITECTURE_BASELINE, the 011 ladder, amended SLICE_011a, and
`docs/prompts/README.md` / `05-implement.md` before editing. Inspect the actual
branch and runtime; the planning baseline was clean `main` at `1635fc4`.

User assignment: **Terra / ultra for implementation; Astra / ultra for
coordination, specification and independent review.** Configure the actual
runner when dispatching and record the returned launch settings; Markdown
does not switch a model. The specification agent's launch was configured
`gpt-6-astra` / `ultra`. Do not silently substitute another implementation model.

Use one short-lived `codex/slice-011b-saved-lists` branch based on the current
approved main, created after the gate. One primary writer. Following completed
verification, the user approved local commit and merge on 2026-09-06. Push,
deployment and unrelated process/service mutations remain outside the scope.

## Ownership and exclusions

The single implementation lane exclusively owns:

- New `backend/crates/crm-app/src/domain/saved_list/` and the minimal registration
  in `domain/mod.rs`, `ids.rs`; one new count projection in
  `domain/person/queries.rs`, using existing `PersonFilterParams`.
- One migration under `backend/crates/crm-api/migrations/`; this lane is the
  **sole database owner**. New `.sqlx` files; existing summary SQL/cache stays.
- `crm-api/src/routes/saved_lists.rs`, route registration, error mappings and
  only necessary `lib.rs` wiring; focused new tests and necessary consolidated
  test-module registration in `crm-api/tests/`.
- `web/src/api/{types,queries}.ts`, `lib/errors.ts`, router/navigation, new
  Lists index/dialog components and a bounded extension/extraction around
  PeopleView. Reuse existing FilterBar/table/PersonPreview. Necessary client
  invalidation mapping and adjacent tests are in scope. Use installed PrimeVue,
  Vue and TanStack Query; no dependency changes or design-system rewrite.

The coordinator owns shared docs (`SLICE_002`, `SLICE_003`, `SLICE_011a`
amendment pointers, ladder, project state and this spec/brief). The implementer
reports exact amendments needed; it does not concurrently edit those files.
If a proposed edit is outside these boundaries, report the concrete need before
expanding. Read-only review agents do not write production files or run a
second DB-backed suite in the same checkout.

Execution refinement, 2026-09-06: the primary Terra / ultra implementer may
delegate remaining test-only work to its inherited Terra / ultra helper while
building Web. The helper exclusively owns `crm-api/tests/db_saved_lists.rs`
and `crm-api/tests/saved_lists.rs` until handback; the primary writer does not
edit them concurrently. The helper owns no production code, migrations or
SQLx cache and runs no DB suites. Database execution remains serialized under
the primary implementer. This retains one implementation lane and its sole
migration owner; independent acceptance review remains Astra / ultra.

After handing back the backend test files, the helper prepared only temporary
QA setup scripts, without executing them. Its next exclusive writing boundary
is the new `web/src/api/savedListCounts.test.ts` file for deferred scheduler
tests. The primary implementer retains PeopleView, all production code, other
tests and all database/process execution. This delegation adds no second
production writer.

No Today/Operator/mobile work, new filter axis, sorting, People search,
in-place scope conversion, personal-list admin access, snapshot membership,
realtime list event, general idempotency service or speculative refactor.

## Execution sequence and contract ownership

1. Re-read the approved spec and inspect actual APIs/schema/UI. State the short
   implementation plan and any genuine authority conflict. This lane owns the
   approved new saved-list schema/HTTP/commands, but does not own unrelated
   shared-contract changes. §8's declarations must remain explicit.
2. Implement typed data/validation and migration. Use existing `sha2` for the
   creation fingerprint, one table with content-cleared tombstones, separate
   scopes/caps and current-membership checks. Keep retry lookup before cap
   enforcement and Organization/actor in every authorization boundary. Explicitly
   reject a direct Rust command's unsupported `filter.version` before calling
   the existing clause validator; HTTP decoding alone is insufficient.
3. Implement typed commands, static metadata/detail/count reads and route/error
   envelopes. Pin no lost updates, cap races, nonleaking IDs, invalid stored
   definitions and no resurrection. No new path in Operator or raw DB UI.
4. Regenerate `.sqlx` with `./scripts/sqlx-prepare`; verify existing summary
   query entries are unchanged. Add focused service-free and DB tests for
   spec §9 items 1–9 and 14 before integrating Web flows.
5. Add the index (25 definitions/page, four count requests maximum), dialogs,
   and named-list workspace. Existing `/people` remains fully functional.
   Keep baseline/working/latest-server states separate. Preserve D-045
   inspector, 011a editor/URL/cancellation behavior and actor/org privacy. The
   four-count scheduling gate covers retries and all invalidations, not only
   initial load. Named-list focus/navigation/Refresh must force the current
   working People query even with an unchanged filter and fresh 30-second cache.
   Cover spec §9 items 10–13 with meaningful router/component/query tests.
6. Complete telemetry/performance evidence and final-tree gates. Provide the
   coordinator the amendment pointers and criterion→test mapping, with actual
   changed-file inventory including generated/untracked files.
7. Conduct the live browser walkthrough with coordinator-authorized local
   runtime preparation and synthetic data. Then independent review using
   `06-verify-and-review.md`; fix actionable findings and rerun checks affected
   by those fixes, plus any explicit final-tree gate requirements.

M-size boundary: the implementation is one lane. If evidence shows it exceeds
M, stop expansion and propose the spec §10 seam (backend/API/counts then Web).
Do not add Today or sorting, weaken acceptance, or silently drop retry/privacy
behavior to hit the size target.

## Required checks and prerequisites

Read current README/scripts for environmental details. Rust workspace is
`backend/`; Web package is `web/`. Load the existing nvm environment when needed
(`source ~/.nvm/nvm.sh`) so Node/pnpm match the repository. Existing toolchains
and installed dependencies are assumed; no new installation is part of scope.

```sh
git diff --check
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

`sqlx-prepare` and `check-db` require documented local PostgreSQL/Centrifugo
services and gitignored `.env`; do not expose its values. `check-db` uses
throwaway databases and includes prepare-check plus the complete ignored DB
suite. **Run database-backed operations serially in this checkout.**
`check` is the service-free gate (format, clippy, production-shape check,
dependency fences, unit/doc tests, Web lint/typecheck/tests/build and worker
tests). Run both full gates on the final implementation tree and report actual
exit results. No telephony or live model test is newly required.

Targeted checks should exercise the new failure and security invariants, not
merely mirror implementation. Use existing DB harness fixtures and deliberate
barriers/locks for races. Create synthetic 50k-Person fixtures in a throwaway
database and record dense/sparse count `EXPLAIN (ANALYZE, BUFFERS)` alongside
existing filtered-query results; compare with
[PERF_BASELINE](../design/PERF_BASELINE.md). Never seed, benchmark or erase the shared
development dataset for this evidence. No arbitrary latency promise is claimed.

Live QA must actually operate the browser: save/reopen, dynamic membership,
two-viewer `me`, admin inability to access another personal list, shared-copy
independence, same-tab and two-tab conflicts, delete retention of People,
offline/retry, keyboard/narrow screen/inspector, missing/invalid definition and
cross-Organization link. Record the exact tested build/API process so the old
orphaned-dev-api hazard cannot invalidate the walkthrough. The coordinator authorizes temporary, isolated local QA processes and synthetic
throwaway data needed for this walkthrough. Do not replace an unrelated running
service or modify the shared development dataset.

## Handoff requirements

Report actual branch/commit/tree, effective launch settings, exact changed
files including migration and SQLx cache, spec criteria mapped to test names
and live evidence, commands with results, conflict/retry/privacy findings,
performance evidence, unresolved deviations and review outcome. Claim neither
approval nor completion from compilation alone. Unavailable usage/cost data
remain unavailable; do not estimate a billed cost from model settings.

Current next action: execute the user-approved local commit and merge.
No implementation work remains in this slice.

Current evidence: [SLICE_011b_VERIFICATION.md](SLICE_011b_VERIFICATION.md), including
the isolated runtime, live observations, automated checks and acceptance map.

## Implementation result (2026-09-06)

Terra implemented the migration, typed commands/reads, capped count query,
HTTP routes, Lists index, save/copy dialogs and named People workspace.
Backend I1–I4, Web AW1–AW7 and coordinator browser findings are resolved.
Independent source and measured-performance review found no remaining
actionable finding. The coordinator completed the synthetic browser walkthrough,
including lost-response and outage checks through an isolated loopback proxy.

Terra's final `./scripts/check` passed in 30 seconds (651 Rust tests,
5 doctests, 388 Web tests and 9 email-worker tests, plus lint/type checking and
build). The subsequent `./scripts/check-db` passed in 139 seconds, including
the schema/cache check and all 379 DB tests. The saved-list subset contains
16 DB tests. Existing People summary functions and all 147 prior SQLx files
are unchanged; 13 new SQLx files support this slice. The 50k dense/sparse
query evidence is retained in the repository with its limitations.

The temporary inherited Terra helper handed back its bounded test files and
script preparation; database execution remained serialized under the primary
implementer. Independent acceptance review remained Astra / ultra.
Local commit and merge are approved; push and deployment are not included.
