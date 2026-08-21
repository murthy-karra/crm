# Slice 000 — Executable Repository Foundation

Status: ACCEPTED (2026-08-20; independently reviewed, user-approved)
Targets: D-013, D-016. No CRM product behavior.

## 1. User-visible outcome

On a fresh clone on the D-016 Mac, a developer can:

1. `./scripts/bootstrap` — verify pinned toolchains (Rust via
   `rust-toolchain.toml`, Node via `.node-version`, pnpm via corepack,
   Docker present), then `cargo fetch --locked` and
   `pnpm install --frozen-lockfile`. Package registries are the only
   external dependency.
2. `./scripts/check` — full repository gate passes with zero services
   running.
3. `./scripts/dev-services` — PostgreSQL and Centrifugo start as
   loopback-only Docker containers with healthchecks;
   `./scripts/check-services` verifies both.
4. `./scripts/dev-api` + `./scripts/dev-web` —
   - `curl -i http://127.0.0.1:3000/api/health` → 200 `{"status":"ok"}`
     with a non-empty `x-request-id` header;
   - `http://127.0.0.1:3000/internal/ready` → 200 when `DATABASE_URL`
     reaches the container, else 503 `{"status":"not_ready"}` (liveness
     unaffected);
   - `http://127.0.0.1:5173` renders an empty Vue shell that fetches
     `/api/health` through the Vite proxy and displays the status.
5. Optionally `./scripts/dev-tunnel` — the same shell reachable at the dev
   hostname on `tarams.org` through Cloudflare Access with real
   certificates (one-time dashboard setup documented in the README).

## 2. In-scope deliverables

### Backend

- `backend/Cargo.toml` — workspace root (`[workspace.dependencies]`,
  `[workspace.lints]`, clippy warnings denied); single member crate
  `backend/crates/crm-api` (lib + thin `main.rs` so integration tests run
  the router in-process). One crate only: with health endpoints there is
  no second module boundary; splitting now would violate AGENTS.md §13.
- Modules: `config.rs` (typed env config, defaults, bounds; reads
  `CRM_API_BIND_ADDR`, default `127.0.0.1:3000`, and rejects non-loopback
  bind addresses in development — symmetric with the web constraint),
  `telemetry.rs` (console `tracing-subscriber` + `EnvFilter`, honors
  `OTEL_SERVICE_NAME`/`RUST_LOG`; the single choke point where an OTLP
  exporter attaches later, per D-016 §9), `routes/health.rs`, `state.rs`
  (lazy SQLx pool).
- `GET /api/health` — liveness, no dependencies, always 200
  `{"status":"ok"}`.
- `GET /internal/ready` — readiness; `SELECT 1` via lazy SQLx pool when
  `DATABASE_URL` is set: 200 `{"status":"ready"}` on success, else 503
  `{"status":"not_ready"}`. Outside `/api` so the proxy and tunnel never
  expose it.
- All specified responses are `application/json`; `x-request-id` appears
  on every response, including 404 and 503.
- Request-id middleware (set + propagate; request-scoped tracing span),
  graceful shutdown on SIGINT/SIGTERM, `dotenvy` load-if-present (D-013).
- SQLx: `runtime-tokio`, `rustls`, `postgres`; connect timeout from
  `CRM_DATABASE_CONNECT_TIMEOUT_MS` (default 2000 ms, bounded 1–30000).
  No schema, no migrations, no query beyond `SELECT 1`.

### Web

- `web/` — Vue 3 + TypeScript + Vite empty shell (`App.vue` fetches
  `VITE_API_BASE_URL` (default `/api`) + `/health`); Vite proxy `/api` →
  `CRM_WEB_API_PROXY_TARGET` (default `http://127.0.0.1:3000`);
  `server.host` from `CRM_WEB_BIND_ADDR` (loopback default);
  `server.allowedHosts` from `CRM_WEB_ALLOWED_HOSTS` (exact tunnel
  hostname, never a wildcard); ESLint flat config + `eslint-plugin-vue`;
  `vue-tsc`. No router, Pinia, or Vitest in this slice.
- `packageManager: "pnpm@11.18.0"`; committed `pnpm-lock.yaml`.

