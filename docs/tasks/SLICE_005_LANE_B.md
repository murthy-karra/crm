# Task brief — Slice 005, Lane B (web frontend)

Parent specification: `docs/specs/SLICE_005.md` (APPROVED 2026-08-22).
Read, in order: `AGENTS.md` (§5, §7), `docs/decisions/DECISION_LOG.md`
(D-017, D-028, D-029), the full SLICE_005 spec (§5, §9, §10
especially), `docs/design/UI_STYLE.md` (binding), `docs/specs/SLICE_003.md`
§10 and SLICE_004 §10 (shell and view conventions), then the existing
code under `web/src/` — `router.ts`, `App.vue`,
`components/AppShell.vue`, `api/{client,queries,types}.ts`,
`views/PeopleView.vue`, `views/TodayView.vue`, and the Vitest setup.

## Outcome

The web half of SLICE_005 §1: the **Ask** drawer (`OperatorPanel.vue`)
with transcript, Person cards from `references`, pending/error states
with the §10 copy, local 6-message history and Clear; the AppShell
button and `⌘K`/`Ctrl+K` toggle; route-derived screen context; the
`useOperatorTurn` mutation and types; Vitest per §13 item 5.

## Ownership boundary

Owns `web/**` only. Does not touch `backend/**`, `scripts/**`,
`infra/**`, or `docs/**`.

Branch: `slice-005-web` in a worktree (`git worktree add ../crm-web
slice-005-web` from `main`). Steps 1–2 run against a mocked §5 contract;
step 3 integrates against Lane A's `slice-005-operator` branch (API on
:3000) once its step 5 has landed. Merges after Lane A.

## Frozen contracts

SLICE_005 §5 (request `{message, history, context}` with the stated
bounds; response `{turn_id, reply, references.people, tool_calls,
outcome}`; error codes `operator_disabled`, `operator_unavailable`,
`operator_busy` + `Retry-After`, `malformed_request`), §10 (drawer
behavior, error copy, context derivation, plain-text rendering). Hard
rules: `reply` is rendered by text interpolation only — never `v-html`,
never markdown-to-HTML, never auto-linking; cards come **only** from
`references.people`; history is component state only (no localStorage,
no server). Any needed contract change stops work and is reported per
`AGENTS.md` §11.

## Sequence (spec §15)

1. `types.ts` (`OperatorTurnRequest`, `OperatorTurnResponse`,
   `OperatorPersonCard`, `OperatorToolCall`) → `queries.ts`
   (`useOperatorTurn` mutation; no query keys needed; no invalidation —
   nothing changes) → `OperatorPanel.vue` against a mocked endpoint:
   transcript, textarea + Send (disabled when empty or pending), card
   list reusing the `PeopleView` summary row styling, Clear, the three
   error-code messages plus the generic fallback, 6-message history
   window (oldest dropped). Vitest for each.
2. `AppShell.vue`: **Ask** button on Organization routes (hidden on
   `/platform/**` and `/invite/**`), drawer persists across route
   changes while open, `⌘K`/`Ctrl+K` toggles, `Esc` closes; context
   derived from the current route (`today` / `person`+id / `people` /
   `other`) at send time, not at open time. Vitest for toggle, key
   handling, and context derivation per route.
3. Integrate against Lane A: run §1 steps 1–5 by hand on loopback;
   confirm a reply containing a UUID or `<a href>` renders as literal
   text; confirm card navigation leaves the drawer open.

## Required checks before reporting done

`pnpm run lint`, `pnpm run typecheck`, `pnpm run test`, `pnpm run build`
from `web/`; the loopback walkthrough of step 3. Never report a check
as passed unless it ran.

## Stop and report (do not work around)

- Any change needed to the §5 shapes or error codes.
- Any need for a backend change (report it; do not stub around it).
- Any temptation to persist history beyond component state or to
  render the reply as anything but text.

## Report format

Files changed (cross-checked against `git status`); behavior delivered
per §1 step; commands run with results; contract changes (should be
none); unresolved risks and assumptions.
