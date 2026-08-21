# Slice 001 — Identity, Tenancy, and Database Foundation

Status: ACCEPTED (2026-08-20; independently reviewed, user-approved)
Builds on: Slice 000 (`e5182d1`). Targets: D-003, D-004, D-013, D-016.
Scope decision (user-accepted 2026-08-20): narrow cut — no lead intake;
all of Person/Inquiry/fact tables move to Slice 002 because D-015 §8
ships the four fact tables together and every fact row needs an actor.
Abstraction decision (user-accepted 2026-08-20): no `IdentityProvider`
trait; the D-016 §3 seam is the `local_credential` table + one session
mechanism + one trusted-context extractor (see §3).

## 1. User-visible outcome

On the D-016 Mac (and through the tunnel):

1. `./scripts/dev-services up` also provisions two database roles;
   `./scripts/db-migrate` brings the schema to current;
   `./scripts/dev-seed` idempotently creates two Organizations and at
   least one User per Organization with a local password.
2. The web shell at `127.0.0.1:5173` shows a login form. Wrong credentials
   fail with one generic error; correct credentials establish a session
   (HttpOnly cookie).
3. Logged in, the shell shows who I am, my active Organization's name, and
   my Organization's member list — the first org-scoped data in the
   product. The other Organization's members are invisible, and tests
   prove it.
4. Logout revokes the session server-side; replaying the old cookie is
   rejected.
5. `./scripts/check` still passes with zero services running; a new
   `./scripts/check-db` runs the DB-backed suite against the local
   container.

## 2. Domain and schema

All tables: UUID primary keys, `timestamptz` timestamps. One initial
migration set.

- `organization` — id, name, created_at, updated_at. No Office/Team
  (nothing consumes them; AGENTS.md §13).
- `app_user` — id, email (unique, case-insensitive via `lower(email)`
  index), display_name, created_at, updated_at. Identity-provider-neutral:
  no password material here.
- `local_credential` — user_id (PK, FK → app_user), password_hash (PHC
  string), updated_at. When ZITADEL arrives, an `external_identity`
  (subject → user) table is added beside it; `app_user` and everything
  downstream never change.
- `organization_membership` — organization_id, user_id, created_at;
  PK (organization_id, user_id). Schema supports multiple memberships
  (AGENTS.md §4.3). No `role`/`status` columns yet — nothing consumes them
  (see §9 default 3).
- `user_session` — id, token_hash (unique), user_id FK,
  active_organization_id FK, created_at, expires_at, revoked_at
  (nullable).

No fact tables, no envelope helpers, no `raw_payload`. User email and
display name are operator-side identity, not CRM Person PII (D-015 §3
concerns customer data, which does not exist yet).

### Migrations harness

- `sqlx::migrate!` with timestamped plain-SQL files in
  `backend/crates/crm-api/migrations/`. No new tool.
- sqlx-cli is not a bootstrap requirement: a small `migrate` binary in the
  crate runs the embedded migrator against `MIGRATION_DATABASE_URL`;
  `scripts/db-migrate` wraps it. Migration files are hand-authored.
- The application never runs migrations at startup; its role cannot DDL.

### Database roles (single D-016 Postgres container)

- `crm_migrator` — owns schema objects and runs DDL; has `CREATEDB` in
  development (required by `#[sqlx::test]` ephemeral databases).
  Provisioning also makes it the **owner of the dev database**
  (`ALTER DATABASE … OWNER TO crm_migrator`): on PostgreSQL 15+, `CREATE`
  on schema `public` belongs to the database owner only, so a role with
  just `CREATEDB` would fail the first migration with "permission denied
  for schema public".
- `crm_app` — DML only, via explicit per-table `GRANT` statements inside
  each migration (explicit grants over `ALTER DEFAULT PRIVILEGES`, for
  auditability). Slice 001 grants are exactly: `SELECT` on
  `organization`, `app_user`, `local_credential`,
  `organization_membership`; `SELECT, INSERT, UPDATE` on `user_session`.
  Nothing else. In Slice 002 this split becomes the D-015 §2 append-only
  enforcement (no UPDATE/DELETE grant on fact tables).
