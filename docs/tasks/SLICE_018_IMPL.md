# Slice 018 — implementation brief (one lane)

**Status: APPROVED (user, 2026-09-10) at the implementation gate.** Parent specification:
[SLICE_018.md](../specs/SLICE_018.md) (reviewed READY WITH CORRECTIONS,
all applied). Decisions: D-057, D-054 §3, D-053, D-034, D-033, D-029,
D-050. One lane, one writer, branch `slice-018-operator-tasks` from
`main` in `../crm-worktrees/018`, the `implement` profile. The
coordinator runs review and test analysis (two rounds at most), the
once-only final-tree gates, the walkthrough, and the commit and merge
gates.

Read first: AGENTS.md (§5, §9, §11, §14); the specification in full;
DECISION_LOG D-057, D-053, D-034, D-029; SLICE_006b (the `start_call`
propose → confirm → receipt precedent you are extending); SLICE_016 §2–§4,
§7 (tasks); SLICE_005 §5, §9 (turn contract, ledger);
`docs/prompts/05-implement.md`; `docs/design/UI_STYLE.md` for the cards.
Inspect before writing: `crm-operator/src/{tools.rs,backend.rs,service.rs,views.rs}`,
`prompts/system.md`, `tests/snapshots/tool_definitions.json`;
`crm-api/src/operator/{backend.rs,filter.rs}`, `routes/operator.rs`,
`error.rs`, the proposal migration `20260827000001`, the task migration
`20260913000001`; `crm-app/src/domain/task/{commands.rs,queries.rs,model.rs}`
(read-only for you); `web/src/components/OperatorPanel.vue` and test,
`web/src/api/{types.ts,queries.ts}`. State a short plan first.

## Order of work and the checkpoint

**Part A, backend.**

1. Migration `20260914000001_operator_task_proposal.sql` exactly as spec
   §4 (the verified constraint names, the two hygiene CHECKs, the sidecar
   with `GRANT SELECT, INSERT` only). Run `./scripts/db-migrate` on a
   scratch DB, confirm the constraint names matched, then
   `./scripts/sqlx-prepare`.
2. crm-operator: `TaskView.task_id`; the two tool definitions (spec §3
   schemas, snapshot +2); the argument parsers including the due-instant
   composition from `TurnState`'s offset; the seam methods and views
   (spec §2); loop rules (one receipt per turn; receipt absent on 503);
   the local-time line; the prompt (spec §7) and prompt-rule tests.
3. crm-api: `TurnInput.utc_offset_minutes` from the turn request (400
   outside −840..=840 or non-integer); `SqlxToolBackend` gains a
   `Publisher` and the two methods (context-mismatch guard first,
   `visible_summary` first, then the task command with
   `CommandContext::for_operator`); the turn response `receipt` and the
   `proposal` union; the confirm route branching after the claim (spec
   §5, both stated branch cases); `ProposalConsumed { call_id, task_id }`
   in `error.rs`; spans (spec §11).
4. Tests: crm-operator unit; crm-api unit; `db_operator_task.rs`
   registered in `tests/all.rs`, per spec §12 with its calibration.

**Checkpoint (stop and report):** when Part A's targeted DB tests are
green and `./scripts/check` is green. The coordinator audits the file
list and releases Part B.

**Part B, Web.** `types.ts` union and receipt type; `queries.ts`
`useConfirmTaskProposal` and the `utc_offset_minutes` on the turn
request; `OperatorPanel.vue` receipt card with Undo through
`useReopenTaskMutation`, the create card, the consumed copy, invalidation
on receipt, clearing on identity change; Vitest per spec §12.

## Rules

- Ownership (spec §14): `backend/crates/crm-operator/**`;
  `backend/crates/crm-api/src/operator/**`, `routes/operator.rs`,
  `error.rs` (the `ProposalConsumed` body only); the new migration;
  `.sqlx/`; `backend/crates/crm-api/tests/db_operator_task.rs`,
  `tests/all.rs`, and the existing `db_operator*.rs` only where the
  snapshot or capture tests require; `web/src/components/OperatorPanel.vue`
  and its test; `web/src/api/types.ts`, `web/src/api/queries.ts`. Not
  edited: `web/src/telephony/**`, `web/src/lib/errors.ts`. Not owned:
  `crm-app/**` (use the task commands as they are; if one needs a change,
  stop and report), `docs/`, `Cargo.*`, any other migration.
- The frozen contracts are spec §3 and §5 verbatim. Anything that would
  change a shape, add a route, add a dependency or touch `crm-app`: stop
  and report (AGENTS §11).
- The model never supplies an instant, a user id, an Organization or an
  actor; the snapshot test's forbidden-property check must stay green.
  Titles never reach spans, logs, error envelopes, the ledger or realtime
  payloads.
- Do not weaken or delete existing tests; the `start_call` chain and the
  `db_operator_call.rs` tests must stay green under the rewritten CHECKs.
- Setup in the fresh worktree: copy `.env` from the main checkout
  (gitignored); `source ~/.nvm/nvm.sh` before `pnpm`/`node`;
  `pnpm install --frozen-lockfile` in `web/`; macOS has no `timeout`:
  `perl -e 'alarm shift; exec @ARGV' 1800 ./scripts/check`. Run
  `./scripts/check` in the background and poll.
- Database-backed tests: targeted `cargo nextest` runs replicating
  `scripts/check-db` step 2 with a name filter, under the shared gate lock
  (`mkdir /private/tmp/claude-501/crm-gate.lock`, retry every 30 s,
  `rmdir` when done, always). Never run `./scripts/check-db`; never
  overlap two DB-backed runs. `./scripts/db-migrate` only on your scratch
  DB, never on `crm_dev` (the coordinator migrates it after the merge).
- Checkpoint-commit after each numbered step with the
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` trailer.
  Commits only; never push, merge or delete branches.
- Report the exact changed-file list (`git diff --name-status main...HEAD`
  plus untracked); the coordinator diffs it against `git status`. Never
  print task titles, note text, message content or secrets in tests,
  fixtures, logs or the handoff.

## Handoff

Per part: outcome per numbered step; the changed-file list; the commit
list; commands and results with exact counts (`check`; each targeted DB
run); anything found outside scope (report, do not fix); unresolved
risks; contract questions.
