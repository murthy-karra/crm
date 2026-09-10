# Slice 018 — Verification record

Coordinator-owned evidence for [SLICE_018.md](../specs/SLICE_018.md) and
its [brief](SLICE_018_IMPL.md) under the D-050 budget. Branch
`slice-018-operator-tasks` from `main` at `511b5ea`, worktree
`../crm-worktrees/018`, one lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1. Implementation gate approved by the user on 2026-09-10; D-057.

| Commit | Part | Content |
|---|---|---|
| `dd4b944` | A1 | Migration `20260914000001_operator_task_proposal.sql`: the three auto-named CHECKs dropped by their verified names and re-created explicitly (`tool` admits `create_task`; proposed also requires `task_id IS NULL`; confirmed per tool), `contact_method_id` nullable with `(tool = 'start_call') = (contact_method_id IS NOT NULL)`, two hygiene CHECKs, `task_id` with `GRANT UPDATE`; the sidecar `operator_task_proposal` (title and kind CHECKs verbatim minus the tombstone arm, composite FKs to `person` and `organization_membership`, cascade on Person delete, `SELECT, INSERT` only). |
| `1dd25de` | A2 | crm-operator: `TaskView.task_id`; `complete_task` and `create_task` tool definitions (snapshot +2) and parsers mirroring `TaskTitle::parse`; the seam methods and views with redacting `Debug`; `TurnInput.utc_offset_minutes`; the due instant composed in the parser (end of day 23:59:59 at the client offset); the receipt slot beside the proposal slot, both suppressed on non-reply outcomes; the local-time line; spans `task_id`, `action_outcome`, `title_chars`; the prompt at ten tools. |
| `6008396` | A3 | crm-api: `SqlxToolBackend` gains a `Publisher` and the two methods (context-mismatch guard, `visible_summary` first, the task command with `CommandContext::for_operator`); the turn route validates the offset (400 `malformed_request` outside ±840 or non-integer); the response `receipt` and the `proposal` union; the confirm route branches on the claimed row's `tool` after the claim (`create_task` needs no telephony or runtime, 201 `{task}`); `ProposalConsumed { call_id, task_id }`. |
| `c6a1d38` | A4 | `db_operator_task.rs` (19 tests) registered in `tests/all.rs`; the CaptureWriter sentinel round for `create_task`; two `proposal_consumed` bodies widened for the additive `task_id`. |
| `edf6906`, `6c27c1c` | A | fmt; `ToolEffect::Receipt` boxed for clippy. |
| `7773b91`, `5e3cf38` | B | Web: the `OperatorProposal` union (`OperatorStartCallProposal` renamed, `OperatorCreateTaskProposal` added), `OperatorReceipt`, the turn request's `utc_offset_minutes`; `useConfirmTaskProposal`; the receipt card with Undo through `useReopenTaskMutation`, the create card, the pinned consumed copy, invalidation on receipt, state cleared on identity change; three additive helpers in `lib/operator.ts`. |
| `e3e062b`, `330579a`, `f7d8062`, `2ba3922` | fixes | Round-1 fixes (below). |
| `20131e9` | walkthrough | The local-time line names the weekday (below). |

Coordinator file-list audits (Part A checkpoint, Part B, after the fix
round): every file inside the brief's ownership except three accepted
excursions: two Today-source test files carrying only the mechanical
`Publisher` argument and `utc_offset_minutes: None` the seam change
forces, and `web/src/lib/operator.ts` plus its test holding three pure
formatting helpers. No `crm-app`, `docs/`, `Cargo.*` or telephony change.
No new dependency. The stash stack empty throughout.

Deviation accepted at the Part A checkpoint: a member deactivated between
propose and confirm is refused 401 by the session extractor before the
route runs, so the spec's 403 is unreachable over HTTP; the lane tests 401
over HTTP and the command's `Forbidden` pass-through directly. Spec §5 and
§10 were amended on `main` (`dedf344`) together with the 400 code name and
the one-action-per-turn wording (the guard keys on a standing receipt).

## Lane gates (own tree)

