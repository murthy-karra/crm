# Slice 011d — live browser walkthrough (Lane W step 5)

Recorded 2026-09-07 against the merged integration branch
(`slice-011d-today-system-feeds` at `3496d71`, merged into
`slice-011d-lane-w`) per [SLICE_011d.md](../../../specs/SLICE_011d.md) §9.7
and §9.12. Real Playwright-driven Chrome against a real crm-api + Vite QA
runtime and a real scratch PostgreSQL database — no mocked routes, no
fabricated screenshots.

## QA runtime

- Scratch database `crm_011d_qa_20260907_085450` in the existing shared
  OrbStack Postgres instance (never `crm_dev`), created as `crm_migrator`
  (`CREATE DATABASE ... OWNER crm_migrator`, `GRANT CONNECT ... TO crm_app`),
  migrated with `cargo run -p crm-api --bin migrate` against an overridden
  `MIGRATION_DATABASE_URL` — the same migration binary the repo's own
  `scripts/db-migrate` invokes, no `.env` file mutated.
- Platform admin bootstrapped with `crm-admin bootstrap-platform-admin`
  (the one sanctioned non-HTTP step, D-021), then two Organizations seeded
  entirely through the running HTTP API with the unmodified
  `scripts/seed_dev.py` (Acme Realty: alice@acme.test admin, carol@acme.test
  member; Best Realty: bob@best.test admin, dave@best.test member) and the
  unmodified `scripts/demo-leads` (five inquiries as alice, run twice, one
  Person — Marcus Chen — assigned to Carol with a repeat inquiry).
- `crm-api` built from this worktree, bound to `127.0.0.1:31011`, with
  `DATABASE_URL` overridden to the scratch database via a process-level env
  var (dotenvy never overrides an already-set var, so the worktree's own
  tracked `.env` — used only for secrets/session config — was read but not
  edited).
- Vite dev server for `web/` on `127.0.0.1:51011`, `CRM_WEB_API_PROXY_TARGET`
  overridden to `http://127.0.0.1:31011` the same way, so the browser only
  ever talks to one origin (`:51011`) and the API proxy carries `/api/*` —
  no CORS/cookie cross-origin concerns, matching normal dev-proxy behavior.
