# Project State

Last updated: 2026-08-22 (Lane A implemented)

## Current phase

**Slice 006c (call outcome correction) IMPLEMENTED on
`slice-006c-outcome` (Lane A `30bce19`, Lane B merged `0064ed1`);
reviewed + adversarially tested per lane, all findings applied; both
gates green (`check` incl. 218 Vitest; `check-db` 142 DB tests incl.
`db_calls` 42). Dev DB migrated; dev API restarted with the new build.
Awaiting the live walkthrough (§13 item 4) and then merge to `main`.**
Delivered: migration widening `contact_attempted.outcome` (+`busy`,
`wrong_number`) and the one-corrector partial index; command + `POST
/api/calls/{id}/outcome`; Today reads the effective attempt; history
detail `call_id`/`corrects_id`/`superseded`; Operator history text marks
superseded rows; web "How did it go?" prompt (Save gated on the server's
terminal status, with a settle-refetch fallback when realtime is down),
history rendering by linkage (not position), Change outcome from
History, ringback tone while ringing, manual dialog gains the two new
outcomes. Spec amendments during implementation: §2 adjacency note
(ring-out corrections sort after `call_completed`); §10 two extra error
messages; ringback shipped. Slice 006 live walkthrough on 2026-08-22:
two real calls answered (the first "answered" in 2.8 s without ringing —
the motivating case). Still unproven live: a busy/decline/ring-out call
("no answer" attempt). Follow-ups: rotate the Telnyx SIP password; the
`placing` sweep horizon vs. slow mic prompts; OrbStack hung twice this
session (recovered with force-quit + `orbctl start`). Hostname is
`livekit1.tarams.org` (spec text says `livekit.tarams.org`).

## Current slice

Slice 006c — Call outcome correction (D-032) — `docs/specs/SLICE_006c.md`
APPROVED 2026-08-22; lane briefs `docs/tasks/SLICE_006c_LANE_{A,B}.md`;
implementation gate pending. Previous:
Slice 006 — Calling — `docs/specs/SLICE_006.md` (MERGED `332e78a`). Previous:
Slice 005 — Operator retrieval — `docs/specs/SLICE_005.md` (APPROVED). Read-only AI Operator: `crm-operator` crate with a
`ToolBackend` trait (five tools: `search_people`, `get_person`,
`get_today`, `get_next_work_item`, `explain_priority`), Groq via a
provider-neutral trait, `POST /api/operator/turns` (stateless,
non-streaming, bounded), append-only `operator_turn` /
`operator_tool_call` ledger, web **Ask** drawer. Closes the thesis §16
proof chain. Slice 004 is complete and merged (see History).

## Current branch

`slice-006c-outcome` at `0064ed1` (Lanes A+B merged, uncommitted docs
edits pending); `main` at `5b7509c`.

## Last accepted decision

2026-08-22, user-accepted (Slice 005 planning):
- D-028 — the AI Operator is an in-process workspace crate
  (`crates/crm-operator`) compiled into `crm-api`; §5 refinement: the
  dependency is inverted (`crm-api → crm-operator` only; `ToolBackend`
  trait is the whole data surface); the `crm-app` extraction is a
  prerequisite for the first Operator mutation slice.
- D-029 — Operator turns are audited as a PII-free ledger; no
  transcripts, no logged message/reply/argument text.

Previous (Slice 004 planning):
2026-08-22, user-accepted (Slice 004 planning):
- D-027 — membership deactivation (status active/inactive) ships in
  004 instead of removal; removal is not a concept; data retained and
  attribution stays visible with per-Person reassignment; last-admin
  rule counts active admins; `organization.status` reserved with
  suspension semantics OPEN as O-009.
- D-026 — Organization admin continuity. Admin-less Organizations stay
  fully operational for members; an Organization admin cannot remove
  the last admin (themselves); the platform admin can always both
  promote an existing member and invite a brand-new admin; admin-less
  Organizations are surfaced as "needs attention" in the platform-admin
  view (pending first-admin invitation is not an error); no push
  notifications in 004. Feeds the Slice 004 plan.

Earlier the same day, user-accepted:
- D-024 — Cloudflare Access removed from the dev tunnel. The app's own
  session login is the only gate now; the tunnel and its TLS are
  unchanged. Amends D-016 §4. Executed live (the `crm-dev` Access
  application deleted via the Cloudflare API and confirmed gone);
  README and SLICE_003 §11/§10 updated to match.
- D-025 — the dev tunnel turned out to be dashboard-managed, not
  file-managed as D-016 §4 documented; `config.yml`'s ingress was never
  actually applied, so the Slice 003 realtime WebSocket silently 404'd
  through the tunnel since it was written. Fixed live by adding/ordering
  the three routes in the Cloudflare dashboard; verified with a real
  101 Switching Protocols upgrade and a full two-browser cross-session
  walkthrough over the actual tunnel. README, `config.yml`'s comments,
  and this file updated to point at the dashboard as the real source of
  truth.

2026-08-21, user-accepted:
- D-023 — realtime model: one Organization channel with server-side
  subscriptions in API-minted short-lived tokens; ids-only events;
  best-effort publish after commit with recovery by refetch; dev
  WebSocket path-routed under the API hostname behind Access.
- Slice 003 specification approved as written, including the §14 safe
  defaults and the declared additive history-kind change to SLICE_002
  §5 (pointer line added there).
- D-022 — a contact attempt is a recorded fact (`contact_attempted`, the
  fifth typed fact table) and the unit of response for Today; any
  member's attempt counts; stage does not remove a Person from Today;
  done/snooze/dismiss remain unspecified (thesis §8).