Part A at `6c27c1c`: unit 102 crm-operator, 119 crm-api; targeted DB run
under the gate lock 79 of 79 across the four `db_operator*` files;
`check` green (804 Rust, 751 Vitest, 11 worker). Part B at `5e3cf38`:
`check` green (804 Rust, 767 Vitest). After the fix round at `2ba3922`:
targeted DB 81 of 81; `check` green (807 Rust, 780 Vitest, 44 s). After
`20131e9`: `check` green (807 Rust, 780 Vitest, 50 s).

## Review round 1 (of two)

Run in two halves so the backend review overlapped the Web build:
backend at `6c27c1c` (reviewer READY WITH FIXES, tester no blocking
finding), Web at `5e3cf38` (reviewer READY WITH FIXES, tester no blocking
finding). Verified: the tool schemas and wire shapes match §3 and §5 byte
for byte; the migration matches §4 with the verified constraint names;
authorization is delegated entirely to the task commands with the adapter
adding nothing; the D-034 fence unchanged; no title reaches spans, logs,
error envelopes, the ledger or realtime; the loop rules and the due-instant
composition; the Web cards render only from server data and the panel
narrows on `kind` before reaching the call host. Applied in one
consolidated fix round (twenty items):

- backend, all test hardening plus one bind: the spec-required 400 tests
  for the offset; the CHECK matrix asserting constraint names for six
  inserts with a positive control and the grants asserting SQLSTATE
  42501; the alice-on-bob's-task isolation case with a real byte-identity
  loop; a same-Organization non-permitted member's Undo → 403; the
  sidecar read on confirm scoped by `organization_id` (defence in depth;
  `.sqlx` regenerated); a foreign-Organization assignee name →
  `needs_clarification` with no row; a full propose → confirm round trip
  with kind, due date at offset 0 and a named assignee pinning the task
  row and `origin = 'operator'`; the race test asserting the tool outcome;
  a positive control on the capture sentinel; a unit test that a
  `complete_task` after `forbidden` still reaches the backend; extra
  due-instant cases (a date-boundary crossing, the exact ±840 instants,
  the DST pair);
- Web, two defects and seven tests: **receipt state was keyed by task id**,
  so completing the same task twice in a session inherited the first
  card's finalized Undo (now keyed by transcript entry); **a confirm that
  failed for a task-level reason stayed retryable** although the backend
  had finalized the proposal, so the second click would have shown "This
  task was already added." with no task (now final with an "ask again"
  copy; only a network error stays retryable; 409 consumed branches on
  `task_id`); a DST-safe "yesterday"; invalidation tests including Undo's
  POST-before-invalidate order; the offset sign pinned through a mocked
  `getTimezoneOffset`; markup-as-text tests; double-click guards and a
  real retry; the confirm helper settling on its variables' Person;
  `break-words` on the titles.

## Review round 2 (confirmation) on `2ba3922`

Reviewer: every one of the twenty items CONFIRMED, nothing weakened or
removed across the whole diff, verdict READY. One new LATER: the "ask
again" suffix reads oddly after the 503 and 401 copies (state machine
correct; copy only).

## Walkthrough (coordinator, 2026-09-10, QA runtime)

Against the lane's own API on `127.0.0.1:31018` and production Web build
on `51018` from the worktree, database `crm_slice018_qa` on the dev
Postgres (owner transferred to `crm_migrator`, migrated with the
worktree's `db-migrate`, platform admin bootstrapped, seeded through the
HTTP API), never `crm_dev` or ports 3000/5173. Archive:
`docs/design/qa/slice-018-2026-09-10/`. The QA preview's websocket proxy
answered 403 throughout (a QA-origin restriction; realtime is outside this
slice), so every page update below happened through the panel's own
invalidation.

1. alice adds Grace Hopper and a task "Call about the listing" due today
   from the Person page.
2. "Mark the call task for Grace Hopper as done" → the Operator completes
   it; receipt card "Completed: Call about the listing · Follow up · was
   due today" with Undo; the Person page shows "No open tasks" without a
   reload (`018-01`).
3. Undo → "Reopened", Undo disabled, the task back in the open list with
   its Complete button (`018-02`).
