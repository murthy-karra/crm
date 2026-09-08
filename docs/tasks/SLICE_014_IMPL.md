# Slice 014 — Perceived latency and FilterBar polish: implementation brief

**Status: SPECIFICATION APPROVED 2026-09-08; lane started the same day in
`../crm-worktrees/014` on `slice-014-perceived-latency`.**
[SLICE_014.md](../specs/SLICE_014.md) is authoritative; this brief sequences
the work. One Web lane, one writer, worktree `../crm-worktrees/014` on
`slice-014-perceived-latency` from `main`. Model assignment per
`docs/prompts/MODEL_ROUTING.md`: `implement` profile writes; the coordinator
runs review, test analysis, the once-only final gate and the tunnel
measurement.

## Read first

AGENTS.md (§11, §14); DECISION_LOG D-016, D-017, D-018, D-023, D-045, D-050;
SLICE_014.md in full; docs/design/perceived-latency-2026-09-07.md (§§1–6);
docs/design/UI_STYLE.md; SLICE_011a §6, SLICE_011d §6, SLICE_011e §5,
SLICE_002 §10; docs/prompts/05-implement.md. Then inspect: `web/vite.config.ts`,
`web/package.json`, `scripts/dev-web`, `web/src/api/client.ts` (base URL
resolution), `web/src/api/queries.ts` (the stage, assignment and tag
mutations; `usePerson`; the Today queries; the key factory),
`web/src/query-client.ts` (defaults), `web/src/router.ts` (guard, lazy
routes), `web/src/sessionLifecycle.ts` (do not change), `web/src/views/{LoginView,TodayView,PeopleView,PersonDetailView}.vue`,
`web/src/components/{DataTable,FilterBar,PersonPreview}.vue` and their tests,
`web/src/realtime/events.ts`, `web/src/lib/filter.ts`, `scripts/check`
(the Web gate), README "Start the applications".

## Outcome

The tunnel serves a production bundle (a handful of requests instead of 50–60
modules); stage, assignee and tag changes on the Person page apply instantly
and roll back honestly; Today's chunk and data load during login; People rows
prefetch the Person detail on hover; the FilterBar shows which triggers are
active, hints that chips are editable, and hides a useless Clear all.

## Order of work (each part gated by its Vitest before the next)

**A. Production serving.**
1. `web/vite.config.ts`: add `preview: { port }` with a comment that host,
   `strictPort`, `allowedHosts` and the proxy entries are inherited from
   `server` by Vite's preview resolver (do not duplicate them).
2. New `scripts/dev-web-prod` (same shape as `scripts/dev-web`): build then
   `pnpm exec vite preview`. README paragraph; `.env.example` comment.
3. Verify locally on a scratch port (`CRM_WEB_PORT=51014` via env) that the
   built app loads at `http://127.0.0.1:51014`, calls `/api` through the
   preview proxy, and that the module/script requests issued before
   `DOMContentLoaded` on a cold `/login` load number ≤ 10 (count with a
   Playwright script; report the Today preload's count separately once part
   C exists). Do NOT bind 5173: the user's dev server holds it until the
   coordinator's confirmation.

**B. Optimistic mutations** in `web/src/api/queries.ts` (spec §3 table):
stage, assignment, tag apply, tag remove; `usePerson` passes `signal`;
`onMutate` cancel + snapshot + write (detail and every cached People variant's
matching row via `setQueriesData` on the People prefix; never membership);
`onError` rollback; `onSuccess` writes `data.person` (stage, assignment) or
`data.tags` into the detail (tag apply/remove return `{tags, changed}`);
`onSettled` invalidates as today. Pessimistic fallback when the stages or
members cache is empty. Vitest for spec §8.3–8.5 (a rejected response, a
racing `person.changed` invalidation, exactly one invalidate on settle, the
membership rule, the empty-cache fallback, tag remove rollback on 404, and
that a cancelled `usePerson` query aborts its mocked request).

**C. Preload and prefetch.**
1. New tiny module exporting `preloadTodayView = () => import('./views/TodayView.vue')`;
   the router's Today route record and `LoginView.vue`'s `onMounted` both
   call it (the test spies on the export).
2. `router.ts` guard: after `me` resolves for a Today target with an
   Organization, call `prefetchTodayData(queryClient, session)` (new export
   in `queries.ts`, using named fetchers factored from the existing inline
   query functions; keys from the factory).
3. `DataTable.vue`: optional `onRowIntent` prop on `pointerenter`/`focusin`;
   `PeopleView.vue`: 150 ms single timer, latest row wins, `prefetchQuery`
   of the Person detail with the default `staleTime`.
Vitest for spec §8.6–8.7. For the "exactly one request per Today endpoint"
test, mock the Today endpoints with deferred promises resolved only after
`TodayView` mounts (a `mockResolvedValue` would resolve before mount and the
`refetchOnMount: 'always'` refetch would then be legitimate, not a defect);
also assert no prefetch for a non-Today target or a platform-only session.

**D. FilterBar residue** (`FilterBar.vue`, `FilterBar.test.ts`): trigger
selected state with count; chevron in the chip edit button; Clear all only
with a non-locked clause. Existing FilterBar, PeopleView and TodayFeedsView
tests must pass unchanged, except a test that clicks Clear all in a
locked-only state, which is updated to assert its absence.

**E. Gate and evidence.** `source ~/.nvm/nvm.sh; ./scripts/check` (the Web
gate is part of it; backend steps are unaffected and fast). Record the
scratch-port cold-load request count in the report. The tunnel probe and the
live walkthrough (spec §8.2, §8.10) are run by the coordinator after the
user's port confirmation.

## Rules

`web/**`, `scripts/dev-web-prod`, README and `.env.example` only; no backend,
`.sqlx` or migration changes; no change to `sessionLifecycle.ts` or the `/me`
flow; no new dependencies; keys only through the factory; D-045 and UI_STYLE
bind; never bind port 5173 or 3000; never `pkill -f`; kill only exact PIDs
you started. Checkpoint-commit per part with the
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` trailer; never
commit on `main`, push, merge or rewrite history. Gates in the background,
polled inside one call; take the shared gate lock
(`mkdir /private/tmp/claude-501/crm-gate.lock`, `rmdir` after) around
`./scripts/check` even though this slice does not touch the database, so a
concurrent lane's `sqlx-prepare` is never interleaved.

## Checkpoints requiring the coordinator

- After part A (config, script, scratch-port count), before part B.
- Any need for a new dependency, a change outside the ownership boundary, or
  a wire-contract change.
