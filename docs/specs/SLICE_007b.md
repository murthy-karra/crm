# Slice 007b — Inbound email endpoint (encrypted raw → Unresolved, zero parsing)

Status: APPROVED (user, 2026-08-24; planner pass + independent review
same day, reviewer amendments applied — no blocking findings; §5
contract and §13 safe defaults accepted as written)

> Amended by SLICE_007d (declared change, AGENTS.md §11): once 007d
> lands, the endpoint parses stored mail and completes intake, so
> criteria 3 ("advisory lock never taken"), 14 (the stuck-`pending`
> rescue now runs the full parse), and 18 ("no changes to
> `receive_inquiry`") are superseded, and valid-MIME unknown-format
> mail lands `email_unrecognized_format` instead of `email_unparsed`
> (the §1/§10 expectations for `plain.eml`/`multipart.eml`). The §5
> HTTP envelope is unchanged.
Ladder: docs/plans/SLICE_007_LADDER.md rung b. Builds on 007a (intake
address; merged `81af77f`). Targets O-014 (mailbox-less receiving,
raw-first), D-012 (raw payloads preserved), D-021 (creation paths),
SLICE_002 (Phase A / Unresolved mechanics), SLICE_003 (realtime
conventions), AGENTS.md §9 (never log secrets/content), §11 (contract
discipline).

## 1. User-visible outcome

An email forwarded to an Organization's intake address (007a's
`leads-<token>@<slug>.elysianfeld.com`) — delivered, for now, by a dev
script rather than real DNS — appears within a second in that
Organization's **Intake → Unresolved** queue as an "Unparsed email"
row (source `email`, real byte size, receipt time), with the raw
RFC 822 bytes stored encrypted for later rungs to parse. Nothing else
happens: no Person, no Inquiry, no facts, no assignment. Live
walkthrough: run `scripts/inbound-email <recipient> <fixture.eml>`
against a seeded org's address (copied from Manage → Intake), watch the
row appear in Unresolved in realtime; run it twice → still one row; run
it with a corrupted token → nothing anywhere.

## 2. In / out of scope

In: the `POST /inbound/email` endpoint (outside CORS, bearer-secret
auth, 2 MiB), the Phase-A-only `receive_inbound_email` entry point, the
organization-by-intake-address lookup (deferred here from 007a, with
its security property tested), the `CRM_INBOUND_EMAIL_SECRET` config,
the `email_unparsed` unresolved reason (backend free-text + the
two-line web label), `scripts/inbound-email`, the first `.eml`
fixtures.

Out: any MIME parsing or `mail-parser` (007d), facts / `FactEnvelope` /
system actor / routing (007c), Person or Inquiry creation (007d),
Unresolved detail / retry / discard (007e), LLM extraction (007f),
provider adapters, signature verification, real DNS/receiving, token
rotation (007g), portal parsers (007h), Operator awareness of intake,
any new backend dependency (base64 is already in the workspace via
telephony), any change to `receive_inquiry` or its HTTP contract.

## 3. Persistence

**No migration.** `raw_payload` (migration `20260821000003`) constrains
only `resolution IN ('pending','resolved','unresolved')`; `source`,
`payload_format`, `origin`, and `unresolved_reason` are free TEXT by
SLICE_002 design (do not add vocabulary CHECKs — unrelated hardening).
`crm_app` already holds every needed grant: `SELECT, INSERT` on
`raw_payload`, `UPDATE (resolution, unresolved_reason, resolved_at,
inquiry_id)`, `SELECT` on `organization`.

Row lifecycle, exactly SLICE_002's mechanics: insert
`resolution='pending'` (own committed transaction, `ON CONFLICT
(organization_id, source, content_hmac) DO NOTHING`), then transition
to `resolution='unresolved'`, `unresolved_reason='email_unparsed'`,
`resolved_at=now()`. A crash between the two commits leaves a `pending`
row — the queue already lists it (`resolution <> 'resolved'`), 007e's
Try-again rescues it, and criterion 14 makes re-delivery rescue it too.
New values: `source='email'`, `payload_format='rfc822_v1'`,
`origin='webhook'` (`Origin::Webhook` already exists).

Idempotency keyspace note: `POST /api/inquiries` callers may also send
`source="email"`, so the two entry points share the `(org, 'email',
content_hmac)` key — a collision requires byte-identical content, which
is the correct dedup outcome; no action. The same applies to the
rescue path (§4 step 4): a stuck-`pending` `generic_v1` row whose bytes
are identical to an inbound email would be flipped to
`unresolved`/`email_unparsed` while keeping `payload_format=
'generic_v1'` — probability ~0 (byte-identical JSON = RFC 822), and
007e's Try-again keys off `payload_format`, so the mismatch is
harmless; stated and accepted (reviewer F3).