### Services

- `infra/development/compose.yaml` — PostgreSQL and Centrifugo, both bound
  to loopback only (`127.0.0.1:5432`, `127.0.0.1:8000`), named volume for
  Postgres data, healthchecks (`pg_isready`; Centrifugo health endpoint).
  Consumes the root `.env` via `--env-file` from the wrapper script.
- `infra/development/centrifugo/config.json` — minimal; API key and token
  HMAC from `CENTRIFUGO_HTTP_API_KEY` / `CENTRIFUGO_TOKEN_HMAC_SECRET`.
  The application does not talk to Centrifugo in this slice; a healthy
  container is the deliverable.

### Scripts (`scripts/`)

`bootstrap`, `dev-services` (up/down/status), `dev-api`, `dev-web`,
`dev-tunnel`, `check`, `check-services`. Bash, `set -euo pipefail`, never
print environment values.

### Pinning

- `rust-toolchain.toml`: channel 1.97.1 (verify current stable at
  implementation; bootstrap validates via rustup + the toolchain file, not
  ambient `rustc`), components rustfmt + clippy.
- `.node-version`: 24.16.0. Corepack 0.35.0; pnpm 11.18.0.
- Committed `Cargo.lock` and `pnpm-lock.yaml`; `--locked` /
  `--frozen-lockfile` everywhere.
- Images: `postgres:18.<latest minor>`, `centrifugo/centrifugo:v6.<latest>`
  (exact tags, arm64). cloudflared via Homebrew (document minimum version;
  not pinned in-repo).

### Housekeeping

- README Development sections rewritten to D-016. Delete (not merely
  supersede): the slots table and `CRM_APPLICATION_SLOT`, Caddy/shared-
  server language, the Infrastructure-001/Infisical block, and
  `check-slice-001` text (pre-reset content conflicting with D-014/D-016).
  Drop `MIGRATION_DATABASE_URL`/`ADMIN_DATABASE_URL` from the variable
  table (Slice 001 re-adds its own). Add tunnel + Access setup
  documentation (one-time dashboard steps).
- `.gitignore` additions: `target/`, `node_modules/`, `web/dist/`.
- `.env.example` amendment (explicit, spec-approved change to an accepted
  artifact): add `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` for
  compose provisioning, and `CRM_WEB_ALLOWED_HOSTS` (used by the Vite
  config; keeps the D-016 §10 single inventory complete). The rewritten
  README states that `.env.example` is the canonical inventory; any README
  table documents defaults/bounds only.
- `docs/plans/PROJECT_STATE.md` updated at completion.

## 3. Explicit exclusions

- No product schema, migrations, or migrations harness (Slice 001).
- No auth code, not even a stub: this slice has no protected resource; the
  D-016 session/identity abstraction is designed against the first
  authenticated endpoint. The tunnel surface is protected by Cloudflare
  Access. `CRM_SESSION_SECRET` stays inventoried, unused.
- No Operator; no `contracts/` directory (created only when first content
  exists).
- No CI; no OpenObserve container; no opentelemetry-sdk crates (tracing +
  `telemetry.rs` choke point are the D-016 §9 "instrumentation in code").
- No Centrifugo application integration (container only).
- Cloudflare: script the run (`dev-tunnel` wraps `cloudflared tunnel run`,
  passing the token from `CLOUDFLARED_TUNNEL_TOKEN` via the `TUNNEL_TOKEN`
  environment variable — never on argv, where it would appear in the
  process list — targeting the web dev server; API reached via the Vite
  `/api` proxy — one hostname, one Access policy).
  Tunnel creation, DNS route, and the Access application are one-time
  dashboard steps documented in the README. `CLOUDFLARE_API_TOKEN` /
  account/zone IDs stay inventoried, unused, in this slice.
- No Kubernetes/production infra, ZITADEL, LiveKit/Telnyx, mobile
  scaffolds, or worktree tooling.

## 4. Contracts

- HTTP (dev-internal, not yet a `contracts/` artifact): `/api/health` and
  `/internal/ready` as specified in §2. No shared contracts are created or
  changed by this slice.
