# Slice 006 — Calling (outbound, browser, human-initiated)

Status: APPROVED (user, 2026-08-22; planner pass, then independent
review — no blocking findings; 8 safe defaults, 9 implementation notes,
3 cuts applied; telephony host on direct DNS + Caddy TLS, no tunnel).
Builds on: Slice 005 (`main` at `c337093`).
Targets: D-001 (self-hosted LiveKit + Telnyx SIP), D-002 (no new
service; the provider is a trait inside `crm-api`), D-005 (Organization
visibility), D-007/D-015 (live state vs. immutable facts), D-008 (one
typed command layer), D-013 (secrets in `.env`), D-016 §5 / AGENTS §7
(media never through cloudflared), D-021 (Slice 006 is calling; no
direct DB writes), D-022 (a contact attempt is a recorded fact and the
unit of response for Today), D-023 (ids-only realtime, invalidate by
refetch), D-030 (scope and dev telephony), D-031 (automatic attempt at
answer/failure time), AGENTS §4.6 ("completed calls" are facts), thesis
§13 proof flow: `… → agent says "Call her" → LiveKit and Telnyx place
the PSTN call → the call is recorded in the timeline → Today advances`.
This slice proves the last three arrows from a button; 006b (D-030)
connects them to the Operator.

Scope decisions (user-accepted 2026-08-22, not re-litigated here):
(1) outbound browser calling from the Person page only; (2) Operator
calling is 006b after the 006a `crm-app` extraction; (3) dev telephony
on a public Linux host with one Telnyx number configured as a LiveKit
outbound trunk; (4) the automatic contact attempt is written at
answer/failure time, voicemail reads `reached` (D-031).

The narrowest cut that proves the chain with real tests:

1. a `call` aggregate (live state) and a `call_completed` fact (the
   next typed fact table, PII-free);
2. a `TelephonyProvider` trait with a LiveKit implementation and a
   scripted test implementation; the browser talks to LiveKit directly
   with a one-room join token minted by the API;
3. four routes (`start`, `dial`, `hangup`, `get`) plus the LiveKit
   webhook, a dial task, and a reconciliation sweep;
4. the automatic `contact_attempted` (D-031) → Today advances →
   `person.changed` and a new `call.changed` realtime event;
5. web: a **Call** button and docked call panel on `/people/:id`;
6. tests: service-free, DB-backed with the scripted provider, an
   optional LiveKit-backed test, Vitest with a fake LiveKit client; one
   live PSTN walkthrough.

## 1. User-visible outcome

On the D-016 Mac, through the tunnel, as `alice`, with the telephony host
up (§11):

1. Open a Today Person whose recommended action is `call`; land on
   `/people/:id`. The header shows a primary **Call** button (disabled
   with "No phone number" when the Person has none; a number picker only
   when there are several).
2. Click **Call** → the browser asks for the microphone → a docked call
   panel shows "Connecting…", then "Ringing…", then "Connected 00:12"
   with **Hang up** and mute.
3. The user's own phone rings from the deployment's caller number;
   answer it; two-way audio works.
4. The moment the call is answered, History on the Person gains
   "Contact attempted — call, reached", and the Person leaves Alice's
   Today in every open tab within a second. On hangup (either side)
   History gains "Call — reached, 1 min 12 s".
5. Decline or let it ring out → the panel shows "No answer"; History
   gains "Contact attempted — call, no answer" and "Call — no answer";
   the Person leaves Today (D-022/D-031).
6. As `bob` (other Organization): the call does not exist (404). As
   `carol` (same Organization, not the caller): `GET /api/calls/{id}`
   is 200 but `hangup` is 403.
7. Stop LiveKit → **Call** answers 503 `telephony_unavailable` with a
   clear message; a call left dangling is finalised by the sweep (§9).
   With `LIVEKIT_API_KEY` unset the API starts, calling is disabled
   (503 `telephony_disabled`), everything else is unaffected.

Nothing here records audio (O-002). Nothing is autonomous (O-003).

## 2. Domain and schema

### Concepts

- **Call** — one outbound attempt from one member to one of a Person's
  phone numbers: `placing → ringing → answered → ended`, or
  `placing|ringing → failed`. Live aggregate state (D-007/D-015): it has
  a mutable lifecycle, so it is a row with status updates, not an
  append-only log. Identified by `call.id`; the LiveKit room is
  `call:<id>`; one `correlation_id` for the whole lifecycle.
- **Completed call** — the immutable, PII-free fact written once on
  every terminal transition (AGENTS §4.6); the next typed fact table.
  Standard fact envelope.
- **Automatic contact attempt** — the existing `contact_attempted` fact,
  written per D-031 with `causation_id = call.id`.
- **Join grant** — a LiveKit access token scoped to exactly one room,
  returned once to the caller, never stored or logged.
