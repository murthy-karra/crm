# Slice 011d — Tweakable built-ins implementation brief

**Status: SPECIFICATION APPROVED 2026-09-07; implementation HELD.** The user
approved [SLICE_011d.md](../specs/SLICE_011d.md) after independent review and
chose to start the lanes in a later session. Before starting, set the model
assignment below, create the integration branch and worktrees, and confirm the
base is still current `main`. The
[plain-language companion](../specs/SLICE_011d_EXPLAINED.md) explains the
effect. The user chose on 2026-09-06 to deliver 011d as one L rung with two
parallel lanes.

Base: local `main` at `f51bff8`. Integration branch:
`slice-011d-today-system-feeds`. Lane worktrees branch from it and merge back
through the coordinator. Model assignment (set 2026-09-07 at the start): **Claude Sonnet 5**
writes each lane in its own worktree (`../crm-worktrees/011d-lane-b` on
`slice-011d-lane-b`, `../crm-worktrees/011d-lane-w` on `slice-011d-lane-w`);
**Claude Fable 5.1** coordinates, merges lanes into the integration branch,
runs the once-only final-tree gates and the independent review and test
analysis. This is the 011c-takeover pattern.

## Read first

AGENTS.md; DECISION_LOG (D-005, D-010, D-022, D-033, D-042, D-043, D-046,
D-047 and the follow-ups under D-047); the architecture baseline; the 011
ladder (decision 2); SLICE_011d.md in full; SLICE_011a §4, SLICE_011b §§3–5,
SLICE_011c §§4/5/8, SLICE_003 §§3–5, SLICE_004 §5, SLICE_006c §5a,
SLICE_009 §6, SLICE_005 §§3/7; `docs/prompts/05-implement.md`;
`docs/design/UI_STYLE.md` (Lane W). Then inspect the code named under each
lane before writing. Report a genuine contract or decision conflict to the
coordinator instead of resolving it locally.

## Outcome

Today is served only through three per-Organization system feeds evaluated in
the filter vocabulary, with byte-identical items, reasons, tiers and order for
an unedited Organization; admins can edit, preview, disable and revert each
feed with an audit fact; members see whether a rule was changed; the three
derived clause kinds work everywhere the vocabulary is accepted.

## Lane B — backend and database (one writer; sole migration and SQLx owner)

Owns everything under `backend/` and the performance/QA evidence directories.
Key files: `crm-app/src/domain/person/{filter.rs,queries.rs,sql/*.sql}`,
`crm-app/src/domain/today/{mod.rs,queries.rs,rank.rs,model.rs,sources.rs,*.sql}`,
a new `crm-app/src/domain/today/system_feeds/` module (model, canonical
defaults, seed, commands, preview, evaluation statements),
`crm-app/src/domain/admin/commands/create_organization.rs`,
`crm-app/src/domain/{facts.rs,envelope.rs}`, a new migration
`crm-api/migrations/20260908000001_today_system_feed.sql` (the 20260907 version
is taken by the saved-list sort migration),
`crm-api/src/routes/{today.rs,organization.rs}` or a new
`routes/today_feeds.rs`, `crm-api/src/operator/*` and `crm-operator/src/views.rs`
for the additive `system_feed_issues` field, tests under `crm-api/tests/`
(`db_people_filter.rs`, `db_today_builtin_parity.rs`, `db_today*.rs`,
`db_schema.rs`, `db_admin.rs`, `db_today_http_perf.rs`, new
`db_today_system_feeds.rs` and `db_today_feed_equivalence.rs`, and the
`tests/fixtures/` frozen-SQL pattern).

Order of work, each step gated by its own tests before the next:

1. **Vocabulary:** three `Clause` kinds, validation, `describe()`, kinds
   field, `PersonFilterParams` (+3 bools, + viewer), the eleven statements
   with NULL-guarded predicates in identical positions, `.sqlx` regenerated,
   parity tests per axis (spec §9.1). No Today change yet.
