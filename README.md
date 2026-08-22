# AI-First Real Estate Relationship Platform

This repository contains an AI-first real-estate CRM and relationship platform.

The product is being designed as a modern alternative to Follow Up Boss.

The two primary product goals are:

1. Make migration from Follow Up Boss safe and practical.
2. Let agents operate the CRM through an AI Operator instead of excessive navigation and form entry.

## Planned Technology

- Backend: Rust, Axum, Tokio, SQLx
- Database: PostgreSQL; CloudNativePG in production
- Web: Vue 3 and TypeScript
- iOS: Swift and SwiftUI
- Android: Kotlin and Jetpack Compose
- Authentication: ZITADEL
- Secrets: local `.env` file in development (D-013); OpenBao in production (D-014)
- Realtime: Centrifugo OSS
- Mobile push: APNs and FCM
- Calls and media: LiveKit and Telnyx SIP
- Observability: OpenTelemetry and OpenObserve
- Production infrastructure: OVH bare metal, Talos, Kubernetes, Cilium and vRack
- AI inference: provider-neutral model layer; Groq is the initial preferred provider

## Repository Areas

- `backend/` — Rust application code
- `web/` — Vue web application
- `ios/` — native iOS application
- `android/` — native Android application
- `infra/` — development and production infrastructure
- `scripts/` — repository automation
- `docs/product/` — current product thesis and product requirements
- `docs/architecture/` — current architecture and ADRs
- `docs/decisions/` — accepted decisions and unresolved decision register
- `docs/plans/` — implementation and execution plans
- `docs/specs/` — vertical-slice specifications
- `docs/tasks/` — bounded implementation task briefs
- `docs/research/` — competitor and technical research
- `.agents/skills/` — repository-local coding-agent workflows
- `.codex/agents/` — project-specific Codex subagent definitions
- `contracts/` — canonical shared HTTP, realtime, Operator-tool, and fixture contracts

## Current State

Slice 000 (executable repository foundation) and Slice 001 (identity, tenancy, and database foundation) are implemented: a Rust/Axum API with health/readiness endpoints and local-username/password session authentication, a Vue 3 + Vite web shell with a login flow, Docker Compose for local PostgreSQL and Centrifugo, a migrations harness with distinct application/schema-owner database roles, and the development scripts below. There is no CRM product behavior (Person, Inquiry, and the rest) yet — see `docs/specs/SLICE_000.md` and `docs/specs/SLICE_001.md`.

Current operational status and the next approval gate are recorded in `docs/plans/PROJECT_STATE.md`.

## Development

### Supported environment and prerequisites

Development happens on a local macOS (Apple Silicon) machine per D-016: the API and web dev server run as local processes; PostgreSQL and Centrifugo run in Docker, loopback-only; authentication is local username/password behind the same abstraction ZITADEL fills in production; external connectivity uses a Cloudflare tunnel on `tarams.org` protected by Cloudflare Access.

Install the pinned prerequisites before bootstrapping:

- Rust, through rustup, at the channel pinned in `rust-toolchain.toml`, with the `rustfmt` and `clippy` components;
- Node, at the version recorded in `.node-version`;
- Corepack (ships with Node; run `corepack enable`);
- pnpm, through Corepack, as pinned by `web/package.json`;
- Docker Desktop, for PostgreSQL and Centrifugo; and
- sqlx-cli, pinned to the workspace's locked `sqlx` minor version, for `scripts/sqlx-prepare` and `scripts/check-db` only (not required for `check`, `dev-api`, or `bootstrap` itself — docs/specs/SLICE_002.md §11):
  ```sh
  cargo install sqlx-cli --version 0.8.6 --locked --no-default-features --features postgres,rustls
  ```

