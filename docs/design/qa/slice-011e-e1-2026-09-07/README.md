# Slice 011e (e1) — live browser walkthrough

Recorded 2026-09-07 against `slice-011e-tags` at `e7530d2` (backend commit
`502f418` + web commit `e7530d2`) per
[SLICE_011e.md](../../../specs/SLICE_011e.md) §9.10, with the walkthrough
scenarios adapted to the seeded roles by the coordinator. Real
Playwright-driven Chrome against a real crm-api + Vite QA runtime and a
real scratch PostgreSQL database — no mocked routes, no fabricated
screenshots.

## QA runtime

- Scratch database `crm_011e_qa_20260907_175409` in the existing shared
  OrbStack Postgres instance (never `crm_dev`), created via `docker exec`
  into the running `development-postgres-1` container as the container
  superuser (`CREATE DATABASE ... OWNER crm_migrator`, `GRANT CONNECT ...
  TO crm_app`) — the same technique `scripts/dev-services`'
  `provision-roles.sql` uses, chosen because no local `psql` client is
  installed on this machine. Migrated with `cargo run -p crm-api --bin
  migrate` against an overridden `MIGRATION_DATABASE_URL` process
  environment variable — the same migration binary `scripts/db-migrate`
  invokes, no `.env` file mutated (`dotenvy::dotenv()` never overrides an
  already-set process variable).
- Platform admin bootstrapped with `crm-admin bootstrap-platform-admin`
  (the one sanctioned non-HTTP step, D-021; its password comes only from
  `CRM_DEV_SEED_PASSWORD`, never printed), then two Organizations seeded
  entirely through the running HTTP API with the unmodified
  `scripts/seed_dev.py` (Acme Realty: alice@acme.test admin, carol@acme.test
  member; Best Realty: bob@best.test admin, dave@best.test member) and the
  unmodified `scripts/demo-leads` run once as alice (five inquiries,
  including Marcus Chen assigned to Carol — the walkthrough's Person
  fixture).
- `crm-api` built from this worktree, bound to `127.0.0.1:31011`, with
  `DATABASE_URL` overridden to the scratch database via a process-level env
  var (the worktree's own tracked `.env` — used only for secrets/session
  config — was read but never edited).
- Vite dev server for `web/` on `127.0.0.1:51011`, `CRM_WEB_API_PROXY_TARGET`
  overridden to `http://127.0.0.1:31011` the same way, so the browser only
  ever talks to one origin (`:51011`) and the API proxy carries `/api/*` —
  no CORS/cookie cross-origin concerns, matching normal dev-proxy behavior.