2. **Persistence:** migration (feed table, fact table with envelope and
   append-only triggers, backfill), seed in `create_organization`, model and
   canonical defaults, `db_schema.rs` enumerations.
3. **Feed path behind a provider seam:** introduce a built-in provider
   (`Legacy | Feeds`) inside the Today query, selectable only under
   `test-support`, default `Feeds`; `Legacy` keeps the compiled-in
   statement so the complete path including the list-source merge runs
   both ways. Add the person-state statement, the call-feed statements, the
   merge function and the feed-row loader with fallback. Land
   `db_today_feed_equivalence.rs` (spec §9.2) and keep every existing Today
   suite green. Stop here and report to the coordinator with the
   equivalence results; **do not delete the `Legacy` provider until told**.
4. **Commands, preview, routes, Operator field, facts, authorization tests**
   (spec §§4/6/9.3–9.6/9.8/9.9).
5. **Performance evidence** (spec §8) under `docs/design/perf/slice-011d-<date>/`,
   using the `Legacy` provider for the paired zero-source baseline, and
   including the EXPLAIN pair with and without `enable_mergejoin = off`.
   Report; the coordinator decides the toggle question.
6. On instruction: delete the `Legacy` provider and its seam, freeze its SQL
   under `tests/fixtures/today_f51bff8/`, rerun the suites.

Rules: static SQL only, literal Organization predicates, no new index or pool
change, no dynamic SQL, no reason-code or tier changes, no edits to Web
files. Run `./scripts/sqlx-prepare`, `./scripts/check` and `./scripts/check-db`
per round in the lane worktree (source `~/.nvm/nvm.sh` first in a
non-interactive shell). Report actual results, never assumed ones.

## Lane W — web (one writer)

Owns everything under `web/`. Key files: `src/api/types.ts` (mirror spec §6
verbatim), `src/api/queries.ts` or the existing query-factory module,
`src/components/FilterBar.vue`, `src/components/AppShell.vue` (nav),
`src/router.ts` (admin-only `/manage/today-feeds` meta), new
`src/views/TodayFeedsView.vue` and its tests, `src/views/TodayView.vue` and
the Manage sources panel (Rules section, notices), Vitest files, and the
walkthrough script/recording under `docs/design/qa/slice-011d-<date>/`.

Order of work:

1. Types and query keys from spec §6; MSW or fetch stubs matching the
   contracts for tests.
2. Filter bar chips for the three boolean kinds with the `describe()`
   wording; locked-clause mode (anchor and `me` visible, not removable) used
   by the feed editor only.
3. Today rules page: cards, edit, preview with member picker and honest empty
   state, revert and off confirmations (typed name for the unanswered-inquiry
   feed), 409 reload flow, uncertain-mutation refetch, session-identity fence.
4. Today: Rules section markers, `system_feed_issues` notices, invalidation
   on feed mutations.
5. Vitest coverage for spec §9.7 and the browser walkthrough once Lane B's
   routes are in the integration branch.

Rules: no re-sorting of Today rows, no rule JSON or names on the realtime
channel, D-045 controls and UI_STYLE, no edits to backend files. Run the Web
lint/type/test gate per round.

## Coordinator

Owns this brief, the spec, the ladder, PROJECT_STATE, DECISION_LOG entries,
amendment pointers in 003/004/005/006c/009/011a/011b/011c, lane merges into
the integration branch, the arm-deletion and toggle decisions, the once-only
final-tree gates (`sqlx-prepare`, `check`, `check-db`), the per-round
file-list audit against `git status`, the independent reviewer and tester
runs, and the commit/merge gates with the user.

## Checkpoints requiring the coordinator

- Lane B step 3 equivalence results (arm deletion gate).
- Any contract deviation from spec §§2–6 (stop and report; AGENTS §11).
- Lane B step 5 evidence and the merge-join toggle question.
- Any need to touch a file outside the lane's ownership.