- D-021 — Slice 004 is administration (platform admin + invitations +
  roles) and, from 004 on, nothing outside migrations writes to the
  database directly: seed/CLI/API all go through the same domain
  functions. Design defaults recorded as O-007 for confirmation when 004
  is planned.
- D-017 — web frontend stack: Tailwind, PrimeVue (unstyled), TanStack
  Table, TanStack Query; Centrifugo events drive Query invalidation.
- D-018 — production ingress: in-cluster `cloudflared` → Cilium Gateway
  API, `HTTPRoute` per hostname, no separate ingress controller.
- D-019 — Person stages are an Organization-scoped table seeded with
  Follow Up Boss's nine defaults, not a fixed enum.
- D-020 — Hot Prospect carries a red `Flame` marker wherever its name is
  shown; the one exception to `UI_STYLE.md` §5/§9, matched on the seeded
  stage name because D-019 stages have no semantic key.
- Slice 002 scope (not log entries): raw-payload key is one `.env`
  value, no DEKs/rotation; routing is "assign to a named member", no
  rules engine; list view + frontend stack + Vue Router land in 002;
  sqlx offline mode adopted in 002.

## Completed work

- 2026-08-22: Slice 005 Lane A implemented and verified on
  `slice-005-operator`. `./scripts/check` green (fmt, clippy `-D
  warnings`, 244 service-free Rust tests incl. 52 in `crm-operator`,
  web lint/typecheck/57 Vitest/build — note: `pnpm` needs
  `~/.nvm/nvm.sh` sourced in a non-interactive shell). `./scripts/check-db`
  green (sqlx `prepare --check`, 116 DB-backed tests incl. 21 new in
  `tests/db_operator.rs`). Live walkthrough §1 steps 1–7 on loopback
  against real Groq, model `openai/gpt-oss-120b`, 0.3–2.0 s per turn
  (1–3 tool calls): next-call answer with card matching `GET /api/today`,
  "why is she first?" citing tier/reasons/rule/ahead counts, find/tell-me
  with cards, bob's cross-Organization ask → "couldn't find" with a
  `search_people → ok`, zero-id ledger row, a polite refusal to "call her",
  dead provider port → 503 `operator_unavailable` in ~100 ms with a
  `provider_error` row (one retry, `model_call_count = 2`).
  Review/tester findings applied: `Usage::add` now saturates (a hostile
  endpoint could panic the spawned turn → 500 and no ledger row); a
  tool in flight at the turn deadline is recorded; NUL/invisible
  characters are stripped from the model's search query (was a fake
  `tool_error` outage via Postgres 22021) and from `UntrustedText`
  (bidi/zero-width); `TodayView.truncated` honours the tool `limit`;
  request body limit 256 KiB; redacting `Debug` on `TurnInput` /
  `TurnOutput` / `ChatRequest`; Groq response body capped at 1 MiB;
  mutex poisoning no longer wedges the in-flight set; the crate fence
  test also catches `package = "sqlx"` / `[dependencies.sqlx]`; new
  DB tests for ledger-insert failure (200 + no row, one transaction),
  LIKE escaping + contact match + foreign contact values, the
  append-only triggers against the owner, and validation boundaries.

- 2026-08-20: Repository initialized; initial commit of carried-over
  documents (AGENTS.md, CLAUDE.md, README.md, product thesis, event-sourcing
  research).
- 2026-08-20: Documentation bootstrap committed (`e5af54c`): canonical
  `docs/` structure, decision log (D-001–D-012, O-001–O-005), architecture
  baseline, this state file.
- 2026-08-20: D-013 accepted and recorded; AGENTS.md/README secrets
  statements reconciled; `.gitignore` added.
- 2026-08-20: D-014/D-015/D-016 accepted; `docs/specs/SLICE_000.md` drafted,
  independently reviewed (ready with minor amendments, all applied), and
  user-approved (`b3bd051`).
- 2026-08-20: Slice 000 implemented on `slice-000-foundation`: Cargo
  workspace with `crm-api` (Axum health/ready, request-id, graceful
  shutdown, typed config with loopback/timeout-bound validation, lazy SQLx
  pool); Vue 3 + Vite web shell with `/api` proxy; Docker Compose for
  PostgreSQL 18 and Centrifugo v6 (loopback-only); `scripts/bootstrap`,
  `dev-services`, `dev-api`, `dev-web`, `dev-tunnel`, `check`,
  `check-services`; README Development section rewritten to match D-016.
- 2026-08-20: Independent review (`crm-reviewer`) and adversarial test
  analysis (`crm-tester`) run against the implementation; both verdicts
  were "ready to merge." Applied fixes: `dev-tunnel` now scopes only the
  tunnel token into `cloudflared`'s environment instead of the whole
  `.env`; dropped an unused `tower-http` Cargo feature; clippy lints set
  to `deny` (matching the spec's wording, not just the check script's `-D
  warnings` flag); readiness failures now log a specific reason (timeout
  vs. query error) instead of none; per-request tracing (method, path,
  latency, status) is now visible under the documented default `RUST_LOG`
  (tower-http's span/request/response levels were DEBUG, silently
  filtered by the default `info` filter); added a test proving the
  readiness timeout bound holds against a peer that completes the TCP
  handshake but never responds (previously only "connection refused" was
  tested); corrected a test comment that mischaracterized sqlx's
  connect-refused retry/backoff as "failing fast."
- 2026-08-20: Slice 000 merged to `main` (`e5182d1`, pushed);
  `slice-000-foundation` deleted.
- 2026-08-20: Slice 001 planned (narrow-cut and no-trait decisions
  user-accepted), spec drafted, independently reviewed (14 findings
  applied — notably: DB-backed tests now exercise the router as `crm_app`
  not the schema owner; `crm_migrator` made the dev database's owner so
  the first migration doesn't fail; login always mints a fresh token
  against session fixation; logout leaves the cookie in place on a
  persistence failure), and user-approved (`b6e8764`).