- Provisioning: an idempotent SQL block piped on stdin to `psql` inside
  the container by `dev-services up`, running as the container superuser.
  `CRM_DB_APP_PASSWORD` and `CRM_DB_MIGRATOR_PASSWORD` are mapped into the
  postgres service environment in `compose.yaml` (same pattern as
  `POSTGRES_PASSWORD`) and read by the SQL via `\getenv` +
  `format('%L')` — never on argv on host or container, never printed.
  Roles are created if absent and always `ALTER ROLE … PASSWORD` so `.env`
  changes propagate. Not `/docker-entrypoint-initdb.d/` (runs only on
  fresh volumes; existing volumes would silently skip it).
- `DATABASE_URL` is redefined as the **`crm_app`** connection.
  `MIGRATION_DATABASE_URL` re-enters `.env.example` (Slice 000 removed it
  pending this slice). The app password therefore appears in two
  variables (`DATABASE_URL` and `CRM_DB_APP_PASSWORD`); `check-db`
  connects with each URL and asserts `current_user` is `crm_app` /
  `crm_migrator` respectively so drift is caught.
- New inventory names: `MIGRATION_DATABASE_URL`, `CRM_DB_APP_PASSWORD`,
  `CRM_DB_MIGRATOR_PASSWORD`, `CRM_SESSION_TTL_HOURS`,
  `CRM_SESSION_COOKIE_SECURE`, `CRM_DEV_SEED_PASSWORD` (see §3).
- Migration ownership: the single Slice 001 writer (AGENTS.md §12).

## 3. Session and identity

**No `IdentityProvider` trait** (user-accepted). The durable seam is:

1. **First-party server-side sessions**, provider-neutral. Any
   authenticator's job ends at `create_session(user_id) → cookie`. Local
   login reaches it via password verification; a future ZITADEL OIDC
   callback reaches it via token exchange + `external_identity` lookup.
   One session table, one cookie, one downstream — no second *session*
   mechanism can exist (native clients later get their own way to obtain
   a session, not a parallel trust path). Architectural implication,
   stated so it is approved knowingly: in production the API is an OIDC
   relying party holding first-party sessions fed by ZITADEL (BFF-style),
   not a resource server validating ZITADEL JWTs on every request. This is
   consistent with D-003 and D-016 §3.
2. **A trusted-context extractor**: `AuthContext { actor_user_id,
   active_organization_id }` implementing Axum `FromRequestParts`. It
   reads the cookie, hashes the token, loads the unexpired/unrevoked
   session, **re-verifies membership in the active Organization on every
   request** (revocation is immediate), and rejects with 401 otherwise.
   Handlers take `AuthContext` as a parameter and never see cookies.
   Organization IDs enter queries only from `AuthContext`, never from
   client input (AGENTS.md §4.2).

**Sessions:** 256-bit random token from the OS RNG, sent to the client
base64url-encoded (43 characters, fixed — the "syntactically invalid
cookie" test depends on this); only its hash is stored (HMAC-SHA256 keyed
by `CRM_SESSION_SECRET`, which gives that already-inventoried variable its
purpose; the API refuses to start if the secret is shorter than 32 bytes;
rotating it invalidates every session). Cookie `crm_session`: HttpOnly,
SameSite=Lax, Path=/, no `Domain`, `Max-Age` matching `expires_at`, and
`Secure` from `CRM_SESSION_COOKIE_SECURE` (boolean, default `false`).
The `Secure` flag is one static setting for the whole process — the API
cannot distinguish loopback from tunnel traffic because the Vite proxy
adds no `X-Forwarded-Proto` — so: set it when working through the tunnel;
Chrome and Firefox still accept `Secure` cookies over `http://127.0.0.1`,
Safari does not, so turn it off for Safari loopback work. Absolute
expiry, `CRM_SESSION_TTL_HOURS` bounded 1–720, default 168; no sliding
renewal; no purge job at dev scale. Cookie rather than bearer token
because the web shell is same-origin behind the Vite proxy and HttpOnly
keeps the credential out of JavaScript; native clients get their
mechanism at the mobile slice.

**CSRF posture:** SameSite=Lax is the control for cookie-authenticated
non-GET requests (Lax, not Strict, because Cloudflare Access re-auth
redirects are top-level GETs that must carry the cookie). It is
sufficient only together with two properties that are therefore part of
this contract: login requires `Content-Type: application/json` and every
JSON-extraction failure (content type, parse error, missing or mistyped
field, oversized body) maps to 400 `malformed_request` — so no HTML form
can perform login-CSRF and cross-origin `fetch` triggers a preflight; and
**no CORS layer is added** — the API is same-origin only. Note that
SameSite's boundary is the registrable domain, so any other `tarams.org`
host (including the future webhook host) is same-site; revisit when one
exists.

