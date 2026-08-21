# Project State

Last updated: 2026-08-20

## Current phase

Slice 000 merged to `main` (`e5182d1`, pushed) with user approval;
Slice 001 planning in progress.

## Current slice

Slice 001: identity, tenancy, and database foundation — narrow cut, no
lead intake. `docs/specs/SLICE_001.md` ACCEPTED 2026-08-20 (independently
reviewed, 14 findings applied, user-approved). Awaiting the
implementation gate.

## Current branch

`main` (single writer; no worktrees active). The merged
`slice-000-foundation` branch has been deleted.

## Last accepted decision

D-016 (2026-08-20): development environment is the developer's Mac (M1
Max) — API and Vite as local services, PostgreSQL and Centrifugo in
Docker, local username/password auth behind the production auth
abstraction, Cloudflare tunnel on tarams.org protected by Cloudflare
Access. Production remains Kubernetes per D-001.

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

## Pending work

1. Implementation gate approval for Slice 001.
2. User-side (whenever convenient, not blocking): fresh-clone walkthrough
   of Slice 000, and the Cloudflare tunnel one-time dashboard setup with
   its negative-Access check.

## Blocking decisions

None for Slice 000 or its merge. O-006 (outbound messaging consent) blocks
the SMS slice; O-002 (recording consent) blocks recording features.

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

2026-08-20, on `slice-000-foundation`, after review fixes, all local (no
CI yet):
- `./scripts/check` — cargo fmt, clippy (`deny`, `-D warnings`), cargo test
  (11/11 passing), web lint, typecheck, build — all green with zero
  services running.
- `./scripts/dev-services up` + `./scripts/check-services` — PostgreSQL and
  Centrifugo healthy; pg_isready, `SELECT 1`, and the Centrifugo health
  endpoint all pass.
- Manual: `/api/health` (200, request-id, JSON body), `/internal/ready`
  against the real database (200 `{"status":"ready"}`) and without one (503
  `{"status":"not_ready"}`, now with a logged reason), unknown route (404
  with request-id), per-request tracing visible under the default
  `RUST_LOG`, the Vite dev server serving the shell and proxying
  `/api/health` correctly, and graceful shutdown on SIGTERM.
- Not yet performed: the fresh-clone walkthrough and the Cloudflare tunnel
  negative-Access check (spec §8) — both require the user's machine/dashboard
  and are called out as pending work above.

## Backlog (deferred from Slice 000 review, not blocking)

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

Implementation gate for Slice 001, then implement on
`slice-001-identity`.

## Approval currently required

Implementation gate for Slice 001 ("Proceed with implementation?").
