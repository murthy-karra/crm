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
- Secrets: unresolved (open decision O-001: OpenBao vs Infisical)
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

The repository was reset again on 2026-08-20 and currently contains documentation only: the decision log, product thesis, architecture baseline, event-sourcing research, and project-lead workflow. No application code, scripts, or contracts exist yet; the Development sections below describe the target foundation that Slice 000 will rebuild and do not work in this repository today.

Current operational status and the next approval gate are recorded in `docs/plans/PROJECT_STATE.md`.

## Development

### Supported environment and prerequisites

Development assumes a Linux or macOS environment with Git, curl, a POSIX shell, and standard Unix utilities. Application processes bind only to loopback addresses. On the shared Linux development server, Caddy is the public HTTPS/WSS entry point; PostgreSQL and internal administration endpoints remain private.

Install the pinned prerequisites before bootstrapping:

- Rust 1.97.1 through rustup, with the `rustfmt` and `clippy` components listed in `rust-toolchain.toml`;
- Node 24.16.0, as recorded in `.node-version`;
- Corepack 0.35.0 (Node releases may require installing this exact version separately); and
- pnpm 11.18.0 through Corepack, as pinned by `web/package.json`.

The bootstrap command verifies those exact versions. It does not install system toolchains or start services:

```sh
./scripts/bootstrap
```

It fetches the locked Cargo dependencies and performs a frozen pnpm install. Package registries are the only external services needed by bootstrap.

### Configuration

Configuration is injected through the process environment. Root `.env.example` is a names-only inventory: every value is deliberately empty, it contains no useful credential, and the application does not load it automatically.

| Variable | Default or requirement | Boundary |
|---|---|---|
| `CRM_ENVIRONMENT` | `development` | Server-only, non-secret |
| `CRM_APPLICATION_SLOT` | `local` | Server-only, non-secret |
| `CRM_API_BIND_ADDR` | `127.0.0.1:3000` | Must be a loopback socket address |
| `DATABASE_URL` | Unset; optional for startup and liveness | Sensitive; required only for successful readiness and the PostgreSQL smoke check |
| `MIGRATION_DATABASE_URL` | Unset | Sensitive; process-injected migration role used only by the Slice 001 disposable-schema check |
| `ADMIN_DATABASE_URL` | Unset | Sensitive; process-injected database owner used only to create/drop the Slice 001 disposable schema |
| `CRM_DATABASE_CONNECT_TIMEOUT_MS` | `2000`, bounded to 1–30000 ms | Server-only, non-secret |
| `OTEL_SERVICE_NAME` | `crm-api` | Server-only, non-secret |
| `RUST_LOG` | `info,sqlx=warn` | Server-only, non-secret |
| `CRM_WEB_BIND_ADDR` | `127.0.0.1` | Server-only; loopback only |
| `CRM_WEB_PORT` | `5173` | Server-only |
| `CRM_WEB_API_PROXY_TARGET` | `http://127.0.0.1:3000` | Server-only; loopback HTTP only |
| `CRM_WEB_ALLOWED_HOSTS` | Unset locally | Server-only; one exact active-slot Caddy hostname, never a wildcard |
| `VITE_API_BASE_URL` | `/api` | Browser-visible, root-relative path |

Never commit a local environment file, secret, token, private key, or realistic credential. Use synthetic development data only. Until a later approved Infisical integration exists, inject sensitive values through an approved process-scoped mechanism. Direct remote database access must use a private path such as SSH port forwarding; do not expose PostgreSQL publicly or place its connection URL in shell history.

### Start the applications

Start the API and web application in separate terminals from the repository root:

```sh
./scripts/dev-api
```

```sh
./scripts/dev-web
```

The API starts without PostgreSQL or a telemetry collector. The web shell makes no external request. Vite proxies only `/api` to the loopback API; it does not proxy `/internal/ready`.

Verify liveness:

```sh
curl --include http://127.0.0.1:3000/api/health
```

The response is HTTP 200 with `{"status":"ok"}` and a non-empty `x-request-id` header. Without `DATABASE_URL`, `http://127.0.0.1:3000/internal/ready` intentionally returns HTTP 503 with `{"status":"not_ready"}` while liveness remains healthy.

### Checks

Run the complete provider-neutral repository gate with no PostgreSQL, telemetry collector, DNS, identity provider, secrets service, realtime service, communications provider, or product account:

```sh
./scripts/check
```

When an approved private PostgreSQL connection has already been injected into the process, run the isolated `SELECT 1` smoke check:

```sh
./scripts/check-postgres
```

The normal repository check never invokes this optional database check.

For the complete Slice 001 database contract, separately inject the application,
migration, and database-owner connections into DATABASE_URL,
MIGRATION_DATABASE_URL, and ADMIN_DATABASE_URL, then run:

~~~sh
./scripts/check-slice-001
~~~

This command uses the database owner only to create and drop a uniquely named
schema. It applies the committed migration as the migration role, tests the typed
intake command, idempotency and failure paths, tenant isolation, Today
pagination/rebuild, outbox minimization, and Operator parity through the runtime
application role, then drops the schema. It never prints a connection value. The
provider-neutral ./scripts/check compiles but does not execute this service-backed
test.

### Shared server slots

The stable integration slot and dormant worktree slots use fixed loopback ports. Activate a worktree slot only under a separately approved task, with its matching application slot, exact Caddy hostname, and isolated PostgreSQL database and role.

| Slot | `CRM_APPLICATION_SLOT` | API port | Web port |
|---|---|---:|---:|
| Integration | `integration` | 3000 | 5173 |
| Worktree 1 | `wt1` | 3001 | 5174 |
| Worktree 2 | `wt2` | 3002 | 5175 |
| Worktree 3 | `wt3` | 3003 | 5176 |

Infrastructure 001 adds repo-owned definitions for a dedicated, loopback-only
development Infisical authority, PostgreSQL, and Centrifugo. Provisioning is
phased so encrypted bootstrap installation, human enrollment, the scoped Agent,
and consumers cannot be collapsed into one unattended operation:

```sh
./scripts/dev-services-validate
sudo ./scripts/dev-services-install files
```

See `infra/development/README.md` for the gated bootstrap/start/stop/verification
workflow and the disposable synthetic-data boundary. Infrastructure 001 does not
change Caddy, DNS, firewall policy, ZITADEL, OpenObserve, LiveKit, Telnyx,
Kubernetes, product migrations, or application behavior.
