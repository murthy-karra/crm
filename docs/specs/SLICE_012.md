# Slice 012 — Denormalized last-activity columns on Person

**Status: APPROVED by the user on 2026-09-08 after independent review; D-052
records rule 1.** Approval covers the declared contracts (§5) and the §9 safe
defaults; it authorizes implementation and tests in the Slice 012 lane, not
commit, merge, push or deployment.
Prepared against `main` at `b45b04f` (Slice 011 ladder complete and pushed). Companion lane: [Slice 013](SLICE_013.md) (Operator `filter_people`),
planned in parallel; see §10 for the ownership split. Brief:
[SLICE_012_IMPL.md](../tasks/SLICE_012_IMPL.md).

Every filter that asks "when was this Person last contacted, last heard from,
last inquired" and both Today person-state feeds compute those dates at query
time by scanning the Person's history rows. That is correct and always fresh,
but the cost grows with Organization size times history: about 100 ms for a
5k-Person team and 966 ms for 100k People in the August baseline, and under
load the 10-connection pool fills. This slice stores the four maxima on the
Person row, keeps them exact with database triggers, backfills them once, and
switches every statement that computed them to read the columns. **Nothing
changes on the wire**: no HTTP, realtime, Operator or filter-vocabulary
change, and every statement must return byte-identical results.

Authority: [D-007, D-015, D-022, D-031, D-032, D-033, D-042, D-043,
D-050](../decisions/DECISION_LOG.md); AGENTS §4.6 (hybrid persistence), §4.7
(read models are intentional; PostgreSQL stays authoritative), §4.8, §8, §11;
the [011 ladder's standing tensions](../plans/SLICE_011_LADDER.md) naming this
lever; [PERF_BASELINE.md](../design/PERF_BASELINE.md) conclusion 1; the
[perceived-latency investigation](../design/perceived-latency-2026-09-07.md);
[011a](SLICE_011a.md) §4c/§4e, [011d](SLICE_011d.md) §5, [011e](SLICE_011e.md)
§4 (the fourteen statements), [003](SLICE_003.md) §3, [009](SLICE_009.md) §6.

## 1. Scope and product behavior

In scope:

- Four nullable `timestamptz` columns on `person`: `last_inquiry_at`,
  `last_contact_at`, `last_inbound_at`, `last_outbound_at`, each equal at
  every commit boundary to the maximum of the corresponding history rows for
  that Person, NULL when there are none.
- Three `AFTER INSERT` triggers (on `inquiry`, `contact_attempted`,
  `correspondence_captured`) maintaining them monotonically.
- One migration: columns, triggers, backfill, one index.
- The read-side switch in every statement that computed one of the maxima
  (§4), behind an equivalence gate that proves byte-identical results.
- D-050 performance evidence on a populated fixture.

Out of scope: denormalizing the latest inquiry's `id`/`source` (a row
pointer with a tie-break, not a monotone maximum; recorded as a later lever
and guarded instead, §4), `inquiry_count`, call state; any change to
`PersonFilterParams`, the vocabulary, Today tiers or reasons; the three
projection-only subselects in `list_summaries`, `search_summaries` and
`summary_by_id` (post-LIMIT, not hot; optional later); pool sizing; Slice
010 import behaviour (which the triggers already cover).

Product rules (rule 1 is the accepted decision D-052; the rest are safe
defaults, veto-able):

1. **Derived columns are maintained by database triggers, not by each typed
   command** (D-052, user, 2026-09-08). The business mutation (the history insert) still passes only
   through typed commands (AGENTS §4.8); the trigger maintains a derived
   read-model column (§4.7), in the same category as the schema's existing
   append-only `reject_mutation` triggers and composite FKs. Reasons: four
   commands already write `contact_attempted` through one insert site and
   the parked FUB import will add batch and backdated writers; 23 test
   files insert history with raw SQL and stay correct unchanged; during a
   deploy an old binary keeps the columns exact. The alternative
   (application maintenance in each command) leaves a stale window during
   rollout and a fixture rewrite in 23 files for no gain.