- **Telephony provider** — the `TelephonyProvider` trait (§3): create
  room, check presence, dial, hang up, room exists. LiveKit in
  production/dev; scripted in tests. Telnyx is not a provider the
  application talks to: it is a LiveKit SIP outbound trunk, configured
  once (§11).

### PII rule

No table stores a phone number for a call. `call` references
`contact_method_id` (bare UUID; erasure orphans it). The dial task reads
the number from `contact_method` at dial time and never logs it. Spans
and ledger-style rows carry ids, statuses, timestamps, and durations
only. The join token and `LIVEKIT_API_SECRET` are never logged.

### Migration (one file, Lane A): `20260825000001_calls.sql`

```sql
CREATE TABLE call (
    id                 UUID PRIMARY KEY,                 -- generated in Rust; room = 'call:' || id
    organization_id    UUID NOT NULL REFERENCES organization(id),
    person_id          UUID NOT NULL,                    -- bare (SLICE_002 §2 rule)
    contact_method_id  UUID NOT NULL,                    -- bare
    caller_user_id     UUID NOT NULL REFERENCES app_user(id),
    origin             TEXT NOT NULL,                    -- 'web_session' (006); 'operator' in 006b
    correlation_id     UUID NOT NULL,
    status             TEXT NOT NULL CHECK (status IN ('placing','ringing','answered','ended','failed')),
    failure_reason     TEXT CHECK (failure_reason IN ('no_answer','busy','declined','cancelled',
                         'ring_timeout','agent_not_joined','provider_error','expired')),
    end_reason         TEXT CHECK (end_reason IN ('agent_hangup','agent_disconnected','remote_hangup','max_duration','reconciled')),
    provider           TEXT NOT NULL,                    -- 'livekit' | 'scripted'
    provider_room      TEXT NOT NULL,
    provider_call_ref  TEXT,                             -- LiveKit sip_call_id; not PII
    placed_at          TIMESTAMPTZ NOT NULL,
    dial_requested_at  TIMESTAMPTZ,                     -- set once by POST /dial; second dial → 409
    ringing_at         TIMESTAMPTZ,
    answered_at        TIMESTAMPTZ,
    ended_at           TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'failed') = (failure_reason IS NOT NULL)),
    CHECK ((status = 'ended') = (end_reason IS NOT NULL)),
    CHECK (status NOT IN ('ended','failed') OR ended_at IS NOT NULL)
);
CREATE UNIQUE INDEX call_one_active_per_caller
    ON call (organization_id, caller_user_id) WHERE status IN ('placing','ringing','answered');
CREATE INDEX call_org_person_placed_idx ON call (organization_id, person_id, placed_at DESC);
GRANT SELECT, INSERT ON call TO crm_app;
GRANT UPDATE (status, failure_reason, end_reason, provider_call_ref, dial_requested_at,
              ringing_at, answered_at, ended_at, updated_at) ON call TO crm_app;
-- The partial unique index doubles as the sweep's index over active calls.

CREATE TABLE call_completed (
    -- envelope columns, CHECKs, FKs verbatim from contact_attempted (SLICE_002 §2)
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,                    -- = ended_at
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,                        -- = call.correlation_id
    causation_id UUID,
    corrects_id UUID REFERENCES call_completed (id),
    call_id UUID NOT NULL,
    person_id UUID NOT NULL,
    contact_method_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('reached','no_answer','busy','declined','cancelled',
                                             'ring_timeout','agent_not_joined','provider_error','expired')),
    answered_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ NOT NULL,
    talk_seconds INTEGER,
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))
);
CREATE INDEX call_completed_org_person_occurred_idx ON call_completed (organization_id, person_id, occurred_at);
CREATE INDEX call_completed_org_correlation_idx ON call_completed (organization_id, correlation_id);
GRANT SELECT, INSERT ON call_completed TO crm_app;
-- reject_mutation row trigger (UPDATE/DELETE) and TRUNCATE statement trigger, verbatim from 20260822000001.
```

`call_completed.outcome` is `reached` for every answered call regardless
of how it ended (`end_reason` stays on `call`), and the `failure_reason`
for a failed one.

### State machine (pure Rust, `domain/telephony/transitions.rs`)

```
placing  --Dialing-->          ringing                     [dial task, after the presence check]
placing  --Cancelled|AgentLeft-->  failed{cancelled}       (no attempt)
placing  --AgentNotJoined-->   failed{agent_not_joined}    (no attempt)
placing  --ProviderError-->    failed{provider_error}      (no attempt)
placing  --Expired-->          failed{expired}             (no attempt)   [sweep]
ringing  --Answered{ref}-->    answered                    (attempt: reached)
ringing  --DialFailed(busy|declined|no_answer|ring_timeout)--> failed{…}   (attempt: no_answer)
ringing  --Cancelled|AgentLeft-->  failed{cancelled}       (attempt: no_answer — D-031: ringing had started)
ringing  --ProviderError-->    failed{provider_error}      (no attempt)
ringing  --Expired-->          failed{expired}             (no attempt)   [sweep]
placing|ringing --RemoteLeft|RoomFinished--> no-op         (the dial task owns these states)
answered --AgentHangup-->      ended{agent_hangup}
answered --AgentLeft-->        ended{agent_disconnected}   [webhook: agent:* participant_left]
answered --RemoteLeft|RoomFinished--> ended{remote_hangup}
answered --MaxDuration-->      ended{max_duration}         (only if LiveKit reports it distinguishably; else remote_hangup)
answered --Reconciled-->       ended{reconciled}           [sweep]
ended | failed: every signal is a no-op (idempotent webhooks).
```

