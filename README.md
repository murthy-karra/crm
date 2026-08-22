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

Development happens on a local macOS (Apple Silicon) machine per D-016: the API and web dev server run as local processes; PostgreSQL and Centrifugo run in Docker, loopback-only; authentication is local username/password behind the same abstraction ZITADEL fills in production; external connectivity uses a Cloudflare tunnel on `tarams.org` (D-024: no Cloudflare Access in front of it — the app's own login is the only gate).

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
| `MIGRATION_DATABASE_URL` | Unset | The `crm_migrator` (schema-owner) connection; used only by `db-migrate`, `crm-admin bootstrap-platform-admin`, and `check-db` |
| `CRM_DATABASE_CONNECT_TIMEOUT_MS` | `2000`, bounded to 1–30000 ms | Bounds how long readiness/auth queries wait on an unreachable database |
| `CRM_DB_APP_PASSWORD` / `CRM_DB_MIGRATOR_PASSWORD` | Unset | Role passwords `dev-services up` provisions into the local Postgres container |
| `CRM_SESSION_SECRET` | Required, no default | HMAC pepper for session tokens; must be at least 32 bytes; rotating it invalidates every session |
| `CRM_SESSION_TTL_HOURS` | `168`, bounded 1–720 | Absolute session expiry |
| `CRM_SESSION_COOKIE_SECURE` | `false` | Set `true` when using the tunnel; Safari rejects `Secure` cookies over plain `http://127.0.0.1`, so keep it `false` for loopback work in Safari |
| `CRM_DEV_SEED_PASSWORD` | Required for `dev-bootstrap` | One password for every seeded user, including the platform admin (`owner@platform.test`); re-applied on every `seed-dev` run, so changing it rotates seeded credentials |
| `CRM_INVITATION_TTL_HOURS` | `168`, bounded 1–720 | Invitation expiry (docs/specs/SLICE_004.md §11); the API refuses to start if out of bounds |
| `CRM_RAW_PAYLOAD_KEY` | Required, no default | Raw lead-payload encryption key (docs/specs/SLICE_002.md §7); exactly 64 hex characters (32 bytes), e.g. `openssl rand -hex 32`; the API refuses to start if missing, the wrong length, or not hex |
| `CRM_CORS_ALLOWED_ORIGIN` | Unset (no CORS layer) | Set only for the two-hostname tunnel setup (e.g. `https://app.tarams.org`); the API and browser app are on different hostnames, so the browser's cross-origin fetch needs an explicit allow-list entry |
| `CRM_SESSION_COOKIE_DOMAIN` | Unset (host-only cookie) | Leave unset even in the two-hostname tunnel setup: only `api.*` is ever called with credentials (the `app.*` host just serves the SPA), so a host-only cookie is sufficient. Setting it while a host-only `crm_session` already exists is a trap — the next login adds a *second* cookie of the same name, the browser sends the older host-only one first, and the server reads that one; logout then clears only the newer cookie. Set it only if something on `app.*` ever needs the session, and clear cookies for the zone in the same step |
| `CRM_WEB_BIND_ADDR` | `127.0.0.1` | Loopback only |
| `CRM_WEB_PORT` | `5173` | |
| `CRM_WEB_API_PROXY_TARGET` | `http://127.0.0.1:3000` | Loopback HTTP only |
| `CRM_WEB_ALLOWED_HOSTS` | Unset locally | One exact tunnel hostname, never a wildcard |
| `VITE_API_BASE_URL` | `/api` | Browser-visible, root-relative path |
| `CENTRIFUGO_HTTP_API_KEY` | Required, no default | Centrifugo publish credential; the same value `dev-services` passes to the Centrifugo container; the API refuses to start if empty |
| `CENTRIFUGO_TOKEN_HMAC_SECRET` | Required, no default, **at least 32 bytes** | Connection-token signing secret; the same value `dev-services` passes to Centrifugo as `client.token.hmac_secret_key`. Regenerate with `openssl rand -hex 32` (64 hex characters = 32 bytes) and run `./scripts/dev-services down && ./scripts/dev-services up` if an existing value is shorter — both the API and the container must agree, so update `.env` and restart the container together |
| `CRM_CENTRIFUGO_API_URL` | `http://127.0.0.1:8000/api` | Centrifugo's HTTP API base URL; `http://` only, no trailing slash |
| `CRM_REALTIME_TOKEN_TTL_SECONDS` | `600`, bounded 60–3600 | Connection-token lifetime |
| `CRM_WEB_REALTIME_PROXY_TARGET` | `http://127.0.0.1:8000` | Vite's WebSocket proxy target for `/connection` |
| `CRM_DEMO_API_URL` | `http://127.0.0.1:3000` | `scripts/demo-leads`' target |

Use synthetic development data only. Direct remote database access must use a private path such as SSH port forwarding; do not expose PostgreSQL publicly or place its connection URL in shell history.

### Start local services

PostgreSQL and Centrifugo run as loopback-only Docker containers:

```sh
./scripts/dev-services up            # start, waiting for both healthchecks
./scripts/dev-services status        # show container status
./scripts/dev-services down          # stop and remove containers, keep data
./scripts/dev-services down -v       # stop and remove containers AND data (true clean slate)
```

Plain `down` keeps the PostgreSQL data volume — use it to restart the containers (e.g. to pick up a changed `.env` value like `CENTRIFUGO_TOKEN_HMAC_SECRET`, or to resync Docker Desktop's VM clock) without losing local data. Use `down -v` when you actually want a fresh database (e.g. before `dev-bootstrap` if a prior session left conflicting data, such as a duplicate Organization name).

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

Bootstrap the platform admin and seed two Organizations, each with the nine D-019 default stages and two local-auth Users — one admin, one member (D-021, D-026; idempotent; re-running rotates every seeded password, including the platform admin's, to match `CRM_DEV_SEED_PASSWORD`, and creates no duplicate Organizations, users, or memberships):

```sh
./scripts/dev-bootstrap
```

This runs `crm-admin bootstrap-platform-admin` (creates `owner@platform.test` via the migrator connection — the only step that uses it) followed by `crm-admin seed-dev` (everything else, as `crm_app`, through the same domain functions the API uses): "Acme Realty" (Alice admin, Carol member) and "Best Realty" (Bob admin, Dave member). `./scripts/demo-leads` (below) is unaffected.

The `crm-admin` binary also has standalone subcommands for ad hoc administration, all but `bootstrap-platform-admin` resolving an actor via `--as <email>` (defaulting to the sole platform admin when there is exactly one):

```sh
cargo run --manifest-path backend/Cargo.toml -p crm-api --bin crm-admin -- create-organization --name "Cedar Realty"
cargo run --manifest-path backend/Cargo.toml -p crm-api --bin crm-admin -- invite --organization <id> --email erin@cedar.test --role admin --print-link
cargo run --manifest-path backend/Cargo.toml -p crm-api --bin crm-admin -- set-password --email erin@cedar.test
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

Log in at `http://127.0.0.1:5173` with a seeded account, e.g. `alice@acme.test` / the value of `CRM_DEV_SEED_PASSWORD`. A successful login shows the Organization name and its member list; logout revokes the session server-side. Alice and Bob are seeded as Organization admins — their sidebar gains a **Manage** group (`/manage/members`) for inviting, promoting/demoting, and deactivating/reactivating members of their own Organization. Log in as the platform admin, `owner@platform.test` / `CRM_DEV_SEED_PASSWORD`, to reach **Platform** (`/platform`): a membership-free operator account (D-021) that creates Organizations and issues/promotes admin invitations, but has no Organization of its own and cannot see any tenant's People, Inquiries, or Today.

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

This runs, in order: `cargo fmt --check`, `cargo clippy` (warnings denied), `cargo test`, web lint, web typecheck, web test (Vitest), and web build. Database-backed tests are compiled here (so fmt/clippy cover them) but `#[ignore]`d, so this stays service-free. `cargo test`/`clippy` type-check every `query!`/`query_as!` macro call offline against the committed `backend/.sqlx/` cache (root `.cargo/config.toml` sets `SQLX_OFFLINE=true` for every cargo invocation) rather than a live database — see "Offline query cache" below.

Run the database-backed suite against the running local container (requires `dev-services up`; never prints a credential value):

```sh
./scripts/check-db
```

This first checks that Centrifugo answers its health endpoint (a clear, immediate failure — never a skip — if the container is down), then re-verifies `backend/.sqlx/` against a throwaway, freshly-migrated database (`cargo sqlx prepare --check --workspace`, catching schema/type drift an offline compile cannot), then exercises the full session lifecycle, tenant isolation between two Organizations, session/membership revocation, the `crm_migrator`/`crm_app` role boundary, the append-only fact tables, the full lead-intake flow (including its two concurrency races), the Today read model, the realtime publisher's exact event contract per command, the Operator endpoint (validation, concurrency limits, tenant isolation through every tool, prompt-injection containment, and the append-only turn ledger — all driven by a scripted provider, never a real model), and — against the real Centrifugo container, reading `CENTRIFUGO_*` values from the environment because they must match the running container — connection-token scoping, cross-Organization channel isolation, expired/mis-signed token rejection, and no-replay reconnect recovery. Each DB-backed test runs against its own fresh ephemeral database with migrations applied from scratch. Requires sqlx-cli (see prerequisites above).

### Operator

The AI Operator (`POST /api/operator/turns`, the web **Ask** drawer) needs `GROQ_API_KEY` in `.env`; leave it empty and the Operator is simply disabled (the drawer says so; nothing else is affected). `CRM_OPERATOR_BASE_URL` accepts any OpenAI-compatible endpoint (`https://`, or `http://` for a loopback host such as a local model server) and `CRM_OPERATOR_MODEL` picks the model; the timeouts and concurrency cap in `.env.example` are validated at startup even without a key. Nothing the Operator does changes data, and the ledger it writes (`operator_turn`, `operator_tool_call`) holds no message, reply, or argument text — only outcomes, counts, timings, and Person ids. No test ever calls a real model; `./scripts/check` and `./scripts/check-db` run with the key unset.

### Demo data

Log in as `alice@acme.test` through the web app (or hold a session cookie some other way), then post five realistic leads over HTTP — one assigned to Carol, mixed phone/email, a fresh `submission_id` per run so repeated runs add repeat inquiries rather than deduping:

```sh
./scripts/demo-leads
```

Requires `dev-api` running and `jq` installed. Reads the login password from `.env` (`CRM_DEV_SEED_PASSWORD`), never from argv; targets `http://127.0.0.1:3000` by default (`CRM_DEMO_API_URL` to override).

### Offline query cache

New backend code uses sqlx's compile-time-checked `query!`/`query_as!` macros, which need either a live database or a precomputed cache to type-check against. This repository always uses the cache — the root `.cargo/config.toml` sets `SQLX_OFFLINE=true` for every `cargo` invocation, so a query change requires regenerating it before the next build:

```sh
./scripts/sqlx-prepare
```

This applies the current migrations to a throwaway database (never the dev database), regenerates `backend/.sqlx/`, and drops the throwaway database again. Commit the updated `backend/.sqlx/` alongside the query change — it contains only query text and column types, no data. Requires sqlx-cli (see prerequisites above); the script itself asserts the installed CLI's minor version matches the workspace's locked `sqlx` minor version and stops with an install hint otherwise.

### External connectivity (Cloudflare tunnel)

This project uses a dedicated tunnel, `crm-dev`, separate from any other tunnel on the account (e.g. a production `k8s-crm` tunnel) — never point dev traffic at a tunnel you don't know the purpose of.

`crm-dev` was set up to be a **locally-managed** tunnel, with ingress rules in the committed `infra/development/cloudflared/config.yml` rather than the Cloudflare dashboard. **In practice it is not** (D-025, discovered 2026-08-22): the tunnel is dashboard-managed, cloudflared ignores the local file's `ingress:` section entirely and applies whatever routes exist in the Cloudflare Zero Trust dashboard instead. The `tunnel:` id and `credentials-file:` lines in `config.yml` are still load-bearing (that's how `cloudflared` authenticates — `cloudflared tunnel create` writes the credentials file outside the repo, `~/.cloudflared/<tunnel-id>.json`, no token in `.env`); the `ingress:` list below them is **not** — it documents intent only. **The real routing lives in the dashboard: Zero Trust → Networks → Tunnels → `crm-dev` → Routes**, and must be kept in sync with `config.yml` by hand. Routes there are evaluated top-to-bottom, first match wins — the more specific `api.tarams.org/connection/websocket` path route must be ordered *above* the plain `api.tarams.org` catch-all, or the catch-all swallows the WebSocket upgrade and it 404s. There is no drag-to-reorder in the current dashboard UI; to reorder, delete the route that should sort later and re-add it (routes appear to be ordered by creation time).

One-time setup:

1. `cloudflared tunnel create crm-dev` (already done for this repo's dev environment; skip if `cloudflared tunnel list` already shows it).
2. Route DNS for both hostnames to the tunnel:
   ```sh
   cloudflared tunnel route dns crm-dev app.tarams.org
   cloudflared tunnel route dns crm-dev api.tarams.org
   ```
3. Add three routes in the dashboard (Zero Trust → Networks → Tunnels → `crm-dev` → Routes → Add route), in this order — **the order matters**, see above: `app.tarams.org` (no path) → `http://localhost:5173`; `api.tarams.org` with path `/connection/websocket` → `http://localhost:8000`; `api.tarams.org` (no path) → `http://localhost:3000`. `infra/development/cloudflared/config.yml`'s `ingress:` section describes the same three rules for reference, but is not what's actually applied (D-025) — if the tunnel ID or your username differ, still update its `tunnel:` and `credentials-file:` lines to match `cloudflared tunnel list` and your `~/.cloudflared/` path, since those two ARE load-bearing.
4. Set `CRM_CORS_ALLOWED_ORIGIN=https://app.tarams.org`, `CRM_SESSION_COOKIE_SECURE=true`, and `CRM_WEB_ALLOWED_HOSTS=app.tarams.org` in `.env`. Leave `CRM_SESSION_COOKIE_DOMAIN` unset — see its row in the table above. `CRM_CORS_ALLOWED_ORIGIN` is not optional: without it the API attaches no CORS layer, the browser silently discards every `api.*` response even though the server answered `200`, and the app sits on a permanent "Loading…" (the API request log will show `/api/me` succeeding and `/api/people` never being requested).

Then run:

```sh
./scripts/dev-tunnel
```

There is no Cloudflare Access step (D-024): the tunnel routes straight to the app, and the app's own login screen is the only gate.

The realtime WebSocket (`wss://api.tarams.org/connection/websocket`) rides the same tunnel as every other `api.*` request — see the dashboard route above (D-025) and `CENTRIFUGO_TOKEN_HMAC_SECRET` above. Before trusting the tunnel, or any time it's misbehaving, run:

```sh
./scripts/check-tunnel
```

This checks only the network plumbing — not part of `./scripts/check`/`./scripts/check-db`, which deliberately test the application over loopback and never depend on the tunnel or external Cloudflare state (see "Why not route the integration tests through the tunnel" below). It confirms `app.tarams.org` reaches the web app, `api.tarams.org/api/health` reaches the Rust API, and a raw WebSocket upgrade to `api.tarams.org/connection/websocket` returns `101 Switching Protocols`, not `404` — printing a specific diagnosis (pointing at D-025's route-ordering fix) when that last check fails the way it did the day this was written.

**Why not route the integration tests through the tunnel instead of loopback:** the DB-backed and Centrifugo-backed suites (`./scripts/check-db`) test *application* correctness — tenant isolation, ranking, event contracts — which is a different concern from whether Cloudflare's routing happens to be configured correctly right now. Running that suite over the tunnel would make every run slower (real network round-trips instead of loopback) and dependent on external, stateful infra (the tunnel process, DNS, your Cloudflare account) for assertions that have nothing to do with any of that — the opposite of the fast, hermetic local loop this project deliberately has instead of CI (D-016 §9). `check-tunnel` exists precisely to cover the one thing the hermetic suite structurally can't: whether the real network path is wired correctly.

### Troubleshooting

- **Centrifugo rejects a freshly-minted token as expired.** Docker Desktop's VM clock can drift after the host sleeps, so the container's notion of "now" runs ahead of or behind the API process's. Restart Docker Desktop (or just the `centrifugo` container: `./scripts/dev-services down && ./scripts/dev-services up`) to resync the clock.
- **The realtime indicator is stuck on "reconnecting…" through the tunnel, but works on loopback.** Run `./scripts/check-tunnel` first — the dashboard route order (D-025) is the most likely cause, not an app bug. Restarting `cloudflared` does not help; it does not re-read `config.yml`'s ingress for this tunnel.