2. **Columns are nullable, no sentinel.** The `never` filter is `ts IS NULL`
   and `waiting_since` is `Option`; nullable keeps every statement's
   `COALESCE(ts, '-infinity')` text and results byte-identical.
3. **Triggers never move a maximum backwards.** The update is guarded by
   `col IS NULL OR col < NEW.ts` (the `WHERE` form, not `GREATEST`, so a
   non-advancing row produces no dead tuple), so a backdated capture (D-042
   retroactive forward), a correction (which inherits its original's
   `occurred_at`) or a deduplicated outbound capture is a no-op.
4. **INSERT only.** No application delete path exists for history
   (`crm_app` lacks DELETE). Any future deletion or redaction (O-013) must
   re-run the backfill statement for the affected Person; the migration
   publishes that statement as a named block so the erasure runbook can
   cite it. Person erasure cascades the columns with the row; history keeps
   orphaned ids (D-015 §5).
5. **`person.updated_at` is untouched** by history inserts (only assignment
   and stage changes set it today), so the change is invisible to any
   consumer of that field.
6. **One index, no partials.** `person_org_last_contact_idx (organization_id,
   last_contact_at ASC NULLS FIRST, id ASC)`, declared with exactly that
   order so a forward scan produces `source_candidates`' ordering (§4). No
   index on `last_inquiry_at`, `last_inbound_at` or `last_outbound_at`: the
   age predicates are NULL-guarded (`($n IS NULL OR (ts IS NULL) = $n)`,
   `COALESCE(ts, '-infinity') > cutoff`) and not index-sargable under the
   generic plans prepared statements use, so such indexes would only add a
   non-HOT row rewrite to every history insert. The win of this slice is
   removing the per-Person history probes; the remaining cost is a linear
   scan of the Organization's slice (≤ 25k rows in the envelope), which is
   D-050-compliant. Further indexes are added only if a §7 EXPLAIN shows a
   use, and are reported, never added silently.

## 2. Persistence

Migration `crm-api/migrations/20260910000001_person_last_activity.sql`, in
one transaction, **in this order**: columns → trigger functions and
triggers → backfill → index. Creating the triggers before the backfill
matters: `CREATE TRIGGER` takes a `SHARE ROW EXCLUSIVE` lock on each history
table, so inserts in flight from a still-running old binary drain before the
backfill reads, and every later insert fires the trigger. Backfilling first
would leave any row committed between the backfill's snapshot and the
migration's commit permanently unreflected.

```sql
ALTER TABLE person
    ADD COLUMN last_inquiry_at  TIMESTAMPTZ,
    ADD COLUMN last_contact_at  TIMESTAMPTZ,
    ADD COLUMN last_inbound_at  TIMESTAMPTZ,
    ADD COLUMN last_outbound_at TIMESTAMPTZ;

CREATE FUNCTION person_touch_last_inquiry() RETURNS TRIGGER AS $$
BEGIN
  UPDATE person SET last_inquiry_at = NEW.received_at
   WHERE id = NEW.person_id AND organization_id = NEW.organization_id
     AND (last_inquiry_at IS NULL OR last_inquiry_at < NEW.received_at);
  RETURN NULL;
END $$ LANGUAGE plpgsql;
-- person_touch_last_contact (contact_attempted.occurred_at → last_contact_at)
-- person_touch_correspondence (occurred_at → last_inbound_at when
-- NEW.direction = 'inbound', last_outbound_at when 'outbound'), same shape.
CREATE TRIGGER inquiry_touch_person AFTER INSERT ON inquiry
    FOR EACH ROW EXECUTE FUNCTION person_touch_last_inquiry();
-- ... two more.

-- BEGIN PERSON_LAST_ACTIVITY_BACKFILL
-- One rewrite of each Person row. Re-run for the affected People after any
-- future history deletion or redaction (O-013); tests extract this block by
-- its markers and re-run it.
UPDATE person p
   SET (last_inquiry_at, last_contact_at, last_inbound_at, last_outbound_at) = (
       (SELECT max(received_at) FROM inquiry i
         WHERE i.person_id = p.id AND i.organization_id = p.organization_id),
       (SELECT max(occurred_at) FROM contact_attempted ca
         WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id),
       (SELECT max(occurred_at) FROM correspondence_captured cc
         WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
           AND cc.direction = 'inbound'),
       (SELECT max(occurred_at) FROM correspondence_captured cc
         WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
           AND cc.direction = 'outbound'));
-- END PERSON_LAST_ACTIVITY_BACKFILL

CREATE INDEX person_org_last_contact_idx
    ON person (organization_id, last_contact_at ASC NULLS FIRST, id ASC);
```