`settle` takes `SELECT … FOR UPDATE` on the `call` row and applies the
transition to the **locked** status — that is what makes "first signal
wins, the other is a no-op" true across the dial task, commands,
webhooks, and the sweep. After settling `Answered`, the dial task makes
one `participant_present("sip:<id>")` check and settles `RemoteLeft` if
the callee is already gone (a sub-second answer-and-hangup race).

`talk_seconds = ended_at - answered_at` (whole seconds) when answered.

### D-031 mapping (the automatic contact attempt)

Written by `settle` (§3) in the same transaction as the transition:

| Transition | `contact_attempted` |
|---|---|
| `→ answered` | `call, reached` at `answered_at` |
| `ringing → failed{no_answer\|busy\|declined\|ring_timeout\|cancelled}` | `call, no_answer` at `ended_at` |
| `placing → failed{*}`, `→ failed{agent_not_joined\|provider_error\|expired}` | none |

Envelope for every call-derived fact: `actor_kind = 'user'`,
`actor_user_id = caller_user_id`, `origin = call.origin`,
`correlation_id = call.correlation_id`, `causation_id = call.id` — the
caller is the actor; the provider merely reports. `Origin` gains no
variant. `contact_attempted.causation_id` semantics are documented in
`envelope.rs`.

### Declared change to an existing contract (AGENTS §11, additive)

`GET /api/people/{id}` `history[]` gains `kind = "call_completed"`,
`kind_rank` 5, `detail: {call_id, outcome, talk_seconds, answered_at}`.
A pointer line is added to SLICE_002 §5 as SLICE_003 did.

## 3. Commands, queries, provider

In `backend/crates/crm-api` (D-002: no new crate; the trait is the
seam, as `Publisher` is for realtime):

```rust
// src/telephony/mod.rs
#[async_trait]
pub trait TelephonyProvider: Send + Sync {
    async fn create_room(&self, room: &str, max_call: Duration) -> Result<(), ProviderError>;
    async fn participant_present(&self, room: &str, identity: &str) -> Result<bool, ProviderError>;
    async fn dial(&self, req: DialRequest) -> Result<DialOutcome, ProviderError>; // Answered{call_ref} | Failed(SipFailure)
    async fn hangup(&self, room: &str) -> Result<(), ProviderError>;              // delete room; not-found is Ok
}
pub enum ProviderError { Timeout, Unavailable(String), Rejected(String) }
pub enum SipFailure { Busy, Declined, NoAnswer, RingTimeout, Other(u16) }
pub struct DialRequest { room: String, to_number: PhoneNumber /* redacting Debug */, participant_identity: String,
                         ring_timeout: Duration, max_call: Duration }
pub struct Telephony { provider: Arc<dyn TelephonyProvider>, signer: JoinTokenSigner,
                       webhook: WebhookVerifier, limits: TelephonyLimits, provider_name: &'static str }
```

- `LiveKitProvider`: Twirp JSON over HTTPS to `LIVEKIT_API_URL`
  (`RoomService/CreateRoom|ListParticipants|DeleteRoom`,
  `SIP/CreateSIPParticipant` with `wait_until_answered`,
  `ringing_timeout`, `max_call_duration`, `sip_trunk_id`); auth is an
  HS256 JWT with `video` grants — the same hand-rolled HS256 as
  `realtime/token.rs` (the `livekit-api` crate is acceptable instead if
  its dependency tree is small — implementation detail, not contract).
- `ScriptedProvider`: a queue of `DialOutcome`/errors; records every
  call; `pub` behind `test-support`, as the Operator's is.
- `JoinTokenSigner`: mints `{iss: api_key, sub: "agent:<user_id>", exp,
  video: {room: "call:<id>", roomJoin: true, canPublish: true,
  canSubscribe: true, canPublishData: false}}`. No `roomCreate`,
  `roomAdmin`, `roomList`. TTL `CRM_TELEPHONY_JOIN_TTL_SECONDS`.
  Implementation note (Lane A, 2026-08-22): the token also carries
  `nbf = now`, as LiveKit's own SDKs do; the claim set is exactly
  `{iss, sub, exp, nbf, video}`.
