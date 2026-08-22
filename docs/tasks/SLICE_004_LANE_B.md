# Task brief — Slice 004, Lane B (web frontend)

Parent specification: `docs/specs/SLICE_004.md` (APPROVED 2026-08-22).
Read, in order: `AGENTS.md`, `docs/decisions/DECISION_LOG.md` (D-017,
D-021, D-026, D-027), the full SLICE_004 spec (§5 and §10 especially),
`docs/design/UI_STYLE.md` (binding), `docs/specs/SLICE_003.md` §10 (the
realtime composable and shell conventions), then the existing code under
`web/src/` — `router.ts`, `App.vue`, `components/AppShell.vue`,
`api/{client,queries,types}.ts`, `query-client.ts`, `views/*`, and the
Vitest setup from Slice 003.

## Outcome

The web half of SLICE_004 §1: the `me` shape change and router guards
for the three session shapes (member, admin, platform-only), the
`Manage` and `Platform` nav groups, the public `/invite/:token` page,
`/manage/members` (members with role/status/People-assigned and the
promote / demote / deactivate / reactivate actions; invitations with
invite / re-issue / revoke and the one-time copy-link panel), and
`/platform` with the Organization detail page (create, state badge,
promote, invite admin, revoke).

## Ownership boundary

Owns `web/**` only. Does not touch `backend/**`, `scripts/**`,
`infra/**`, or `docs/**`.

Branch: `slice-004-web` in a worktree (`git worktree add ../crm-web
slice-004-web` from `main`). Step 1 has no backend dependency beyond the
frozen `me` shape; steps 2–4 run against Lane A's `slice-004-admin`
branch (API on :3000) once its steps 2 and 4 have landed. Merges after
Lane A.

## Frozen contracts

SLICE_004 §5 (every route, body, status, and error code; the `me`
shape with nullable `organization {id, name, role}` and `platform_admin`;
`accept_path` returned as a path that the client absolutizes with
`window.location.origin`; the token posted in the JSON body of
`POST /api/invitations/preview` and `/accept`), §9 (error → message
mapping: `last_admin`, `invitation_used`, `invitation_expired`,
`invitation_not_acceptable`, `already_member`, `weak_password`,
`organization_name_taken`), §10 (routes, `meta: { public: true,
allowAuthenticated: true }` for the invite page, nav groups, states,
dialogs). The query-key factory in `web/src/api/queries.ts` is the
single source of keys — extend it, never hand-write a key. After any
mutation invalidate `me` and `['org', orgId, 'members']` (plus the
invitations and platform keys you add). Any needed contract change stops
work and is reported per `AGENTS.md` §11.

## Sequence (spec §15)

1. `types.ts` (`MeResponse` with nullable `organization` + `role`,
   `platform_admin`; member/invitation/platform item types) → router
   guards (platform-only → `/platform`; `/manage/*` requires
   `organization.role === 'admin'` else `/today`; `/platform/*` requires
   `platform_admin` else `/today`; `/invite/:token` public and not
   redirecting a signed-in visitor) → `AppShell` (`Manage` group for
   admins; `Platform`-only shell with no realtime connection for a
   platform-only session; null-safe `me.organization` everywhere; a
   `Platform` footer link for a user who is both) → Vitest tests for the
   guards across the three session shapes and the `organization: null`
   branch. No backend needed.
2. `/invite/:token` (`InviteView.vue`): preview on load; states loading /
   invalid (404) / expired (410) / used (409) / valid → form (display
   name, password with a 12-character minimum hint); submit → set `me`
   from the response → `/today`. Vitest state-machine test over mocked
   responses.
3. `/manage/members` (`MembersView.vue`): members table (name, email,
   role, status, joined, People assigned); row actions with confirm
   dialogs; `last_admin` → inline "You are the last active admin.
   Promote someone else first."; Promote disabled for inactive rows.
   Invitations section (email, role, status, expires, invited by;
   Revoke; Re-issue for pending). Invite dialog (email, role) → on 201 a
   one-time copy-link panel; closing is final.
4. `/platform` (`PlatformOrganizationsView.vue`): table ordered as the
   API returns it, state badge (`ok` / `pending first admin` / `needs
   attention`), New Organization dialog. `/platform/organizations/:id`
   (`PlatformOrganizationView.vue`): members (Promote only, active rows
   only) and invitations (Invite admin / Revoke) with the same copy-link
   panel; a `needs_attention` Organization leads with "Restore admin:
   promote a member or invite a new admin" and both actions enabled.

## Required checks before reporting done

`npm run lint`, `npm run typecheck`, `npm test`, `npm run build` in
`web/` — all green; the §1 walkthrough steps 2–9 in a real browser
against Lane A's branch with zero console errors. Never report a check
as passed unless it ran.

## Stop and report

- Any response shape that differs from §5 as implemented by Lane A
  (report it; do not adapt the client to an undeclared shape).
- Any need for a route or field §5 does not define.
- Any guard case the three session shapes in §10 do not cover.

## Report format

Files changed (cross-checked against `git status` in the worktree);
behavior delivered per §1 step; commands run with results; contract
changes (should be none); unresolved risks and assumptions.