- The trigger matches both `id` and `organization_id`, so a fact row whose
  Organization differs from the Person's updates nothing (defence in depth;
  the composite FKs already forbid such rows where they exist).
- No new grants: `crm_app` already holds `UPDATE ON person`; trigger
  functions run as the invoker. `db_schema.rs` gains the three triggers and
  the index in its enumerations.
- The `ALTER TABLE person` holds `ACCESS EXCLUSIVE` for the migration's
  duration, blocking the old binary's People and Today reads for that
  window: sub-second at the 25k envelope, a few seconds at the 100k-People
  baseline (four correlated maxima over indexed history tables plus one
  rewrite of the Person rows). Batching is not needed; the measured duration
  is recorded in the performance archive.
- The backfill block is load-bearing for tests: `db_person_last_activity.rs`
  extracts it from the migration text by its markers, seeds history, nulls
  the four columns as the migrator role, re-runs the block and asserts
  `column == max(history)` per Person, because a `sqlx::test` database is
  migrated empty and would otherwise never exercise the backfill over data.

## 3. The invariant and its writers

**For every Person, at every commit boundary, each column equals the maximum
of the corresponding history rows for that Person, NULL when there are
none.** Concurrency: `GREATEST` is monotone, so commit order is irrelevant;
under READ COMMITTED the second updater waits on the row lock and
re-evaluates against the committed row. Contention is bounded by the write
rate (measured 104–127 leads/s sustained). Writers covered without code
change: `LogContactAttempt`, the call settle path, `CorrectCallOutcome`
(inherits `occurred_at`, no-op), capture's automatic outbound attempt,
inbound and outbound capture including backdated forwards, `ReceiveInquiry`
first and repeat, and any future importer.

## 4. Read-side switch

Every statement that computes one of the maxima changes to read the column,
keeping statement shape, parameter positions, guards, ORDER BY and LIMIT
identical:

| # | Statement | Change |
|---|---|---|
| 1–8 | `person/sql/filtered_summaries.sql` and its seven sorted copies | drop the four `LEFT JOIN LATERAL max()` blocks; `*_ts.ts` → `p.last_*_at`; the projection subselect for `last_inquiry_at?` → the column; `has_replied` `EXISTS inbound` → `p.last_inbound_at IS NOT NULL` |
| 9 | `count_filtered_matches` (`person/queries.rs`) | same substitutions |
| 10 | `today/source_membership.sql` | same |
| 11 | `today/source_candidates.sql` | same; its `(ts IS NOT NULL) ASC, ts ASC, id ASC` ordering is rewritten to the output-identical `last_contact_at ASC NULLS FIRST, id ASC` (`false < true` already puts NULLs first; both tie-break on `id ASC`) so the §2 index's forward scan serves it; the equivalence tests pin the order |
| 12 | `system_feeds/sql/person_state.sql` (both chains) | same; plus two exact equivalences: gate the `waiting` probe on `p.last_inquiry_at > COALESCE(p.last_contact_at, '-infinity')` ("exists an inquiry after X" ⇔ "max > X"; ties are strictly-greater on both sides) **placed inside the probe subquery's `WHERE`**, where it references only outer columns, is pseudo-constant and becomes a `One-Time Filter` that skips the scan (in the `ON` clause it would be a join qual evaluated after the probe ran); replace `latest.id IS NOT NULL` with `p.last_inquiry_at IS NOT NULL`, guard the `latest` LATERAL on **both** feeds' source parameters (`$5::text[] IS NOT NULL OR $27::text[] IS NOT NULL`), and hydrate `latest_inquiry_id/source/received_at` over the capped prefix as `source_candidates` already does; feed A's `fresh` reads `p.last_inquiry_at` |
| 13–14 | `system_feeds/sql/call_membership.sql`, `call_only.sql` | same substitutions |

