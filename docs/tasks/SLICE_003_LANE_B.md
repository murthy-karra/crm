# Task brief — Slice 003, Lane B (web frontend)

Parent specification: `docs/specs/SLICE_003.md` (APPROVED 2026-08-21).
Read, in order: `AGENTS.md`, `docs/decisions/DECISION_LOG.md` (D-017,
D-022, D-023), the full SLICE_003 spec (§5, §6, §9, §10, §13 criterion 8
are yours), `docs/specs/SLICE_002.md` §10 (the stack and layout you
extend), `docs/design/UI_STYLE.md` (binding), then the existing code under
`web/src/`.

## Outcome

The web half of SLICE_003 §1: the Today view as the landing route, the
Log-contact dialog (Today rows and Person detail), the `contact_attempted`
history entry, and the realtime composable that turns Centrifugo events
into TanStack Query invalidations with refetch-on-reconnect and a quiet
connection indicator — with Vitest coverage of the pure and composable
logic.

## Ownership boundary

Owns `web/**` only. Does not touch `backend/**`, `scripts/**`,
`infra/**`, or `docs/**`.

Branch: `slice-003-web` in a worktree (`git worktree add ../crm-web
slice-003-web` from `main`). Step 1 has no backend dependency; step 2
runs against Lane A's `slice-003-realtime` branch (API on :3000,
Centrifugo on :8000) once its step 1 has landed. Merges after Lane A.

## Frozen contracts

SLICE_003 §5 (`TodayItem`, `TodayReason` discriminated on `code`, the
contact-attempts 201 body, `{"token"}`), §6 (event JSON, the exact query
keys to invalidate, `organization_id` drop rule), §9 (`getToken` error
mapping), §10 (files, SDK-state → status mapping, pill visibility). The
query-key factory in `web/src/api/queries.ts` is the single source of
keys — extend it, never hand-write a key. Any needed contract change
stops work and is reported per `AGENTS.md` §11.

## Sequence (spec §15)

1. `centrifuge` (pinned) + `vitest` (devDep; `"test": "vitest run"`;
   ESLint/tsconfig cover `*.test.ts`) + Vite `/connection` ws proxy
   (`CRM_WEB_REALTIME_PROXY_TARGET`) + `realtime/client.ts`
   (`resolveRealtimeUrl(location)`), `realtime/events.ts`
   (`invalidationsFor`, `reconnectInvalidations`),
   `realtime/useRealtime.ts` (injected client factory; status mapping;
   250 ms coalescing; invalidate-all on reconnect; `getToken`: 401 →
   `UnauthorizedError` + invalidate `me`, anything else rethrown;
   disconnect on unmount / empty orgId) — all with Vitest tests
   (criterion 8). `pnpm install` updates the lockfile; `bootstrap` keeps
   `--frozen-lockfile`.
2. `api/types.ts` + `api/queries.ts` (`queryKeys.today`, `useToday` with
   `refetchInterval: 60_000`, `useLogContactMutation`,
   `fetchRealtimeToken`) → `TodayView.vue`, `LogContactDialog.vue`,
   `PersonDetailView.vue` additions (history icon/summary for
   `contact_attempted`; "Log contact" secondary button), `AppShell.vue`
   (nav `Work: Today, People`; pill only for `reconnecting` /
   `unavailable`; installs `useRealtime` once), `router.ts` + the two
   other hard-coded `/people` landings (`LoginView.vue` post-login,
   signed-in visitor to `/login`) → `/today`.

UI: `UI_STYLE.md` governs; reasons render in wire order as neutral
badges; priority by text weight, not color; the dialog's button is the
view's one primary; "Updated …" uses TanStack `dataUpdatedAt`.

## Required checks before reporting done

`pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build` (the web part of
`./scripts/check`). Then the live check against Lane A's branch: two
browser tabs, a lead posted via `scripts/demo-leads` appears without
refresh; Log contact removes the row in both tabs; stopping Centrifugo
shows the pill and restarting it clears it with a refetch. Never report a
check as passed unless it ran.

## Stop and report

- Any §5/§6 shape that does not match what Lane A's branch actually
  returns (do not adapt silently — report the discrepancy).
- The `centrifuge` SDK's `UnauthorizedError` / refresh semantics
  differing from §9 on the pinned version.
- Anything requiring a change outside `web/**`.

## Report format

Files changed (cross-checked against `git status` in the worktree);
behavior delivered per §1 step; commands run with results; contract
discrepancies found; unresolved risks.
