# LATER batch (2026-09-08) — implementation brief

**Status: APPROVED by the user on 2026-09-08 ("Do the ones worth a small
batch soon"); lane started the same day in `../crm-worktrees/later-1` on
`chore/later-batch-2026-09-08`.** No specification: every item is a recorded
LATER from an existing verification record (011d, 011e, 012, 014) and changes
no wire contract, no product behaviour visible to a user beyond removing
transient flicker, and no decision. One lane, one writer; the coordinator runs
review, the once-only final gates, and the commit and merge gates.

## Items

1. **`inquiry` append-only triggers** (012 LATER). Migration
   `crm-api/migrations/20260911000001_inquiry_append_only.sql`: bind the
   existing `reject_mutation()` function to `inquiry` exactly as
   `20260821000004_inquiry_and_facts.sql` does for the fact tables
   (`BEFORE UPDATE OR DELETE FOR EACH ROW`, `BEFORE TRUNCATE FOR EACH
   STATEMENT`). Production code never updates or deletes `inquiry` (`crm_app`
   holds SELECT/INSERT); the one test that does, `db_operator.rs` ~940
   (`UPDATE inquiry SET received_at …` as the migrator to backdate an
   inquiry), must instead insert the inquiry with the backdated `received_at`
   directly (the Slice 012 trigger then maintains `last_inquiry_at`, so the
   hand fix-up next to it goes away). `db_schema.rs` gains `inquiry` in the
   append-only enumeration and a test that an `UPDATE`/`DELETE` as the
   migrator is rejected. Person erasure (`inquiry` cascades from `person`) is
   a cascade, not a `DELETE FROM inquiry`; confirm the cascade still works
   under the trigger. *Checkpoint outcome (2026-09-08):* the lane proved
   empirically that a plain `reject_mutation()` `BEFORE DELETE` binding
   blocks the cascade, and the fact tables give no precedent because they
   reference `person` by bare uuid without a foreign key. Coordinator
   decision (safe default, schema integrity only): a new
   `reject_direct_mutation()` rejects every `UPDATE`, rejects a `DELETE` at
   `pg_trigger_depth() = 0` (a hand-run statement) and allows it when
   cascaded from `person` (depth ≥ 1, inside the referential-integrity
   trigger), so D-015 §5 erasure keeps working; `TRUNCATE` uses the existing
   `reject_mutation()`. Tests pin all four cases (direct update rejected,
   direct delete rejected, Person delete cascades, backdated insert allowed).
2. **The `db_calls` timing flake** (011d LATER):
   `a_second_correction_chains_onto_the_first_with_strictly_increasing_recorded_at`
   fails about one run in three under full-suite load because two history
   rows can share a microsecond `recorded_at` and the history sort has no
   tie-break. Diagnose first (run it in isolation and under load; read the
   history statement's `ORDER BY` in `person/queries.rs` ~722 and the test's
   assertion). Preferred fix: a deterministic secondary key (`id`) on the
   history `ORDER BY` if SLICE_003 §3's contract permits (it orders by time
   and says nothing against a tie-break) plus the test asserting strictly
   increasing `(recorded_at, id)`; if the test itself manufactures the tie,
   fix the test. Report which it was. `.sqlx` regenerated if the statement
   changes. *Outcome (2026-09-08, `2482b4c`):* not an ordering flake. The
   history query already tie-breaks on `id` per SLICE_002 §5 and the test's
   own query ordered by `(recorded_at, id)`; the assertion compared
   `recorded_at` alone, which two sequential writes can tie on under load.
   Test-only fix: compare the `(recorded_at, id)` tuple. No statement or
   `.sqlx` change.
3. **Split the three largest test files** with no test changes:
   `db_saved_lists.rs` (4,298 lines), `db_calls.rs` (3,371),
   `db_today_system_feed_commands.rs` (2,961) into two or three files each
   by topic, registered in `tests/all.rs` in alphabetical position, shared
   fixtures moved to `tests/common/` only if two new files need them.
   Acceptance: the reconciled set of test names keyed on `(file group,
   test name)` before and after is identical and the count is unchanged
   (record both counts).
4. **Field-only success writes** (014 LATER): in `useChangeStageMutation`
   and `useAssignPersonMutation`, `onSuccess` writes only the mutated field
   (`stage` or `assigned_user`) from `data.person` into the detail and the
   People rows, not the whole `person`, so a rapid stage-then-assignee pair
   never shows the first response's value for the second field.
   `onSettled` still invalidates once. Vitest: the rapid pair, with the
   first response resolving after the second `onMutate`, ends with both
   optimistic values intact until each settles.
5. **`isMutating` guards** (014 LATER): give the four Person mutations a
   `mutationKey` that includes the Person id; in `onSettled`, skip the
   Person-scoped invalidation while another mutation for the same Person is
   pending (the last one settles and invalidates); in the realtime
   invalidation path (`realtime/events.ts` / `useRealtime.ts`), while any
   Person mutation is pending, mark the People and Person queries stale
   without refetching (`refetchType: 'none'`) so an unrelated Person's
   `person.changed` cannot revert an optimistic row mid-flight; the
   settle-invalidate refetches. Vitest for both.

Not in this batch: everything else in the LATER lists (accessibility items,
test-only hardenings, performance levers, design residue).

## Delivery and rules

Owns `backend/**` for items 1–3 (sole migration owner) and `web/**` for items
4–5; nothing under `docs/` except this brief's status line is coordinator-owned.
Order: 2 (diagnose while the tree is untouched) → 1 → 3 → 4 → 5, a checkpoint
commit per item with the `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>` trailer. Gates per round from the worktree root under
the shared gate lock (`mkdir /private/tmp/claude-501/crm-gate.lock`, `rmdir`
after, also on failure), in the background, polled inside one call:
`source ~/.nvm/nvm.sh; ./scripts/sqlx-prepare && ./scripts/check` then
`./scripts/check-db`. Never bind ports 5173 or 3000 (the tunnel is served by
`dev-web-prod` from the main checkout; the dev API runs there too); never
`pkill -f`; kill only exact PIDs you started; never commit on `main`, push,
merge or rewrite history; never claim a check passed unless you saw it.

## Checkpoints requiring the coordinator

- Item 1 if the cascade finding blocks the trigger as specified.
- Item 2's diagnosis before changing the statement.
- Any file outside the ownership boundary or any contract question.
