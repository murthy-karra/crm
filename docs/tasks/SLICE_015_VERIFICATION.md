# Slice 015 (Notes) — Verification record

Coordinator-owned evidence for [SLICE_015.md](../specs/SLICE_015.md) and the
[implementation brief](SLICE_015_IMPL.md) under the D-050 budget. Branch
`slice-015-notes` from `main` at `238c3a7`, worktree `../crm-worktrees/notes-1`,
one lane (Claude Sonnet 5, `implement` profile), coordinated by Claude Fable
5.1 on 2026-09-09.

| Commit | Step | Content |
|---|---|---|
| `51d9d70` | 1 | Migration `20260912000001_note.sql` byte-faithful to spec §2 (five CHECKs, three composite FKs, `ON DELETE CASCADE` on the Person FK only, two indexes incl. the partial unique import key, `GRANT SELECT, INSERT, UPDATE` with no `DELETE`); `NoteId`; `db_schema.rs` grant and index pins. |
| `db9217d` | 2 | `crm-app/src/domain/note/` (model with `NoteBody::parse`, queries, three commands, error). Lock order person `FOR UPDATE` → note `FOR UPDATE` → membership `FOR SHARE`; the note lookup binds `id`, `organization_id`, `person_id` and `deleted_at IS NULL`; rule-1 permission (admin or author) decided under the lock; edit-to-same commits nothing and publishes nothing; tombstone touches only `body`, `deleted_at`, `deleted_by_user_id`; `PersonChange::NoteChanged` after commit on changing writes only; `note` history kind (rank 7, positioned at `created_at`); no `Debug` on body-carrying structs, spans `skip_all`. Fourteen tests in `db_notes.rs`. |
| `09984e7` | 3 | `routes/notes.rs` (POST, PUT, DELETE under `/api/people/{person_id}/notes`; `PersonNoteIdsPath`; per-route 128 KiB `DefaultBodyLimit`; `deny_unknown_fields`; JSON rejection mapped without its message); `NoteError → ApiError` (no new code); the people detail route overwrites `can_manage` on `note` entries from the viewer's role and id. `db_admin.rs` 401 enumeration with explicit PUT and DELETE; `db_realtime.rs` payload-has-no-body pin; `db_people.rs` People rows / `person.updated_at` / D-052 columns / Today untouched. |
| `4d3951e` | 4 | Operator `PersonDetail.notes` (latest five, `UntrustedText`, 500-char clip); `history` filters the `note` kind before the `MAX_HISTORY` truncation; no `history_detail` arm; the system prompt's untrusted-text parenthetical gains "notes". `db_operator.rs` sentinel test (S2–S6 once, S1 never, none in history or the ledger row) and the `CaptureWriter` capture test. |
| `0482a36`, `6f82709` | gates | `cargo fmt`; a scoped `#[allow(clippy::too_many_arguments)]` on a ten-argument test helper (the existing `create_person` precedent). |
| `095b67f` | 5 | Web: `Note`/`NoteDetail` types and the `HistoryEntry` `note` arm; three pessimistic mutations keyed with `personMutationKey`, settled through `settlePersonMutation` on the person key; `note_changed` invalidates only `queryKeys.person`; composer at the top of the History card (Ctrl/Cmd+Enter, code-point counter past 9,000, disable past 10,000, draft kept on failure); note rows pre-wrap with "Note by <author> · edited", Edit/Delete only where `can_manage` (40px, accessible names); inline editor as local state keyed by note id surviving a refetch that removes the note; ConfirmDialog with the exact copy and confirm disabled while pending; any unrecognised history `kind` renders a generic "Activity" row. `PersonPreview.vue` narrows the widened union to keep notes out of the preview (compile touch, recorded in spec §5). Eleven Vitest cases. |
| `3031f28` | 6 | Walkthrough record under `docs/design/qa/slice-015-2026-09-09/` (eight screenshots) against the lane's own API (`127.0.0.1:31015`), Web (`51015`) and database `crm_slice015_qa`, seeded through the HTTP API (D-021). Centrifugo was not wired for that environment, so the live cross-tab `note_changed` path was not observed; every scenario used a page refetch. |
| `b983f8b` | round 1 | Backend test fixes (below). |
| `10df5e2` | round 2 | Add-path assertions the round-1 batch had missed, the `forbidden` copy on Save, the Escape-while-pending guard, Web test tightenings (below). |
| `082310a` | round 2 completion (coordinator) | Test-only: the add-404 Vitest's mock swap moved before the click (deflaked); the add test's router built with the test's recording publisher so its no-publish assertion observes the HTTP path. |

