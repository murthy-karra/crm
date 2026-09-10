# LATER batch (2026-09-10) — Verification record

Coordinator-owned evidence for [LATER_BATCH_2026-09-10.md](LATER_BATCH_2026-09-10.md)
under the D-050 budget. Branch `chore/later-batch-2026-09-10` from `main` at
`ac85eb5`, worktree `../crm-worktrees/later-2`, one lane (Claude Sonnet 5),
coordinated by Claude Fable 5.1. Implementation gate approved by the user on
2026-09-10.

| Commit | Item | Content |
|---|---|---|
| `ca3a7ff` | 1 | The five reason lines in `operator/explain.rs` use `to_rfc3339_opts(SecondsFormat::Secs, true)` (`Z`, whole seconds); `task_kind_label` renders `follow_up` as "follow-up" for prose. Two pinned unit tests updated, two added. No `db_operator` test pinned the old prose, so nothing moved there. |
| `cf09fda` | 2 | `TaskTitle::parse` also rejects U+2028 and U+2029, and a title that is empty after removing U+200B–U+200D, U+2060 and U+FEFF. The error is the existing `MalformedRequest` (there is no `EmptyTitle` variant; a new one would be a wire change). The DB CHECK (`20260913000001_task.sql`) is untouched and strictly wider. Checkpoint evaluated, not hit. |
| `67a1210` | 3 | The composite-FK rejections assert SQLSTATE `23503` through `as_database_error().code()`: three in `db_notes.rs::note_composite_fk_rejections`, five in `db_schema.rs::task_composite_fk_rejections`. The brief's pointers were wrong: `db_notes.rs` 174–288 is the note CHECK matrix (SQLSTATE 23514, converted in the fix round), and `db_tasks.rs` has no composite-FK test; the task one lives in `db_schema.rs`, which is why that file is in the change set. |
| `8711645` | 4 | Escape on the "deleted elsewhere" textarea does what Dismiss does; the inline note Save is `secondary` (the composer's Add note is the one primary in the History card); after a delete, focus moves to the composer textarea, or to the History heading (`tabindex="-1"`) if the composer were absent. One Vitest each for Escape and for focus. |
| `997527d` | 5 | A double-submit test dispatches two Ctrl+Enter keydowns with no tick between them and asserts one POST. The guard already existed; the lane proved the test detects its removal (two POSTs). Test-only. |
| `9e3aea9` | 6 | `PersonPreview.test.ts` mounts a history fixture carrying a note plus a stage change and pins that the note body never renders and exactly one timeline item does. |
| `6f47bb1` | 7 | Three curated task-axis tests: 199 + 1 task-only item fills the 200 cap with `truncated == false`; three task-only items sharing `due_at` order by ascending Person id (the SLICE_016 §5 tie-break); the exhausted-recovery test asserts the `task_due` issue token with `Unavailable` and `fallback == false`. None of the excluded variants. |
| `90e9d28` | 8 | `drop(sealed)` right after `store::insert_pending` in both `intake/receive.rs` and `capture/receive.rs`; the inbound route decodes `raw` and `recipient` in a block that ends the borrow of the body and drops the body before `receive_inbound_email`. No Phase B read-path or test change; checkpoint evaluated, not hit. Lane measurement, trend only, `/usr/bin/time -l` on the 20 MiB multipart intake test: maximum RSS 331 MB before, 289 MB after (about 44 MB, 13 % lower). |
| `b0b817e`, `1e782f7`, `9671d65` | fixes | Round-1 fixes (below). |

Coordinator file-list audit (twice, after the lane handoff and after the fix
round): 12 modified files, none new, none untracked, all inside the brief's
ownership (`db_schema.rs` justified under item 3); no migration, `.sqlx`,
`Cargo.*`, wire or `docs/` change; no new dependency; the shared stash stack
left empty after the lane's stash-isolated RSS measurement.

## Lane gates (own tree)

Per item: `cargo fmt --check`, `clippy -D warnings`, `cargo check`, targeted
`cargo test --lib`; `vue-tsc`, `eslint`, targeted `vitest run`. Targeted
DB-backed `nextest` runs under the gate lock, never overlapped: the two
composite-FK tests 2 of 2; `db_today_task_axis` and
`db_today_task_axis_failures` 24 of 24; `db_inbound_email`,
`db_inbound_email_intake` and `db_capture_receive` 56 of 56 including both
20 MiB tests. `./scripts/check` green on `90e9d28` (782 Rust, 5 doc, 751
Vitest, 11 worker) and again on `9671d65` with the same counts.

## Review round 1 (of two) on `90e9d28`

Reviewer READY WITH FIXES, no blocking finding; tester no blocking finding.
Verified by both: every reason line uses the `Z` whole-second form and no
`+00:00` pin exists outside the changed tests; the code-point set matches
the brief and an ignorable inside real text is still accepted; the 23503
assertions cannot pass vacuously (each case satisfies its single-column FK,
so only the composite FK fires); the Escape handler is bound only to the
gone-draft textarea; the focus test attaches to `document.body` and fails
without the fix; in a real browser PrimeVue's dialog restores focus during
the render flush and the view's `nextTick` focus runs after it, so the
composer wins; the double-submit test proves the reactive guard because
TanStack query-core dispatches `pending` before its first `await` and
vue-query updates state synchronously; the three task-axis tests are the
curated cells and not restatements; Phase B reads the nonce and ciphertext
back from the locked row in both paths and the route adds no clone. Nothing
in the batch touches concurrency, retries, idempotency, tenant isolation,
realtime or Operator safety. Applied in the fix round:

- **item 2 (BOUNDARY, both reports):** `all(is_default_ignorable)` on the
  trimmed title accepted `"\u{200B} \u{200B}"` because `trim()` strips only
  White_Space; the predicate is now "empty after removing ignorables"
  (`filter(!ignorable).all(is_whitespace)`), with two new assertions
  (`b0b817e`);
- **item 3 (RESTATES, applied under D-050 as a few lines):** the seven note
  CHECK-matrix assertions the brief pointed at now assert SQLSTATE `23514`
  through the house pattern; all seven fire the CHECK, including the
  NULL-author case (`1e782f7`);
- **item 7b (BOUNDARY, tester):** with server-generated ids the tie-break
  test would still pass one run in six if `id ASC` were dropped (the index
  order is by task id); the three Persons now carry explicit ids inserted
  in non-ascending order, with tasks created in a third order, through a
  test-local helper (`9671d65`).

Round 2 not needed.

## Recorded LATER (D-050)

- The History-heading focus fallback is unreachable today (the composer has
  no `v-if`) and carries `focus:outline-none`; UI_STYLE §8 keeps focus rings
  if it is ever reached. Nothing pins that the inline Save is no longer
  `primary`.
- `SecondsFormat::Secs` truncates fractional seconds in the reason lines
  while wire timestamps keep microseconds; every test instant is
  whole-second, so a one-line test with a fractional instant would document
  the divergence.
- `task_kind_label` has a wildcard arm; an exhaustive match would make a
  future underscore variant a compile error instead of leaking its wire
  value into prose.
- The PersonPreview note fixture has two entries, so it cannot distinguish
  filter-then-`slice(-3)` from slice-then-filter; four entries with the note
  newest would.
- A pre-existing Vitest flake not touched by this batch:
  `PersonDetailView.test.ts` "editing without touching the date re-sends
  the stored instant" failed once in three runs for the tester
  (`putBody.title` undefined) and passed 101 of 101 twice in isolation.
- The brief's item 3 pointers (`db_notes.rs` 174–288, `db_tasks.rs`) did
  not match the code; future briefs should cite the test name.
- Asserting `constraint()` names on the composite-FK rejections would pin
  which FK fired.

## Final-tree gates (coordinator, once, on `9671d65`, 2026-09-10)

Run by the coordinator from the worktree root under the shared gate lock, in
one sequence, each once:

| Gate | Result |
|---|---|
| `./scripts/check` | all checks passed, 13 s: fmt, clippy, cargo check, crate fences, **782** Rust tests, 5 doc tests, Web lint/typecheck/**751** Vitest/build, email-worker 11 tests |
| `./scripts/check-db` | all checks passed, 261 s: **720 of 720** DB-backed tests on the first run; neither known flake occurred |

## Merge readiness

Source `chore/later-batch-2026-09-10` at `9671d65` plus this record;
destination `main` (one docs-only commit ahead of the branch base
`ac85eb5`: the project-state record of the gate approval, so no code
conflict is possible). Migration impact: none. Runtime impact: the dev API
must be restarted by exact PID for items 1, 2 and 8, and the production web
server (`dev-web-prod`) rebuilt and restarted for item 4. Unresolved risks:
the LATER list above. Push and deployment are not authorized by this record.
