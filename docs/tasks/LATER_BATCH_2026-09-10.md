# LATER batch (2026-09-10) — implementation brief

**Status: DRAFT (2026-09-10), pending the user's approval at the
implementation gate.** No specification: every item is a recorded LATER from
an existing verification record (015, 016a, 016b, 017) and changes no wire
contract, no product decision, and no user-visible behaviour beyond a
consistency fix in Operator text, a stricter title validator for characters
no client can type, and three small Person-page niceties. One lane, one
writer, branch `chore/later-batch-2026-09-10` from `main` in
`../crm-worktrees/later-2`, the `implement` profile; the coordinator runs
review and test analysis (two rounds at most, D-050), the once-only
final-tree gates, and the commit and merge gates.

Read first: AGENTS.md (§9, §11, §14); DECISION_LOG D-050, D-053, D-054,
D-056; the LATER sections of `docs/tasks/SLICE_015_VERIFICATION.md`,
`SLICE_016a_VERIFICATION.md`, `SLICE_016b_VERIFICATION.md` and
`SLICE_017_VERIFICATION.md`; `docs/prompts/05-implement.md`;
`docs/design/UI_STYLE.md` for item 4. Inspect the code named below before
writing; state a short plan first.

## Items

1. **Operator reason timestamps and the task-kind wording** (016b LATER).
   `crm-api/src/operator/explain.rs` formats five reason lines with
   `to_rfc3339()` (`+00:00`) — "no contact attempt since", "a call at … has
   no outcome yet", "the client replied at", "a task was due at", "a … task
   is due at" — while `crm-operator/src/service.rs:380` and every wire
   timestamp use `Z` with whole seconds. Switch all five to
   `to_rfc3339_opts(chrono::SecondsFormat::Secs, true)`. Render the task
   kind for prose through a small label map (`follow_up` → "follow-up";
   the others unchanged) instead of `as_str()`. Both forms are the "absolute
   RFC 3339" SLICE_005 §5 and SLICE_016 §7 require, so no contract changes;
   the unit and `db_operator` tests that pin the text move with it.
2. **Task title validation** (016a LATER). `crm-app/src/domain/task/model.rs`
   ~147 rejects `is_control()` only. Also reject U+2028 and U+2029 (line
   and paragraph separators) as line breaks, and treat a title that is
   empty after removing default-ignorable code points (U+200B–U+200D,
   U+2060, U+FEFF) as empty (`EmptyTitle`). The DB CHECK stays as it is
   (the validator is strictly narrower). Unit tests for each; the Web input
   cannot produce these, so no Web change.
3. **SQLSTATE assertions on composite-FK rejections** (015 and 016a LATER).
   The seven `is_err()` assertions in `crm-api/tests/db_notes.rs`
   (~174–288) and their `db_tasks.rs` siblings assert the database error
   code `23503` (`sqlx::Error::Database` → `.code()`), the house
   CHECK-matrix pattern. Test-only; counts unchanged.
4. **Note composer niceties** (015 LATER, Web, `web/src/views/PersonDetailView.vue`
   and its test). Escape on the "deleted elsewhere" textarea does what
   Dismiss does; the inline Save while editing a note is not a third
   `primary` in the region (UI_STYLE: one primary per region — the composer
   keeps it); after a delete, focus moves to the composer textarea (or the
   Notes heading if the composer is not rendered) instead of falling to the
   body. One Vitest each for Escape and for focus after delete.
5. **Composer double-submit test** (015 LATER, `PersonDetailView.test.ts`): a
   second submit while the add-note mutation is pending posts nothing (mock
   the mutation as pending; assert one call). If the guard is missing, add
   it (disable the Save button and ignore Enter while pending).
6. **PersonPreview note fixture** (015 LATER, `PersonPreview.test.ts`): a
   fixture carrying notes pins that notes never render in the preview.
7. **Task-axis tests** (016b LATER, curated; the 016b Today task-axis test
   file registered in `tests/all.rs`): (a) task-only items count against
   the list cap (a dedicated test at the cap boundary); (b) task-only order
   with three items sharing `due_at` is stable on the documented tie-break;
   (c) the exhausted-recovery test asserts the `task_due` issue token, not
   only the item set. Not in scope: the 199/200/201 × 0–3 grid, the
   `+24 h + 1 s` baseline, the 5 s bound and the retained-item variants
   (RESTATES or UNREACHABLE in the record).
8. **Inbound handler memory trim** (017 LATER). In
   `crm-app/src/domain/intake/receive.rs` (~163–185) and
   `crm-app/src/domain/capture/receive.rs` (~70–85) the sealed blob is dead
   after `store::insert_pending` (Phase B reads the ciphertext back for the
   AAD-correct decrypt): drop it right after the insert. In
   `crm-api/src/routes/inbound_email.rs`, produce `raw: Vec<u8>` in a block
   that ends the borrow so `req` and the `Bytes` body are dropped before
   `receive_inbound_email` runs. Behaviour unchanged; the existing
   `db_inbound_email` and `db_capture_receive` suites (including the two
   20 MiB tests) are the proof. Report once, trend only: peak RSS of the
   20 MiB intake test before and after (`/usr/bin/time -l` on the targeted
   run), if that costs less than ten minutes.

Not in this batch (recorded, needs a decision or its own rung): the snooze
24-hour-window nuance (a 48-hour "Due soon" lookahead or a snoozed group
changes the D-054 axis — the user's call); client-minted `NoteId`/`TaskId`
for idempotent creates (a contract addition); the `db_calls` timing flake
(unreproduced); the per-Organization inbound byte budget and the
pre-authentication concurrency limit (production-deployment trigger); the
`NOT EXISTS` self-anti-join over in-window tasks (FUB-import trigger).

## Delivery and rules

- Ownership: `backend/crates/crm-api/src/operator/explain.rs`;
  `backend/crates/crm-app/src/domain/task/model.rs`;
  `backend/crates/crm-app/src/domain/intake/receive.rs`;
  `backend/crates/crm-app/src/domain/capture/receive.rs`;
  `backend/crates/crm-api/src/routes/inbound_email.rs`; the named test
  files under `backend/crates/crm-api/tests/`; `web/src/views/PersonDetailView.vue`
  and its test; `web/src/components/**/PersonPreview.test.ts` (find it).
  Not owned: migrations, `.sqlx/` (no statement changes; if one becomes
  necessary, stop and report), `Cargo.*`, `docs/`.
- No migration, no wire-contract change, no new dependency. Anything that
  would need one: stop and report (AGENTS §11).
- Setup in the fresh worktree: `source ~/.nvm/nvm.sh` before `pnpm`/`node`;
  `pnpm install --frozen-lockfile` in `web/`; the first Cargo build is cold;
  copy `.env` from the main checkout for targeted DB tests (gitignored,
  never committed). **macOS has no `timeout`:** wrap long runs as
  `perl -e 'alarm shift; exec @ARGV' 1800 ./scripts/check`.
- Checkpoint-commit after each item; run `./scripts/check` in the
  background and poll; run the DB-backed tests you touch as targeted
  `nextest` runs replicating `scripts/check-db` step 2 with a name filter;
  never run `./scripts/check-db` (the coordinator runs it once on the final
  tree) and never overlap two database-backed runs.
- Report the exact changed-file list; the coordinator diffs it against
  `git status` including untracked files. Never print note or task text,
  message content or secrets in tests, fixtures or the handoff.

## Checkpoints requiring the coordinator

- Item 2: if the DB CHECK would need to change to stay consistent, stop and
  report (it must not).
- Item 8: if dropping the sealed blob early changes any Phase B read path
  or a test's expectations, stop and report with the diff.