- Realtime, Operator tools, persistence: none.

## 5. Authorization and tenant isolation

No tenant data and no authenticated endpoint exist in this slice.
Tenant-isolation tests are explicitly N/A (stated here so AGENTS.md §14 is
answered, not skipped). External exposure is protected by Cloudflare
Access per D-016.

## 6. Observability

Console tracing via `telemetry.rs` with `EnvFilter`; request-id on every
request and in every request span. No collector, no exporter.

## 7. Failure behavior

- Readiness returns 503 without a reachable database; liveness stays 200.
- Bootstrap and scripts fail fast (`set -euo pipefail`) with clear
  messages; no script prints an environment value.
- API refuses to start on invalid config (bind address, timeout bounds).

## 8. Checks (definition of done)

`./scripts/check`, in order, green with no services running:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo test --workspace --locked`
4. `pnpm -C web lint`
5. `pnpm -C web typecheck`
6. `pnpm -C web build`

Plus: `./scripts/check-services` green with containers up (pg_isready +
in-container `SELECT 1`; Centrifugo health endpoint) — outside the gate so
the repository check stays service-free. Plus the §1 walkthrough verified
from a fresh clone, including one manual tunnel check with a **negative
Access verification**: an unauthenticated request (fresh private window or
curl) to the dev hostname must land on the Cloudflare Access challenge,
not the shell, before the tunnel is considered verified. The README tunnel
documentation includes this step.

Required tests (in-process, no services): health 200 + request-id header;
ready 503 without DB; ready 503 within the timeout bound when
`DATABASE_URL` is set but unreachable (closed loopback port, short
`CRM_DATABASE_CONNECT_TIMEOUT_MS`); 404 unknown route; config
default/bounds parsing.

## 9. Safe defaults adopted (overridable at approval)

1. PostgreSQL major 18 (reversible; no schema exists).
2. No auth code in this slice.
3. Tunnel targets the web dev server (5173); single hostname, single
   Access policy; webhook hostname deferred (D-016 §4).
4. Readiness performs a live `SELECT 1` when `DATABASE_URL` is set.
5. `.env.example` amended with the three `POSTGRES_*` names.
6. Rust 1.97.1 / pnpm 11.18.0 pins, refreshed to current stable at
   implementation.

## 9a. Implementation notes (from independent review)

- `dev-web` honors `CRM_WEB_PORT` (default 5173) and `dev-tunnel` derives
  its target from it; use `127.0.0.1` consistently, never `localhost`
  (which may resolve to `::1` while Vite binds IPv4 loopback only).
- Tests construct configuration explicitly; they never read ambient
  environment or load `.env`, so `cargo test` stays green regardless of
  the developer's local `.env`.
- Compose receives only explicitly mapped variables, not the full `.env`
  (which holds unrelated secrets); scripts never run `docker compose
  config` (it prints resolved values). Centrifugo v6 native env names
  differ from the repo's — compose maps them explicitly; the committed
  `config.json` contains no secret or placeholder values.

## 10. Lane ownership and sequencing

Single branch `slice-000-foundation`, one writer, sequential:

1. Pins + hygiene (`rust-toolchain.toml`, `.node-version`, `.gitignore`,
   `.env.example`).
2. Backend workspace + Axum skeleton + tests (checks 1–3 green).
3. Web scaffold + proxy + shell (checks 4–6 green).
4. `infra/development/` compose + Centrifugo config; readiness verified
   against the real container.
5. Scripts.
6. README rewrite + tunnel documentation + `PROJECT_STATE.md`.
7. Fresh-clone verification of the full §1 walkthrough.

Estimated 5–7 bounded tasks, roughly 1–2 focused days. Risk concentrates
in fresh-clone honesty (step 7) and tunnel documentation accuracy.

## 11. Likely Slice 001 (so this slice does not overbuild)

Database foundation and first product substrate: migrations harness with
distinct application/migration roles, Organization/User tables, the
session/identity abstraction with local username/password — leading into
the thesis §16 lead-intake slice with D-015's four fact tables. Slice 000
therefore ships no migrations directory, no session code, no fact-table
envelope helpers, and no `contracts/`.