- Chrome driven via `playwright-core`'s `chromium.launch({ channel: 'chrome',
  headless: true })` (already a `web/` dev dependency; the same mechanism
  `scripts/capture-create-list-guide.mjs` uses) from a scratch script, run
  and then deleted — not part of this commit.
- **User's environment untouched throughout and verified after teardown**:
  dev-api on `:3000` and dev-web (`pnpm run dev`, pid 24542) on `:5173` kept
  answering `200` before, during and after the QA runtime ran; `crm_dev` was
  never connected to by anything in this walkthrough.
- **Teardown**: QA `crm-api` and Vite processes killed by their own exact
  PIDs (never `pkill -f` a binary path — the documented hazard for this
  worktree); ports `31011`/`51011` confirmed free afterward; scratch
  database `crm_011d_qa_20260907_085450` dropped; the scratch walkthrough
  script removed from `web/scripts/`. Evidence for all of this is in the
  final report, not this file (this README is evidence of the *application
  behavior*, not the infrastructure teardown).

## Results

| # | Scenario (spec §9.7/§9.12) | Screenshot | Result |
|---|---|---|---|
| 1 | Admin reaches `/manage/today-feeds`; three cards in fixed order, Default status | [01](01-admin-today-feeds-default.png) | PASS |
| 2 | Edit locks the anchor clause (`awaiting_response`) and `me` — both visible, neither has a remove control | [02](02-admin-edit-stage-clause-added.png) | PASS |
| 3 | Admin adds a `Stage: Lead` clause to the unanswered-inquiry feed | [02](02-admin-edit-stage-clause-added.png) | PASS |
| 4 | Preview defaults the member picker to the admin, re-runs for the chosen agent (Carol), shows the Today row rendering | [03](03-admin-preview-for-carol.png) | PASS |
| 5 | Save (loaded revision) → card reads "Customized by Alice Anderson on `<date>`" | [04](04-admin-after-save-customized.png) | PASS |
| 6 | Agent's (Carol's) Today changes; Rules section marks the feed "Changed by your admin" with its live description | [05](05-agent-today-rules-changed.png) | PASS |
| 7 | Nav entry "Today rules" hidden for a member | (no image; DOM query) | PASS |
| 8 | Member navigating to `/manage/today-feeds` is bounced to `/today` | [06](06-member-bounced-from-admin-route.png) | PASS |
| 9 | Member gets HTTP 403 from `GET /api/organization/today-feeds` | (network status only) | PASS (`status=403`) |
| 10 | Revert restores Default | [07](07-admin-after-revert-default.png) | PASS |
| 11 | Turn off `unanswered_inquiry`: typed-confirmation submit starts disabled, stays disabled for a partial match ("Unanswered"), enables only for the exact display name | [08](08-admin-typed-confirm-turn-off.png) | PASS (all three sub-checks) |
| 12 | Turn off → card reads Off | [09](09-admin-after-turn-off.png) | PASS |
| 13 | Agent's Rules section shows Off; the feed's item (Marcus Chen) leaves the agent's Today entirely | [10](10-agent-today-after-turn-off.png) | PASS |
| 14 | Turn on (no confirmation dialog) restores Default | (no image; DOM query) | PASS |
| 15 | `client_replied` customized with a 1-hour freshness window | [11](11-admin-client-replied-1h-window.png) | PASS |
| 16 | `call_outcome_needed` left at Default throughout | [11](11-admin-client-replied-1h-window.png) | PASS |
| 17 | The three new chips available on People; `awaiting_call_outcome` renders the exact spec wording | [12](12-people-awaiting-call-outcome-chip.png) | PASS |
| 18 | A saved list built from a new derived-kind filter | [13](13-saved-list-with-new-chip.png) | PASS |
| 19 | That saved list enabled as a Today source ("Remove from Today" now shown, "N of 5 Today sources") | [14](14-saved-list-enabled-as-today-source.png) | PASS |
| 20 | Second Organization's (Best Realty) admin sees only its own three Default feeds — none of Acme's customization | [15](15-second-org-admin-defaults.png) | PASS |
| 21 | Second Organization's agent (Dave)'s Today unaffected throughout | [16](16-second-org-agent-today-unaffected.png) | PASS (no shared state observed; Dave's book is empty by seed design) |
| 22 | Fallback state (`filter_error` non-null; "using the default because the saved rule is invalid") | — | **Not reproducible** — see below |
| 23 | Partial notice (`system_feed_issues` unavailable entry) | — | Skipped per instruction (needs failure injection) |

22 of 22 attempted checks passed (24 counting the two split sub-assertions
above as one row each; the automated script recorded 24 individual
assertions, all PASS on the final run — one earlier run's single failure
was a Playwright timing race in the walkthrough script itself, not an
application defect: it read a status label before the PUT it triggered had
resolved and the query had refetched. Fixed by waiting for the network
response before reading the label; re-run confirmed correct application
behavior).

## Fallback state — not reproducible through the application

Rule 6 (§1) describes falling back to the canonical default when a stored
feed definition becomes invalid after saving — the two ways named are "a
referenced stage was deleted" or "the stored JSON is unsupported by the
running binary." Neither is reachable through this application as shipped:

- There is no stage-deletion route, admin UI control, or CLI command
  anywhere in `backend/` (checked `crm-api`'s routes and `crm-app`'s domain
  layer; the only hit for "delete" near "stage" is a code comment
  describing the *rule*, not a capability). Stages are seeded at
  Organization creation and are otherwise immutable in this slice.
- Producing "stored JSON unsupported by the running binary" requires either
  a direct database write (forbidden, D-021/AGENTS §4.8) or literally
  running an older/newer binary against the same row, which is out of scope
  for a Lane W web walkthrough.

Recorded as genuinely not reproducible rather than faked, per the
coordinator's explicit instruction. `TodayFeedsView.vue`'s handling of
`filter_error` (the "Using the default because the saved rule is invalid"
status line and the locked-out effective-rule chips) is exercised by
Vitest (`web/src/views/TodayFeedsView.test.ts`, "shows Off and the
invalid-fallback status") with a synthetic `filter_error` in the mocked
response, which is the closest available coverage.

## Defects found

None in `web/`. The one failing check across the run (see above) was
traced to the walkthrough script's own timing, not application behavior;
confirmed by re-running with a corrected wait and by directly querying the
scratch database's `today_system_feed` row mid-run, which already showed
the correct `enabled: true` state before the script's fixed assertion
re-ran and passed.

No backend defects found in this walkthrough. All backend behavior
exercised (six admin/preview/member routes, the typed-confirmation-gated
disable, the 403 on the admin route for a member, second-Organization
isolation, saved-list-as-Today-source enablement) matched the spec exactly.