- `WebhookVerifier`: LiveKit's scheme — `Authorization: <JWT>` signed
  with the API secret, `iss = api_key`, `exp`, `sha256` claim = base64
  SHA-256 of the raw body; constant-time compare.

Commands (`src/domain/commands/`):

- `start_call(pool, publisher, telephony, ctx, StartCall{person_id,
  contact_method_id}) -> (CallView, JoinGrant)`: tx: `lock_person`
  (Organization) → `PersonNotFound`; load `contact_method` by `(id,
  person_id, organization_id, kind = 'phone')` → `InvalidContactMethod`
  (nonexistent, foreign, other Person's, or email — identical); insert
  `call(status = 'placing')`; commit (`23505` on the partial unique
  index → `CallInProgress`); then `provider.create_room` — on failure
  settle `failed{provider_error}` and map to `TelephonyUnavailable`.
  Mint the grant last. Publishes `call.changed`.
- `dial_call(…, call_id)`: caller only; `UPDATE call SET
  dial_requested_at = now() WHERE … AND dial_requested_at IS NULL AND
  status = 'placing'` (0 rows → `InvalidCallState`, 409); then spawns the
  dial task. Returns 202 with the call still `placing`.
- `hangup_call(…, call_id)`: caller only; idempotent. **Settle first**
  (row lock): `answered → ended{agent_hangup}`, `placing|ringing →
  failed{cancelled}`, terminal → no-op; **then** `provider.hangup`
  best-effort — including on an already-terminal call, so a client retry
  after a 503 still cleans the room up. Settling before deleting the
  room is what keeps a concurrent dial-task `ProviderError` from winning
  and changing the recorded outcome.
- `settle(tx, call, transition)` (`src/domain/telephony/settle.rs`): the
  **one** write path for every signal source (command, dial task,
  webhook, sweep): `UPDATE call`; insert `contact_attempted` per D-031;
  insert `call_completed` on a terminal transition; after commit publish
  `call.changed` and, when an attempt was written, `person.changed
  {contact_attempted}` (existing event).

Dial task (spawned from `dial_call`, `.instrument(Span::current())`,
bounded by `10 s + ring_timeout + 10 s`): wait ≤ 10 s for
`agent:<user_id>` to be present in the room, else settle
`failed{agent_not_joined}`; then settle `Dialing` (`placing → ringing`)
and call `provider.dial(...)` with `participant_identity =
"sip:<call_id>"` and the number from `contact_method.normalized_value`
(E.164-style; `value` is never used) → `Answered` → settle (then the
presence re-check above); `Failed(SipFailure)` → map (486 → busy, 603 →
declined, 480/408 → no_answer, ring timeout → ring_timeout, other →
provider_error) → settle.

Sweep (`tokio` interval, 30 s, one pod) with per-state horizons:
`placing` older than 10 s + 30 s → `failed{expired}`; `ringing` older
than `ring_timeout` + 30 s → `failed{expired}`; `answered` older than
`max_call` + 60 s → `ended{reconciled}`; each followed by best-effort
`provider.hangup`. No room-existence query. In-process, noted with the
Operator's guards for the multi-pod backlog.

Queries (`src/domain/telephony/queries.rs`): `call_by_id(conn,
organization_id, id)`, `active_call_for_user` (used for the 409 body).
`person/queries.rs` gains the `call_completed` history source. Reads
(`GET /api/calls/{id}`, history) work with telephony disabled.

## 4. Operator tools

None. The Slice 005 tool set, prompt rule ("cannot take actions"), and
ledger are untouched. 006b adds `start_call` on top of the `StartCall`
command (D-030).

## 5. HTTP contract (Lane A; frozen at approval)

All routes take `AuthContext` (any active member) except the webhook.
`{"error": "<code>"}` envelope; 401/503 conventions unchanged.

| Route | Request | Success | Errors |
|---|---|---|---|
| `POST /api/people/{id}/calls` | `{"contact_method_id": uuid}` (`deny_unknown_fields`) | 201 `{"call": CallView, "join": {"url", "token", "room"}}` | 400 `malformed_request`; 404 `not_found` (foreign/nonexistent Person, byte-identical); 422 `invalid_contact_method`; 409 `{"error": "call_in_progress", "call_id": uuid}` (the one envelope extension, so the client can offer "hang up the previous call"); 503 `telephony_disabled`; 503 `telephony_unavailable`; 503 `unavailable` |
| `POST /api/calls/{id}/dial` | — | 202 `{"call"}` (still `placing`; the dial task moves it to `ringing`) | 404 (foreign); 403 `forbidden` (not the caller); 409 `invalid_call_state` (already requested or not `placing`) |
| `POST /api/calls/{id}/hangup` | — | 200 `{"call"}`, idempotent | 404; 403 |
| `GET /api/calls/{id}` | — | 200 `{"call"}` (any member) | 404 |
| `POST /webhooks/livekit` | raw JSON ≤ 64 KiB, `Authorization: <LiveKit JWT>` | 200 `{}` for any valid signature (unknown room/event ignored) | 401 `unauthenticated`; 413 |

`CallView` (PII-free): `{id, person_id, contact_method_id, caller: {id,
display_name}, status, failure_reason, end_reason, placed_at,
ringing_at, answered_at, ended_at, talk_seconds}`.

New `ApiError` variants: `InvalidContactMethod`, `CallInProgress`,
`InvalidCallState`, `TelephonyDisabled`, `TelephonyUnavailable`.

Why two steps (`start` then `dial`): the callee must never answer to
silence. The browser must be in the room before the PSTN leg is dialed,
and the server verifies presence itself before dialing. It also makes
the ring-timeout and agent-never-joined cases testable.

Declared changes to existing contracts: the additive `history[].kind`
in §2. `GET /api/today`, `GET /api/people`, the session model, and the
Operator contract are unchanged.

## 6. Realtime contracts

Additive third event type under `v: 1` (SLICE_003 §6: clients ignore
unknown types): `{"v": 1, "type": "call.changed", "organization_id",
"occurred_at", "correlation_id", "data": {"call_id", "person_id"}}` —
ids only, published after every committed transition. Web mapping:
`call.changed` → `['org', orgId, 'call', callId]` and `['org', orgId,
'person', personId]`. The automatic attempt publishes the existing
`person.changed{contact_attempted}` (Today/People/person keys). In-call
ring/answer state for the caller comes from LiveKit itself (participant
events, the SIP participant's `sip.callStatus` attribute); Centrifugo
stays invalidation-only (D-023).

## 7. Authorization, tenant isolation, secrets

- Organization and caller enter only via `AuthContext → CommandContext`;
  `PersonVisibilityScope::Organization` for the Person; the contact
  method is resolved server-side by `(id, person_id, organization_id,
  kind = 'phone')`. **The client never supplies a phone number** — the
  CRM cannot be used to dial arbitrary numbers, and `call` stays
  PII-free.
- Any active member may call any Person of their Organization (D-005;
  SLICE_003 §7's "any member may log attempts"); only the caller may
  `dial`/`hangup`; any member may `GET`. 404 for a foreign Organization
  on every route; tests probe both directions.
- Join token: exactly one room, one identity, join/publish/subscribe
  only, short TTL, returned once, never logged or persisted. Room
  `max_participants = 2`.
- Webhook: no `AuthContext`; `DefaultBodyLimit::max(64 KiB)` as
  `routes/intake.rs`; signature verified with `LIVEKIT_API_SECRET`
  (LiveKit's scheme: bare JWT in `Authorization`, `iss` = API key,
  `exp`/`nbf` with small skew, `sha256` claim = **standard** base64 of
  the body hash, verified with `Mac::verify_slice`); ingress is a path on
  the existing `api.tarams.org` route (D-030 safe default amending
  D-016 §4). A webhook naming another Organization's room cannot touch
  it: room → call → Organization is resolved server-side.
- Media: browser ↔ LiveKit SFU ↔ LiveKit SIP ↔ Telnyx directly over the
  host's public UDP (D-016 §5, AGENTS §7). Only LiveKit *signaling*
  (`wss://`) may traverse a tunnel. The API's own LiveKit calls are
  HTTPS.
- Secrets (D-013): `LIVEKIT_API_SECRET` in `.env`; `TELNYX_SIP_*` are
  read only by `scripts/telephony-trunk`, never by the API. Redacting
  `Debug` newtypes for the secret, the join token, and `PhoneNumber`.
- The partial unique index is the guard against a second concurrent
  call by one user (409 derives from `23505`, never a pre-check).

## 8. Notification behavior

None beyond Centrifugo invalidation. No push, no ringing of other
devices (inbound is out of scope).

## 9. Observability and failure behavior

Spans: `call.start`, `call.dial` (`call_id`, `person_id`,
`organization_id`, `actor_id`, `correlation_id`, `outcome`,
`sip_status_class`), `call.hangup`, `call.webhook` (`event`, `room`,
`outcome ∈ applied | ignored | unknown_room | invalid_signature`),
`call.sweep` (`finalized`). Never the number, token, secret, the webhook
body, or a `ListParticipants` response / participant attributes (LiveKit
puts `sip.phoneNumber` in them). A log-capture test (§13) proves the
fixture number never appears in output. `/internal/ready` unchanged (LiveKit is not a readiness
dependency).

| Condition | Behavior | Records |
|---|---|---|
| `LIVEKIT_API_KEY` unset | 503 `telephony_disabled`; Call button explains | none |
| Bad telephony config | refuse to start (as `Config::from_source`) | — |
| LiveKit unreachable at `start` | `failed{provider_error}`; 503 `telephony_unavailable` | `call_completed`; no attempt |
| Agent never joins / never dials | dial task (10 s) or sweep → `failed{agent_not_joined}`/`{expired}`; room deleted | `call_completed`; no attempt |
| Agent's browser leaves (tab closed, sleep) | webhook `participant_left` for `agent:*` → `ended{agent_disconnected}` or `failed{cancelled}`; room deleted best-effort | `call_completed` (+ attempt per D-031) |
| Second `POST /dial` | 409 `invalid_call_state` | — |
| Caller already has an active call | 409 `call_in_progress` with `call_id`; the panel offers "Hang up the previous call" | — |
| Ring timeout (45 s) / busy / declined | `failed{…}` | `call_completed` + attempt `no_answer` |
| Trunk auth failure / SIP 5xx | `failed{provider_error}` | `call_completed`; no attempt |
| Answered | `answered` | attempt `reached` → Today advances |
| Agent hangs up | `hangup` → provider → `ended{agent_hangup}` or `failed{cancelled}` | `call_completed` (+ attempt per D-031) |
| Callee hangs up | browser sees the SIP participant leave → `hangup` (idempotent); webhook `participant_left`/`room_finished` → `ended{remote_hangup}`; first wins, the other is a no-op | `call_completed` |
| Webhook lost / API down at webhook time | agent's hangup or the sweep → `ended{reconciled}` / `failed{expired}` | `call_completed` |
| Duplicate / out-of-order webhook | terminal absorbs; 200 always | — |
| Bad webhook signature | 401; nothing written; `warn` | — |
| Publish failure | logged, never a failed command (D-023) | — |
| Mic permission denied | client calls `hangup` → `failed{cancelled}` before ringing | `call_completed`; no attempt |
| Max call duration (60 min) | SIP participant leaves → `ended{max_duration}` if LiveKit's `disconnect_reason` distinguishes it, else `remote_hangup` | `call_completed` |
| `CreateSIPParticipant` blocks up to `ring_timeout` | HTTP client timeout = `ring_timeout` + 15 s; `LIVEKIT_API_URL` is the host directly (TLS via Caddy), never a Cloudflare-proxied hostname (100 s cap) | — |

## 10. Frontend (Lane B, `web/**`; D-017; `UI_STYLE.md` binds)

- `livekit-client` pinned. `telephony/useCall.ts` with an injected client
  factory (fake in tests, as `useRealtime`): `idle → requesting_mic →
  joining → placing → ringing → connected → ended | failed`, driven by
  `POST …/calls` → `room.connect(url, token)` → `POST …/dial` → LiveKit
  events (`participantConnected` with identity `sip:*`,
  `participantAttributesChanged` `sip.callStatus ∈ dialing | ringing |
  active | hangup`, `participantDisconnected`, `disconnected`) →
  `hangup` on local end or remote leave (exactly once); `GET
  /api/calls/{id}` as the authoritative fallback on `call.changed`.
- `PersonDetailView.vue`: primary **Call** in the header card ("Log
  contact" becomes secondary — one primary per view); number picker only
  with several phones; `HISTORY_ICON`/`historySummary` gain
  `call_completed`.
- `CallPanel.vue`: docked, status line, elapsed timer, mute, **Hang up**
  (primary while active), post-call line "Logged as contact attempt —
  call, reached / no answer" (replaced by SLICE_006c §10's "How did it
  go?" prompt, which renders under the same condition). Error copy: `telephony_disabled` →
  "Calling is not configured on this server."; `telephony_unavailable`
  → "Calling is temporarily unavailable — try again in a moment.";
  `call_in_progress` → "You already have a call in progress." with a
  **Hang up previous call** action using the returned `call_id`;
  `invalid_contact_method` → "That number can't be called."; others →
  the generic pattern.
- `api/types.ts`, `api/queries.ts`: `CallView`, `StartCallResponse`,
  `useStartCall`, `useDialCall`, `useHangupCall`, `useCall(callId)`;
  `realtime/events.ts` mapping for `call.changed`.
- Today's `call` recommended action remains a link to the Person page.

## 11. Development environment, transport, and configuration

**Telephony host** (Lane C, `infra/telephony/`): a public-IP Linux host
(small VPS or the OVH box) running `compose.yaml` — `livekit/livekit-
server`, `redis`, `livekit/sip` — with `use_external_ip: true`, UDP
50000–60000, 7881/TCP, SIP 5060/UDP+TCP and the SIP RTP range open;
a plain DNS A record `livekit.tarams.org` → the host (no tunnel: the
host is public); TLS terminated on the host by Caddy (Let's Encrypt) on
443, reverse-proxying to LiveKit's 7880 for both browser signaling
(`wss://`) and the API's Twirp calls (`https://`); ICE candidates point
at the host IP; firewall: 443/TCP, 7881/TCP, 50000–60000/UDP, SIP
5060/UDP+TCP and the SIP RTP range;
`webhook.urls: [https://api.tarams.org/webhooks/livekit]` (reaches the
Mac API through the existing tunnel); no egress service. README gains a
"Telephony" section (firewall, DNS, how to verify with `lk`).

**Telnyx** (one-time, user's account): one US number with caller-ID
set; a credential-based SIP connection with an outbound voice profile
assigned; `scripts/telephony-trunk` (bash + `lk` CLI; reads
`TELNYX_SIP_USERNAME`, `TELNYX_SIP_PASSWORD`, `TELNYX_NUMBER_E164` from
`.env`, never argv) creates the LiveKit outbound trunk and prints its
id for `LIVEKIT_SIP_OUTBOUND_TRUNK_ID`. The application never holds
Telnyx credentials.

`.env.example` gains:

```
LIVEKIT_URL=                         # wss://livekit.tarams.org (browser signaling; Caddy → LiveKit)
LIVEKIT_API_URL=                     # https://livekit.tarams.org (API; http:// only for loopback; never Cloudflare-proxied)
LIVEKIT_API_KEY=                     # empty/unset = calling disabled
LIVEKIT_API_SECRET=
LIVEKIT_SIP_OUTBOUND_TRUNK_ID=
CRM_TELEPHONY_RING_TIMEOUT_SECONDS=45      # bounds 10–120
CRM_TEST_LIVEKIT_API_URL=                  # scripts/check-telephony only; never read by check-db
CRM_TELEPHONY_MAX_CALL_SECONDS=3600        # bounds 60–14400
CRM_TELEPHONY_JOIN_TTL_SECONDS=300         # bounds 60–900
TELNYX_SIP_USERNAME=                 # scripts/telephony-trunk only
TELNYX_SIP_PASSWORD=
TELNYX_NUMBER_E164=
```

`LIVEKIT_API_KEY` empty → calling disabled, not a startup error; bounds
and URLs validated regardless. The same `LIVEKIT_API_KEY`/`SECRET` pair
must be in the host's `livekit.yaml` (including its webhook `api_key`)
and the Mac's `.env` — a manual sync the README calls out. A loopback LiveKit container on the Mac
(optional `infra/development` profile) serves browser-only development
and the optional LiveKit-backed test; it cannot carry PSTN media.

## 12. Explicit exclusions

Inbound calls; native mobile calling (CallKit/Telecom); recording,
egress, transcription, summaries (O-002); Telnyx API/webhooks in the
app; per-Organization or per-user numbers and number-ownership facts
(thesis §5.3); SMS (O-006); call disposition/notes UI (outcome picker added by SLICE_006c;
notes still excluded); corrections of
auto-logged attempts (superseded by SLICE_006c for call-derived
attempts); O-008 suggestions; Operator `start_call` (006b);
the `crm-app` extraction (006a); streaming/voice Operator (declared
deferral of SLICE_005 §16); DNC/quiet-hours compliance (O-011);
multi-pod sweep/dial coordination; TURN/TCP media fallback (UDP-blocked
networks fail at ICE).

## 13. Checks and tests

1. **Service-free (`./scripts/check`)** — join-token signer claims and
   signature; webhook verifier (valid / tampered body / expired / wrong
   key; constant-time); `transitions::apply` over every state × signal;
   SIP failure → outcome mapping; D-031 attempt mapping; config parsing,
   bounds, redaction; 401/400/503 on every new route; `ScriptedProvider`
   unit tests; `Config` refuses bad telephony config.
2. **DB-backed (`check-db`)** — migration and grants (column-level
   UPDATE on exactly the status/timestamp columns; `call_completed`
   append-only for `crm_app` and owner; `db_schema.rs` enumerations
   extended); `start`: 201 with a token whose claims are exactly §3's,
   foreign Person 404 byte-identical, email/other-Person/foreign contact
   method → 422 identical, a real concurrent second call → 409 from the
   unique index (`tokio::join!`), provider create failure →
   `failed{provider_error}` + `call_completed` + no attempt + 503;
   scripted `Answered` → `answered`, exactly one `contact_attempted(call,
   reached)` with caller actor / `web_session` / call correlation /
   `causation_id = call.id`, the Person leaves the caller's and a
   non-caller member's Today, the recording publisher has exactly
   `call.changed` then `person.changed{contact_attempted}`; busy /
   declined / ring-timeout → `no_answer`; `agent_not_joined` /
   `provider_error` → no attempt; cancel before vs. after ringing;
   hangup idempotent, non-caller 403, foreign 404; webhook
   `participant_left` for `sip:<id>` → `ended{remote_hangup}` with
   `talk_seconds`, duplicate `room_finished` no-op, unknown room 200 and
   nothing written, tampered / wrong secret / expired → 401; sweep
   finalises a backdated `placing` and `ringing` to `failed{expired}`
   (no attempt) and a backdated `answered` to `ended{reconciled}`;
   webhook `participant_left` for `agent:*` in `answered` →
   `ended{agent_disconnected}` and in `ringing` → `failed{cancelled}`
   with one `no_answer` attempt; an out-of-order `participant_left` for
   `sip:*` in `ringing` is a no-op and the dial task's later `Failed`
   still yields exactly one attempt; `hangup` while the scripted `dial`
   is blocked (released by a oneshot) → `failed{cancelled}` with exactly
   one attempt and the dial task's later result a no-op; double
   `POST /dial` → 409; `GET /api/calls/{id}` and Person history with
   telephony disabled; a log-capture test that the fixture number never
   appears in spans or log lines; history `call_completed` (`kind_rank`
   5) sorts after a same-instant `contact_attempted`.
3. **LiveKit-backed (`scripts/check-telephony`, gated on
   `CRM_TEST_LIVEKIT_API_URL`; fails loudly on unreachable, never skips;
   `check-db` stays loopback-only per SLICE_001 §9)** — create room →
   list → delete with real auth; webhook round-trip with a signature
   produced by the real algorithm.
4. **Web (Vitest)** — `useCall` with a fake LiveKit client: every
   transition, remote leave → `hangup` exactly once, mic denied →
   `hangup`, `call.changed` → refetch; `invalidationsFor('call.changed')`;
   Call disabled without a phone; number picker only with ≥ 2 phones;
   `call_completed` history rendering; the 409 "hang up previous call"
   affordance; error copy per code.
5. **Live walkthrough** (not CI): §1 steps 1–7 from
   `https://app.tarams.org`, with a second browser watching Today.

Required before "done": `./scripts/check` with no services;
`./scripts/check-db` with `dev-services up` (and the telephony host for
item 3); web lint/typecheck/test/build; the live walkthrough.

## 14. Safe defaults adopted (overridable at approval)

1. Ring timeout 45 s; max call 60 min; join-token TTL 5 min.
2. One active call per user (partial unique index).
3. Org-wide read of a call; caller-only `dial`/`hangup`.
4. Two-step `start` → `dial`.
5. Sweep every 30 s, in-process (one pod).
6. `call_completed` is `kind_rank` 5; `call.changed` under `v: 1`.
7. No new crate; `TelephonyProvider` lives in `crm-api`; `ScriptedProvider`
   behind `test-support`.
8. Telnyx credentials only in the LiveKit trunk, never in the app.
9. Webhook path under `api.tarams.org` (amends D-016 §4).
10. One deployment-level caller number (D-030).
11. Voicemail reads `reached` (D-031).
12. Mic denial before ringing is `cancelled` with no attempt.
13. Review amendments (2026-08-22): `dial` returns 202 with `placing`
    and the dial task owns `placing → ringing`; `Expired` only from
    `placing|ringing`; per-state sweep horizons, no `room_exists`;
    `AgentLeft` signal and `agent_disconnected` end reason; `hangup`
    settles before deleting the room; `RemoteLeft`/`RoomFinished` are
    no-ops before `answered`; 409 `call_in_progress` carries `call_id`;
    the LiveKit-backed test lives in `scripts/check-telephony`.

## 15. Lane ownership and sequencing

Three lanes after approval; contracts frozen by §2 (DDL), §3 (trait,
commands, token claims), §5 (HTTP), §6 (realtime), §11 (config).

- **Lane A — backend** (`backend/**`, `scripts/**`, `.env.example`,
  README dev section, the migration): (1) config + `telephony/` trait,
  LiveKit provider, signer, verifier; (2) migration + `settle`;
  (3) commands + routes + webhook — **HTTP contract reachable here**;
  (4) dial task + sweep; (5) history kind; (6) tests;
  (7) `scripts/telephony-trunk`. ≈ 5 days.
- **Lane B — web** (`web/**`): `useCall` + `CallPanel` + Person header
  against the frozen §5/§6 contracts with a fake client; integrates after
  A(3). ≈ 2.5 days.
- **Lane C — infra** (`infra/telephony/**`, README "Telephony"): the
  host compose, LiveKit/SIP config, firewall and DNS notes; needs the
  user's host and Telnyx credentials; starts day 1 and is the critical
  path for the live walkthrough. ≈ 1–2 days plus provisioning.
- **Coordinator** owns `docs/**`, `PROJECT_STATE.md`, the decision log.

Risk concentrates in: LiveKit SIP ↔ Telnyx trunk configuration (Lane C,
mitigated by `scripts/telephony-trunk` and the `lk` CLI checks); the
lost-signal paths (mitigated by `settle` being the single write path, the
sweep, and the scripted tests); and keeping numbers out of logs.

## 16. Next

006a — the `crm-app` extraction (D-028 §5), no behaviour change. 006b —
the Operator `start_call` tool with D-009 preview → confirm → receipt.
Then inbound calling (routing, multi-device ringing, CallKit/Telecom
with the mobile slices), recording once O-002 is decided, SMS once
O-006 is decided.