**Session fixation and re-login:** `POST /api/session` ignores any
presented session cookie, always mints a fresh token, and overwrites the
cookie. If the presented session was valid, it is revoked on successful
login (best effort).

**Passwords:** Argon2id via the `argon2` crate at its current defaults
(m = 19 MiB, t = 2, p = 1 — recorded so the PHC-format test can assert
them), PHC-string storage, verification under `spawn_blocking`. Login
verifies against a static dummy hash whenever no `local_credential` row
exists — unknown user *or* existing user without a local password — so
there is no timing oracle, and returns one generic 401 for every
credential failure. Passwords are never logged; the trace layer does not
log headers or bodies — keep it that way.

**Active Organization:** at login, load memberships; sole membership →
active. Multiple memberships → deterministic choice: `ORDER BY
created_at, organization_id` (the tie-break matters for same-transaction
seeds). Switching is an excluded later endpoint. One trusted active
Organization per session, per AGENTS.md §4.3. Valid credentials with
**zero memberships** → 403 `{"error":"no_membership"}`, no session
created.

**Users exist only by seeding.** No registration, invites, or admin
endpoints. `scripts/dev-seed` wraps a small binary sharing the
application's hashing code. It connects via `MIGRATION_DATABASE_URL` (so
`crm_app` never needs INSERT on identity tables) and reads one password
for all seeded users from `CRM_DEV_SEED_PASSWORD` (required; re-hashed on
every run so rotation works) — never argv, never printed.

## 4. HTTP contracts

All JSON; every response carries `x-request-id` via existing middleware;
unknown routes stay 404. Error envelope: `{"error": "<code>"}` with the
appropriate status; 401 is `{"error":"unauthenticated"}` (no redirects —
the shell decides what to render).

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `POST /api/session` | JSON `{"email","password"}`, `Content-Type: application/json` required | 200 `{"user":{"id","email","display_name"},"organization":{"id","name"}}` + `Set-Cookie` (fresh token, always) | 400 `malformed_request` (any extraction failure); 401 `invalid_credentials` (generic); 403 `no_membership`; 503 `unavailable` when the database is down |
| `DELETE /api/session` | cookie | 204; revokes the row; clears the cookie with a `Set-Cookie` carrying identical attributes (`Path=/`, `HttpOnly`, `SameSite=Lax`, `Secure` per config) and `Max-Age=0`; idempotent (204 with no or invalid session) | 503 `unavailable` when the revocation cannot be persisted — cookie **not** cleared, so the user is not told they are logged out while the token stays valid |
| `GET /api/me` | cookie | 200, same shape as login success; returns only the active Organization, never enumerates other memberships | 401; 503 |
| `GET /api/organization/members` | cookie | 200 `{"members":[{"user_id","display_name","email","joined_at"}]}`, strictly active-Organization scoped, ordered by `joined_at, user_id` | 401; 503 |

`/api/health` and `/internal/ready` are unchanged. Once authenticated, the
request span may add `actor_id` and `organization_id` fields (AGENTS.md
§10). These are dev-internal contracts recorded here; the `contracts/`
directory remains deferred until a second consumer or the Operator exists
(§9 default 7).

Login and logout are session mechanics, not business mutations in the
AGENTS.md §4.8 sense; no typed domain commands are introduced.

## 5. Authorization and tenant isolation

First slice where AGENTS.md §4.3/§14 apply. Seed and test fixtures always
contain **two** Organizations. Required properties, tested over HTTP:

- Organization A's member list never contains Organization B data
  (positive and negative assertions).
- No endpoint accepts an Organization ID from the client; a well-formed
  request from an A-member cannot yield B data under any input.
- Session lifecycle: forged cookie, tampered token (right format, wrong
  value), expired session, and revoked-after-logout replay are all 401.
- Membership deleted → 401 on the very next request (per-request
  re-verification).
- Role privileges: connected as `crm_app`, DDL is denied (ignored Rust
  test in `check-db`; see §9).

`PersonVisibilityScope` (D-005) does not appear: it is Person-specific and
arrives with Person in Slice 002.

## 6. Observability

Existing console tracing. Authenticated request spans may carry
`actor_id`/`organization_id`. Never log passwords, tokens, cookies, or
session hashes. Login failures log at most the generic outcome.

