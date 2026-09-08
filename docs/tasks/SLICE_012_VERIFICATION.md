# Slice 012 — Verification record

Coordinator-owned evidence for [SLICE_012.md](../specs/SLICE_012.md) §8 under
the D-050 budget (two review rounds at most; performance gated on paired
regression and plan shape only).

Branch `slice-012-activity-columns` from `main` at `61b08ac`, worktree
`../crm-worktrees/012`, one lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1, run in parallel with the Slice 013 lane (gate runs serialized by a
directory lock; the only shared file is `tests/all.rs`). Commits in order:

| Commit | Content | Lane gates (own tree) |
|---|---|---|
| `a503640` | Migration `20260910000001_person_last_activity.sql` in the reviewed order (columns → three `AFTER INSERT` triggers → marked backfill block → one `NULLS FIRST` index); `db_schema.rs` pins; `db_person_last_activity.rs` (11 tests: backfill block extracted from the migration text and re-run over nulled columns, every write path, real two-transaction concurrency, isolation, `xmin` no-op, `updated_at` untouched) | — |
| `805699b` | The fourteen pre-switch statements frozen as a Rust module under `tests/fixtures/statements_b45b04f/` (byte-identical to `61b08ac`); `db_statement_equivalence.rs` green against the live text; `pub` widening on the Today source/feed statement functions so the test can call them | `check` 717; `check-db` 602/602 |
| `9f02ead` | Read-side switch of all fourteen statements (LATERAL maxima → columns; `has_replied` → `last_inbound_at IS NOT NULL`; `source_candidates` ORDER BY → output-identical `NULLS FIRST`; `person_state` both chains with the `waiting` gate inside the probe's WHERE, `latest` guarded on `$5 OR $27`, hydration over the capped prefix); the §8.6 pins; one fixture fix in `db_operator.rs` | `check` 717; `check-db` 603/603 |
| `df72926` | Performance archive `docs/design/perf/slice-012-2026-09-08/` with the `perf-harness` feature-gated harness committed | — |
| `77a51c8` | Round-1 fixes (below) | `check` 718; `check-db` 603/603 |

Coordinator file-list audit per checkpoint against `git diff --name-only`: all
files under `backend/` and the perf archive; no `PersonFilterParams`,
`to_query_params` or Rust signature change; `web/`, `crm-operator/**`,
`crm-api/src/operator/**` untouched.

### Criterion mapping (§8.1–8.10)

| § | Proof |
|---|---|
| 8.1 Migration | fresh database via `sqlx::test`; the marked backfill block extracted from the migration text, re-run as the migrator role over nulled columns for two Organizations incl. a zero-history Person and a cross-Organization `contact_attempted` row that must not correlate; triggers pinned `AFTER INSERT FOR EACH ROW`; the index pinned by full `indexdef`; migration order pinned by text position |
| 8.2 Invariant per write path | log contact attempt; call settle; outcome correction unchanged; outbound capture; deduplicated outbound capture advances nothing; inbound capture; backdated forward earlier (no change) and later (advances); first and repeat inquiry incl. an earlier `received_at` |
| 8.3 Concurrency | two transactions inserting attempts for one Person, second asserted blocked on the row lock, both commit orders leave the greater |
| 8.4 Isolation | a history row with a mismatched `organization_id` updates nothing; existing tenant-isolation suites unchanged and green |
| 8.5 Fourteen-statement equivalence | frozen vs live text, identical bindings, ordered full rows / counts / candidate signatures on a rich two-tenant fixed-clock fixture across sixteen axes incl. `last_inquiry`/`last_inbound` × three ops, `has_replied`, `awaiting_response`, `client_replied_unanswered`, and `person_state` with a `source` clause on feed A only, B only and both |
| 8.6 Today equivalence | `db_today_feed_equivalence.rs`, `db_today_builtin_parity.rs`, `db_today_source_filter_parity.rs` pass unchanged (frozen fixtures still compute from history); explicit pins for the gated `waiting` probe, the equality tie, the client-replied tie, zero-inquiry exclusion, repeat inquirers and reply members |
| 8.7 No-op backdated rows | `xmin` unchanged after a correction and after an earlier backdated forward |
| 8.8 `updated_at` untouched | asserted on the contact-attempt path; no `updated_at` trigger exists on `person` |
| 8.9 Gates | final-tree gates below |
| 8.10 Performance | archive: gate 1 seven rows within max(25 ms, 10%) with payloads equal — `person_state` 353 → 22 ms, `source_candidates` 46 → 2 ms and 48 → 1 ms, `filtered_summaries` never 22 → 5 ms; gate 2 EXPLAINs archived in both plan regimes: the default (custom plans, the regime production runs while the generic estimate is worse) shows `Index Scan Backward` on `person_organization_created_idx` with the column tests as row filters and the `waiting` probe once per gated candidate; a forced-generic re-capture shows the NULL-guarded parameters cannot be folded and the same statements fall to full scans (382/393/1,660 ms), disclosed as report-only; backfill 323 ms on 25k People; seed with triggers 2,118 ms; Today HTTP serial p95 34 ms (trend only, different book from 011d's 177 ms) |

### Review round 1 (of two) on `df72926`

Reviewer READY WITH FIXES, tester no blocking finding. Verified: migration
order and guards; all fourteen statements byte-identical in parameter
positions, guards and LIMIT, the `NULLS FIRST` rewrite output-identical;
both `person_state` chains switched in step; the frozen texts equal
`61b08ac`; the equivalence test compares results not text; the concurrency
test really blocks; no other direct history mutation exists (`crm_app` holds
SELECT/INSERT only on the history tables). Applied in `77a51c8`:

- equivalence test exercises the `last_inquiry` and `last_inbound` axes and
  the new `$5 OR $27` source guard (feed A only, B only, both), plus a
  client-replied tie row;
- backfill test adds a cross-Organization `contact_attempted` row; the
  concurrency test asserts the second insert blocked; the migration order
  and the full `indexdef` are pinned;
- correspondence trigger routes `outbound` with `ELSIF` (fails closed on a
  widened CHECK);
- the `db_operator.rs` fix-up sets `last_inquiry_at` from `max(received_at)`
  exactly;
- gate-2 plans re-captured under `plan_cache_mode = force_generic_plan`
  (the first capture showed custom plans with inlined constants); README
  notes the frozen `count_filtered_matches` text omits two SQL comments.

### Recorded LATER (D-050)

- `inquiry` has no `reject_mutation` trigger: the migrator role can update
  it (`db_operator.rs` does, hand-maintaining the column); `crm_app` cannot.
  Add the append-only triggers in a schema slice.
- Every history insert now takes the Person row lock; a future multi-Person
  importer (Slice 010) must insert in stable Person order (BEYOND_ENVELOPE).
- Gate 1's payload equality compares ordered id lists plus `truncated`, not
  full rows (the equivalence suite covers rows).
- Column-level `UPDATE` grants excluding the four columns plus a
  `SECURITY DEFINER` trigger function would make the invariant
  database-enforced against application bugs (TRUST).
- The projection-only maxima in `list_summaries`, `search_summaries` and
  `summary_by_id` still compute from history (post-LIMIT; optional switch).
- Plan regime: under `plan_cache_mode = force_generic_plan` the fourteen
  statements lose their index scans because `($n IS NULL OR …)` guards cannot
  be folded; PostgreSQL's default `auto` mode keeps custom plans while the
  generic estimate is worse, which is the regime measured in gate 1 and the
  first gate-2 capture. Pre-existing (the frozen text has the same guards);
  the ladder's recorded "custom-plan mode" lever (`SET LOCAL plan_cache_mode
  = force_custom_plan` on the hot statements) is the fix if a generic-plan
  flip is ever observed in production.
- The lane's earlier "25 vs 27 parameters" claim about an 011e-era
  perf-harness test is retracted as unverified (inferred from reading, never
  executed; the tester found both harness targets compile).

### Final-tree gates (coordinator, once, on `77a51c8`, 2026-09-08)

Run by the coordinator from the worktree root under the shared gate lock, in
one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; the tree stayed clean (no metadata drift) |
| `./scripts/check` | all checks passed, 32 s: fmt, clippy, cargo check, crate fences, **718** Rust tests, doc tests, Web lint/typecheck/**614** Vitest/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 203 s: **603 of 603** DB-backed tests on the first run; the pre-existing `db_calls` timing flake did not occur |

Push, deployment and the shared development runtime migration are not
performed or authorized by this record.

### Merge readiness

Source `slice-012-activity-columns` at `77a51c8` plus this record;
destination `main` (docs-only commits ahead of the branch base `61b08ac`:
project state records, so no code conflict is possible). Migration impact:
one additive migration `20260910000001_person_last_activity.sql` (four
columns, three triggers, a backfill, one index); `crm_dev` must be migrated
with `./scripts/db-migrate` and the dev API restarted by exact PID after the
merge, with the user's approval; the backfill took 323 ms on 25k People.
Unresolved risks: the LATER list above. Slice 013 rebases onto this merge.