- Chrome driven via `playwright-core`'s `chromium.launch({ channel: 'chrome',
  headless: true })` (already a `web/` dev dependency; the same mechanism
  `web/scripts/capture-create-list-guide.mjs` uses) from a scratch script
  (`web/scripts/qa-011e-walkthrough.mjs`), run and then deleted — not part
  of this commit. Each scenario used its own isolated `BrowserContext` per
  actor (Carol, Alice, Bob, Dave, the platform admin, and an unauthenticated
  context), mirroring separate real sessions.
- **User's environment untouched throughout and verified after teardown**:
  dev-api on `:3000` (`GET /api/health` → `{"status":"ok"}`) and dev-web on
  `:5173` (`200`) kept answering before, during, and after the QA runtime
  ran; `crm_dev` was never connected to by anything in this walkthrough
  (confirmed after the fact: `SELECT datname FROM pg_database WHERE
  datname LIKE 'crm%'` on the shared instance returns only `crm_dev` once
  the scratch database was dropped).
- **Teardown**: QA `crm-api` (PID 23352) and the QA Vite process (PID
  24298, its `pnpm run dev` parent PID 24284) killed by their own exact
  PIDs (never `pkill -f` a binary path — the documented hazard for this
  worktree, since the user's own dev-api and dev-web run the same binary
  paths); ports `31011`/`51011` confirmed free afterward with `lsof`;
  scratch database `crm_011e_qa_20260907_175409` dropped and confirmed
  absent; the scratch walkthrough script removed from `web/scripts/`.
  Evidence for all of this is in the implementation lane's final report,
  not this file (this README is evidence of the *application behavior*,
  not the infrastructure teardown).

## Results

Fixture: `Marcus Chen`, created by `demo-leads`' manual lead and assigned
to Carol, is the walkthrough's one Person throughout.

| # | Scenario (spec §9.10, adapted) | Screenshot(s) | Result |
|---|---|---|---|
| 1 | Carol creates "Investr" inline on the Person page: `POST /api/tags` then `PUT /api/people/{id}/tags/{tag_id}`, in that order; the chip appears, monochrome, with an accessible "Remove tag Investr" button (40px target) | [01a](01a-carol-create-tag-popover.png), [01b](01b-carol-chip-created.png) | PASS |
| 2 | Carol removes the chip (`DELETE`, `changed:true`); it disappears | [02](02-carol-chip-removed.png) | PASS |
| 3 | Carol reaches `/manage/tags` via the Manage nav (visible to a member); "Investr" shows people 0, Rename/Delete enabled (creator, unused); renames it to "Investor" inline | [03a](03a-carol-manage-tags-before-rename.png), [03b](03b-carol-manage-tags-after-rename.png) | PASS |
| 4 | Carol re-applies "Investor" to the Person from the popover (existing tag → `PUT` only, no `POST`) | [04](04-carol-reapplies-existing-tag.png) | PASS |
| 5 | `/manage/tags` now shows people 1 with Rename/Delete disabled and the tooltip "Only an admin can change a tag that is in use"; a direct `PUT /api/tags/{id}` as Carol (in-page `fetch`) is `403 forbidden` | [05a](05a-carol-manage-tags-locked.png) | PASS |
| 6 | Alice (admin) opens the same Person: chip visible; opens the People preview for that Person: read-only chip, no remove control | [06a](06a-alice-person-page-chip.png), [06b](06b-alice-people-preview-readonly-chip.png) | PASS |
| 7 | Alice on `/manage/tags`: Rename enabled; renames to "Investor client"; Delete opens the confirm dialog with count 1 and the "Saved lists and Today rules..." sentence; confirm; row gone | [07a](07a-alice-renamed-investor-client.png), [07b](07b-alice-delete-confirm-dialog.png), [07c](07c-alice-after-delete-row-gone.png) | PASS |
| 8 | Carol reloads the Person: chip gone | [08](08-carol-person-no-chips-after-delete.png) | PASS |
| 9 | Best Realty isolation: bob's `/manage/tags` is empty; `GET /api/tags` as bob returns `{"tags":[]}`; dave's People show no chips | [09a](09a-bob-manage-tags-empty.png), [09b](09b-dave-people-no-chips.png) | PASS |
| 10 | Unauthenticated `GET /api/tags` → 401; a platform-only session (owner@platform.test) `GET /api/tags` → 401 | [10](10-platform-admin-session.png) | PASS |
| 11 | Escape closes the Add tag popover and returns focus to the Add tag button (DOM assertion: `document.activeElement`) | [11a](11a-carol-popover-open.png), [11b](11b-carol-popover-closed-focus-returned.png) | PASS |

11 of 11 scenarios passed on the final run (`web/scripts/qa-011e-walkthrough.mjs`'s
own `results.json`, captured before the script was deleted, is reproduced
below). Two earlier runs each had one failing assertion, both traced to the
walkthrough script's own timing, not application behavior — see "Script
issues found and fixed" below; the underlying HTTP/DOM evidence for the
corrected scenarios is otherwise identical across all three runs (same
request order, same response bodies).

<details>
<summary>Raw <code>results.json</code> from the final run</summary>

```json
[
  { "n": 1, "pass": true, "detail": "requests=[\"GET /api/tags\",\"POST /api/tags\",\"GET /api/tags\",\"PUT /api/people/e60d6300-1020-4ebd-a22d-57abb53e03db/tags/f99899e9-9810-49ca-8147-4a06d00e3497\",\"GET /api/tags\"] tagId=f99899e9-9810-49ca-8147-4a06d00e3497" },
  { "n": 2, "pass": true, "detail": "changed=true" },
  { "n": 3, "pass": true, "detail": "people=0 renamed=Investor" },
  { "n": 4, "pass": true, "detail": "requests=[\"GET /api/tags\",\"PUT /api/people/e60d6300-1020-4ebd-a22d-57abb53e03db/tags/f99899e9-9810-49ca-8147-4a06d00e3497\",\"GET /api/tags\"]" },
  { "n": 5, "pass": true, "detail": "people=1 disabled=true tooltip=Only an admin can change a tag that is in use directStatus=403 directBody={\"error\":\"forbidden\"}" },
  { "n": 6, "pass": true, "detail": "chipVisible=true previewChipVisible=true previewChipText=Investor removeButtons=0" },
  { "n": 7, "pass": true, "detail": "dialogText=Delete tagDelete \"Investor client\"? 1 person will lose this tag. Saved lists and Today rules that use this tag will show an invalid-filter notice until they are edited. Cancel Delete" },
  { "n": 8, "pass": true, "detail": "chipCount=0" },
  { "n": 9, "pass": true, "detail": "bobTags={\"tags\":[]}" },
  { "n": 10, "pass": true, "detail": "anon=401 platform=401" },
  { "n": 11, "pass": true, "detail": "focusedTestId=add-tag-button" }
]
```

</details>

## Script issues found and fixed (not application defects)

Two assertions failed on the first two runs; both were bugs in the
throwaway walkthrough script, not in `web/` or `backend/`:

1. **Scenario 1's request-order check** originally asserted `POST
   /api/tags` and the following `PUT` at fixed array indices `[0]`/`[1]`.
   The real, correct sequence interleaves `GET /api/tags` calls (the
   popover's own tag-list load, and the query-invalidation refetch after
   each mutation) around the `POST`/`PUT` pair — exactly as
   `web/src/api/queries.ts`'s `useCreateTagMutation`/`useAddPersonTagMutation`
   are supposed to behave. Fixed by checking the *relative* order of the
   `POST` and `PUT` entries (`indexOf` comparison) instead of fixed
   positions.
2. **Scenario 6's preview check** read `previewChipVisible` immediately
   after the outer `[data-testid="person-preview"]` dialog appeared,
   without waiting for `PersonPreview.vue`'s own independent `usePerson`
   fetch (a second, separate query from the Person page's) to resolve.
   `isVisible()`/`textContent()` do not auto-retry the way an `expect()`
   matcher does, so the first check could observe the chip container
   before Vue had rendered it. Fixed by waiting for
   `[data-testid="person-preview-tags"]` to attach before reading it.

A third, cosmetic-only issue: the delete-confirm screenshot (07b) initially
captured the dialog mid-transition (PrimeVue's 150ms enter animation,
`opacity-0 scale-95` → visible) — the functional assertions (which read
`textContent`, unaffected by CSS transitions) passed regardless, but the
image itself showed only the darkened backdrop. Fixed by waiting 300ms
after the dialog attaches before screenshotting; re-run confirmed the
fully-rendered dialog.

## Defects found

None in `web/` or `backend/`. All eleven scenarios' application behavior —
inline create-then-apply in the correct request order, the 20/200-tag
rule-1 permission surface (`can_manage`, the disabled-with-tooltip state,
the live 403 on a stale-permission direct request), the read-only preview
chip, the rename/delete confirm flow with the exact spec copy, realtime-free
label propagation on reload, cross-Organization isolation (both UI and
`GET /api/tags`), the 401 enumeration for both an unauthenticated and a
platform-only session, and the Escape/focus-return popover contract —
matched the spec exactly on the corrected script.