Coordinator file-list audit per round against `git status` and
`git diff --stat`: 50 files on the branch (5,752 insertions, 42 deletions);
everything under `backend/crates/crm-api/migrations/`, `backend/.sqlx/` (six
new statements), `backend/crates/crm-{app,api,operator}/`, `web/src/` and
`docs/design/qa/`; the three files outside the brief's list are
`crm-api/src/error.rs` (the required error mapping), `crm-operator/src/{lib,
service}.rs` (a re-export and a mock-backend field) and
`web/src/components/PersonPreview.vue` (the compile touch). No new
dependency; no existing statement text changed; no filter, Today or D-052
change. The lane wrote screenshots into the main checkout once (the
Playwright MCP server's working directory), moved them and removed the stray
`.playwright-mcp/` directory; the main checkout was verified clean apart from
the coordinator's own planning edits.

## Lane gates (own tree, per round)

At `6f82709`: `sqlx-prepare && check` green (764 Rust, 656 Vitest, 43 s);
`check-db` 642 of 642 (211 s) on the second run. The first `check-db` run,
and one the coordinator started at the same moment, both failed on
`_sqlx_test_*` database-name collisions ("duplicate key value violates unique
constraint pg_database_datname_index"); the cause was the overlap, not the
code. Rule adopted from then on: only the coordinator runs `check-db`. At
`095b67f`: `check` green (764 Rust, 667 Vitest, 20 s). At `b983f8b`: `check`
green (764 Rust, 667 Vitest, 21 s). At `10df5e2`: `check` green (764 Rust,
677 Vitest, 14 s).

## Review round 1 (of two)

**Backend, on `6f82709`** (reviewer READY WITH FIXES, tester no
implementation defect): every load-bearing clause verified as implemented
(migration, validator strictly narrower than the CHECK, lock order, the
three-column note lookup, tombstone, publish-after-commit, `skip_all` spans,
no body in any error path, Operator filter before truncation, ids-only
realtime). Gaps were tests only; applied in `b983f8b`: composite-FK
rejection inserts; real cross-Organization ids on delete; pure demotion on a
member-authored note for edit and delete plus deactivation on delete; the
`FOR SHARE` membership re-read proven against an in-flight uncommitted
deactivation on a second connection; the same-instant note-vs-correspondence
rank-7 tie-break with equal explicit timestamps and edit keeping position;
the Operator stage-change-survival test made discriminating (the stage
change now older than twenty notes); capture-test positive controls and a
sentinel in the rejected body; the malformed-uuid precedence request sent
with an empty cookie; detail-read assertions after edit and delete; a
normalised-equal edit body → `changed: false`, no publication.

**Web, on `095b67f`**, and verification of `b983f8b` (reviewer READY WITH
FIXES, tester two minor defects): types, realtime scoping, keyed pessimistic
mutations, composer, rows, inline editor, dialog, unknown-kind fallback,
D-045 conformance and text-only rendering all verified. Findings applied in
`10df5e2`:

- the round-1 batch's commit message claimed the add-path cross-Organization
  and `correlation_id` assertions, which had not landed; they now have
  (HTTP POST to a real Organization-B Person and to a random uuid → 404 with
  byte-identical bodies, row counts and publisher unchanged; the detail
  entry's `correlation_id` equals the session's, `occurred_at == recorded_at
  == created_at`);
- the 400 `malformed_request` body is asserted free of the rejected-body
  sentinel;
- **defect (minor):** a Save that returned 403 showed only the generic
  "Could not save this note." and left the editor live; `lib/errors.ts` now
  carries "You can no longer edit this note." for `forbidden`, the detail is
  refetched and the buttons disappear once `can_manage` flips; Save-403,
  Delete-404 and Delete-403 are each pinned;
- **defect (minor):** Escape bypassed the pending guard that the Cancel
  button honoured; `cancelEditNote` now returns while `editNote.isPending`;
- the `can_manage` test located buttons by count and would have passed with
  inverted logic; it now asserts inside each note's own list item;
- new tests: composer pending/double-submit (one POST across a second click
  and Ctrl+Enter), keying and settle (`isMutating` on `personMutationKey`,
  `invalidateQueries` with the person key and never the People key),
  realtime-driven "deleted elsewhere" with no mutation in flight and the
  draft preserved, literal-markup rendering with no `img` element,
  `actor: null` → "Note", `metaKey` and a plain Enter no-op, the edited
  marker matched on the summary element, exactly one Activity row, add-404;
- the walkthrough record's rows 4 and 5 restated to what was observed.

Both reviewers confirmed the `.bg-accent` "one primary" test exclusion is a
single-testid filter that keeps the invariant enforced, and that the
`PersonPreview.vue` touch changes nothing for the other kinds.

## Review round 2 (of two): confirmation on `10df5e2`

Read-only confirmation by the reviewer of every round-2 item, with the
two-round budget spent. Nine of eleven landed and discriminate; the round-2
production lines are the Escape guard, the `forbidden` copy and a test id,
with one recorded edge (a 403 on the composer's POST would now read the
edit-specific copy; LATER). Two items were defective and are corrected in
the coordinator's completion commit (test-only, a few lines each, no
production change), on the reviewer's finding and the coordinator's own
reproduction:

- the add-404 Vitest swapped its GET mock after the click, racing the
  settle refetch (a macrotask from `onError` against `flushPromises`'s own
  macrotask); it failed 1 in 3 on an idle machine. The swap now precedes
  the click; 5 of 5 runs green afterwards;
- the add test's "no publish on either 404" assertion read the test-local
  recording publisher while the HTTP router had minted its own; the router
  is now built with `build_router_with_publisher`, so the assertion
  observes the HTTP path (targeted run green).

Also noted by the reviewer, recorded LATER: the composer double-submit test
proves the disabled attributes rather than the guard (test-utils skips
events on disabled elements); the three refetch tests assert the swapped
GET's effect rather than an explicit GET count.

## Final-tree gates (coordinator, once, on `082310a`)

Run by the coordinator from the worktree root under the shared gate lock,
in one sequence, each once, on 2026-09-09 (an earlier identical sequence on
`10df5e2` had also passed: 764 / 677 / 647; it is superseded by this run on
the final tree):

| Gate | Result |
|---|---|
| `./scripts/sqlx-prepare` | clean; no metadata change (the six new statements were already cached) |
| `./scripts/check` | all checks passed, 32 s: fmt, clippy, cargo check, crate fences, **764** Rust tests, doc tests, Web lint/typecheck/**677** Vitest (48 files)/build, email-worker tests |
| `./scripts/check-db` | all checks passed, 210 s: **647 of 647** DB-backed tests on the first run (twenty-five more than `main`'s 622); neither known flake occurred |

## Recorded LATER (D-050)

- Composite-FK rejection inserts assert `is_err()` rather than SQLSTATE
  `23503` (the house CHECK-matrix pattern).
- The delete-path deactivated-author step asserts no write but not "no
  publication".
- `PersonPreview.test.ts` has no note fixture pinning "notes never render in
  the preview" (the type narrowing guarantees it today).
- The "deleted elsewhere" textarea has no Escape handler (Dismiss only);
  the inline Save button is a third `primary` while editing; focus after a
  delete falls to the body (UI_STYLE nicety).
- Two viewers editing the same note: last writer wins silently (spec rule 4;
  a revision is additive).
- Live cross-tab `note_changed` was not observed (Centrifugo not wired in
  the QA environment); the contract is pinned by `db_realtime` and the
  `invalidationsFor` Vitest. Observe it on the shared dev runtime after the
  merge.
- The ledger sentinel assertion is vacuous by schema (`operator_tool_call`
  has no free-text column); the per-route 128 KiB body limit is
  unobservable behind the 10,000-character validator.
- `NoteError::Database` derives `Debug` (a CHECK violation's detail would
  carry the body; unreachable through the commands because the validator
  is strictly narrower than the CHECK). One line for the O-013 runbook.
- Visually empty bodies (U+200B, U+FEFF) pass validation; same posture as
  inquiry messages.
- Client-minted `NoteId` for an idempotent add across a lost response
  (spec §1).
- Concurrent-`check-db` collision: recorded in the lane-operations memory
  and the brief rule for the next slice.

## Merge readiness

Source `slice-015-notes` at `082310a` plus this record; destination `main`
(docs-only commits ahead of the branch base: the coordinator's spec wording
amendments and project-state records, so no code conflict is possible).
Migration impact: one additive migration `20260912000001_note.sql` (one
table, two indexes, grants; no data change); `crm_dev` must be migrated with
`./scripts/db-migrate` after the merge, with the user's approval; the dev API
must be restarted (new routes and the Operator view), and the production web
server (`dev-web-prod`) rebuilt and restarted for the Web changes. The
`crm_slice015_qa` database is left on the dev Postgres for the user's
inspection and can be dropped afterwards. Unresolved risks: the LATER list
above. Push and deployment are not authorized by this record.
