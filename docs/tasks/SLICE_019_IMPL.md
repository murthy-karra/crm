# Slice 019a — implementation brief (one lane)

**Status: APPROVED (user, 2026-09-10) at the implementation gate.** Parent specification:
[SLICE_019.md](../specs/SLICE_019.md). Decisions: D-058, D-051, D-053,
D-050, D-021, D-029, D-045. One lane, one writer, branch
`slice-019-custom-fields` from `main` in `../crm-worktrees/019`, the
`implement` profile. The coordinator runs review and test analysis (two
rounds at most), the once-only final-tree gates, the walkthrough, and the
commit and merge gates.

Read first: AGENTS.md (§4.4, §4.6, §4.8, §9, §11, §14); the specification
in full; DECISION_LOG D-058, D-051, D-053, D-021, D-029; SLICE_011e §2,
§3, §5, §6 (the tags precedent you are following for tables, commands,
routes and the Manage page); SLICE_016 §2–§4, §9 (the task pattern for
`origin`/`correlation_id`, grants and authorization);
`docs/prompts/05-implement.md`; `docs/design/UI_STYLE.md`. Inspect before
writing: `crm-api/migrations/20260909000001_tag.sql` and
`20260913000001_task.sql`; `crm-app/src/domain/tag/**` and
`task/commands.rs` (locks, membership re-reads, error shapes);
`crm-app/src/domain/person/queries.rs` (`lock_person`, the detail read);
`crm-api/src/auth/extractors.rs` (`OrgAdminContext`);
`crm-api/src/routes/{tags.rs,people.rs}`; `crm-api/src/error.rs`;
`crm-app/src/realtime/events.rs`; `crm-operator/src/views.rs` and
`crm-api/src/operator/backend.rs` (~396); `web/src/router.ts`,
`web/src/views/TagsView.vue` and test, `web/src/views/PersonDetailView.vue`
(the stage `Select` ~1388 and the card layout), `web/src/api/queries.ts`
(`queryKeys`, the person-mutation key, the settle set),
`web/src/realtime/events.ts`; `crm-api/tests/db_tags.rs` and
`db_schema.rs` (test shapes). State a short plan first.

## Order of work and the checkpoint

**Part A, backend.**

1. Migration `20260915000001_custom_field.sql` exactly as spec §2. Run
   `./scripts/db-migrate` on a scratch database (never `crm_dev`), verify
   the constraints exist by name, then `./scripts/sqlx-prepare` under the
   gate lock.
2. `crm-app/src/domain/custom_field/` (model, error, queries, the seven
   commands per spec §3, the number pattern validator and its unit
   tests); the `PersonChange::CustomFieldChanged` variant and the token
   table; the `custom_fields` assembly in `routes/people.rs::get_person`
   (spec §4; no change to `crm-app/src/domain/person/`).
3. `crm-api/src/routes/custom_fields.rs` (six routes) and the two value
   routes in `routes/people.rs` exactly as spec §4, with numbers bound
   and read as text (spec §2); the `ApiError` variants; spans per spec
   §9.
4. Operator: `PersonDetail.custom_fields` (spec §6), the adapter, the
   prompt parenthetical and a new assertion in the `service.rs` prompt
   tests that the rendered system prompt names "custom field labels and
   values".
5. Tests: `db_custom_fields.rs` registered in `tests/all.rs`;
   `db_schema.rs` enumerations; the `db_operator.rs` sentinel round; per
   spec §12 with the 06-verify calibration (TRUST and CONTRACT cases
   stay; RESTATES cases are one-liners or LATER). Report once the
   `EXPLAIN` of the `person_count` statement at a seeded 25k × 5 shape
   (spec §11), if it costs under ten minutes.

**Checkpoint (stop and report):** when Part A's targeted DB tests are
green and `./scripts/check` is green. The coordinator audits the file
list and releases Part B.

**Part B, Web.** Types, `queryKeys.customFields` (in `queries.ts`, with
the 10 s `staleTime`), queries and mutations on the person-mutation key;
the realtime token and its Person-only handler arm; the Details card on
the Person page with the per-type editors (spec §7); `FieldsView.vue` at
`/manage/fields` with `requiresOrgAdmin` and the nav entry in
`AppShell.vue`; Vitest per spec §12.

## Rules

- Ownership is spec §14 verbatim. Not owned: `crm-app/src/domain/person/`,
  the fourteen filter statements and their `.sqlx` entries,
  `PersonSummary`, `crm-operator/src/tools.rs` and the tool snapshot,
  `docs/`, `Cargo.*` (so no decimal crate: numbers cross as text, spec
  §2), any other migration. Anything that would change a frozen shape, add a
  dependency, touch a filter statement or need a `PersonSummary` change:
  stop and report (AGENTS §11).
- Static SQL only; every statement in the offline cache; literal
  `organization_id` predicates; typed commands for every mutation
  (D-021); values and labels never in spans, logs, error envelopes, the
  ledger or realtime payloads; redacting `Debug` on any struct carrying a
  value or label.
- Do not weaken or delete existing tests. `GET /api/people` and the
  People rows must stay byte-identical (a test pins it).
- Setup in the fresh worktree: copy `.env` from the main checkout
  (gitignored); `source ~/.nvm/nvm.sh` before `pnpm`/`node`;
  `pnpm install --frozen-lockfile` in `web/`; macOS has no `timeout`:
  `perl -e 'alarm shift; exec @ARGV' 1800 ./scripts/check`. Run
  `./scripts/check` in the background and poll.
- Database-backed tests: targeted `cargo nextest` runs replicating
  `scripts/check-db` step 2 with a name filter, under the shared gate lock
  (`mkdir /private/tmp/claude-501/crm-gate.lock`, retry every 30 s,
  `rmdir` when done, always). Never run `./scripts/check-db`; never
  overlap two DB-backed runs; `./scripts/db-migrate` only on your scratch
  database.
- Checkpoint-commit after each numbered step with the
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` trailer.
  Commits only; never push, merge or delete branches.
- Report the exact changed-file list (`git diff --name-status main...HEAD`
  plus untracked); the coordinator diffs it against `git status`. Never
  print field labels, values, note text or secrets in tests, fixtures,
  logs or the handoff beyond synthetic fixture strings.

## Handoff

Per part: outcome per numbered step; the changed-file list; the commit
list; commands and results with exact counts (`check`; each targeted DB
run); the constraint names observed on the scratch database; anything
found outside scope (report, do not fix); unresolved risks; contract
questions.