Untouched by design: `list_summaries`, `search_summaries`, `summary_by_id`
(projection-only maxima over ≤501 rows). `.sqlx` metadata is regenerated for
the changed statements; `PersonFilterParams`, `to_query_params` and every
Rust signature are unchanged, so the companion Slice 013 lane, which only
calls these functions, is unaffected.

**Equivalence gate (before any statement changes).** The pre-switch text of
all fourteen statements is frozen from the branch point into
`tests/fixtures/statements_b45b04f/` as a copied Rust module in the
`today_f51bff8` style (not bare `.sql` files: `person_state`'s 55 parameters
must be bound identically on both sides, and frozen files run through
`query_file_as!` keep their old `.sqlx` hashes alive, which is harmless).
One new database test file runs the frozen and the live text with identical bindings
over a rich two-tenant fixed-clock fixture (corrections, a backdated inbound
capture, an outbound capture with its automatic attempt, a repeat inquiry
with an out-of-order earlier `received_at`, zero-history People) and asserts
identical ordered id lists, counts and rows for every axis and for
`has_replied`, `awaiting_response` and `client_replied_unanswered`. The
existing `db_today_feed_equivalence.rs` and `db_today_builtin_parity.rs`
already compare live Today against frozen history-computing statements and
become the Today gate for free.

## 5. Contracts

Wire-invisible. Declared per AGENTS §11 anyway, because the persistence
contract changes:

| Previous → proposed | Reason / affected | Compatibility and amendment |
|---|---|---|
| No derived-data writer outside typed commands and migrations → trigger-maintained read-model columns (rule 1) | The repository's first such writer; future importers will cite it | **D-052** in DECISION_LOG on approval: "trigger-maintained derived columns are a read-model mechanism; the history insert remains the only business mutation" (refines D-021's sanctioned-writer list). |
| `person` without activity columns → four trigger-maintained columns and one index | Measured Today and "never"-filter cost | Additive; no consumer change. SLICE_002 §2 pointer. |
| Fourteen statements compute maxima → read columns | Same results, lower cost | Byte-identical by §4 gate. 011a §4c/§4e, 011d §5, 011e §4 (the statement enumeration and parity discipline), 003 §3, 009 §6 pointers; PERF_BASELINE conclusion 1 and the ladder's standing tension marked built. |

## 6. Failure and observability

No new failure modes on the read side. A trigger failure fails the history
insert's transaction, which is the correct fail-closed outcome (the fact and
the derived column commit together or not at all). No new spans; existing
statement telemetry is unchanged. Nothing new is logged.

## 7. Performance (D-050)

Fixture: the `perf-harness` feature-gated harness in
`db_today_feeds_http_perf.rs` (kept behind its `required-features` gate, not
deleted before commit, so the archive stays reproducible at zero cost to
normal builds) extended with the batch-SQL history seed already in
`tests/fixtures/today_http_perf_fixture.rs` (inquiries, attempts,
corrections, correspondence) at 25k People. One benchmark run (D-050); no
100k run. Not `./scripts/perf seed` (fifteen minutes, dev database) and not
the build-hash-gated 011c fixture. The seed's own duration before and after
the triggers is recorded as the trigger write-cost figure (report only).

1. **Paired relative regression**, same build, machine, fixture and clock:
   frozen versus live text for `filtered_summaries` (default sort),
   `count_filtered_matches`, `person_state` and `source_candidates`, with
   `last_contact never`, `last_contact not_within_days 7` and the canonical
   feed definitions bound; payloads equal; p95 within max(25 ms, 10%),
   expected to improve substantially. The Today HTTP p95 on the new build is
   reported against the archived 011d figure as trend, not gate.
2. **Plan shape**: `EXPLAIN (ANALYZE, BUFFERS)` after `VACUUM (ANALYZE)`,
   taken through `PREPARE`/`EXECUTE` of the exact `.sqlx` statement text so
   the archived plan is the generic plan production runs, for
   `filtered_summaries` with `last_contact never`, with
   `last_contact not_within_days 7`, and for `person_state`, showing column
   tests and no per-Person probe over the whole Organization except the
   gated `waiting` probe, whose `loops` count must not exceed the number of
   People passing the gate.