## 4. Domain

New `domain/intake/receive.rs` (beside `address.rs`) — deliberately
**not** in `domain/commands/`: everything there takes a
`CommandContext`, and this entry point has no actor by design (the
system actor is 007c's declared contract change, not pulled forward).

```rust
pub enum InboundEmailOutcome { Stored { raw_payload_id: Uuid }, Duplicate, Rejected }
/// Phase A only: parse recipient → resolve org → seal + insert →
/// mark unresolved → publish. No CommandContext, no facts, no
/// advisory lock (that guards Phase B's identify/route path only —
/// this endpoint can never return `intake_busy`).
pub async fn receive_inbound_email(
    pool, key: &RawPayloadKey, publisher: &Publisher,
    cfg: &IntakeMailConfig, recipient: &str, raw: &[u8],
    received_at: DateTime<Utc>,
) -> Result<InboundEmailOutcome, ReceiveInboundEmailError>;
```

Flow, reusing Phase A unchanged (`crypto::content_hmac`, `crypto::seal`
with AAD = `organization_id ‖ raw_payload_id`, `store::insert_pending`
— already fully parameterized; no refactor):

1. `IntakeAddress::parse_recipient(recipient, cfg)` (007a; accepts both
   schemes, case-insensitive, never logs input) → `None` → `Rejected`
   before any DB work (reveals only syntactic validity — public
   knowledge).
2. New query `organization_by_intake_slug(conn, slug) ->
   Option<(organization_id, intake_token)>` — one indexed SELECT by the
   unique slug index, then compare the presented token in Rust with a
   constant-time comparison (the repo's "never `==` on secrets" rule;
   copy the 10-line `constant_time_eq` from
   `crm-app/src/telephony/webhook.rs` — the §13-minimal choice; note it
   early-returns on length mismatch, so the dummy token below must be
   8 bytes, the length `parse_recipient` guarantees for the presented
   token). Wrong token and unknown slug thus do identical work: one
   indexed SELECT, one constant-time compare (against an 8-byte dummy
   token on unknown slug), → `Rejected`, nothing stored.
3. `content_hmac` → mint candidate id → `seal` → `insert_pending(org,
   "email", "rfc822_v1", Origin::Webhook, …)` (returns the *stored*
   row's id — differs from the candidate on duplicate).
4. New store fn `mark_unresolved_if_pending(conn, id, org, reason) ->
   bool` (`UPDATE … SET resolution='unresolved', unresolved_reason=$..,
   resolved_at=now() WHERE id=.. AND organization_id=.. AND
   resolution='pending'`): `true` → fresh row (or a stuck-`pending`
   rescue) → publish + `Stored`; `false` → already terminal →
   `Duplicate`, no publish. No change to the existing
   `mark_unresolved`.
5. Publish `RealtimeEvent::intake_unresolved_changed` (existing event;
   ids-only, `v:1`) via `publish_after_commit` on `org:<id>`;
   `occurred_at = received_at`; `correlation_id` = fresh
   `Uuid::new_v4()` (there is no `CommandContext` to inherit one from).

## 5. HTTP contract (frozen at approval; AGENTS.md §11)

`POST /inbound/email` — its own router, `.merge`d **outside** the CORS
layer exactly like `routes/livekit_webhook.rs`, wrapped in
`with_request_tracing`, with a per-route `DefaultBodyLimit::max(2 MiB)`
(so the effective max raw RFC 822 is ~1.5 MiB after base64 — the
ladder's settled cap applies to the envelope). No path conflict
(`/api/*`, `/internal/ready`, `/webhooks/livekit`).

Request: `Authorization: Bearer <CRM_INBOUND_EMAIL_SECRET>`; JSON body
`{"recipient": string, "raw": string}`, `raw` = strict padded standard
base64, no embedded whitespace.

Handler order: bearer first (constant-time compare), then JSON, then
base64 — a bad bearer never gets its body interpreted. (Axum's
extractor still buffers the body, so an oversize body with a bad bearer
413s before the handler — livekit-webhook behavior, accepted and
stated.)

Responses (error envelope `{"error": code}` as everywhere):

| Case | Response |
|---|---|
| Stored **or** byte-identical duplicate | 200 `{"status":"accepted"}` — idempotent success; a machine caller needs only "safe to discard"; no `raw_payload_id` or any id leaked |
| Recipient unparseable, unknown slug, or wrong token | 200 `{"status":"rejected"}` — **byte-identical across all three**; nothing stored |
| Missing/wrong bearer, or secret unset (endpoint disabled) | 401 `{"error":"unauthenticated"}` — before any body interpretation |
| Body over 2 MiB | 413 `{"error":"payload_too_large"}` — new **additive** `ApiError::PayloadTooLarge` variant so the 413 keeps the envelope |
| Non-JSON, missing field, invalid base64, empty raw after decode | 400 `{"error":"malformed_request"}` |
| Database unreachable | 503 `{"error":"unavailable"}` |
| Internal failure (e.g. `crypto::seal` error) | 500 `{"error":"internal_error"}` (existing variant); row state per §3's crash-window rule (a committed `pending` row may remain — rescued by re-delivery or 007e) |
| — | `intake_busy` is impossible here (no advisory lock); stated, tested |

## 6. Config

Following 007a's shape exactly (crm-app owns the type; crm-api
`from_source` parses; `AppState` carries; `.env.example` documents
name-only per D-013):

- crm-app `config.rs`: `pub struct InboundEmailSecret(Vec<u8>)`,
  redacted `Debug`, `parse()` enforcing **min 32 bytes** (same value as
  crm-api's private `MIN_SESSION_SECRET_BYTES`; implement on the
  crm-app `MIN_REALTIME_TOKEN_SECRET_BYTES` template — a value match,
  not a shared const).
- crm-api `Config.inbound_email_secret: Option<InboundEmailSecret>`:
  unset/empty → `None` → **endpoint disabled: every request 401**
  (except the pre-handler extractor 413 on an oversize body, §5 — the
  livekit-webhook-while-telephony-disabled precedent, one warn
  log; 401 rather than 404/503 keeps disabled and wrong-secret
  indistinguishable to a prober). Set but short → new
  `ConfigError::InboundEmailSecretTooShort` with a Display arm in the
  existing style; startup fails.
- `AppState.inbound_email_secret`. Never logged (AGENTS.md §9);
  redacted `Debug` covers accidental `{:?}`.

## 7. Web

One two-line change (the only web work): `web/src/api/types.ts`'s
closed `UnresolvedReason` union gains `'email_unparsed'` and
`web/src/lib/labels.ts` `UNRESOLVED_REASON_LABEL` gains `"Unparsed
email"` (without it the Reason cell renders `undefined`). One Vitest
line pins the label. The Unresolved table then renders an email row
acceptably: source `email`, reason "Unparsed email", size, received
time. No new views, routes, queries, or realtime handling — the client
already maps `intake.unresolved_changed` → invalidate
`queryKeys.unresolved(orgId)`.

## 8. Authorization & tenant isolation

The **token is the only tenant credential**: the Organization comes
solely from `(slug, token)` in the recipient — never from a header,
body field, or bearer (the bearer is deployment-level transport auth,
shared by all tenants, per ladder cross-rung decision 2). Security
properties, each pinned by a test in §10: org A's slug + org B's token
stores nothing anywhere; all rejection shapes are byte-identical with
identical work profiles (no address-enumeration oracle); the raw
content is readable by no one in this rung (`GET /api/intake/unresolved`
returns metadata only, unchanged — raw reading is 007e's blocking
decision).

## 9. Observability

Span `intake.inbound_email` records only: `outcome`
(`accepted` | `duplicate` | `rejected` | `unauthenticated` |
`malformed` | the error-variant name on 500/503, keyed by variant only,
never error text — the `receive_inquiry` wrapper pattern), `byte_len`,
and `raw_payload_id` when stored. One
undifferentiated `rejected` even internally — the simplest guarantee
that nothing recipient-derived leaks; 007g's provider adapter is the
place for richer ops signal. Never in any span/log: recipient, slug,
token, bearer value, raw bytes, base64 text. A tracing-capture test
(007a's pattern) proves it.

## 10. Acceptance criteria

1. `POST /inbound/email` with a valid bearer, a valid recipient for
   org A, and base64 RFC 822 bytes → 200 `{"status":"accepted"}` and
   exactly one `raw_payload` row in org A: `source='email'`,
   `payload_format='rfc822_v1'`, `origin='webhook'`,
   `resolution='unresolved'`, `unresolved_reason='email_unparsed'`,
   `received_at` = receipt time, correct `byte_len`.
2. The stored ciphertext decrypts (AAD = org ‖ row id) back to the
   exact original raw bytes.
3. No Person, Inquiry, fact, or routing row is created; the intake
   advisory lock is never taken; the endpoint can never return
   `intake_busy`.
4. Re-POSTing byte-identical raw to the same recipient → 200
   `{"status":"accepted"}`, nothing new stored, no second realtime
   publish (asserted via the recording publisher).
5. The same raw bytes to two different orgs' valid addresses → one row
   per org.
6. Org A's slug with org B's (or any wrong) token → 200
   `{"status":"rejected"}`, zero rows anywhere.
7. Unknown slug and syntactically invalid recipient → responses
   byte-identical to criterion 6's; org lookup is one indexed SELECT
   and token comparison is constant-time (dummy compare on unknown
   slug).
8. Missing or wrong bearer → 401 `{"error":"unauthenticated"}` before
   any body interpretation, nothing stored; secret unset behaves
   identically.
9. Body over 2 MiB → 413 `{"error":"payload_too_large"}`; malformed
   JSON / invalid base64 / empty raw → 400
   `{"error":"malformed_request"}`; DB down → 503
   `{"error":"unavailable"}`.
10. The route is outside the CORS layer with its own 2 MiB limit: with
    `CRM_CORS_ALLOWED_ORIGIN` configured, its responses carry no CORS
    headers.
11. On first storage, `intake.unresolved_changed` (`v:1`, ids-only,
    `occurred_at` = receipt time, fresh `correlation_id`) publishes
    best-effort after commit on `org:<id>`.
12. `GET /api/intake/unresolved` shows the row to org A members only;
    the web table renders reason "Unparsed email" (Vitest-pinned).
13. Tracing capture over accepted, rejected, **malformed (400), and
    bad-bearer (401)** requests: recipient, slug, token, bearer, and
    raw/base64 content appear in no span or log line (the malformed
    paths are the leak-prone ones — `JsonRejection`/base64-error
    `Display` strings must not be recorded).
14. A delivery finding an existing `pending` row for the same bytes
    (crash between the two commits) transitions it to
    `unresolved`/`email_unparsed` and publishes exactly once.
15. All DB tests run as `crm_app` with existing grants; the slice ships
    no migration.
16. Config: secret ≥ 32 bytes when set (startup `ConfigError`
    otherwise), `Debug`-redacted, name-only in `.env.example`.
17. `scripts/inbound-email <recipient> <file.eml>` posts a fixture
    end-to-end in dev (live walkthrough per §1).
18. No new backend dependencies; no changes to `receive_inquiry`, its
    HTTP contract, or existing store/crypto signatures.
19. Two concurrent identical POSTs (`tokio::spawn`/`join!`, the
    SLICE_002 criterion-17 pattern) → exactly one row, exactly one
    realtime publish — pinning the `ON CONFLICT` + `WHERE
    resolution='pending'` re-check under READ COMMITTED.
20. An org whose token is a 007a-backfill hex token (outside the
    `[a-z2-7]` mint alphabet) accepts mail end-to-end — the lookup +
    compare path is proven for legacy orgs, not only freshly minted
    ones.

## 11. Tests

DB-backed, new `tests/db_inbound_email.rs` (`db_intake.rs` /
`db_intake_address.rs` patterns): criteria 1–9, 11–15, 19–20 above,
including the recording-publisher single-publish assertions, the
tracing capture, the stuck-`pending` rescue, and the concurrency race. Service-free, new `tests/inbound_email.rs`
(`tests/intake.rs` pattern): 401 without bearer on a stateless app; 503
with unreachable DB + valid bearer + well-formed body; no CORS headers
on responses with a configured origin; oversize body **with a bad
bearer** → 413 (pins the stated extractor-before-handler ordering). Unit: `mark_unresolved_if_pending`
false on terminal rows; `constant_time_eq`; config parse
(unset → None, short → error, redacted Debug); web label Vitest.

Fixtures: two synthesized `.eml` files (one plain-text, one multipart —
content is opaque bytes this rung) under
`backend/crates/crm-api/tests/fixtures/email/`, shared by tests and
`scripts/inbound-email` (bash, `demo-leads` style: sources `.env`,
secret never on argv, base64 without line-wrapping).

## 12. Lane and checks

Single lane, branch `slice-007b-inbound-email`, one writer, owning:
crm-app command + query + config type, crm-api route + config +
`ApiError` variant, all tests, the script, fixtures, and the two-line
web types/labels touch (too small for a second lane; splitting would
put `types.ts` on a shared boundary for no benefit). No migration
exists to own. Gates: `./scripts/check` and `./scripts/check-db` green,
web lint/typecheck/test/build green, live walkthrough per §1.

## 13. Safe defaults adopted (reviewer/user may veto)

(a) 200 `{"status":"accepted"}` uniformly for stored and duplicate — no
201/200 split, no ids in success bodies; (b) secret unset ⇒ 401 (not
404/503), the merged livekit precedent; (c) min secret length 32 bytes;
(d) all rejection variants byte-identical 200 `{"status":"rejected"}`,
with recipient-parse failure short-circuiting before the DB; (e) label
"Unparsed email"; (f) additive `ApiError::PayloadTooLarge` → 413
envelope; (g) fresh UUID as event `correlation_id`; (h)
`receive_inbound_email` lives in `domain/intake/`, not
`domain/commands/`, because it deliberately has no `CommandContext`.