The bootstrap command verifies those exact versions (sqlx-cli's presence and version are checked too, but only as a non-fatal warning). It does not install system toolchains or start services:

```sh
./scripts/bootstrap
```

It fetches the locked Cargo dependencies and performs a frozen pnpm install. Package registries are the only external services needed by bootstrap.

### Configuration

Configuration is injected through the process environment. Root `.env.example` is the single canonical, names-only inventory of every variable the development environment requires (D-016): every value is deliberately empty, it contains no useful credential, and nothing loads it automatically. Copy it to `.env` (gitignored) and fill in values locally; never commit `.env`.

Notable defaults and constraints, applied in code:

| Variable | Default | Constraint |
|---|---|---|
| `CRM_API_BIND_ADDR` | `127.0.0.1:3000` | Must be a loopback socket address |
| `DATABASE_URL` | Unset; optional for startup and liveness | The `crm_app` (DML-only) connection; required only for a successful `/internal/ready` and authenticated endpoints |
| `MIGRATION_DATABASE_URL` | Unset | The `crm_migrator` (schema-owner) connection; used only by `db-migrate`, `dev-seed`, and `check-db` |
| `CRM_DATABASE_CONNECT_TIMEOUT_MS` | `2000`, bounded to 1–30000 ms | Bounds how long readiness/auth queries wait on an unreachable database |
| `CRM_DB_APP_PASSWORD` / `CRM_DB_MIGRATOR_PASSWORD` | Unset | Role passwords `dev-services up` provisions into the local Postgres container |
| `CRM_SESSION_SECRET` | Required, no default | HMAC pepper for session tokens; must be at least 32 bytes; rotating it invalidates every session |
| `CRM_SESSION_TTL_HOURS` | `168`, bounded 1–720 | Absolute session expiry |
| `CRM_SESSION_COOKIE_SECURE` | `false` | Set `true` when using the tunnel; Safari rejects `Secure` cookies over plain `http://127.0.0.1`, so keep it `false` for loopback work in Safari |
| `CRM_DEV_SEED_PASSWORD` | Required for `dev-seed` | One password for every seeded user; re-hashed on every run, so changing it rotates seeded credentials |
| `CRM_RAW_PAYLOAD_KEY` | Required, no default | Raw lead-payload encryption key (docs/specs/SLICE_002.md §7); exactly 64 hex characters (32 bytes), e.g. `openssl rand -hex 32`; the API refuses to start if missing, the wrong length, or not hex |
| `CRM_CORS_ALLOWED_ORIGIN` | Unset (no CORS layer) | Set only for the two-hostname tunnel setup (e.g. `https://app.tarams.org`); the API and browser app are on different hostnames, so the browser's cross-origin fetch needs an explicit allow-list entry |
| `CRM_SESSION_COOKIE_DOMAIN` | Unset (host-only cookie) | Leave unset even in the two-hostname tunnel setup: only `api.*` is ever called with credentials (the `app.*` host just serves the SPA), so a host-only cookie is sufficient. Setting it while a host-only `crm_session` already exists is a trap — the next login adds a *second* cookie of the same name, the browser sends the older host-only one first, and the server reads that one; logout then clears only the newer cookie. Set it only if something on `app.*` ever needs the session, and clear cookies for the zone in the same step |
| `CRM_WEB_BIND_ADDR` | `127.0.0.1` | Loopback only |
| `CRM_WEB_PORT` | `5173` | |
| `CRM_WEB_API_PROXY_TARGET` | `http://127.0.0.1:3000` | Loopback HTTP only |
| `CRM_WEB_ALLOWED_HOSTS` | Unset locally | One exact tunnel hostname, never a wildcard |
| `VITE_API_BASE_URL` | `/api` | Browser-visible, root-relative path |

Use synthetic development data only. Direct remote database access must use a private path such as SSH port forwarding; do not expose PostgreSQL publicly or place its connection URL in shell history.

### Start local services

PostgreSQL and Centrifugo run as loopback-only Docker containers:

```sh
./scripts/dev-services up      # start, waiting for both healthchecks
./scripts/dev-services status  # show container status
./scripts/dev-services down    # stop and remove containers
```

Verify both are healthy (outside the main repository gate, since it must stay service-free):

```sh
./scripts/check-services
```

`dev-services up` also (re-)provisions two database roles inside the container: `crm_migrator` (schema owner, runs DDL) and `crm_app` (DML only, exactly the grants each migration adds). Application code always connects as `crm_app`; nothing except migrations, seeding, and `check-db` ever uses the migrator role.

### Database schema

Apply pending migrations (idempotent — safe to run repeatedly):

```sh
./scripts/db-migrate
```

Seed two Organizations, each with the nine D-019 default stages and two local-auth Users — a second member so reassignment has a target (idempotent; re-running rotates the seeded password to match `CRM_DEV_SEED_PASSWORD` and creates no duplicate stages or memberships):

```sh
./scripts/dev-seed
```

### Start the applications

Start the API and web application in separate terminals from the repository root:

```sh
./scripts/dev-api
```

```sh
./scripts/dev-web
```

The API starts without PostgreSQL or a telemetry collector; it logs to the console. The web shell checks for an existing session on load, then shows a login form or the signed-in view. Vite proxies only `/api` to the loopback API; it does not proxy `/internal/ready`.

Log in at `http://127.0.0.1:5173` with a seeded account, e.g. `alice@acme.test` / the value of `CRM_DEV_SEED_PASSWORD`. A successful login shows the Organization name and its member list; logout revokes the session server-side.

Verify liveness:

```sh
curl --include http://127.0.0.1:3000/api/health
```

The response is HTTP 200 with `{"status":"ok"}` and a non-empty `x-request-id` header. Without `DATABASE_URL` reaching a running PostgreSQL, `http://127.0.0.1:3000/internal/ready` intentionally returns HTTP 503 with `{"status":"not_ready"}` while liveness remains healthy; with it, readiness returns HTTP 200 with `{"status":"ready"}`.

### Checks

Run the complete repository gate with no services running:

```sh
./scripts/check
```

This runs, in order: `cargo fmt --check`, `cargo clippy` (warnings denied), `cargo test`, web lint, web typecheck, and web build. Database-backed tests are compiled here (so fmt/clippy cover them) but `#[ignore]`d, so this stays service-free. `cargo test`/`clippy` type-check every `query!`/`query_as!` macro call offline against the committed `backend/.sqlx/` cache (root `.cargo/config.toml` sets `SQLX_OFFLINE=true` for every cargo invocation) rather than a live database — see "Offline query cache" below.

Run the database-backed suite against the running local container (requires `dev-services up`; never prints a credential value):

```sh
./scripts/check-db
```

This first re-verifies `backend/.sqlx/` against a throwaway, freshly-migrated database (`cargo sqlx prepare --check --workspace`, catching schema/type drift an offline compile cannot), then exercises the full session lifecycle, tenant isolation between two Organizations, session/membership revocation, the `crm_migrator`/`crm_app` role boundary, the append-only fact tables, and the full lead-intake flow (including its two concurrency races), each against its own fresh ephemeral database with migrations applied from scratch. Requires sqlx-cli (see prerequisites above).

### Offline query cache

New backend code uses sqlx's compile-time-checked `query!`/`query_as!` macros, which need either a live database or a precomputed cache to type-check against. This repository always uses the cache — the root `.cargo/config.toml` sets `SQLX_OFFLINE=true` for every `cargo` invocation, so a query change requires regenerating it before the next build:

```sh
./scripts/sqlx-prepare
```

This applies the current migrations to a throwaway database (never the dev database), regenerates `backend/.sqlx/`, and drops the throwaway database again. Commit the updated `backend/.sqlx/` alongside the query change — it contains only query text and column types, no data. Requires sqlx-cli (see prerequisites above); the script itself asserts the installed CLI's minor version matches the workspace's locked `sqlx` minor version and stops with an install hint otherwise.

### External connectivity (Cloudflare tunnel)

This project uses a dedicated tunnel, `crm-dev`, separate from any other tunnel on the account (e.g. a production `k8s-crm` tunnel) — never point dev traffic at a tunnel you don't know the purpose of.

`crm-dev` is a **locally-managed** tunnel: its ingress rules live in the committed `infra/development/cloudflared/config.yml`, not the Cloudflare dashboard. (Dashboard-managed ingress is a one-way migration Cloudflare warns is irreversible; a local config file is simpler and keeps the setup reproducible from the repo, consistent with how Centrifugo and the Postgres roles are configured.) It authenticates via the credentials file `cloudflared tunnel create` writes outside the repo (`~/.cloudflared/<tunnel-id>.json`) — no token in `.env`.

One-time setup:

1. `cloudflared tunnel create crm-dev` (already done for this repo's dev environment; skip if `cloudflared tunnel list` already shows it).
2. Route DNS for both hostnames to the tunnel:
   ```sh
   cloudflared tunnel route dns crm-dev app.tarams.org
   cloudflared tunnel route dns crm-dev api.tarams.org
   ```
3. `infra/development/cloudflared/config.yml` already declares the ingress: `app.tarams.org` → `localhost:5173` (web dev server), `api.tarams.org` → `localhost:3000` (API, called directly from the browser — see `CRM_CORS_ALLOWED_ORIGIN` above). If the tunnel ID or your username differ, update the `tunnel:` and `credentials-file:` lines to match `cloudflared tunnel list` and your `~/.cloudflared/` path.
4. In the Cloudflare Zero Trust dashboard (**Access → Applications**), create **one** Access application covering **both** `app.tarams.org` and `api.tarams.org`, with a long-lived session duration and an Allow policy for your email. Enable **`options_preflight_bypass`** on the application — without it, Access intercepts the browser's CORS preflight (`OPTIONS`) requests to `api.tarams.org` before they ever reach the API, and the login form fails with a CORS error.
5. Set `CRM_CORS_ALLOWED_ORIGIN=https://app.tarams.org`, `CRM_SESSION_COOKIE_SECURE=true`, and `CRM_WEB_ALLOWED_HOSTS=app.tarams.org` in `.env`. Leave `CRM_SESSION_COOKIE_DOMAIN` unset — see its row in the table above. `CRM_CORS_ALLOWED_ORIGIN` is not optional: without it the API attaches no CORS layer, the browser silently discards every `api.*` response even though the server answered `200`, and the app sits on a permanent "Loading…" (the API request log will show `/api/me` succeeding and `/api/people` never being requested).

Then run:

```sh
./scripts/dev-tunnel
```

Before trusting either hostname, verify Access is actually in front of both: from a fresh private browser window (or `curl` with no session cookie), a request to `app.tarams.org` **and** to `api.tarams.org` must each land on the Cloudflare Access challenge, not the application. Only after both negative checks pass is the tunnel considered verified.

**Access sessions are scoped per hostname, not per Application** — logging in at `app.tarams.org` does not also authenticate `api.tarams.org`, even though they share one Application and one policy. Visit each hostname directly once (a real top-level page load, e.g. `https://api.tarams.org/api/health`) to establish its own session; only then will the browser's background `fetch()` calls from `app.*` to `api.*` carry a valid session and succeed.

Set `CRM_SESSION_COOKIE_SECURE=true` before logging in through the tunnel, and confirm the app's own login still works once you're past Access — the two are independent layers.

A future webhook-receiving hostname will bypass Access (webhooks cannot complete a login challenge) and instead verify requests itself, e.g. via provider signatures.