Also recorded: the backfill duration. The transaction-local
`enable_mergejoin = off` in Today stays untouched. Evidence under
`docs/design/perf/slice-012-<date>/`.

## 8. Acceptance criteria and verification

1. **Migration** applies on a fresh database; the backfill block, extracted
   from the migration by its markers and re-run over seeded history with the
   columns nulled, leaves every column equal to `max(history)` for every
   Person and NULL for zero-history People; the three triggers exist and are
   `AFTER INSERT` only; `db_schema.rs` enumerations and grant pins pass. (db)
2. **Invariant per write path**, through the typed commands: log contact
   attempt; call settle; outcome correction (unchanged); outbound capture
   (contact and outbound advance, inbound unchanged); a deduplicated
   outbound capture (`ON CONFLICT DO NOTHING`) advances nothing; inbound
   capture; backdated forward earlier than the current maximum (no change)
   and later (advances); first and repeat inquiry including an out-of-order
   earlier `received_at`. (db)
3. **Concurrency**: two transactions inserting attempts for one Person,
   committed in either order, leave the greater timestamp. (db)
4. **Isolation**: a history row whose Organization differs from the Person's
   updates nothing; every existing tenant-isolation test passes unchanged.
   (db)
5. **Fourteen-statement equivalence** per §4 over the rich fixture. (db)
6. **Today equivalence**: `db_today_feed_equivalence.rs`,
   `db_today_builtin_parity.rs`, `db_today_source_filter_parity.rs` pass
   unchanged (their frozen fixtures still compute from history and stay
   untouched); explicit pins for the gated `waiting` probe including the
   equality tie (`received_at = last_contact_at` is not waiting on either
   text), a zero-inquiry Person with contacts and an inbound reply staying
   excluded, and `waiting_since`, `fresh`, ordering and truncation on repeat
   inquirers and reply members. (db)
7. **No-op backdated rows**: the Person row's `xmin` is unchanged after a
   backdated insert. (db)
8. **`updated_at` untouched** by history inserts. (db)
9. **Gates**: `.sqlx` regenerated; `./scripts/sqlx-prepare`,
   `./scripts/check`, `./scripts/check-db` once on the final tree by the
   coordinator; the lane runs them per round.
10. **Performance**: §7 items, archived. Independent reviewer and tester,
    read-only, at most two rounds (D-050).

## 9. Safe defaults adopted (veto-able)

Nullable columns; one index (`last_contact_at`, NULLS FIRST) and no
partials; latest inquiry and `inquiry_count` not denormalized; the two
`person_state` probe gatings; frozen-text equivalence rather than a provider
seam; the three projection subselects left alone; the perf harness kept
behind its feature gate; migration stamp `20260910000001`; archive name
`slice-012-<date>`. Recorded LATER: column-level `UPDATE` grants excluding
the four columns plus a `SECURITY DEFINER` trigger function would make the
invariant database-enforced against application bugs (TRUST; not needed
now).

## 10. Delivery

One backend lane, one writer, worktree `../crm-worktrees/012` on
`slice-012-activity-columns` from `main`; size **M**. Owns `backend/**`
except `crm-api/src/operator/**` and `crm-operator/**`, which belong to the
Slice 013 lane, and except two touches granted to that lane:
`crm-api/src/routes/operator.rs` (which this lane never edits) and
`crm-api/tests/all.rs`, where each lane registers its own test files in
alphabetical position (this lane's two after `db_people_sort` and after
`db_schema`) so the hunks never overlap. Sole owner of the migration
directory and of `.sqlx` regeneration for the statements it changes. Order: migration → invariant
tests → frozen fixtures and the equivalence test (green before any statement
changes) → read-side switch → performance evidence. Merges to `main` before
Slice 013, which rebases. This specification authorizes nothing until the
user approves it after independent review; approval will authorize
implementation and tests, not commit, merge, push or deployment.