- 2026-08-20: Slice 001 implemented on `slice-001-identity`: migrations
  harness (`sqlx::migrate!`, hand-authored SQL, embedded `migrate`
  binary) with the `crm_migrator`/`crm_app` role split provisioned by
  `dev-services up`; `organization`/`app_user`/`local_credential`/
  `organization_membership`/`user_session` schema; session/auth core
  (Argon2id, HMAC-SHA256 session tokens, `AuthContext` extractor
  re-verifying membership every request); `POST`/`DELETE /api/session`,
  `GET /api/me`, `GET /api/organization/members`; `seed` binary +
  `scripts/dev-seed` (two Organizations, idempotent); web login UI
  (conditional rendering, no router/Pinia); `scripts/db-migrate`,
  `check-db`.
- 2026-08-20: Independent review (`crm-reviewer`) and adversarial test
  analysis (`crm-tester`) run in parallel against the implementation;
  both independently found the same two real security bugs. Fixed:
  Argon2id password verification now runs under `tokio::task::
  spawn_blocking` (it was running synchronously on the async runtime,
  so a burst of login attempts — including the dummy-hash path, which
  needs no valid account — could stall every other concurrent request
  on that worker thread); the session-fixation revoke-on-relogin now
  fires only after login has actually succeeded and only when the
  presented cookie belongs to the same user who just authenticated
  (previously, presenting *any* leaked session token — even a different
  account's — during one's own login would silently revoke it with no
  ownership check; manually verified fixed by having one seeded user
  log in while presenting a second seeded user's real cookie and
  confirming the second user's session survives). Also strengthened the
  `crm_app`-grants test to check all four read-only tables (previously
  only spot-checked one) and added the members-list assertion a
  differently-named test had promised but not made, a body-based
  org-id-ignored probe, and a real second-logout-call idempotency test —
  15 DB-backed tests total, all passing after the fixes.
- 2026-08-20: Slice 001 merged to `main` (`587a087`, pushed);
  `slice-001-identity` deleted.
- 2026-08-20: Created a dedicated `crm-dev` Cloudflare tunnel (separate
  from the account's other tunnels — notably an existing `k8s-crm`
  tunnel with live traffic, deliberately left untouched) after the user
  chose `app.tarams.org` (browser) / `api.tarams.org` (API) as two
  separate hostnames with the browser calling the API directly. Added,
  on `tunnel-cors-config`: two new optional config variables
  (`CRM_CORS_ALLOWED_ORIGIN`, `CRM_SESSION_COOKIE_DOMAIN`; both unset by
  default, preserving Slice 001's exact original no-CORS/host-only-cookie
  behavior for loopback dev and any single-hostname tunnel); a CORS layer
  added to the API only when configured, using `AllowOrigin::list` (not
  the single-`HeaderValue` form, which — caught by a new test —
  unconditionally echoes the configured origin regardless of the
  request's actual `Origin`, rather than only when it matches); the
  session cookie's `Domain` attribute now configurable; the web shell
  now detects an `app.*` hostname at runtime and calls `api.*` directly,
  leaving loopback/Vite-proxy behavior untouched. 9 new tests (2 config
  parsing, 2 cookie-domain, 3 CORS behavior at the HTTP level, plus
  fixing 2 pre-existing cookie tests whose expectations didn't account
  for the `cookie` crate's RFC 6265 leading-dot normalization). Full
  suite green (34 unit + 17 integration + 15 DB-backed).
- 2026-08-21: Live tunnel setup and end-to-end verification, working
  through real obstacles rather than the originally-planned dashboard
  flow:
  - `crm-dev` turned out to be a **locally-managed** tunnel (Cloudflare's
    dashboard Public Hostname UI only works for dashboard-managed
    tunnels, and migrating is irreversible); pivoted to a committed
    `infra/development/cloudflared/config.yml` declaring both ingress
    rules instead, authenticating via the credentials file `cloudflared
    tunnel create` already wrote outside the repo — no token needed.
    `scripts/dev-tunnel` and `.env.example` updated accordingly
    (`CLOUDFLARED_TUNNEL_TOKEN` removed as unused).
  - The Access application was created via the Cloudflare API (a
    dashboard token with "Access: Apps and Policies" Edit scope could
    create the application but got `auth.forbidden` on both the nested
    and top-level policy-creation endpoints; the policy was ultimately
    attached by including it inline in the same `PUT` that updates the
    application, which the same token *could* do).
  - **Cloudflare Access scopes its login session per hostname, not per
    Application** — authenticating at `app.tarams.org` does not also
    authenticate `api.tarams.org`, even under one Application with one
    policy, contrary to the original assumption when this design was
    chosen. Visiting each Access-protected hostname once directly
    (a real top-level page load) establishes a session for that
    hostname; only after both are established does the browser's
    background `fetch()` from `app.*` to `api.*` carry a valid session.
    Documented for anyone extending this pattern later.
  - CORS preflight (`OPTIONS`) requests were being intercepted by Access
    itself (redirected to its login page) before ever reaching the
    API's own CORS layer; fixed via the Access application's
    `options_preflight_bypass: true` setting, which lets preflight
    requests through to the origin.
  - Found and fixed a real bug while wiring this up: `web/src/App.vue`'s
    `fetch()` calls all used `credentials: 'same-origin'`, which does
    not send or store cookies on the genuinely cross-origin `app.*` →
    `api.*` calls this design requires. Changed to `credentials:
    'include'` (correct and harmless for the same-origin/loopback case
    too).
  - End-to-end login through the tunnel confirmed working by the user
    after these fixes.
- 2026-08-21: Tunnel work merged to `main` (`3b6df76`, `122c7e4`).
- 2026-08-21: Frontend stack (D-017), production ingress (D-018), and
  stage model (D-019) decided with the user and logged.
- 2026-08-21: Slice 002 planned with `crm-planner` (scope decisions
  fixed by the user first), plan independently reviewed by
  `crm-reviewer` (17 findings — notably: a per-contact-value advisory
  lock would miss mixed email+phone payloads, so intake takes one
  per-Organization lock; sqlx macros would silently compile online
  against the drifted dev DB via dotenvy's parent-directory walk, so
  `SQLX_OFFLINE` is defaulted through a cargo config; a plain SHA-256 of
  a tiny PII payload in an immutable row is a dictionary oracle after
  erasure, so the content hash is a keyed HMAC; the planner's
  Organization-wide `/inquiries` list was scope creep and is cut),
  spec drafted, spec re-reviewed by the same reviewer (12 further
  items — notably: the cargo config must live at the repo root because
  cargo walks from cwd not `--manifest-path`; the `sqlx-prepare` script
  as first written was circular and now migrates the throwaway DB with
  the CLI; intake's four facts share both `occurred_at` and
  `recorded_at` so history ordering needs an explicit `kind_rank`; the
  trigger test must hit a real row or passes vacuously; history `detail`
  shapes and `display_name` derivation were unspecified for the
  frontend lane). All findings applied as safe defaults; the reviewer
  classified zero items as blocking decisions.
- 2026-08-21: Slice 002 implemented in two parallel lanes (backend on
  `slice-002-intake`, frontend on `slice-002-web` worktree; sqlx-cli
  0.9.0 replaced with the locked 0.8.6). Both self-verified green
  (`check`/`check-db` backend; lint/typecheck/build frontend), then sent
  through independent review and adversarial testing against the real
  diffs. Review: contract cross-check between backend response bodies
  and frontend types found zero discrepancies; a few IMPLEMENTATION_DETAIL/
  LATER nits. Adversarial testing found real issues, all fixed:
  - **An undisclosed file**: the backend lane had built
    `dump_raw_payloads.rs` + `scripts/dump-raw-payloads`, a tool that
    decrypted every Organization's raw lead payloads into one permanent,
    unscoped plaintext table — never in the spec, never in the
    implementer's own "files changed" report, caught only by diffing
    actual `git status` against the report. Deleted; a standing memory
    now says to always cross-check implementer-reported file lists
    against real git state.
  - A `TRUNCATE` gap in the append-only trigger (row-level triggers
    don't fire on `TRUNCATE`; `crm_migrator` could empty a fact table)
    — fixed with a second `FOR EACH STATEMENT` trigger + test.
  - `routing_decision.strategy` had no CHECK constraint backing a
    Rust `unreachable!()` — added the constraint, changed to a typed
    error.
  - Three `raw_payload` queries skipped the `organization_id` predicate
    every other query in the slice includes — added for defense in
    depth.
  - No logging on intake failure paths — added, keyed by error variant
    name only, never error text.
  - A cross-tenant availability bug: one Organization's intake burst
    could exhaust the shared, unconfigured connection pool and 503
    every *other* Organization's logins/reads (empirically reproduced:
    12 concurrent requests against a held lock). User chose to fix the
    actual mechanism over containment or a PgBouncer-based approach
    (which wouldn't have helped — transaction-mode pooling still binds
    a connection for an open transaction's full duration): replaced the
    blocking `pg_advisory_xact_lock` with a bounded `pg_try_advisory_xact_lock`
    retry loop (3s budget, jittered backoff, releases the connection
    between attempts) failing closed to a new 503 `intake_busy` +
    `Retry-After` rather than parking a connection indefinitely.
    Re-reproduced the original exploit as a test: busy Organization
    fails in ~3.2s (not the full 7s external hold), an unrelated
    Organization's 5-request burst during that same hold completes in
    ~500ms, all succeeding.
  - Frontend: a 401 encountered outside the router guard (session
    expiring while idle on a page) didn't redirect to `/login`,
    contradicting the code's own comment — fixed with a global
    `QueryCache`-level handler, mutexed against the guard's own redirect.
  - History timeline rows were ~64px against `UI_STYLE.md`'s specified
    56px — fixed.
  Spec `§3`/`§5`/`§8`/`§9` updated to describe the as-built bounded-retry
  mechanism and the `intake_busy` error code.
- 2026-08-21: Full live walkthrough run — both dev servers together
  (backend from the main checkout, frontend from the worktree, real
  Postgres), driven with a headless Playwright browser script (no
  project `run` skill existed for this repo; none created, since
  the setup — two servers in two locations, real login — was specific
  to this verification, not a reusable pattern yet). One real
  environment hazard found and fixed along the way: `.env` was left in
  tunnel-mode config (`CRM_SESSION_COOKIE_DOMAIN=tarams.org`,
  `CRM_SESSION_COOKIE_SECURE=true`) from earlier tunnel testing, which
  a loopback browser correctly refuses (a `Domain=tarams.org` cookie
  from a `127.0.0.1` response is invalid) — login 200'd but the cookie
  never stuck; temporarily switched to loopback values for the test,
  restored exactly afterward. Also found and killed a stale Vite
  process serving the *old* Slice 001 frontend on the same port,
  which the readiness check had passed against by coincidence.
  Walkthrough covered: login; new lead → Person detail with all four
  history facts; stage change; reassignment; repeat lead with the same
  email → dedup, `kept_existing` routing confirmed live; unresolved
  lead (no contact method); duplicate delivery; logout + cross-Organization
  isolation (second Organization's login shows zero of the first's
  data). Zero console or page errors. Found and fixed one cosmetic nit
  live (`DataTable`'s footer said "1 unresolved leads"; added proper
  singular/plural support) and re-verified.
- 2026-08-21 (`fac3413`, fast-forwarded to `main` from `post-002-ui-fixes`;
  root-level screenshots ignored in `e6dbd50`), user-reported from live use
  through the tunnel: three fixes, no slice open.
  1. **Stage marker (D-020)**: `components/StageLabel.vue` +
     `lib/stages.ts`; wired into the People table's stage badge and the
     Person detail stage `Select` (`#value` and `#option` slots).
     `UI_STYLE.md` §5 and §9 amended to record the exception.
  2. **`Select` menu overflowed its own panel**: `lib/controls.ts`'s
     `selectPt()` put `max-h-64 overflow-auto` on the `list` `<ul>`, but
     PrimeVue sets an inline `max-height: {scrollHeight}` (14 rem) on the
     `listContainer` and, unstyled, ships no `overflow` for it — so with
     nine stages (D-019) the 368 px `<ul>` spilled out below the panel's
     background and border and painted over the page beneath it (the user
     saw "Trash" sitting on top of the Inquiries card). Scroll moved to
     `listContainer`; measured after the fix, the container clips the
     139 px of overflow. Affects every `Select`, not just stage. `root`
     also gained `gap-2` so the value cannot touch the chevron.
  3. **Permanent "Loading…" whenever `me` failed with anything but a 401**:
     the router guard deliberately lets a non-401 `me` failure through so
     "the view's own query can show an error state" (`router.ts`), but each
     view derives `orgId` from `me` and keeps its queries `enabled: false`
     until it resolves — and a *disabled* TanStack query reports
     `isPending` forever. The intended error state was therefore
     unreachable: every screen sat on "Loading…" with nothing to click.
     `AppShell.vue` now renders the `me` error plus a "Try again" button in
     place of the routed view, guarded on `me` having no cached data at all
     so a failed background refetch cannot replace a working screen.
- 2026-08-21 (environment, same session): the tunnel was broken and the
  cause was `.env` drift, not the app. `CRM_CORS_ALLOWED_ORIGIN` was empty,
  so the API attached no CORS layer and the browser discarded every
  cross-origin response from `api.tarams.org` to the `app.tarams.org` page
  — the API logged twelve `/api/me` 200s the client never saw, and
  `/api/people` was never requested (this is what surfaced fix 3 above).
  Restored to `https://app.tarams.org` and the API restarted;
  `/api/session` and `/api/people` immediately 200'd from the user's
  browser. Note that the Slice 002 walkthrough entry above claims the
  tunnel-mode values were "restored exactly afterward" — they were not:
  all three (`CRM_CORS_ALLOWED_ORIGIN`, `CRM_SESSION_COOKIE_DOMAIN`,
  `CRM_SESSION_COOKIE_SECURE`) were left at loopback settings. See Pending
  work item 4.
- 2026-08-22: D-024 (Cloudflare Access removed from the dev tunnel) and
  D-025 (the tunnel is actually dashboard-managed; the real WebSocket
  routing fix and its verification) — full narrative in the decision
  log. Net effect verified live over the real tunnel with two separate
  headed-Chrome sessions (Alice, Carol): a brand-new lead posted
  directly to the API appeared on the assignee's Today in under a
  second with no reload; reassigning it from the Person-detail page
  removed it from Alice's Today and added it to Carol's, live, in a
  second real browser window, in under a second. This is the first time
  the Slice 003 realtime path has been proven working through the
  actual public tunnel rather than only on loopback.

- 2026-08-22: Slice 004 implemented in two parallel lanes (backend on
  `slice-004-admin` in the main checkout, owning the migration; web on
  `slice-004-web` in the `../crm-web` worktree, built against the
  frozen §5 contract without waiting on a running backend). Both
  self-verified green, then independently reviewed and adversarially
  tested against the real diffs:
  - `crm-reviewer` found zero blocking issues; confirmed the one
    coordinator-approved deviation from the spec's literal grant block
    (`GRANT SELECT ON invitation TO crm_app;` — every sibling table in
    that line already had SELECT from an earlier migration, `invitation`
    was the one omission, and no narrower column-level grant works
    because `token_hash` is used in the same `WHERE` clauses that
    authenticate a token) as a safe mechanical correction, not a policy
    decision. Two IMPLEMENTATION_DETAIL findings: a stale `stage.rs` doc
    comment (fixed) and a defensible, non-fixed query-invalidation
    narrowing in `web/src/api/queries.ts`.
  - `crm-tester` found no exploitable tenant-isolation or
    authorization-bypass bug. One real low-severity finding: `IssueInvitation`
    racing a concurrent `AcceptInvitation` for the same `(org, email)`
    could surface a bare 503 instead of a clean error, or leave a
    harmless dangling invitation row, under READ COMMITTED's
    per-statement snapshot — fixed with a re-check immediately before
    insert plus a `check_violation` retry. Two test-coverage gaps
    closed: a true concurrent double-accept test (mirroring the
    existing last-admin race test's `tokio::spawn`/`tokio::join!`
    pattern, not sequential accept-then-accept) and platform-route
    403-for-org-admin coverage extended to all five routes (previously
    three of five).
  - Both lane file lists cross-checked against real `git status`/`git
    diff --stat` at every hand-off — no undisclosed files, no repeat of
    the Slice 002 lesson.
  - Live headless-browser walkthrough of spec §1 steps 2–9 (real
    backend on :3000, real frontend, real Postgres/Centrifugo): 46/46
    assertions passed — platform-admin Organization creation and
    first-admin invitation, promote/demote/deactivate/reactivate with
    the last-admin invariant enforced through real UI actions, revoke/
    re-issue, and every tenant-isolation and platform-vs-tenant
    authorization boundary probed directly (403/404/401 exactly as
    contracted). Zero real application console errors (the walkthrough
    logged Chrome's automatic "failed to load resource" lines for
    intentional negative-path 401/403/404/409 probes and the anonymous
    session-check on public routes — not unhandled errors).
  - Final counts, verified by direct re-run rather than trusted from
    agent reports: backend 143 lib unit tests + 133 integration tests
    (38 service-free via `./scripts/check`, 95 DB/Centrifugo-backed via
    `./scripts/check-db`, including 24 new in `db_admin.rs`); web 57
    Vitest tests across 5 files. All green.
  - Found live (not a slice defect, an environment-tooling gap):
    `./scripts/dev-services down` does not drop the Postgres named
    volume, so it does not give a true clean slate on its own — needs
    `down -v` or an explicit volume prune. Noted in Pending work below.

- 2026-08-22: Slice 004 spec §1 steps 2–9 re-walked-through through the
  real Cloudflare tunnel (`https://app.tarams.org`/`https://api.tarams.org`,
  not loopback), specifically because this project's history (D-024/D-025)
  has repeatedly found tunnel-only bugs that loopback testing misses (the
  Slice 003 realtime WebSocket 404'd silently through the tunnel for weeks
  before D-025 caught it). Fresh `dev-bootstrap` seed data
  (`owner@platform.test`, Acme/Best); Cedar Realty and Erin/Frank/Gina
  created live during the walkthrough per the spec script. Headless
  Playwright against `https://app.tarams.org` (adapted from the loopback
  walkthrough script, with `apiFetch` resolving explicitly against
  `https://api.tarams.org` since the app's own `resolveApiBaseUrl()` does
  that at runtime and a bare relative fetch from an `app.*` page would
  otherwise wrongly resolve against `app.*` itself): **48/48 assertions
  passed** (the original 46, plus 2 new ones instrumenting step 6's
  realtime disconnect directly). `./scripts/check-tunnel` confirmed
  plumbing first (app 200, API health 200, WebSocket upgrade 101).
  - **Step 6 (Frank deactivates Erin) got explicit WebSocket lifecycle
    instrumentation** via Playwright's `page.on('websocket')`, tracking
    Erin's live Centrifugo connection (`wss://api.tarams.org/connection/
    websocket`) with open/close timestamps, attached before any
    navigation. Confirmed live, with no page reload: Erin's WebSocket
    closed **179ms after** Frank's deactivation request resolved, and her
    tab's URL had not changed at that moment (ruling out the close being
    a side effect of a forced navigation rather than a genuine server-
    initiated disconnect). Cross-checked against the API's own log: zero
    `realtime disconnect failed` warnings anywhere in the run (the
    `disconnect_user` codepath in `realtime/publisher.rs` only warns on
    failure; success is `debug!`-only and filtered by the default log
    level, so silence plus the observed close is the expected signature
    of success). This is the first time this exact codepath (Centrifugo's
    HTTP disconnect API call, followed by the client's WebSocket actually
    closing) has been proven over the real tunnel rather than loopback.
  - A first run of the instrumented script scored 46/48 — a bug in the
    script, not the app: the WebSocket tracker was attached *after*
    navigating to `/today` and waiting, by which point the connection
    (opened within about a second of mount) had already been missed
    (`page.on('websocket')` only fires for connections opened after the
    listener is registered). Fixed by attaching the tracker at page
    creation, before any navigation, and re-run clean; required a full
    `dev-services down -v && up && db-migrate && dev-bootstrap` reset
    first since the first run's Cedar Realty/Erin/Frank/Gina data would
    otherwise collide with the second run's unique-name/invitation-state
    assumptions.
  - No other tunnel-specific behavioral differences from the loopback
    run: CORS, the `Secure` cross-origin cookie, and the WebSocket upgrade
    path all held up unchanged. Zero unhandled console/page errors; all
    observed "Failed to load resource" console lines were the anonymous
    session-check on public routes or the walkthrough's own deliberate
    401/403/404/409 negative-path probes.
  - Backend/frontend/tunnel processes stopped after the run; dev-services
    (Postgres, Centrifugo) left running per instruction.

## Pending work

1. Resolved 2026-08-22 (D-024): the Cloudflare Access application was
   deleted rather than documented more durably — the user decided the
   app's own login is sufficient for the dev tunnel and Access was
   redundant friction. No longer applicable.
2. User-side (whenever convenient, not blocking): fresh-clone walkthrough
   of Slice 000.
3. Not blocking: the local dev database (`crm_dev`) now has a few
   Slice-002-testing rows from review/adversarial/manual verification
   (e.g. "Ada Lovelace", "Grace Hopper", an unresolved "website" entry).
   Harmless local data; `./scripts/dev-services down` + `up` + re-migrate
   + re-seed resets it if a clean slate is wanted before real use.
6. Resolved 2026-08-22: `./scripts/dev-services down` did not drop the
   `crm_postgres_data` named volume, so `down && up` alone was not a
   true clean slate (discovered during the Slice 004 live walkthrough —
   a prior review session's "Cedar Realty" Organization was still
   present). Fixed: `down` now accepts an optional `-v`/`--volumes` flag
   (`compose down -v`); plain `down` still keeps data (needed for the
   restart-only use cases already documented — Centrifugo HMAC-secret
   changes, Docker Desktop clock drift). README's dev-services section
   documents both forms. Used immediately after fixing to get a real
   clean slate (`down -v && up && db-migrate && dev-bootstrap`) before
   the tunnel walkthrough below.
5. Not blocking, dev-only (D-025): the dev tunnel's real routing config
   lives only in the Cloudflare dashboard now, not in the repo — a fresh
   clone or a recreated tunnel would need the three dashboard routes
   (and their order) set up by hand per the README, since `config.yml`'s
   `ingress:` section is documentation only and is not applied. Worth
   revisiting if this needs to survive an account change or if Cloudflare
   ever offers a path back to file-managed mode.
4. Resolved 2026-08-21 (user: "I always go through the tunnel"): `.env` is
   committed to tunnel mode — `CRM_CORS_ALLOWED_ORIGIN=https://app.tarams.org`
   and `CRM_SESSION_COOKIE_SECURE=true`. `CRM_SESSION_COOKIE_DOMAIN` stays
   **unset**, deviating from what README step 5 used to prescribe: only
   `api.*` is ever called with credentials, so a host-only cookie is
   sufficient, and adding a `Domain` cookie while a host-only one already
   exists sends two same-named cookies (older, host-only one first), which
   the server reads in preference while logout clears only the newer — a
   401 loop with no way out but clearing cookies by hand. README's variable
   table and step 5 were corrected to say this, and step 5 now states that
   `CRM_CORS_ALLOWED_ORIGIN` is not optional and names the symptom of
   forgetting it. Loopback dev still works with these values (Chromium
   accepts `Secure` cookies on `http://127.0.0.1`; Safari would not).

## Blocking decisions

None for Slice 000, 001, or 002, or their merges. O-006 (outbound
messaging consent) blocks the SMS slice; O-002 (recording consent)
blocks recording features.

## Safe defaults adopted

- Slice 004: `GRANT SELECT ON invitation TO crm_app;`, added by Lane A
  beyond SLICE_004 §2's literal grant block and confirmed by both
  `crm-reviewer` and the coordinator as a mechanical correction of an
  editing omission (every sibling table in the same GRANT line already
  had SELECT), not a policy decision — no narrower grant is possible
  since `token_hash` backs the same `WHERE` clauses that authenticate a
  token, and no `token_hash` value is ever returned to a client.
- The event-sourced aggregates document is classified as research
  (precedence level 6), not accepted architecture, because its scope
  conflicts with the thesis's deferred capabilities and D-007. Recorded as
  D-015 (resolving O-005).
- Toolchain pins refreshed to current stable at implementation time, per
  spec §9 default 6: Rust 1.98.0 (not the estimated 1.97.1), pnpm 11.22.0
  (not 11.18.0).
- TypeScript pinned to 6.0.3, not the newer 7.0.2: TypeScript 7 is a new
  native-compiler major that `vue-tsc` 3.3.10 cannot yet load (`typescript/
  lib/tsc` is not exported under its new package layout). Revisit when
  vue-tsc adds support.
- PostgreSQL 18's official image changed its data-directory convention
  (single mount at `/var/lib/postgresql`, not `.../data`); `compose.yaml`
  mounts accordingly.
- Centrifugo v6's `health` config key is a nested object (`{"enabled":
  true}`), not the boolean used by older versions; `config.json` written
  accordingly.

## Latest verification

2026-08-21, post-merge fixes on `main` (stage marker, `Select` overflow,
session-unavailable state), web-only — no backend file changed, so the
cargo half of `./scripts/check` was not re-run:
- `pnpm lint`, `pnpm typecheck`, `pnpm build` green.
- Live headless-Playwright walkthrough against the running dev stack
  (real API, real Postgres, logged in as the seeded `alice@acme.test`):
  People list badge and the Person detail `Select` both show the flame on
  Hot Prospect only; the stage menu is clipped inside its own panel with
  the last row cut as a scroll affordance (DOM measurement: `<ul>`
  overflows its container by 139 px, `overflow: auto` on the container).
- The permanent-"Loading…" defect was reproduced first (abort `/api/me`,
  which is what a discarded cross-origin response looks like to the
  client) and then re-run against the fix: both People and Person detail
  now render "Could not reach the server…" and a "Try again" button.
- Not covered: no automated web test suite exists (no vitest/Playwright
  runner in `web/`), so all three fixes are pinned by manual verification
  only. Worth a real frontend test setup before the web surface grows.

2026-08-21, Slice 002, on `slice-002-intake` + `slice-002-web`, after
implementation, independent review, adversarial testing, and fixes, all
local (no CI yet):
- Backend `./scripts/check` and `./scripts/check-db` (run twice) both
  green: 109 unit/service-free tests, 46 DB-backed tests (append-only
  via grants and via the TRUNCATE-statement trigger; both concurrency
  races — same-email dedup and duplicate-delivery — plus the new
  advisory-lock-contention race proving an unrelated Organization is
  unaffected; encryption round-trip, tamper, and wrong-key failure
  modes; tenant isolation on every endpoint in both directions;
  `crm_app`/`crm_migrator` grants including the column-level
  `raw_payload` grant and its trigger backstop; seed idempotency run
  three times manually plus in-suite).
- Frontend `pnpm lint`/`typecheck`/`build` green; no test framework
  exists yet for this project (noted, not blocking — a future decision
  if frontend tests are wanted).
- Live walkthrough (both processes together, real Postgres, headless
  browser): login; new lead → Person detail with 4 correctly-ordered
  history facts; stage change and reassignment, each producing exactly
  one new fact; repeat lead by email → no new Person, `kept_existing`
  routing confirmed on screen; unresolved lead (no contact method);
  duplicate delivery; full cross-Organization isolation on logout/relogin.
  Zero console or page errors throughout.

2026-08-20, on `slice-001-identity`, after both reviews' fixes, all local
(no CI yet):
- `./scripts/check` — cargo fmt, clippy (`deny`, `-D warnings`), cargo test
  (25 unit + 15 integration passing, 15 DB-backed tests correctly
  `#[ignore]`d), web lint, typecheck, build — all green with zero
  services running.
- `./scripts/check-db` — all 15 DB-backed tests pass against fresh
  ephemeral databases: full login→me→members→logout→replay lifecycle;
  wrong-password/unknown-user/no-local-credential all return identical
  401; expired session, tampered token, revoked membership all 401;
  re-login rotates the token and revokes the old one; logout is
  idempotent against an already-revoked session; zero-membership login
  is 403 with no session row created; two-Organization isolation in
  both directions; client-supplied Organization ID (query string,
  header, and login-request body) ignored; multi-membership picks the
  earliest deterministically and its members list is provably scoped to
  only that Organization (a second, later-org-only person does not
  leak in); `crm_app`/`crm_migrator` `current_user` checks; `crm_app`
  denied DDL and INSERT/UPDATE on all four read-only tables, has
  exactly its `user_session` grants (SELECT/INSERT/UPDATE, DELETE
  denied); the real `seed` binary run twice via subprocess produces no
  duplicates.
- Manual, against the real dev database through the real Vite proxy:
  login sets a correctly-attributed cookie (`HttpOnly`, `SameSite=Lax`,
  `Path=/`, `Max-Age=604800`); `/api/me` and `/api/organization/members`
  return correctly-scoped data; a second Organization's members are
  invisible; logout returns a matching clearing cookie and the old
  cookie is rejected afterward; wrong-password and unknown-user timing
  are comparable (~350–550ms, both dominated by Argon2id), confirming no
  enumeration oracle; the cross-account revoke exploit found by review
  (finding: presenting a leaked session cookie during an unrelated
  login silently revoked it) is confirmed fixed — a second seeded user
  logging in while presenting the first user's real cookie no longer
  disturbs the first user's session.
- Not yet performed: the fresh-clone walkthroughs and the Cloudflare
  tunnel negative-Access check (both Slices' spec §8/§9) — require the
  user's machine/dashboard, called out as pending work above.

## Backlog (deferred, not blocking)

- **O-008 (user intent, 2026-08-21): AI next-step suggestions** — after
  every communication attempt/completion (call, email, SMS, chat) and
  once a day, run the Person's communication history through an AI and
  suggest next steps. Reminder only; design open; no work before the
  communication slices. Full note in the decision log.

From the Slice 001 reviews — appropriately low priority per both
reviewers' own framing (dev-only script, latent/library-internal, or
requires a self-registration flow that doesn't exist yet):
- `seed.rs`'s `find_or_create_organization` is not race-safe (no unique
  constraint on `organization.name`); two concurrent `dev-seed` runs
  could create duplicate Organizations. Local dev-bootstrap script only,
  recoverable by hand. **Scheduled for Slice 004** (D-021/O-007 §6),
  where `seed.rs` is rewritten onto the application path anyway.
- Cookie-parsing edge cases (duplicate same-name cookies, percent-encoded
  values) are handled correctly today per `axum-extra`/`cookie`'s
  internals, but pinned only by library behavior, not an explicit test —
  latent risk on a future dependency bump.
- No trimming/Unicode-normalization on login email input; will matter
  once a registration flow exists (none does yet — all emails today are
  fixed seed literals). **Scheduled for Slice 004** (O-007 §6), which
  introduces the first user-entered emails via invitations.
- `migrate`/`seed` binaries propagate connection errors via `Debug`,
  which doesn't currently echo a DSN/password in practice but isn't
  provably safe for every error variant; a generic-message wrapper on
  connect failure would close this defensively.
- `logout()` doesn't format-check the cookie before its DB call (unlike
  `AuthContext`); spec-compliant since logout must accept even invalid
  sessions to stay idempotent, just a possible-but-optional round-trip
  saving for obviously-garbage cookies.

From the Slice 000 review — deferred until `/internal/ready` carries
real operational weight:

Raised by the `crm-tester` adversarial pass; reasonable to defer until
`/internal/ready` carries real operational weight, since a proper fix
means either a hand-rolled Postgres wire-protocol mock or real signal
delivery in a spawned subprocess — disproportionate for this scaffolding
slice:
- No automated test exercises `crm_api::run()`/`shutdown_signal()`
  directly (real TCP bind, real SIGTERM) — currently covered only by the
  manual walkthrough.
- No test for a connection that is *acquired* (fully authenticated) and
  then hangs mid-query, as distinct from a hung/refused *connection
  attempt* (which the new test now covers).
- No test for concurrent `/internal/ready` load or a burst-then-recover
  transition (a manual stress check found this clean under current
  dependency versions, but it is unpinned by the test suite).
- `CRM_API_BIND_ADDR` accepts IPv6 loopback (`[::1]`) silently; untested
  and asymmetric with the Vite dev server's IPv4-only assumption.
- Client-supplied `x-request-id` headers are trusted and echoed verbatim
  (tower-http's standard behavior) — undecided whether that trust is
  correct once the tunnel/Access sits in front of this in earnest.
- `DATABASE_URL=""` (empty-but-set) and a wrong-scheme URL (e.g.
  `mysql://...`) are untested edge cases; both currently fail at a
  reasonable point but with no test pinning the behavior.

## Next recommended action

1. Live walkthrough of 006c from `app.tarams.org`: one call → answer →
   hang up → "How did it go?" → Voicemail → History shows the original
   "(superseded)" + "Outcome corrected — voicemail"; second tab's Today
   card shows the corrected outcome; ringback audible while ringing.
   Ideally also one declined call (proves the "no answer" path).
2. Merge `slice-006c-outcome` → `main`, push.
3. Rotate the Telnyx SIP password; 006a (`crm-app` extraction); 006b.

## Approval currently required

Merge of `slice-006c-outcome` → `main` after the walkthrough.