## 7. Failure behavior

- Database unavailable: login returns 503 `unavailable`; authenticated
  endpoints return 503; `/api/health` stays 200.
- Malformed login body (any JSON-extraction failure): 400
  `malformed_request`.
- Valid credentials, no membership: 403 `no_membership`, no session.
- Logout that cannot persist revocation: 503, cookie left in place.
- Invalid config (missing or short `CRM_SESSION_SECRET`, TTL out of
  bounds, unparsable `CRM_SESSION_COOKIE_SECURE`): API refuses to start,
  consistent with Slice 000.
- Seed run twice: no duplicates, exit 0.

## 8. Explicit exclusions

- All lead intake: Person, contact methods, Inquiry, stages, assignment,
  routing, `raw_payload`, unresolved queue, the four D-015 fact tables,
  envelope helpers, typed domain commands.
- Operator, Groq, Centrifugo application integration (container keeps
  running untouched), Today in any form.
- ZITADEL/OIDC, `external_identity`, MFA, password reset/change,
  registration, invites, user/membership administration, Organization
  switching, roles/permissions, rate limiting/lockout (the dev surface sits
  behind Cloudflare Access per D-016 §4).
- `contracts/` directory, CI, OpenTelemetry exporter, mobile scaffolds,
  webhook hostname.
- Vue router/Pinia (two view states fit conditional rendering in the
  existing shell).

## 9. Checks and tests

`./scripts/check` stays service-free and unchanged in shape. DB-dependent
tests are compiled in the main gate (so fmt/clippy cover them) but marked
`#[ignore]`, keeping `cargo test --workspace --locked` service-free.
Runtime-checked `sqlx::query`/`query_as` this slice (no offline `.sqlx`
cache or compile-time `DATABASE_URL`); revisit offline mode in Slice 002.

New `./scripts/check-db` (peer of `check-services`; requires
`dev-services up`): runs `cargo test … -- --ignored`. `#[sqlx::test]`
reads its master connection from the `DATABASE_URL` environment variable
specifically, so `check-db` exports `DATABASE_URL` set to the
`MIGRATION_DATABASE_URL` value **for that test process only** — this is
the sole test code that reads ambient environment (a documented exception
to Slice 000 §9a). Each test gets an ephemeral database, owned by
`crm_migrator`, with migrations applied from scratch, so migration
verification is implicit on every run (sqlx also creates a `_sqlx_test`
bookkeeping schema in the master database, which is why `crm_migrator`
must own it).

**The router under test runs as `crm_app`, not the migrator.** The pool
`#[sqlx::test]` hands a test is a migrator connection; if the application
were built on it, a forgotten `GRANT` would pass the whole suite and fail
only in `dev-api`. So DB-backed tests take the `PgConnectOptions` from
`#[sqlx::test]`, swap the credentials to `crm_app` /
`CRM_DB_APP_PASSWORD`, and build the application pool from that; fixtures
and teardown use the migrator pool. (`crm_app`'s per-migration grants are
cluster-wide role grants, so they apply inside the ephemeral database.)
The `crm_app`-cannot-DDL assertion is an ignored Rust test over that same
`crm_app` connection, not a psql command. Fallback if `#[ignore]` and
`#[sqlx::test]` interact badly: a `db-tests` cargo feature (then
`check-db` must also run clippy with that feature).

**Required tests — service-free (main gate):** config parsing for all new
variables (bounds, defaults, missing or short secret, boolean parsing of
`CRM_SESSION_COOKIE_SECURE`); password hash/verify roundtrip and PHC
format asserting the recorded Argon2id parameters; 401 with no cookie and
with a syntactically invalid cookie (extractor rejects before touching the
database); login 400 on malformed body, on non-JSON content type, and on
mistyped fields; login and every authenticated endpoint return 503
`unavailable` against an unreachable database (closed loopback port, as
`tests/health.rs` already does); login `Set-Cookie` carries `HttpOnly`,
`SameSite=Lax`, `Path=/`, no `Domain`, and `Secure` iff configured;
logout emits a matching clearing cookie; logout idempotency at the
handler level; error envelope shapes; existing Slice 000 tests untouched.

