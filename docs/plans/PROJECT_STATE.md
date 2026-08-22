# Project State

Last updated: 2026-08-21

## Current phase

Slice 002 (lead intake) merged to `main` (`0e764aa` backend,
`70a33f2`/`1c4384b` frontend). Implemented, independently reviewed,
adversarially tested, fixed, verified with a live two-process browser
walkthrough, and re-verified green on `main` after merge (`./scripts/check`
full pass: fmt, clippy, all Rust tests, web lint/typecheck/build). Ready
to plan Slice 003.

## Current slice

None active. Next up: Slice 003 (Today + realtime; spec sketch at
`docs/specs/SLICE_002.md` §16).

## Current branch

`main`, clean, both feature branches deleted and the frontend worktree
removed after merging (`slice-002-intake`, `slice-002-web`).

## Last accepted decision

2026-08-21, user-accepted:
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
- 2026-08-21 (post-merge, on `main`, user-reported from live use through
  the tunnel): three fixes, no slice open.
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

## Pending work

1. User-side, whenever convenient, not blocking: the Access application's
   configuration (two hostnames, one policy, `options_preflight_bypass:
   true`) was created via a series of API calls rather than a
   reproducible dashboard/IaC flow and isn't visible from the repo —
   worth documenting more durably (e.g. a short note in the README) or
   recreating via Terraform/dashboard if this needs to survive account
   changes.
2. User-side (whenever convenient, not blocking): fresh-clone walkthrough
   of Slice 000.
3. Not blocking: the local dev database (`crm_dev`) now has a few
   Slice-002-testing rows from review/adversarial/manual verification
   (e.g. "Ada Lovelace", "Grace Hopper", an unresolved "website" entry).
   Harmless local data; `./scripts/dev-services down` + `up` + re-migrate
   + re-seed resets it if a clean slate is wanted before real use.
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

From the Slice 001 reviews — appropriately low priority per both
reviewers' own framing (dev-only script, latent/library-internal, or
requires a self-registration flow that doesn't exist yet):
- `seed.rs`'s `find_or_create_organization` is not race-safe (no unique
  constraint on `organization.name`); two concurrent `dev-seed` runs
  could create duplicate Organizations. Local dev-bootstrap script only,
  recoverable by hand; fix is a one-line unique index whenever it's
  next convenient.
- Cookie-parsing edge cases (duplicate same-name cookies, percent-encoded
  values) are handled correctly today per `axum-extra`/`cookie`'s
  internals, but pinned only by library behavior, not an explicit test —
  latent risk on a future dependency bump.
- No trimming/Unicode-normalization on login email input; will matter
  once a registration flow exists (none does yet — all emails today are
  fixed seed literals).
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

Plan Slice 003 (Today + realtime) with the `crm-planner` subagent,
following the same plan → review → spec → review → approve → implement
→ review → test → verify → commit → merge flow used for Slices 000–002.

## Approval currently required

None right now. Next gate: approval of the Slice 003 scope/plan.
