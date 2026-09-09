# LATER batch (2026-09-08) — Verification record

Coordinator-owned evidence for [LATER_BATCH_2026-09-08.md](LATER_BATCH_2026-09-08.md)
under the D-050 budget. Branch `chore/later-batch-2026-09-08` from `main` at
`ae680fa`, worktree `../crm-worktrees/later-1`, one lane (Claude Sonnet 5),
coordinated by Claude Fable 5.1.

| Commit | Item | Content |
|---|---|---|
| `2482b4c` | 2 | The `db_calls` flake was a test asserting strict order on `recorded_at` alone; the history query already tie-breaks on `id` (SLICE_002 §5). Test compares the `(recorded_at, id)` tuple. No statement or `.sqlx` change. |
| `c18d7e6`, `1b488a9` | 1 | Migration `20260911000001_inquiry_append_only.sql`: `reject_direct_mutation()` rejects every UPDATE and any DELETE at `pg_trigger_depth() <= 1` (a hand-run statement's own trigger runs at depth 1), allows the `person` cascade (depth 2, inside the RI trigger); TRUNCATE bound to `reject_mutation()`. Five tests in `db_inquiry_append_only.rs`; `db_operator.rs` backdates by direct INSERT with the hand fix-up removed; `db_schema.rs` enumeration. |
| `b4a4a04` | 3 | `db_calls.rs` → `db_calls` / `db_calls_corrections` / `db_calls_outcome_today` (30+13+6 = 49); `db_saved_lists.rs` → `db_saved_lists` / `db_saved_lists_sort` / `db_saved_lists_tags` (16+13+3 = 32); `db_today_system_feed_commands.rs` → itself / `db_today_system_feed_evaluation` / `db_today_system_feed_preview` (11+11+8 = 30); shared fixtures moved to `tests/common/{calls,saved_lists,today_system_feed}.rs`; test-name sets identical per group; whole-binary multiset 729 = 729; no test body changed. |
| `cbdcbc7` | 4, 5 | `onSuccess` writes only the mutated field for stage and assignment; `mutationKey: ['person-mutation', orgId, personId]` on the four Person mutations; the settle-invalidate skipped while `isMutating` for the same Person is greater than one (the calling mutation counts itself because TanStack v5 runs `onSettled` before the success state change); the realtime flush marks People/Person queries stale with `refetchType: 'none'` while any Person mutation is pending. |
| `28a3bf1` | fixes | Round-1 fixes (below); lane gates `check` 757 Rust / 656 Vitest, `check-db` 622 of 622 first run |

Two bugs the lane caught in its own first attempts, before landing:
`pg_trigger_depth() > 0` would have let every delete through (any trigger
already runs at depth 1); `isMutating > 0` would have skipped every settle
invalidation. Both are documented in code and commit messages.

Coordinator file-list audit: 22 files (11 new, 11 modified), all under
`crm-api/tests/`, `crm-api/migrations/` and `web/src/`; no signature or wire
change; `sessionLifecycle.ts` untouched; no new dependency.

## Lane gates (own tree)

`sqlx-prepare` no diff; `check` green (757 Rust, 653 Vitest, build); `check-db`
622 of 622 on the second run. The first run stopped at 609 of 610 on
`db_today_system_feed_evaluation::call_feed_customized_with_a_stage_clause_narrows_evaluation_to_that_stage`
(a moved, unchanged test), which passed 6 of 6 in isolation and on the rerun;
classification in the review below.

## Review round 1 (of two) on `cbdcbc7`

Reviewer READY WITH FIXES; tester one BLOCKING finding. Verified by both:
`pg_trigger_depth() > 1` is exactly the cascade boundary (a top-level
statement's row trigger runs at depth 1; the RI cascade's child-row trigger
at depth 2); the split is byte-faithful (111 annotated tests, identical
sets per group, fixtures moved not altered, no shared state introduced);
TanStack v5 runs `onSettled` before its success dispatch so the calling
mutation counts itself (verified in `mutation.js`); the rapid
stage-then-assignee test is non-vacuous; the once-failed
`db_today_system_feed_evaluation` test has an unchanged body, no shared
fixture state, and passes in isolation and with its sibling files
(pre-existing load flake). Applied in `28a3bf1`:

- **regression (item 5):** two sibling mutations for the same Person
  settling in one microtask flush each saw the other as pending and both
  skipped the invalidation; the decision is now taken after the mutation's
  own state flips (a deferred check invalidates when the Person's mutation
  count is zero), and the same check releases the realtime hold by refetching
  active stale queries under the Organization prefix from all four mutations,
  so a tag settle or an error rollback no longer leaves a held stale row;
- **item 2 reverted to the spec-backed strict assertion:** a correction's
  `recorded_at` is stamped after the call lock and is strictly later than
  its head (SLICE_006c §2); the tuple comparison was tautological; both
  values now appear in the failure message; the flake stays open,
  unreproduced in 21 runs;
- **item 1:** `TRUNCATE inquiry` asserted rejected; a depth-2 delete
  additionally requires the parent Person to be gone, so only a genuine
  cascade passes; the migration header states that no trigger may delete
  from `inquiry`;
- **item 3:** a duplicated `busy_call` fixture removed.

## Recorded LATER (D-050)

- An in-trigger `DELETE FROM inquiry` would run at depth 2; none exists and
  the header forbids one, and the parent-gone check now also applies.
- Same-field rapid pairs (stage A then stage B, A's response last) show A
  until B settles (BEYOND_ENVELOPE; one tab).
- The realtime hold is Organization-blind (only one Organization is active
  at a time).
- `useLogContactMutation` still writes the whole `person` and has no
  mutation key (same race class, outside this batch's four).
- The `db_calls` correction-ordering flake: not reproduced in 21 runs,
  assertion kept strict with diagnostic values; still open.
- The `db_today_system_feed_evaluation::call_feed_customized_with_a_stage_clause…`
  load flake: one failure in a full run, passes isolated and with its
  siblings; the fixture mixes Rust `Utc::now()` with database `now()`, a
  possible clock-skew sensitivity; capture the failing assertion text next
  time.
- `db_saved_lists` and `db_today_system_feed_commands` registrations in
  `tests/all.rs` were already out of alphabetical order before the split
  and were left as found.

## Final-tree gates (coordinator, once, on `28a3bf1`, 2026-09-08)

Run by the coordinator from the worktree root under the shared gate lock, in
one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; no metadata change (no statement text changed) |
| `./scripts/check` | all checks passed, 32 s: fmt, clippy, cargo check, crate fences, **757** Rust tests, doc tests, Web lint/typecheck/**656** Vitest/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 203 s: **622 of 622** DB-backed tests on the first run; neither known flake occurred |

## Merge readiness

Source `chore/later-batch-2026-09-08` at `28a3bf1` plus this record;
destination `main` (docs-only commits ahead of the branch base `ae680fa`:
the brief's status and outcome lines and project-state records, so no code
conflict is possible). Migration impact: one additive migration
`20260911000001_inquiry_append_only.sql` (two triggers and one function on
`inquiry`; no data change); `crm_dev` must be migrated with
`./scripts/db-migrate` after the merge, with the user's approval; the dev API
needs no restart for a trigger-only migration but the production web server
(`dev-web-prod`) must be rebuilt and restarted for the Web changes. Unresolved
risks: the LATER list above. Push and deployment are not authorized by this
record.