**Required tests — DB-backed (`check-db`):** full login → me → members →
logout → replay-401 lifecycle; wrong password and unknown user both 401
with identical body; expired session 401; tampered token 401; two-
Organization isolation on members (both directions); probes that send an
Organization ID in query string, header, and body are ignored (makes §5
bullet 2 a test, not a statement); membership revoked → 401 on the next
request; multi-membership deterministic active-Organization selection
*and* that its members list contains only the active Organization's
members; zero-membership login → 403 `no_membership`; re-login mints a
new token and the previous cookie is rejected; `crm_app` cannot DDL and
has exactly the §2 grants; `current_user` is `crm_app` / `crm_migrator`
for the respective URLs; seed idempotency; migrations apply cleanly from
empty (implicit in every `#[sqlx::test]`).

**Manual walkthrough:** login through the web shell locally and through
the tunnel (confirming `Secure` cookie behavior behind Access).

## 10. Safe defaults adopted (overridable at approval)

1. Narrow cut (user-accepted); all intake and fact tables to Slice 002.
2. Email is the "username" of D-016 §3.
3. No `role`/`status` on membership; user administration is seed-only.
   When roles/invites/deactivation arrive, that is a genuine product
   decision (who administers users, which roles exist — thesis §3
   broker-control territory) and goes to the user.
4. Server-side DB sessions + HttpOnly SameSite=Lax cookie; 7-day absolute
   expiry; `CRM_SESSION_SECRET` as the token-hash pepper.
5. Argon2id at current OWASP parameters.
6. No `IdentityProvider` trait (user-accepted).
7. `contracts/` still deferred; this document §4 is the contract of
   record.
8. Multi-membership login picks the earliest membership; switching later.
9. The members list shows colleague emails to all members (consistent
   with D-005's Organization-wide spirit).

Implementation details (not decisions): table naming, UUID version,
`lower(email)` index vs citext, `#[ignore]` vs feature gating, embedded
migrator vs sqlx-cli, `updated_at` maintenance.

## 10a. Implementation notes (from independent review)

- Adding `migrate` and `seed` binaries to the `crm-api` crate requires
  `default-run = "crm-api"` in its `Cargo.toml`, or `scripts/dev-api`'s
  `cargo run -p crm-api` breaks.
- `sqlx` needs the `migrate`, `uuid`, and a time-type feature (`chrono`
  or `time`) added in `backend/Cargo.toml`.
- The `AuthContext` extractor should resolve session + membership in one
  SQL statement (join `user_session` to `organization_membership` on the
  active Organization) so revocation and membership removal are checked
  atomically per request.
- `CRM_DB_APP_PASSWORD` and `CRM_DB_MIGRATOR_PASSWORD` are mapped into
  the postgres service environment in `compose.yaml` solely so
  provisioning SQL can `\getenv` them; the API process reads only
  `DATABASE_URL`.

## 11. Lane ownership and sequencing

Single branch `slice-001-identity`, one writer. Migrations have a single
lane regardless (AGENTS.md §12); the auth core, endpoints, and tests are
one coupled chain; the web login UI is too small to justify a worktree.

1. Role provisioning in `dev-services` + `.env.example` additions +
   `DATABASE_URL` redefinition documented.
2. Migration harness (embedded runner binary, `scripts/db-migrate`) +
   initial migrations with grants.
3. Session/auth core: hashing, token/session create-verify-revoke,
   `AuthContext` extractor, error envelope (service-free tests green).
4. Endpoints + handler tests.
5. Seed binary + `scripts/dev-seed` (two-Organization seed).
6. `scripts/check-db` + full DB-backed suite including isolation tests.
7. Web shell: login form, me/members view, logout.
8. README + `.env.example` final pass + `PROJECT_STATE.md`; manual local
   and tunnel walkthrough.

Roughly 2–3 focused days. Risk concentrates in steps 3 and 6 (the trust
boundary and its tests); review attention goes there.

## 12. Likely Slice 002

Lead intake on this substrate: Person + contact methods (CRUD, PII per
D-015 §3), Inquiry with preserved source attribution (D-006), encrypted
`raw_payload` + visible unresolved queue (D-015 §4), all four fact tables
with envelope and DB-enforced append-only (`crm_app` grants doing the
enforcing), the first typed commands (`ReceiveInquiry`, minimal
assignment/routing, `ChangePersonStage`), `PersonVisibilityScope` with the
single Organization variant, a simulated dev lead ingress, and a minimal
person/inquiry list view — leaving Today + realtime for Slice 003 and
Operator retrieval for Slice 004 to complete the thesis §16 proof chain.