4. "Add a task to call Grace Hopper on Friday" → a proposal card "Add task
   for Grace Hopper: Call Grace Hopper · Call · due Sun, Sep 13, 11:59 PM ·
   assigned to Alice Anderson" (`018-03`). **Finding:** today is Thursday
   the 10th, so the model resolved "Friday" two days late; the card
   exposed it before anything existed, which is what D-057 §2 is for. The
   local-time line gave an RFC 3339 instant with no weekday. Fixed in
   `20131e9`: the with-offset line reads "Thursday, 2026-09-10T…-07:00";
   the `Z` arm unchanged; unit test pins both. Confirm on this card →
   "Task added."; the task row `kind = 'call'`, `origin = 'operator'`,
   `correlation_id` set, `due_at` 2026-09-13 23:59:59 local; the proposal
   `confirmed` with its `task_id` (`018-04`).
5. bob (Best Realty) POSTs the confirm for a fresh alice proposal → 404
   `not_found`; the proposal stays `proposed`; alice's session restored.
6. On the fixed build: the same Friday request → "due Fri, Sep 11,
   11:59 PM" (`018-05`). The QA API restarted with `GROQ_API_KEY` unset
   ("operator disabled" logged); Confirm on that card within the TTL →
   "Task added."; the task row due Fri 2026-09-11 23:59:59 local,
   `origin = 'operator'` (`018-06`).
7. The earlier unconfirmed proposal → 409 `proposal_expired`.
8. Not exercised live: a second `complete_task` in one turn (unit-tested
   in crm-operator) and the timeline after completing from the page (the
   row's `origin` is the evidence; the timeline names the completer).

## Recorded LATER (D-050)

- A `forbidden` ledger row carries no Person id (the card list is empty
  on `Forbidden`; §2 freezes the unit variant) — a spec amendment if the
  ledger should name the Person.
- The "ask again" suffix after the 503 and 401 copies on the create card.
- The `create_task` tool description still says the current-time line
  gives the local time; it now also names the weekday (description only).
- `already_completed` for a non-permitted member returns `forbidden`
  (correct, untested); `proposal_consumed` on the confirm 404s asserts
  status only; `finalize` ignores `rows_affected == 0`; "Me" is not
  treated as "me".
- Expiry-during-display for both cards is untested (fake timers); a
  future `due_at` reads "was due <date>"; the org id is parsed from the
  identity key; `completed_at` is `Option` on the backend view while the
  wire contract says it is always present on a completed receipt.
- Sidecar title retention until Person erasure (spec §4; a sweep or
  empty-on-finalize if ever needed).
- The migration on populated data: every pre-018 row satisfies the new
  CHECKs by construction (reasoning, no test).

## Final-tree gates (coordinator, once per final tree, under the gate lock)

| Tree | Gate | Result |
|---|---|---|
| `2ba3922` | `sqlx-prepare` | clean, no diff |
| `2ba3922` | `check` | all checks passed, 32 s: **807** Rust, 5 doc, **780** Vitest, 11 worker |
| `2ba3922` | `check-db` | all checks passed, 280 s: **741 of 741** on the first run |
| `20131e9` | `sqlx-prepare` | clean, no diff |
| `20131e9` | `check` | all checks passed, 33 s: **807** Rust, 5 doc, **780** Vitest, 11 worker |
| `20131e9` | `check-db` | all checks passed, 254 s: **741 of 741** on the first run |

## Merge readiness

Source `slice-018-operator-tasks` at `20131e9` plus this record and the
QA archive; destination `main` (docs-only commits ahead of the branch
base: the spec wording corrections, so no code conflict is possible).
Migration impact: one additive migration on `operator_proposal` plus the
new `operator_task_proposal` table; `crm_dev` must be migrated with
`./scripts/db-migrate` after the merge and the dev API restarted by exact
PID (the Operator crate, the adapter and the routes changed); the
production web server (`dev-web-prod`) rebuilt and restarted for the
panel. The `crm_slice018_qa` database can be dropped. Unresolved risks:
the LATER list above. Push and deployment are not authorized by this
record.
