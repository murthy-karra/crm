# Slice 003 — Today + Realtime

Status: APPROVED (user, 2026-08-21; planner draft independently
reviewed — 17 findings, all applied as safe defaults or implementation
notes; spec re-reviewed by the same reviewer — 13 further amendments
applied, none blocking; one product decision taken by the user, recorded
as D-022; the realtime model recorded as D-023 at approval).
Builds on: Slice 002 (`main` at `7c3a324`, plus the D-021 docs commit
`df6356d`).
Targets: D-005 (personal Today ownership by assignment), D-007/D-015
(one new PII-free fact), D-010 (deterministic, explainable Today),
D-011 (realtime is delivery, not truth), D-017 §5 (events drive TanStack
Query invalidation), D-021 (sequencing); thesis §8, §13, and the middle of
the §16 proof chain: `correct CRM state → Today → realtime delivery`.
Operator retrieval is Slice 005 and calling Slice 006 per D-021
(SLICE_002 §16's numbering is superseded).

Scope decisions (user-accepted 2026-08-21, not re-litigated here):
(1) a contact attempt is a recorded fact and the unit of response for
Today (D-022; the fifth typed fact table, extending D-015 §8's
first-slice count of four); (2) safe defaults in §14 stand unless
overridden at approval.

The narrowest cut that proves the chain with real tests:

1. one new fact `contact_attempted` + command `LogContactAttempt`;
2. a computed, deterministic `GET /api/today`;
3. Centrifugo integration — API-minted connection tokens, one
   server-side Organization channel per connection, ids-only events,
   best-effort publish after commit;
4. web Today view, "Log contact", and a realtime composable that
   invalidates TanStack Query caches and refetches on reconnect;
5. the first realtime delivery, isolation, and reconnect/recovery tests.

## 1. User-visible outcome

On the D-016 Mac, through the tunnel (the user's normal path) and on
loopback:

1. `./scripts/dev-services down && ./scripts/dev-services up` (Centrifugo
   reads the new `org` namespace config at start) → `./scripts/db-migrate`
   (one new migration) → `./scripts/dev-seed` (unchanged) →
   `./scripts/dev-api` (now refuses to start without the two
   `CENTRIFUGO_*` values, §11) → `./scripts/dev-web` → `./scripts/dev-tunnel`.
2. Log in as `alice@acme.test`. You land on **Today** (new default
   route). Sidebar `Work`: Today, People. Today is empty: "Nothing needs
   your attention."
3. Run `./scripts/demo-leads`. It logs in as Alice over HTTP and POSTs
   five leads (zillow / realtor_com / website / referral / manual; mixed
   phone and email; one assigned to Carol). Within about a second Alice's
   Today shows four rows **without a refresh**: priority High, reasons
   "New inquiry · No contact attempt", recommended action Call (phone
   present) or Email (email only), waiting-longest first. The
   Carol-assigned lead is absent; logging in as Carol in another browser
   shows exactly that one.
4. Click **Log contact** on a row → dialog (Channel: Call, Outcome: No
   answer) → the row leaves Today in every open tab; the Person's History
   shows a fifth entry "Contact attempted — call, no answer".
5. Submit a repeat lead for that Person (same email, different source) →
   the Person returns to Today with "New inquiry · No contact attempt ·
   Inquired again (2)" (reasons render in §3's fixed order).
6. Reassign a Today Person to Carol from Person detail → it leaves
   Alice's Today and appears live on Carol's.
7. `docker stop` the Centrifugo container → the sidebar footer shows
   "Realtime: reconnecting…". Run `demo-leads` again → the API still
   returns 201 for every lead (API log: one `warn` per failed publish).
   Start the container again → the indicator clears and Today refetches,
   showing the new leads (recovery by refetch, D-011). Even with
   Centrifugo down, Today self-heals within 60 s (interval) or on window
   focus.
8. Log in as `bob@best.test` → Today empty. Bob's connection is
   subscribed to `org:<Best Realty>` only; nothing from Acme reaches it
   (proved by the Centrifugo-backed test, observable in the network tab).
9. Logout closes the WebSocket. `./scripts/check` is green with no
   services running; `./scripts/check-db` is green and now also exercises
   the running Centrifugo container.

## 2. Domain and schema

### Classification (D-007, D-015)

- **Immutable, PII-free history** (D-015 §2/§3): `contact_attempted` —
  the fifth typed fact table, same envelope and discipline as the four
  from Slice 002. A contact attempt is a real-world event with historical
  meaning: first-response time is the broker's metric (thesis §3, §14),
  and it is exactly what the calling slice will record automatically.
  It carries no free text (notes are CRUD, a later slice).
- **Computed read model** (AGENTS.md §4.7, D-010): Today. Not a table.
  Computed per request from authoritative rows inside one statement:
  one consistent snapshot, no projection lag, no second source of truth
  and therefore none of the concurrent-write projection-gap class of
  defect a materialized view would need to solve first. For a 10–50
  agent team the query touches one agent's assigned People (new index),
  each one's latest Inquiry (existing `inquiry_org_person_received_idx`)
  and last contact attempt (new index) — milliseconds at thousands of
  People per agent. Materialize only when reasons depend on high-volume
  signals (web/property activity, email sync) or cross-agent aggregation.
- **Realtime events**: not persisted. Ids-only invalidation hints
  (D-011, D-017 §5), never state, never PII (§6).

No erasable-CRUD, blob, or reference-data changes.

### Migration (one, Lane A, AGENTS.md §12): `20260822000001_contact_attempted.sql`

```sql
CREATE TABLE contact_attempted (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    causation_id UUID,
    corrects_id UUID REFERENCES contact_attempted (id),
    person_id UUID NOT NULL,   -- bare UUID, no FK (SLICE_002 §2 rule)
    channel TEXT NOT NULL CHECK (channel IN ('call', 'text', 'email', 'other')),
    outcome TEXT NOT NULL CHECK (outcome IN ('reached', 'no_answer', 'left_message', 'sent')),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);
-- SLICE_006c widens the outcome CHECK with 'busy', 'wrong_number' and adds
-- a partial unique index on corrects_id (one corrector per row).
CREATE INDEX contact_attempted_org_person_occurred_idx
    ON contact_attempted (organization_id, person_id, occurred_at);
CREATE INDEX contact_attempted_org_correlation_idx
    ON contact_attempted (organization_id, correlation_id);
GRANT SELECT, INSERT ON contact_attempted TO crm_app;
CREATE TRIGGER contact_attempted_append_only
    BEFORE UPDATE OR DELETE ON contact_attempted
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER contact_attempted_no_truncate
    BEFORE TRUNCATE ON contact_attempted
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
-- Today: "People assigned to me" (IS NULL lookups are served by the same btree)
CREATE INDEX person_org_assignee_idx ON person (organization_id, assigned_user_id);
```

`sent` exists so an email or text can be logged honestly; `reached`,
`no_answer`, `left_message` are call-shaped. Outcome is not
cross-validated against channel (the UI picks sensible defaults per
channel). No other table, grant, seed, or CHECK changes. `.sqlx/` is
re-prepared (SLICE_002 §11).

D-015 §8's "exactly four typed fact tables" was scoped to the *first*
history-bearing slice; this slice adds the fifth in the same style
(D-022), not a generic event store.

## 3. Today

### Semantics

A **Today item** is a Person, assigned to the viewer, whose latest
Inquiry has not been responded to: there is no `contact_attempted` for
that Person with `occurred_at >= latest_inquiry.received_at`. A contact
attempt at or after the Inquiry's `received_at` answers it.

Consequences, stated so nobody is surprised (reviewer finding; recorded
in D-022):

- **The only exits are a contact attempt, reassignment, or
  unassignment.** A contact attempt logged by **any** member of the
  Organization counts as the response (the predicate does not check who
  logged it; consistent with 002's org-wide assign/stage in the no-roles
  model). Opening a Person does nothing (thesis §8). There is no
  done/snooze/dismiss — the thesis defers those semantics and nothing
  here pre-decides them.
- **Stage does not remove a Person from Today.** A Person moved to the
  seeded "Trash" or "Closed" stage stays listed until a contact attempt
  is logged. Reason: D-019 stages have no semantic key, and D-020 says a
  second name-based hinge is a new decision, not a precedent. The proper
  fix is a stage semantic key (a D-019 follow-up, `LATER`). Until
  completion/snooze/dismiss semantics are specified, the workaround for
  a junk lead is to log a contact attempt (`other` / `no_answer`) —
  which is itself recorded history.
- People with no Inquiry are never on Today (none exist via the
  application path). Unassigned People are on nobody's Today.

### Ranking (exact; D-010)

For each candidate, with `now` = the request's `generated_at`:

- `latest_inquiry` = the Person's Inquiry with the greatest
  `received_at`, tie-break `id DESC`.
- `last_contact_attempt` = the Person's `contact_attempted` with the
  greatest `occurred_at`, tie-break `id DESC`; null if none. SLICE_006c:
  only *effective* rows count (rows with no corrector via `corrects_id`).
- `waiting_since` = the `received_at` of the **earliest Inquiry not yet
  answered** — the earliest Inquiry with `received_at >
  last_contact_attempt.occurred_at`, or the earliest Inquiry overall when
  there is no attempt. A repeat inquiry therefore does not reset the
  clock: someone who inquired three days ago and again an hour ago has
  been waiting three days. (The alternative, `latest_inquiry.received_at`,
  was considered and rejected because "waiting longest first" would then
  be false for repeat inquirers; the Operator's explanation must match.)
- `fresh` = `latest_inquiry.received_at > now − 24 h`
  (`FRESH_INQUIRY_WINDOW`; strict). **Computed once, in SQL**, emitted as
  a boolean column of `TodayCandidate` and used in the `ORDER BY` below;
  `rank()` reads it and never re-evaluates the window (two evaluations of
  the boundary — SQL with `$now` and Rust with its own `now` — could put
  a row in the `high` tier that `rank()` labels `normal`). `now` is only
  echoed as `generated_at`.
- Reasons, in this fixed order, each a typed code with parameters:
  1. `new_inquiry { source, received_at }` if `fresh`;
  2. `no_contact_attempt { since }` always, `since = waiting_since`;
  3. `repeat_inquiry { inquiry_count }` if the Person's total Inquiry
     count ≥ 2.
- `priority = high` iff `new_inquiry` is present, else `normal`.
- `recommended_action = call` if `primary_phone` is present, else
  `email`.
- Order: `high` before `normal`; within a tier `waiting_since ASC`;
  tie-break `person.id ASC`. **The tier is part of the SQL `ORDER BY`**
  (`fresh DESC, waiting_since ASC, id ASC`) **before `LIMIT 201`**, so
  fresh leads can never fall off the cap behind stale ones. `rank()`
  preserves SQL order (it never re-sorts); a test pins this (§13
  criterion 3).
- Cap 200 with `truncated` (002 convention). `generated_at` is returned
  so an explanation is reproducible.

The reason codes, parameters, and ordering are the contract a Slice 005
Operator tool explains; it calls `today::query`, never a separate path.

## 4. Commands and queries

Module layout (existing style):

```text
backend/crates/crm-api/src/
  config.rs                        + CentrifugoApiKey, RealtimeTokenSecret (redacted Debug),
                                     centrifugo_api_url, realtime_token_ttl
  state.rs                         + publisher: realtime::Publisher; for_tests(pool, &config, publisher)
  realtime/mod.rs
  realtime/token.rs                mint(secret, user_id, organization_id, now, ttl) -> String  (HS256 JWT, §6)
  realtime/events.rs               RealtimeEvent, PersonChange, Envelope, channel_for(organization_id)
  realtime/publisher.rs            Publisher { Centrifugo(CentrifugoTransport), Recording(Arc<Mutex<Vec<Published>>>) }
                                   publish_after_commit(&self, Envelope)      Centrifugo: tokio::spawn, budget §9;
                                                                              Recording: synchronous
                                   publish_now(&self, &Envelope) -> PublishOutcome   (the spawned body; also used by
                                                                              failure-path tests)
  domain/today/{mod.rs, model.rs, queries.rs, rank.rs}
  domain/commands/log_contact_attempt.rs
  domain/facts.rs                  + insert_contact_attempted
  domain/person/queries.rs         + contact_attempted in the history UNION (kind_rank 4)
  routes/today.rs                  GET /api/today
  routes/realtime.rs               POST /api/realtime/token
  routes/people.rs                 + POST /api/people/{id}/contact-attempts (reuses the PersonId extractor)
```

```rust
// domain/commands/log_contact_attempt.rs
pub enum ContactChannel { Call, Text, Email, Other }                 // "call" | "text" | "email" | "other"
pub enum ContactOutcome { Reached, NoAnswer, LeftMessage, Sent }     // "reached" | "no_answer" | "left_message" | "sent"
pub struct LogContactAttempt { pub person_id: Uuid, pub channel: ContactChannel, pub outcome: ContactOutcome }
pub struct ContactAttemptRef { pub id: Uuid, pub channel: ContactChannel, pub outcome: ContactOutcome,
                               pub occurred_at: DateTime<Utc> }
// One transaction: lock_person through the Organization (else PersonNotFound); insert one
// contact_attempted fact, occurred_at = Utc::now(), envelope from CommandContext; commit;
// then publish person.changed{contact_attempted}. Not idempotent (a double submit is two
// facts) — the dialog disables its button while pending; a client attempt id is LATER.
pub async fn log_contact_attempt(pool: &PgPool, publisher: &Publisher, ctx: &CommandContext,
                                 cmd: LogContactAttempt) -> Result<(PersonSummary, ContactAttemptRef), CommandError>;

// domain/today
pub const FRESH_INQUIRY_WINDOW: Duration = 24 h;
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TodayReason {
    NewInquiry { source: String, received_at: DateTime<Utc> },
    NoContactAttempt { since: DateTime<Utc> },
    RepeatInquiry { inquiry_count: i64 },
}
pub enum TodayPriority { High, Normal }
pub enum RecommendedAction { Call, Email }
pub struct TodayItem { person: PersonSummary, priority, recommended_action, reasons: Vec<TodayReason>,
                       waiting_since: DateTime<Utc>, latest_inquiry: InquiryRef,
                       last_contact_attempt: Option<ContactAttemptRef> }
pub struct TodayCandidate { /* raw row: summary columns + latest inquiry + last attempt + waiting_since + inquiry_count
                               + fresh: bool (computed in SQL, §3) */ }
pub async fn candidates(conn, scope: &PersonVisibilityScope, viewer: Uuid, now: DateTime<Utc>)
    -> Result<(Vec<TodayCandidate>, bool /* truncated */), sqlx::Error>;
pub fn rank(candidates: Vec<TodayCandidate>, now: DateTime<Utc>) -> Vec<TodayItem>;   // pure; preserves order
pub async fn query(conn, scope, viewer, now) -> Result<TodayList, sqlx::Error>;       // what a Slice 005 tool calls
```

`candidates` starts from `person WHERE organization_id = $scope AND
assigned_user_id = $viewer` (the new index) and joins the latest Inquiry,
the last contact attempt, and the earliest unanswered Inquiry per Person
(`LATERAL` or `DISTINCT ON`, Lane A's choice — the tie-breaks in §3 are
not optional). `assigned_user_id = $viewer` is a **Today-ownership rule
applied inside `PersonVisibilityScope::Organization`**, not a new scope
variant: nothing adds an `AssignedUser` variant to `visibility.rs`
(AGENTS.md §4.4). The viewer is always `AuthContext.actor_user_id`.

**Existing commands** (`receive_inquiry`, `assign_person`,
`change_person_stage`) gain a `publisher: &Publisher` parameter and call
`publish_after_commit` after `tx.commit()`: intake resolved (new or
matched Person) → `person.changed{inquiry_received}`; intake unresolved →
`intake.unresolved_changed`; duplicate delivery → nothing;
assign/stage → an event only when `changed`; `IntakeBusy` and errors →
nothing. One event per command execution, not per fact — a matched-Person
intake that writes two facts publishes exactly one event. Event
`occurred_at` is the fact's `occurred_at` (for intake, `received_at`);
for `intake.unresolved_changed`, which has no fact, it is the
`raw_payload.received_at`. Never publish time.

## 5. HTTP contracts

New endpoints only. All via `AuthContext`; `{"error": code}` envelope;
401 without a valid session and 503 `unavailable` on database failure
exactly as Slices 001–002; non-UUID path → 400 `malformed_request`.
**No new `ApiError` variants**: an invalid `channel`/`outcome` is a serde
rejection → 400 `malformed_request`. CORS method list (GET/POST/DELETE)
unchanged.

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `GET /api/today` | — (no parameters; the viewer is `AuthContext.actor_user_id`, never client input) | 200 `{"generated_at": ts, "items": [TodayItem…], "truncated": bool}` ordered per §3, cap 200 | 401, 503 |
| `POST /api/people/{id}/contact-attempts` | `{"channel": "call"\|"text"\|"email"\|"other", "outcome": "reached"\|"no_answer"\|"left_message"\|"sent"\|"busy"\|"wrong_number"}` (last two added by SLICE_006c); JSON only; `occurred_at` is server time (not accepted from the client this slice) | 201 `{"person": {…PersonSummary}, "contact_attempt": {"id", "channel", "outcome", "occurred_at"}}` | 400 `malformed_request`, 404 `not_found` (byte-identical for other Organizations' ids), 503 |
| `POST /api/realtime/token` | — (cookie) | 200 `{"token": "<jwt>"}`; minting is local (HMAC) and never contacts Centrifugo | 401 (including a revoked membership — this is the refresh cut-off, §7), 503 |

`PersonSummary` is exactly the `GET /api/people` item shape of SLICE_002
§5. `TodayItem`:

```json
{
  "person": { "…": "PersonSummary exactly as SLICE_002 §5 GET /api/people" },
  "priority": "high",
  "recommended_action": "call",
  "reasons": [
    { "code": "new_inquiry", "source": "zillow", "received_at": "2026-08-21T18:02:11.512Z" },
    { "code": "no_contact_attempt", "since": "2026-08-21T18:02:11.512Z" },
    { "code": "repeat_inquiry", "inquiry_count": 2 }
  ],
  "waiting_since": "2026-08-21T18:02:11.512Z",
  "latest_inquiry": { "id": "…", "source": "zillow", "received_at": "2026-08-21T18:02:11.512Z" },
  "last_contact_attempt": { "id": "…", "channel": "call", "outcome": "no_answer", "occurred_at": "…" }
}
```

**Change to an existing SLICE_002 §5 contract (AGENTS.md §11 — declared
here, additive, approved with this spec):** `GET /api/people/{id}`
`history[].kind` gains a fifth value `contact_attempted` with `detail:
{"channel", "outcome"}` and `kind_rank` 4; the ordering rule is
otherwise unchanged. Affected: `web/src/api/types.ts` (`HistoryEntry`
union) and `PersonDetailView.vue` (`HISTORY_ICON` / `historySummary`; the
exhaustive switch fails typecheck until handled). Nothing else in
SLICE_002 §5 changes. No migration impact beyond §2. With approval the
coordinator adds one line to SLICE_002 §5's "History entries" paragraph
pointing here, so the contract of record for history kinds is not split
across two specs (AGENTS.md §11 item 6).

## 6. Realtime contracts

**Channel.** `org:<organization_id>` (lowercase hyphenated UUID),
namespace `org`. One channel per Organization. No per-user channel this
slice (nothing targets a user yet; additive later).

**Connection token.** HS256 JWT signed with
`CENTRIFUGO_TOKEN_HMAC_SECRET` (the `.env` value compose already maps to
Centrifugo's `client.token.hmac_secret_key`). Header
`{"alg":"HS256","typ":"JWT"}`, base64url without padding, compact JSON:

```json
{ "sub": "<user_id>", "iat": 1787500000, "exp": 1787500600, "channels": ["org:<active_organization_id>"] }
```

`channels` is a **server-side subscription**: the client never names a
channel, and the channel set is fixed for the life of the connection
(no reliance on Centrifugo's refresh-time channel handling). TTL
`CRM_REALTIME_TOKEN_TTL_SECONDS`, default 600, bounds 60–3600. No names,
emails, or `info` in the token. The client refreshes through the SDK's
`getToken` callback, which `POST`s `/api/realtime/token` with credentials.

**Event envelope**, published with `POST {CRM_CENTRIFUGO_API_URL}/publish`,
header `X-API-Key: <CENTRIFUGO_HTTP_API_KEY>`, body
`{"channel": "org:…", "data": <envelope>}`:

```json
{
  "v": 1,
  "type": "person.changed",
  "organization_id": "…",
  "occurred_at": "2026-08-21T18:02:11.512Z",
  "correlation_id": "…",
  "data": { "person_id": "…", "change": "inquiry_received" }
}
```

`data.change` ∈ `inquiry_received | assignment_changed | stage_changed |
contact_attempted`. Second type:

```json
{ "v": 1, "type": "intake.unresolved_changed", "organization_id": "…", "occurred_at": "…",
  "correlation_id": "…", "data": { "raw_payload_id": "…" } }
```

No `today.changed` type: Today is computed, so a Today change *is* a
Person change. Additive fields are allowed under `v: 1`; a breaking change
bumps `v`. Events are ids-only by construction (the D-015 §3 discipline
applied to the wire), which is what makes the token-TTL exposure window in
§7 harmless.

**Client invalidation mapping (exact query keys, `web/src/api/queries.ts`
factory):**

- `person.changed` → `['org', orgId, 'person', person_id]`,
  `['org', orgId, 'people']`, `['org', orgId, 'today']`; when `change ===
  'inquiry_received'` also `['org', orgId, 'unresolved']` (a re-POST that
  resolves a `pending` row removes it from the queue but publishes only
  `person.changed`).
- `intake.unresolved_changed` → `['org', orgId, 'unresolved']`.
- `connected` after any prior disconnect → `['org', orgId]` (everything;
  D-011 recovery).
- Unknown `type` → ignored. `organization_id !== me.organization.id` →
  dropped with `console.warn` (defense in depth; should be impossible).

Known gap, accepted: an intake whose second phase fails or hits
`intake_busy` leaves a new `pending` row visible in the queue and
publishes nothing; the queue catches up on focus/refetch.

**Centrifugo config** (`infra/development/centrifugo/config.json`; keys
verified against the v6.9.2 image's own defaults — the existing top-level
`log_level` is not a v6 key and becomes `log.level` in the same edit):

```json
{
  "log": { "level": "info" },
  "health": { "enabled": true },
  "client": {
    "allowed_origins": ["http://127.0.0.1:5173", "http://localhost:5173", "https://app.tarams.org"]
  },
  "channel": {
    "namespaces": [ { "name": "org" } ]
  }
}
```

`client.token.hmac_secret_key` and `http_api.key` keep coming from the
environment via compose. The `org` namespace keeps the defaults:
`allow_subscribe_for_client: false` (verified default), no history, no
presence. `client.allowed_origins` is required in **both** modes:
Centrifugo sees `Origin: https://app.tarams.org` through cloudflared and
`http://127.0.0.1:5173` through Vite (`changeOrigin` rewrites `Host`, not
`Origin`); it must list the actual dev origin(s) if `CRM_WEB_BIND_ADDR` /
`CRM_WEB_PORT` differ from the defaults.

## 7. Authorization and tenant isolation

- Organization enters Today, the contact command, and the token only from
  `AuthContext` → `PersonVisibilityScope` / `CommandContext`. A
  client-supplied `user_id`/`organization_id` in query, header, or body
  is ignored (probed in tests, Slice 001 style).
- A client can never receive another Organization's events, by four
  independent layers: (1) the `org` namespace denies client-initiated
  subscribe, so the only subscriptions are the token's `channels`; (2)
  the token is signed server-side with a secret the client never holds
  and carries exactly `AuthContext.active_organization_id`; (3) the
  client drops any event whose `organization_id` is not its own; (4)
  events carry ids only.
- `POST …/contact-attempts` is allowed for any member on any Person of
  the Organization (no-roles model, consistent with 002). Nothing here
  contradicts the `role` column D-021 adds in 004.
- **Logout vs. a live connection:** `AppShell` (mounted once for every
  non-public route) owns the connection and disconnects on unmount or
  when the viewer's `orgId` becomes empty — navigating to `/login` on
  logout covers it without coupling to the logout mutation. A client
  that does not cooperate keeps, for at most one TTL (10 min), a token
  that yields ids-only invalidation hints for an Organization it was a
  member of; every refetch 401s. No server-side disconnect in 003 (it
  would also drop that user's other valid sessions/tabs). Hook noted for
  004's revoke-membership command: `realtime::disconnect_user(user_id)`
  via Centrifugo's `disconnect` API.
- **Membership loss while connected:** bounded by the same TTL. The
  SDK's refresh calls `POST /api/realtime/token`; `AuthContext`
  re-verifies membership and returns 401 → the client throws
  `UnauthorizedError` → Centrifugo closes the connection with no
  reconnect. Expired tokens are refused at connect (tested). When the
  per-hostname Cloudflare Access session for `api.*` expires, the SDK
  loops in "reconnecting…" until the user re-visits `api.*` top-level
  (the gotcha PROJECT_STATE records for `fetch`) — a README
  troubleshooting line, not UI copy (§14a).

## 8. Observability

- Publisher: `#[instrument(skip_all, fields(channel, event_type,
  organization_id, correlation_id, outcome))]`, `outcome` ∈ `published |
  timeout | transport_error | api_error:<code>`; `warn!` on failure,
  `debug!` on success. The spawned task is `.instrument(Span::current())`
  so `correlation_id` reaches the `warn!`. Never the API key, never the
  payload beyond type and ids.
- Token endpoint: span with `actor_id` / `organization_id`; the token is
  never logged.
- `log_contact_attempt` follows the existing command `outcome` logging
  convention.
- `/internal/ready` is unchanged — Centrifugo is not a readiness
  dependency (delivery, not truth).
- Web: `console.debug` on connection transitions in dev builds only,
  never including the token.

## 9. Failure behavior

- **Centrifugo down at publish:** the command result is fixed before
  publish; publish runs after commit, off the request path
  (`tokio::spawn`; 500 ms connect, 2 s total budget); failure is logged;
  HTTP still 2xx; no compensation. PostgreSQL stays authoritative by
  construction. Delivery is **at-most-once** to connected clients;
  staleness is bounded by the backstops below; correctness never depends
  on delivery.
- **Graceful shutdown:** `axum::serve(...).with_graceful_shutdown` drains
  connections, not detached tasks — an in-flight publish at shutdown is
  cancelled. Accepted (at-most-once, D-011).
- **Down at token mint:** unaffected — minting is local.
- **Down at connect / mid-session:** the SDK reconnects with exponential
  backoff; the UI shows "reconnecting"; data keeps flowing over HTTP;
  Today refetches every 60 s (`refetchInterval`, paused in background
  tabs) and on window focus (TanStack defaults).
- **Missed events:** every `connected` after a disconnect invalidates
  `['org', orgId]`; plus interval/focus. No Centrifugo history (keeps it
  stateless and PII-free in memory).
- **Duplicate events / self-echo:** invalidation is idempotent; TanStack
  dedupes in-flight fetches; the composable coalesces invalidations
  within 250 ms so a burst of leads is one refetch, not twenty.
- **Ordering:** irrelevant — events are hints; every refetch reads
  authoritative state.
- **`getToken` errors:** an `ApiError` with `status === 401` → throw the
  SDK's `UnauthorizedError` (terminal, no reconnect) **and** invalidate
  the `me` query: its refetch turns the dead session into a *query*
  failure, which `query-client.ts`'s `QueryCache.onError` 401 handler
  routes to `/login` (the router guard only runs on navigation, and a
  plain `apiFetch` failure never reaches the handler on its own); any
  other error → rethrow, so the SDK retries with backoff (a transient
  503 must not kill realtime permanently).
- **Secret mismatch / bad config:** connect fails with a terminal
  `invalid token` → indicator "Realtime unavailable — data may be
  delayed"; the Centrifugo-backed test catches it. The API refuses to
  start on a missing/short secret or an empty API key (§11).
- **Foreign-Organization event (should be impossible):** dropped
  client-side, warned.
- **Why no outbox:** an outbox gives at-least-once to Centrifugo, but a
  client that was disconnected still needs the refetch, so it only
  shortens the staleness window for "API crashed between commit and
  publish" and brief Centrifugo outages — at the cost of a table, grants,
  a publisher loop, and multi-pod ordering. Revisit when an event has
  semantics beyond invalidation (calls) or at multi-pod. The `Publisher`
  enum keeps the commands untouched if the transport changes.

## 10. Frontend (Lane B, `web/**`; D-017; `UI_STYLE.md` binds)

```text
package.json             + centrifuge (pin); devDeps + vitest; "test": "vitest run"
vite.config.ts           + proxy '/connection': { target: CRM_WEB_REALTIME_PROXY_TARGET ?? 'http://127.0.0.1:8000',
                                                  ws: true, changeOrigin: true }
api/types.ts             + TodayResponse, TodayItem, TodayReason (discriminated on code), RealtimeTokenResponse,
                           LogContactRequest/Response, ContactChannel, ContactOutcome,
                           HistoryEntry + contact_attempted member
api/queries.ts           + queryKeys.today(orgId) = ['org', orgId, 'today']; useToday(orgId) { refetchInterval: 60_000 };
                           useLogContactMutation(orgId) (setQueryData person; invalidate ['org', orgId]);
                           fetchRealtimeToken()
realtime/client.ts       resolveRealtimeUrl(location: Location): app.* → wss://api.<rest>/connection/websocket;
                           else ws(s)://<host>/connection/websocket   (parameterized so it is unit-testable)
realtime/events.ts       RealtimeEvent types; invalidationsFor(event, orgId): QueryKey[]; reconnectInvalidations(orgId)
realtime/useRealtime.ts  composable: status ref ('idle' | 'connecting' | 'connected' | 'reconnecting' | 'unavailable');
                           mapping from SDK state: `connecting` before the first successful connect → 'connecting';
                           `connecting` after any `connected` → 'reconnecting'; `connected` → 'connected';
                           `disconnected` with a terminal code (`unauthorized`, Centrifugo 3500–3999 or 4500–4999,
                           corrected 2026-08-22 after adversarial testing found the narrower original range left
                           status stuck at 'connected' with no indicator) → 'unavailable';
                           our own disconnect() → 'idle'. Client factory injected (fake client in tests, no SDK).
                           connect after `me` resolves (watch orgId); getToken per §9; on 'publication' → invalidate
                           mapped keys (250 ms coalesce); on 'connected' after a disconnect → invalidate ['org', orgId];
                           disconnect on unmount and when orgId becomes empty; org change → new client
realtime/events.test.ts, realtime/client.test.ts, realtime/useRealtime.test.ts   Vitest, service-free, fake emitter
views/TodayView.vue      page header "Today" + subtitle "Updated <relative TanStack dataUpdatedAt — not the server
                           generated_at, which reads "in 2 seconds" under clock skew> · N need a response";
                           one table card: Name (over primary contact), Reasons (neutral badges, rendered in wire
                           order — §3's fixed order), Priority (text weight, not color), Waiting (relative; absolute
                           in tooltip), Recommended (Call + phone / Email + address), per-row **secondary** "Log
                           contact" button (@click.stop inside the row link; the dialog's is the view's one primary,
                           UI_STYLE §5); empty state "Nothing needs your attention."
components/LogContactDialog.vue  PrimeVue Dialog (unstyled pt; UI_STYLE §2 floating surface): Channel + Outcome
                           Selects with per-channel default outcome; primary "Log contact"; button disabled while pending
views/PersonDetailView.vue       + "Log contact" secondary button in the header card; history icon/summary for
                           contact_attempted
components/AppShell.vue  nav Work: Today (Lucide `Sun`), People; footer pill rendered only for 'reconnecting'
                           ("Realtime: reconnecting…") and 'unavailable' ("Realtime unavailable — data may be
                           delayed") — never for 'connecting', or every page load flashes it; muted; installs
                           useRealtime once
router.ts                + /today; '/' and the catch-all redirect to /today; the signed-in-visitor-to-/login
                           redirect (router.ts) and LoginView.vue's post-login default both move from /people
                           to /today (three hard-coded /people landings exist today)
```

ESLint and `tsconfig` cover `*.test.ts`. Vitest runs in `./scripts/check`
between lint/typecheck and build. The raw-`fetch` rule of SLICE_002 §10
stands: every HTTP call goes through `apiFetch`.

## 11. Development environment, transport, and configuration

**Note (2026-08-22, post-implementation): D-024 removed Cloudflare
Access from the dev tunnel.** The Access-specific details below (the
Access application, its session behavior, the WebSocket riding "the
same tunnel and Access session") describe the environment as it stood
when this spec was approved and implemented; they no longer apply. The
tunnel, TLS, and path-routed WebSocket ingress are otherwise unchanged.

**Config (Lane A; `.env.example` and README updated):**

| Variable | Required | Meaning |
|---|---|---|
| `CENTRIFUGO_HTTP_API_KEY` | now required by the API | Publish credential; the same value compose passes to Centrifugo |
| `CENTRIFUGO_TOKEN_HMAC_SECRET` | now required by the API; **≥ 32 bytes** | Token signing secret; the same value compose passes to Centrifugo. README: regenerate with `openssl rand -hex 32` and restart Centrifugo if the existing value is shorter |
| `CRM_CENTRIFUGO_API_URL` | optional, default `http://127.0.0.1:8000/api` | `http://` only, no trailing slash (validated like `CRM_CORS_ALLOWED_ORIGIN`) |
| `CRM_REALTIME_TOKEN_TTL_SECONDS` | optional, default 600, bounds 60–3600 | Connection-token lifetime |
| `CRM_WEB_REALTIME_PROXY_TARGET` | optional, default `http://127.0.0.1:8000` | Vite WebSocket proxy target |
| `CRM_DEMO_API_URL` | optional, default `http://127.0.0.1:3000` | `scripts/demo-leads` target |

`Debug` on `Config` redacts both secrets. Startup fails with a specific
message on a missing/short secret or an empty API key.

**Transport through the tunnel (safe default; overridable):** one
path-routed rule in the committed `infra/development/cloudflared/config.yml`,
placed before the API rule:

```yaml
  - hostname: api.tarams.org
    path: ^/connection/websocket$
    service: http://localhost:8000
  - hostname: api.tarams.org
    service: http://localhost:3000
```

Rationale: `app.tarams.org` → `api.tarams.org` is same-site (eTLD+1
`tarams.org`), which is why the existing `credentials: 'include'` fetch
already works through Cloudflare Access; a `wss://api.tarams.org` upgrade
from the same page rides the same `CF_Authorization` cookie, and Access
validates upgrade requests like any other. Centrifugo stays behind
Access, its HTTP API is never routed (the anchored regex routes only the
WebSocket path; everything else on `api.*` still reaches the Rust API,
which has no `/api/publish`), and no new hostname or Access application
is needed. D-018 expresses the same thing in production as an
`HTTPRoute` path match, so nothing is throwaway. Loopback uses the Vite
`ws: true` proxy. **Fallback** if Access is shown to break the upgrade: a
separate realtime hostname that bypasses Access, authenticated by
Centrifugo's own token (D-016 §4's "bypasses Access, verified by the
application") — that is a security-boundary change and needs the user
before it is adopted.

**`scripts/demo-leads` (Lane A):** bash + `curl`. Logs in as
`alice@acme.test` with the password read from `.env`
(`CRM_DEV_SEED_PASSWORD`) and sent via `--data-binary @-` from stdin —
never on argv (visible in `ps`), never with `-v` or `set -x`; cookie jar
from `mktemp` with `trap` cleanup; `GET /api/me` after login and a loud
failure on anything but 200 (`.env` is in tunnel mode —
`CRM_SESSION_COOKIE_SECURE=true` — and curl 8.7 replays a `Secure` cookie
to `http://127.0.0.1:<port>` but silently drops it for
`http://localhost:<port>`, so `localhost` is not interchangeable with the
default); fetches Carol's id from `GET /api/organization/members` for the
one Carol-assigned lead; random per-run `submission_id` in each payload
so a rerun produces repeat inquiries rather than duplicates. Backdating is
impossible by design (`received_at` is server time; no §5 change) — the
24 h tier is proved by fixture-backed tests, not the demo. The seed binary
is untouched (D-021 rewrites it in 004).

## 12. Explicit exclusions

- Unresolved-queue resolve/discard (SLICE_002 §16 said "if small"; it is
  not: discard needs the application's first DELETE grant or a new
  `resolution` value plus a recorded deletion fact under an erasure
  runbook that does not exist yet; resolve needs decrypting payloads to
  the screen). Next intake slice or 004.
- Today completion / snooze / dismiss (thesis §8: specified separately;
  nothing here pre-decides it).
- Stage-aware Today exclusions (needs a stage semantic key; D-019
  follow-up).
- An "Unassigned" group on Today (additive when roles arrive, D-021).
- Toasts, sounds, badges, browser or mobile push (which events may
  interrupt is a later product decision, thesis §5.4/§12.7; a "New lead:
  Sarah" toast would need PII on the wire or a refetch-then-toast
  pipeline).
- Per-user channels; transactional outbox; Centrifugo history /
  recovery / presence; server-side forced disconnect on logout (hook
  noted for 004); idempotency key for contact attempts; any `GET
  /api/people` shape change; seed changes; Operator.

## 13. Checks and tests

### Acceptance criteria

1. Fresh database: the migration applies; `crm_app` has exactly SELECT +
   INSERT on `contact_attempted`; UPDATE/DELETE fail as `crm_app` (grant)
   and as `crm_migrator` (trigger, against an existing row — SLICE_002
   §13 criterion 2's non-vacuous pattern); `TRUNCATE` fails.
2. `LogContactAttempt` writes exactly one fact with the full envelope
   (`actor_kind = 'user'`, `origin = 'web_session'`, fresh
   `correlation_id`); invalid channel/outcome or non-JSON → 400;
   other-Organization Person → 404 byte-identical to nonexistent; a
   migrator fixture inserts a `stage_changed` and a `contact_attempted`
   for the same Person with identical `occurred_at` **in one transaction**
   (so `recorded_at` is identical too — rows created through HTTP differ
   in `occurred_at` and would never exercise `kind_rank`) → the detail
   history sorts the contact attempt last and its `detail` is exactly
   `{"channel", "outcome"}`.
3. Today, over HTTP as `crm_app`:
   - with one unanswered lead assigned to Alice, one to Carol (same
     Organization), and one to Bob (the other Organization): Alice's
     Today contains exactly her item with `new_inquiry` +
     `no_contact_attempt`, `high`, and the correct `recommended_action`;
     Carol's exactly hers; Bob's exactly his — none of the others' (an
     empty Today would pass vacuously); a user with zero assigned People
     gets an empty list;
   - an unassigned Person with an unanswered Inquiry appears on nobody's
     Today;
   - Alice logs a contact → the row leaves her Today; **Carol (a
     non-assignee) logs a contact on Alice's Person → the row leaves
     Alice's Today** (§3 disclosure pinned);
   - a repeat Inquiry after an earlier attempt re-adds the row with
     `repeat_inquiry { inquiry_count: 2 }` **and** `last_contact_attempt`
     set to the earlier attempt, and `waiting_since` = the repeat
     Inquiry's `received_at` (the earlier one was answered);
   - with no attempt, `waiting_since` = the earliest Inquiry's
     `received_at`, not the latest;
   - a fixture-backdated Inquiry (> 24 h) sorts after fresh ones with
     `normal` priority and no `new_inquiry` reason;
   - **201+ stale candidates plus one fresh Inquiry → the fresh Person is
     in the response and `truncated` is true** (tier-before-LIMIT);
   - two candidates with identical `waiting_since` order by `person.id`;
     two Inquiries with identical `received_at` pick the greater `id` as
     latest;
   - client-supplied `user_id` / `organization_id` in query, header, or
     body are ignored.
4. Token: `sub` = the actor, `channels` = exactly `org:<active
   Organization>` (also for a user with two memberships), `exp − iat` =
   TTL, signature verifies with the configured secret, the header is
   exactly `{"alg":"HS256","typ":"JWT"}` and the claim set is exactly
   `sub, iat, exp, channels` (emitted in that order) with those values;
   no session → 401; revoked membership / expired session → 401.
5. Publisher (`Recording`, DB-backed): after each command the exact §6
   envelope as `(channel, serde_json::Value)` — intake new Person,
   intake matched Person (**exactly one** event although it may write two
   facts), intake unresolved (`occurred_at` = `raw_payload.received_at`),
   assignment changed, stage changed, contact attempted; **nothing** on
   duplicate intake, unchanged assign/stage, `IntakeBusy`, invalid
   assignee (422), invalid stage (422), or other-Organization Person
   (404); `occurred_at` equals the fact's `occurred_at`.
6. Publish failure: `publish_now` against a closed loopback port, and
   against a listener that accepts and never responds, returns a failure
   outcome within the budget and the command that triggered it still
   returned `Ok` with its rows committed.
7. Centrifugo-backed (in `check-db`), all in **one** test so the negative
   assertion is meaningful: a WebSocket client holding Organization A's
   token receives A's event (exact §6 JSON) within 5 s; B's connection
   receives nothing in the following 500 ms (pings `{}` filtered); B's
   client-side `subscribe("org:<A>")` → permission denied (error 103 —
   a 102 "unknown channel" means the `org` namespace is missing: restart
   `dev-services`); an expired token and a token signed with a different
   secret are refused at connect; **recovery**: after A receives its
   event, A's socket is closed, a second command runs, and A reconnects
   with a fresh token → no push for the missed event arrives within
   500 ms (no replay — pins the stateless, no-history posture, which
   would otherwise change silently if someone enabled `history_size`)
   while `GET /api/today` already reflects the change; the test **fails**
   with a clear "Centrifugo not reachable at CRM_CENTRIFUGO_API_URL"
   message when the container is down — never skips.
8. Web (Vitest): `invalidationsFor` for every event type, the
   `inquiry_received` → `unresolved` extra key, foreign-Organization
   events dropped, unknown types ignored; `resolveRealtimeUrl` for
   `app.*`, loopback `http`, and `https`; the composable's state machine
   with an injected fake client: connect after `me`, the §10 SDK-state →
   status mapping (`connecting` before and after a first `connected`
   yields `'connecting'` then `'reconnecting'`; terminal codes →
   `'unavailable'`), `publication` → coalesced invalidation,
   `connected`-after-disconnect → invalidate all, `getToken` 401 →
   `UnauthorizedError` + `me` invalidated, `getToken` 503 → rethrown,
   disconnect on unmount / empty orgId.
9. API refuses to start on a missing or short `CENTRIFUGO_TOKEN_HMAC_SECRET`,
   an empty `CENTRIFUGO_HTTP_API_KEY`, a non-`http://` or trailing-slash
   `CRM_CENTRIFUGO_API_URL`, or a TTL outside 60–3600; `Debug` redacts both
   secrets.
10. `./scripts/check` green with no services (now including `pnpm test`);
    `./scripts/check-db` green (sqlx `prepare --check`, DB-backed,
    Centrifugo-backed).
11. Manual: the §1 walkthrough through the tunnel in two browsers,
    including the Centrifugo stop/start recovery and the logout WebSocket
    close.

### Required tests — service-free (main gate, `./scripts/check`)

Config parsing for every new variable and bound (criterion 9); the JWT
signer (header, payload, signature verified with `hmac`, exact claims,
TTL); `rank()` tables (reasons, priority, recommended action, order
preservation); `channel_for`; serialization of every `RealtimeEvent`
variant to the exact §6 JSON; `publish_now` failure paths (criterion 6's
transport half); 401 without a cookie on the three new endpoints; 400 on
a non-UUID path; 503 per new endpoint against a closed DB port. Existing
suites: every `test_config` helper (`session.rs`, `health.rs`,
`people.rs`, `intake.rs`, `stages.rs`, `db_identity.rs`, `common/mod.rs`)
gains the two required values — known churn, as in SLICE_002 §14a.

Not service-free, deliberately: 400 for a bad `channel`/`outcome`/non-JSON
body (`AuthContext` runs before the body extractor — `tests/people.rs`
documents this for 002), and "each command publishes the expected
envelope" (the commands need a database). Both live in `check-db`.

### Required tests — DB-backed (`check-db`)

Criteria 1–6 over HTTP with `#[sqlx::test]` and the `crm_app` router
(criterion 6's command half uses a real `Centrifugo` transport pointed at
a closed loopback port).
Backdated Inquiries and contact attempts are migrator-role fixtures
(fixtures are test setup, not application data; D-021 governs dev/prod
data). `db_schema.rs`'s grant loop, `insert_one_row_per_fact_table`, and
both trigger tests enumerate tables by name and gain `contact_attempted`.

### Required tests — Centrifugo-backed (`check-db`)

Criterion 7 with a `tokio-tungstenite` dev-dependency (default features,
no TLS) speaking Centrifugo's JSON client protocol (`{"id":1,"connect":
{"token":…}}` → reply carrying `subs`; `{"push":{"channel","pub":
{"data"}}}`; answer empty `{}` pings with `{}`). The test reads
`CENTRIFUGO_TOKEN_HMAC_SECRET` / `CENTRIFUGO_HTTP_API_KEY` from the
environment because they must match the running container — recorded as
the second exception to "tests never read ambient environment", beside
`CRM_DB_APP_PASSWORD` in `tests/common/mod.rs`; `check-db` already
sources `.env` with `set -a`. Random Organization ids keep the tests
isolated from dev use (Centrifugo is stateless). `#[sqlx::test]` runs on
a current-thread runtime, so spawned publishes progress only at await
points: these tests await the WebSocket receipt before asserting and
never assert that a spawned publish "happened" without awaiting.

### Browser

Vitest for the pure and composable logic (criterion 8). The live two-tab
walkthrough is the browser-level reconnect/recovery test (ad hoc
Playwright as in 002; not committed as a runner this slice).

## 14. Safe defaults adopted (overridable at approval)

- Today = the viewer's assigned People only; no Unassigned group; no
  other member's items (D-005 fixes ownership by assignment; a pond and
  an admin view are additive when roles arrive).
- Today is the landing route after login.
- Unresolved resolve/discard out (§12).
- `waiting_since` = earliest unanswered Inquiry (§3).
- Outcome vocabulary includes `sent`; no channel/outcome cross-validation.
- `occurred_at` for contact attempts is server-only this slice.
- Contact attempts may be logged by any member on any Person.
- Token TTL 600 s; channel `org:<uuid>`; envelope `v: 1` with the two
  event types; `POST` mint endpoint; no per-user channel; no server-side
  disconnect on logout; channel set fixed per connection.
- 24 h fresh window; reason set and ordering of §3; cap 200.
- Best-effort publish after commit, 500 ms / 2 s budget, cancelled at
  shutdown; no outbox; Centrifugo not a readiness dependency.
- Client: invalidate-all on reconnect; 250 ms coalescing; Today
  `refetchInterval` 60 s; indicator only when not connected.
- `intake.unresolved_changed` kept (≈15 lines; completes the queue's
  invalidation story); Person-detail "Log contact" button kept (the
  Person page is where an agent lands after a call).
- `CRM_REALTIME_TOKEN_TTL_SECONDS` as config rather than a constant.
- Hand-rolled HS256 (sign only, never verify) on the existing `hmac` /
  `sha2` / `base64` crates — no `jsonwebtoken` tree for three base64url
  segments.
- `reqwest = { version = "0.12", default-features = false, features =
  ["json"] }` (no `native-tls`/OpenSSL in a production image for a
  loopback `http://` call; add `rustls-tls` only if an `https://`
  Centrifugo is ever needed); `tokio-tungstenite` dev-only; `centrifuge`
  npm client pinned; Vitest.
- `scripts/demo-leads` in bash; `CRM_DEMO_API_URL` default
  `http://127.0.0.1:3000`.
- `kind_rank` 4 for `contact_attempted`.
- Commands take a `publisher: &Publisher` parameter (vs. a deps struct —
  implementation detail).
- Transport: path-routed WebSocket under `api.tarams.org` behind Access
  (§11); the Access-bypassing fallback is **not** adopted without the
  user.

## 14a. Implementation notes (from independent review)

- `Publisher::Recording` captures `(channel, serde_json::Value)` exactly
  as sent on the wire, not a Rust struct, so the test pins the contract
  Lane B consumes.
- `rank()` never re-sorts and never re-evaluates the 24 h window; the
  SQL `ORDER BY` of §3 (on the SQL-computed `fresh` column) is the single
  ordering authority, and the service-free `rank()` tests feed `fresh`
  directly. Tie-breaks (`received_at DESC, id DESC` for the
  latest Inquiry; `occurred_at DESC, id DESC` for the last attempt) are
  contract, not choice.
- Spawn the publish task `.instrument(Span::current())`.
- The `getToken` mapping of §9 is the one place a transient error could
  permanently kill realtime or a dead session could reconnect forever —
  test both directions (criterion 8).
- Centrifugo v6.9.2 keys used here were verified against the image's
  defaults: `client.allowed_origins`, `client.token.hmac_secret_key`,
  `channel.namespaces`, `channel.without_namespace.allow_subscribe_for_client`
  (default `false`), `http_api.key`, `http_api.handler_prefix` (`/api`),
  `websocket.handler_prefix` (`/connection/websocket`), `log.level`.
- README troubleshooting lines: Docker Desktop's VM clock can drift
  after sleep, making Centrifugo reject fresh tokens as expired; an
  expired per-hostname Cloudflare Access session for `api.*` leaves the
  client in "reconnecting…" until `api.*` is re-visited top-level.
- `Publisher` has no `Disabled`/CLI variant; Slice 004's CLI will add one.
- Slice 004's revoke-membership command should call
  `realtime::disconnect_user` (Centrifugo `disconnect` API) — noted, not
  built.

## 15. Lane ownership and sequencing (SLICE_002 §15 model)

Two worktrees after approval; contracts frozen by §5/§6 of this approved
document; any change goes through AGENTS.md §11.

- **Lane A — backend** (owns `backend/**`, `scripts/**`, `infra/**`,
  `.env.example`, the README Development section, and **the
  migration**): (1) config + JWT signer + publisher + `POST
  /api/realtime/token` + Centrifugo `config.json` + cloudflared path rule
  — first, since Lane B integrates against the token endpoint and the
  running container; (2) migration + `contact_attempted` fact + command +
  history kind + `.sqlx`; (3) Today query / `rank` / endpoint; (4) wire
  the publisher into the three existing commands; (5) service-free, DB,
  and Centrifugo tests; (6) `scripts/demo-leads`; (7) `check` / `check-db`
  updates and README.
- **Lane B — frontend** (owns `web/**`): (1) `centrifuge` + Vitest + Vite
  proxy + `realtime/*` with tests — no backend dependency beyond the
  frozen token contract; (2) Today view, dialog, Person-detail additions,
  router/shell — integrates against Lane A's branch once A(1) lands;
  merges after Lane A.
- **Coordinator** owns `docs/**` and `PROJECT_STATE.md`.

No file overlap. Effort: Lane A 3.5–4.5 focused days, Lane B 2–2.5,
coordinator/integration/review 1; ≈ 5 days wall-clock in parallel.

Risk concentrates in: the Cloudflare Access + WebSocket upgrade through
the path-routed ingress (unverified until tried; fallback in §11); the
`centrifuge` SDK's `UnauthorizedError`/refresh semantics on the pinned
version; realtime test flakiness (generous waits, reachability
precheck); the `reqwest` dependency tree; the `CENTRIFUGO_TOKEN_HMAC_SECRET`
in the user's `.env` possibly being shorter than 32 bytes.

## 16. Next

Slice 004 is administration per D-021 / O-007 (platform admin,
invitations, roles, seeding through the application path). Operator
retrieval is Slice 005 — its first tools are `today::query` with the §3
reason codes and the Person detail read model. Calling is Slice 006 and
writes `contact_attempted` automatically.
