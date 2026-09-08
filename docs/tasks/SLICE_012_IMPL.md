# Slice 012 — Denormalized last-activity columns: implementation brief

**Status: SPECIFICATION APPROVED 2026-09-08 (D-052 recorded); lane started
the same day in `../crm-worktrees/012` on `slice-012-activity-columns`.**
[SLICE_012.md](../specs/SLICE_012.md) is authoritative for every contract;
this brief sequences the work. One backend lane, one writer, worktree
`../crm-worktrees/012` on `slice-012-activity-columns` from `main`. A
sibling lane (Slice 013) works concurrently in `crm-operator/**` and
`crm-api/src/operator/**`; do not touch those paths. Model assignment per
`docs/prompts/MODEL_ROUTING.md`: `implement` profile writes; the coordinator
runs review, test analysis and the once-only final-tree gates.

## Read first

AGENTS.md (§4.6, §4.7, §4.8, §8, §11, §14); DECISION_LOG D-007, D-015, D-022,
D-031–D-033, D-042, D-050; SLICE_012.md in full; SLICE_011a §4c/§4e,
SLICE_011d §5, SLICE_011e §4 (the fourteen statements and the parity
discipline), SLICE_003 §3; docs/design/PERF_BASELINE.md; the two perf archive
READMEs under docs/design/perf/slice-011d-2026-09-07 and slice-011e-2026-09-08;
docs/prompts/05-implement.md. Then inspect the code named below.

## Outcome

Four trigger-maintained `last_*_at` columns on `person`, backfilled once;
every statement that computed those maxima reads the columns instead with
byte-identical results proven before the switch; D-050 evidence shows the
"never" filters and Today's person-state feed no longer scale with history.

## Key files

Migration `crm-api/migrations/20260910000001_person_last_activity.sql`
(new; the 20260909 stamp is taken by 011e). Statements:
`crm-app/src/domain/person/sql/filtered_summaries*.sql` (8),
`crm-app/src/domain/person/queries.rs` (`count_filtered_matches`),
`crm-app/src/domain/today/{source_membership,source_candidates}.sql`,
`crm-app/src/domain/today/system_feeds/sql/{person_state,call_membership,call_only}.sql`.
Frozen texts: new `crm-api/tests/fixtures/statements_b45b04f/` (README + SHA
file, the `today_f51bff8` pattern). Tests: new `db_person_last_activity.rs`
(invariant per write path, concurrency, isolation, no-op backdated,
`updated_at` untouched) and new `db_statement_equivalence.rs` (frozen vs live
text over the rich fixture; reuse the fixture recipe in
`db_today_source_filter_parity.rs`), `db_schema.rs` (triggers, indexes),
registration in `tests/all.rs` (each new file in alphabetical position:
after `db_people_sort` and after `db_schema`; the Slice 013 lane registers
its own file after `db_operator_call`, so hunks never overlap). Perf: a
`perf-harness` feature-gated addition to `db_today_feeds_http_perf.rs`,
kept and committed, seeding history with the batch SQL from
`tests/fixtures/today_http_perf_fixture.rs`.

## Order of work, each step gated by its own tests before the next

1. **Migration**, in the spec's order: columns → three trigger functions
   and triggers → the single backfill `UPDATE` between the
   `-- BEGIN/END PERSON_LAST_ACTIVITY_BACKFILL` markers → the one
   `NULLS FIRST` index; `db_schema.rs` enumerations; the backfill test that
   `include_str!`s the migration, extracts the marked block, seeds history,
   nulls the columns as the migrator role, re-runs the block and asserts
   `column == max(history)` per Person (spec §8.1).
2. **Invariant tests** through the typed commands for every write path,
   concurrency, isolation, backdated no-op (`xmin` unchanged),
   `updated_at` untouched (spec §8.2–8.4, 8.7, 8.8). No statement changes
   yet.
3. **Freeze and prove**: copy the fourteen pre-switch statements into the
   fixture directory as a Rust module in the `today_f51bff8` style (so
   `person_state`'s 55 parameters bind identically on both sides); write
   `db_statement_equivalence.rs` so that it is green with the live text
   still equal to the frozen text (it must compare results, not text).
4. **Read-side switch** per spec §4, one statement family at a time,
   re-running the equivalence test after each; `./scripts/sqlx-prepare`.
   Keep parameter positions, guards and LIMIT identical; the only ORDER BY
   change is `source_candidates`' output-identical `NULLS FIRST` rewrite. In
   `person_state.sql` place the `waiting` gate inside the probe subquery's
   `WHERE`, guard the `latest` LATERAL on both `$5` and `$27`, and keep both
   chains in step. Then `db_today_feed_equivalence.rs`,
   `db_today_builtin_parity.rs` and `db_today_source_filter_parity.rs` must
   pass unchanged; add the explicit pins of spec §8.6 (gated probe, equality
   tie, zero-inquiry exclusion).
5. **Performance evidence** (spec §7) under
   `docs/design/perf/slice-012-<date>/`: one benchmark run; paired
   regression on the four named statements with the three bindings, payload
   equality; the three EXPLAINs taken through `PREPARE`/`EXECUTE` of the
   exact `.sqlx` text, with the `waiting` probe's `loops` recorded; the
   backfill duration; the seed duration before and after triggers (report
   only). Keep the harness behind the `perf-harness` feature gate and commit
   it. README in the 011e archive format with raw outputs. Scratch database
   only, never `crm_dev`, no servers on 3000/5173.

## Rules

Static SQL only; literal Organization predicates; no change to
`PersonFilterParams`, `to_query_params`, any Rust signature, `list_summaries`,
`search_summaries` or `summary_by_id`; no partial indexes unless an EXPLAIN in
step 5 shows a need (report, do not add silently); triggers `AFTER INSERT`
only; no edits under `web/`, `docs/` (other than the perf archive),
`crm-operator/**`, `crm-api/src/operator/**`. Gates per round from the
worktree root: `source ~/.nvm/nvm.sh; ./scripts/sqlx-prepare && ./scripts/check`
then `./scripts/check-db`, run in the background and polled inside one call
(never yield while a gate runs). Checkpoint-commit after each step with the
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` trailer; never
commit on `main`, push, merge or rewrite history; kill only exact PIDs you
started; never claim a check passed unless you saw it.

## Checkpoints requiring the coordinator

- After step 3 (frozen fixtures and a green equivalence test), before any
  statement changes.
- Any need for a partial index, a signature change, or a file outside the
  ownership boundary.
- Any equivalence failure that cannot be explained as a fixture bug.
