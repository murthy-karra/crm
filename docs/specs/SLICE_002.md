# Slice 002 — Lead Intake

Status: IMPLEMENTED, verified (2026-08-21; planner draft independently
reviewed — 17 findings applied; spec re-reviewed — 12 further items
applied; §14 defaults reviewed with and approved by the user;
implemented in two lanes, then independently reviewed and adversarially
tested against the real working tree — one undisclosed out-of-scope
file removed, five correctness/observability findings fixed, one
cross-tenant availability finding (§3's lock acquisition, §5's
`intake_busy`, §9) fixed per an explicit user decision to correct the
mechanism rather than contain it; §1's full walkthrough run live —
both processes together, real browser, real Postgres — covering new
lead, dedup, stage/reassignment, unresolved, duplicate delivery, and
cross-Organization isolation, zero console/page errors; one cosmetic
singular/plural nit found and fixed during the walkthrough. Awaiting
merge to `main`.)
Builds on: Slice 001 (`587a087`) + tunnel follow-up (`3b6df76`).
Targets: D-005, D-006, D-007, D-012, D-015, D-017, D-019; the first two
links of the thesis §16 proof chain (`New lead → correct CRM state`).
Scope decisions (user-accepted 2026-08-21, not re-litigated here):
(1) one symmetric raw-payload key in `.env`, no per-payload DEKs or
rotation; (2) routing is "assign to a named Organization member", no
rules engine; (3) stages per D-019; (4) the minimal list view and the
D-017 frontend stack (plus Vue Router) land here; (5) sqlx offline mode
is adopted here.

## 1. User-visible outcome

On the D-016 Mac (and through the tunnel):

1. `./scripts/dev-services up` → `./scripts/db-migrate` →
   `./scripts/dev-seed` (now also seeds the nine D-019 stages per
   Organization and a **second member per Organization** so reassignment
   has a target) → `./scripts/dev-api` → `./scripts/dev-web`.
2. Log in as `alice@acme.test`. The restyled shell shows a nav: People,
   Unresolved, New lead.
3. **New lead**: enter a source (free text; the form suggests `zillow`,
   `realtor_com`, `website`, `referral`, `manual`), first/last name,
   email, phone, message, and optionally an assignee. Submit → redirected
   to the new Person. The detail page shows contact methods, stage
   "Lead", assignee (Alice unless chosen otherwise), the Inquiry with its
   source, and a history timeline with four entries sharing one
   correlation id: InquiryReceived, RoutingDecision, AssignmentChanged,
   StageChanged.
4. Change stage to "Hot Prospect"; reassign to the second member → two
   more history rows.
5. Submit a second lead with the same email and a different source → no
   new Person; inquiry count 2; the first Inquiry's source is untouched
   (D-006).
6. Submit a lead with neither email nor phone → it appears in
   **Unresolved** with reason `no_contact_method`; no Person is created;
   the payload is encrypted at rest (`psql` shows no email bytes in
   `raw_payload.ciphertext`).
7. Submit the identical lead twice → one Inquiry; the second response
   carries `"duplicate": true`.
8. Log in as `bob@best.test` → People and Unresolved are empty.
9. `./scripts/check` passes with zero services running (offline `.sqlx`
   cache); `./scripts/check-db` verifies the cache against a freshly
   migrated throwaway database and runs the DB-backed suite.

## 2. Domain and schema

> Amended by SLICE_004 §2 (declared change, AGENTS.md §11): default
> `stage` rows are seeded through the application path
> (`CreateOrganization` → `stage::seed_defaults`, `crm_app` INSERT
> granted); "never by the application" below is superseded.

### Classification (D-007, D-015)

- **Erasable CRUD set** (plaintext, D-015 §3/§6): `person`,
  `contact_method`, `inquiry`. Erasure (deferred, D-015 §5) deletes the
  `person` row and cascades to the other two. This names explicitly what
  D-015 §3 calls "the Person table": contact methods are CRUD per D-007,
  and `inquiry.message` is the only free-text customer content outside
  the encrypted blob. The D-015 §7 erasure runbook must cover all three.
- **Encrypted deletable blob** (D-015 §4): `raw_payload`.
- **Reference data**: `stage` (D-019).
- **Immutable, PII-free history** (D-015 §2/§3): `inquiry_received`,
  `routing_decision`, `assignment_changed`, `stage_changed`.

### Facts reference erasable rows by bare UUID, not FK

A consequence of D-015 §5 ("history remains intact with orphaned IDs")
plus §2 (append-only): `ON DELETE NO ACTION` would block erasure,
`SET NULL` would both mutate an append-only row and destroy the orphaned
ID D-015 wants kept. So fact tables reference `person`, `inquiry`, and
`raw_payload` by plain `UUID` columns with no foreign key, and FK only to
rows that are never deleted: `organization`, `app_user`, `stage`.
Reversible — FKs can be added if tombstoning is ever chosen instead.

### Tables

- `stage`: `id`, `organization_id` FK, `name`, `position SMALLINT`,
  `created_at`, `updated_at`. `UNIQUE (organization_id, name)`,
  `UNIQUE (organization_id, position)`, `UNIQUE (id, organization_id)`
  (for composite FKs below). Seeded by a library helper
  `stage::seed_defaults(&mut tx, organization_id)` (idempotent on name)
  used by the seed binary and test fixtures — never by the application
  (`crm_app` has SELECT only).
- `person`: `id`, `organization_id` FK, `first_name TEXT NULL`,
  `last_name TEXT NULL`, `stage_id NOT NULL`, `assigned_user_id UUID NULL`
  FK `app_user`, `created_at`, `updated_at`. `UNIQUE (id,
  organization_id)`. Composite FK `(stage_id, organization_id) →
  stage (id, organization_id)` so another Organization's stage can never
  be persisted even if an application check regresses. Index
  `(organization_id, created_at)`.
- `contact_method`: `id`, `organization_id`, `person_id`, `kind TEXT
  CHECK (kind IN ('email','phone'))`, `value TEXT` (as received),
  `normalized_value TEXT`, `created_at`. Composite FK `(person_id,
  organization_id) → person (id, organization_id) ON DELETE CASCADE`.
  `UNIQUE (person_id, kind, normalized_value)`. Index `(organization_id,
  kind, normalized_value)` — the identify lookup. Deliberately **not**
  unique per Organization: shared household emails and phones are real.
  Normalization: email = trim + lowercase; phone = digits only, 10-digit
  US → `+1` prefix, 11-digit starting with 1 → `+` prefix, otherwise
  `+` + digits; anything with fewer than 10 digits or no `@` respectively
  is not a normalizable contact method.
- `raw_payload`: `id` (generated in Rust — it is part of the AEAD
  associated data), `organization_id` FK, `source TEXT`, `payload_format
  TEXT` (`'generic_v1'` — D-012 "record source versions"), `origin TEXT`,
  `received_at`, `nonce BYTEA` (24), `ciphertext BYTEA`, `content_hmac
  BYTEA` (32, see §7), `byte_len INT`, `resolution TEXT CHECK
  (resolution IN ('pending','resolved','unresolved'))`, `unresolved_reason
  TEXT NULL`, `resolved_at TIMESTAMPTZ NULL`, `inquiry_id UUID NULL`
  (bare), `created_at`. `UNIQUE (organization_id, source, content_hmac)`
  — the delivery idempotency key for this slice's generic ingress (§3).
- `inquiry`: `id`, `organization_id`, `person_id`, `raw_payload_id UUID`
  (bare), `source TEXT`, `source_external_id TEXT NULL`, `message TEXT
  NULL` (truncated to 4 KiB on insert; the encrypted raw payload keeps
  the full text), `received_at`, `created_at`. Composite FK `(person_id,
  organization_id) → person ON DELETE CASCADE`. Indexes
  `(organization_id, person_id, received_at)`, `(organization_id,
  received_at)`.

### Fact envelope (every history table, exact columns)

`id UUID PK`, `organization_id UUID NOT NULL` FK, `actor_kind TEXT NOT
NULL CHECK (actor_kind IN ('user','system'))`, `actor_user_id UUID NULL`
FK `app_user`, `on_behalf_of_user_id UUID NULL` FK `app_user` (always
NULL this slice), `origin TEXT NOT NULL` (`'web_session'` now;
`'webhook'`, `'operator'`, `'migration'` later), `occurred_at TIMESTAMPTZ
NOT NULL`, `recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()`,
`correlation_id UUID NOT NULL`, `causation_id UUID NULL`, `corrects_id
UUID NULL` self-FK (D-015 §2 fix-forward hook; nothing writes it yet).
`CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL))`.

Fact-specific columns:

- `inquiry_received`: `inquiry_id`, `person_id`, `raw_payload_id`,
  `content_hmac`, `source`, `person_created BOOL`, `matched_by TEXT NULL`
  (`'email'` | `'phone'`).
- `routing_decision`: `inquiry_id`, `person_id`, `strategy TEXT`
  (`'explicit'` | `'actor_default'` | `'kept_existing'` — plus
  `'organization_default'` | `'unassigned'`, added by SLICE_007c §3/§5
  as a declared additive change (AGENTS.md §11) for system-actor
  intake; they can surface in `POST /api/inquiries` responses only on
  `duplicate: true` replays of system-routed rows),
  `assignee_user_id UUID NULL` FK `app_user`.
- `assignment_changed`: `person_id`, `from_user_id UUID NULL`,
  `to_user_id UUID NULL` (both FK `app_user`), `reason TEXT` (`'intake'`
  | `'manual'`). On intake, `causation_id` = the `routing_decision.id`.
- `stage_changed`: `person_id`, `from_stage_id UUID NULL`, `to_stage_id
  UUID NOT NULL`, both composite-FK to `stage (id, organization_id)`,
  `reason TEXT` (`'intake'` | `'manual'`).

Indexes on each fact table: `(organization_id, person_id, occurred_at)`,
`(organization_id, correlation_id)`.

### Append-only enforcement

Two layers, both required: `crm_app` receives no UPDATE/DELETE on fact
tables (grants), **and** one trigger function `reject_mutation()` bound
`BEFORE UPDATE OR DELETE` on each of the four tables raises an exception
— so the migrator role, fixtures, and future scripts cannot mutate
history either. The trigger does not interfere with `corrects_id`
fix-forward inserts.

### Grants (`crm_app`)

`stage` SELECT; `person` SELECT, INSERT, UPDATE; `contact_method` SELECT,
INSERT; `inquiry` SELECT, INSERT; `raw_payload` SELECT, INSERT, and
**column-level** `UPDATE (resolution, unresolved_reason, resolved_at,
inquiry_id)` so nonce, ciphertext, and hash are immutable to the
application; the four fact tables SELECT, INSERT. No DELETE on any table
(erasure is deferred, D-015 §5/§7). Postgres checks column privileges
before execution, so the existing `… WHERE false` grants-test pattern
applies; `SELECT … FOR UPDATE` on `raw_payload` needs UPDATE on at least
one column, which the column grant satisfies.

### Migrations (one lane, AGENTS.md §12)

`20260821000001_stage.sql`, `…02_person_contact_method.sql`,
`…03_raw_payload.sql`, `…04_inquiry_and_facts.sql` — grants and trigger
inside each file, Slice 001 style.

## 3. Intake

### Parsing (`generic_v1`)

A JSON object with optional `first_name`, `last_name`, `email`, `phone`,
`message`, `external_id`. Not valid JSON → `invalid_json`; not an object →
`not_an_object`; no normalizable email **and** no normalizable phone →
`no_contact_method`. These are the three `UnresolvedReason` values.

### Identify (dedup) — Organization-scoped, never across Organizations

1. Take one per-Organization intake lock, keyed
   `hashtextextended('intake:' || $org_id::text, 0)`. Serializing all
   intake within an Organization is fine at 10–50 agents, and — unlike a
   per-contact-value lock — it covers the mixed case where one payload
   carries `{email, phone}` and a concurrent one carries only `{phone}`.
   **Acquired via bounded retry, not a blocking wait** (revised after
   adversarial testing found the original blocking
   `pg_advisory_xact_lock` call holds a pooled connection for the full
   duration another request holds the lock — with an unconfigured
   process-wide pool, one Organization's intake burst could exhaust it
   and 503 every *other* Organization's logins and reads; empirically
   reproduced and fixed). See "Two-phase `ReceiveInquiry`" below for the
   mechanism.
2. Look up `contact_method` by `(organization_id, 'email',
   normalized)`. If one or more Persons match, take the earliest-created
   (deterministic). Otherwise repeat by phone. Email wins when email and
   phone match *different* Persons. Otherwise create the Person.
3. On a match, `SELECT … FOR UPDATE` the matched `person` row before
   deciding routing (so a concurrent manual reassignment cannot be
   recorded as `kept_existing` with a stale assignee), then add any
   payload contact methods not already on that Person (`ON CONFLICT DO
   NOTHING`). Names are **never** overwritten on a match.

### Routing

- `assign_to_user_id` present → must be a member of the actor's
  Organization (`organization_membership` row exists; there is no status
  column — Slice 001 §2) — else `invalid_assignee`, checked **before**
  anything is stored so a rejected request leaves no `pending` row;
  strategy `explicit`.
- Absent → the actor (`actor_default`, mirroring Follow Up Boss manual
  add).
- If the matched Person **already has** an assignee → strategy
  `kept_existing`, no `assignment_changed`; the 201 body reports the
  strategy and the actual assignee so the client can show that an
  explicit choice was not applied.
- If the matched Person has **no** assignee → the routing outcome above
  applies and one `assignment_changed` from NULL is written (a repeat
  lead must not leave a Person ownerless).
- New Person → `assignment_changed` NULL → assignee, and
  `stage_changed` NULL → the Organization's position-1 stage.

### Two-phase `ReceiveInquiry`

- **Phase A** (own transaction, committed before parsing — D-015 §4
  "stored before parsing"): compute `content_hmac`, encrypt, `INSERT
  raw_payload … resolution='pending' ON CONFLICT (organization_id,
  source, content_hmac) DO NOTHING`, re-select the id. Commit.
- **Phase B**: a bounded retry loop, each iteration its own transaction.
  `SELECT … FROM raw_payload WHERE id=$1 AND organization_id=$2 FOR
  UPDATE`. If already `resolved` or `unresolved` → return the stored
  outcome with `duplicate: true`, committing without ever touching the
  advisory lock (duplicate/parse-failure short-circuits are not subject
  to lock contention). Otherwise attempt the per-Organization lock via
  `pg_try_advisory_xact_lock` (non-blocking):
  - **Acquired**: decrypt **the stored row** (not the request), parse,
    identify, then atomically: Person (+ contact methods) if new;
    `inquiry`; `inquiry_received`; `routing_decision`;
    `assignment_changed` / `stage_changed` per the routing rules;
    `UPDATE raw_payload SET resolution='resolved', resolved_at,
    inquiry_id`. Parser rejection → `UPDATE raw_payload SET
    resolution='unresolved', unresolved_reason` and commit (it is an
    outcome, not a failure). Commit and return.
  - **Not acquired**: roll back immediately — the connection is released
    back to the pool, not held while waiting — then sleep with jittered
    exponential backoff (starts ~25–37ms, doubles, capped ~250ms) and
    retry from the top of the loop. A deadline bounds total wall-clock
    time across all attempts at **3 seconds**, comfortably longer than
    any legitimate contender's Phase B (a handful of small indexed
    queries) but short enough to fail predictably instead of parking a
    connection. Past the deadline: return `CommandError::IntakeBusy`
    (§5, §9) with the `raw_payload` row untouched — still `pending` from
    Phase A, exactly as if this attempt had never happened.
  - Infrastructure error at any point → rollback; the row stays
    `pending`, same as before this mechanism existed.

### Idempotency scope

`UNIQUE (organization_id, source, content_hmac)` is the idempotency key
**of this slice's generic ingress only**. It makes a double-submitted
form a true duplicate but also collapses a prospect who legitimately
re-sends byte-identical content weeks later. Acceptable for the dev
ingress; future webhook adapters key on provider delivery IDs instead.
The New lead form **may** include a client-generated `submission_id` in
the payload so a fresh entry is a new Inquiry while a retry of the same
form instance still dedupes (implementation detail).

### Unresolved

A `raw_payload` row whose `resolution <> 'resolved'`: `unresolved`
(parser/identify rejected it; reason recorded) or `pending` (stored, but
Phase B failed: DB error, or the ciphertext cannot be decrypted with the
current key). Visible at `GET /api/intake/unresolved` and the Unresolved
screen as **metadata only** (id, source, received_at, resolution, reason,
byte_len) to every member — the queue never decrypts to the screen.

This slice ships the queue view only. Re-POSTing the same bytes re-runs
Phase B against the stored row, which recovers a transient DB failure or
a temporarily wrong key; it does **not** recover a corrupted ciphertext
row — such a row stays `pending` and visible until the next intake slice
adds a manual resolve/discard command. Stated plainly so it is not
mistaken for a general fix.

## 4. Commands and queries

Module layout under `backend/crates/crm-api/src/`:

```text
domain/mod.rs
domain/envelope.rs        CommandContext, FactEnvelope, Origin, ActorKind
domain/stage.rs           seed_defaults, list
domain/person/{model.rs, queries.rs, visibility.rs}
domain/contact.rs         normalization + identify
domain/raw_payload/{crypto.rs, store.rs}
domain/inquiry/{parse.rs, queries.rs}
domain/facts.rs           four typed insert fns
domain/commands/{receive_inquiry.rs, assign_person.rs, change_person_stage.rs}
routes/{people.rs, intake.rs, stages.rs}
```

```rust
pub struct CommandContext {
    organization_id: Uuid, actor_user_id: Uuid, origin: Origin, correlation_id: Uuid,
}
impl CommandContext {
    // origin = WebSession; fresh server-generated correlation_id (never x-request-id)
    pub fn from_auth(auth: &AuthContext) -> Self;
}

pub enum PersonVisibilityScope { Organization(Uuid) }   // D-005; the only variant
impl PersonVisibilityScope {
    pub fn from_auth(auth: &AuthContext) -> Self;
    pub fn organization_id(&self) -> Uuid;
}
// Every *Person-visibility* read — people list, person detail, that Person's
// inquiries and history — takes `&PersonVisibilityScope`, not a raw Uuid; an
// exhaustive `match` forces any future variant through every such query site.
// Stage and unresolved-queue reads take `organization_id` from `AuthContext`
// directly: they are Organization data, not Person visibility (D-005), and must
// not become team-scoped if a Team variant ever arrives.

pub struct ReceiveInquiry {
    source: Source, payload: Vec<u8>, assign_to_user_id: Option<Uuid>, received_at: DateTime<Utc>,
}
pub enum ReceiveInquiryOutcome {
    Resolved { inquiry_id, person_id, person_created: bool, routing_strategy, assigned_user_id: Option<Uuid>, duplicate: bool },
    Unresolved { raw_payload_id, reason: UnresolvedReason, duplicate: bool },
}
pub struct AssignPerson { person_id: Uuid, assigned_user_id: Option<Uuid> }  // None = unassign
pub struct ChangePersonStage { person_id: Uuid, stage_id: Uuid }
pub enum CommandError {
    PersonNotFound, InvalidAssignee, InvalidStage, NoStagesConfigured, Crypto, Database(sqlx::Error),
}
```

`assign_person` / `change_person_stage`: one transaction; load the Person
through the scope (else `PersonNotFound`); validate the target belongs to
the Organization; no-op success **without a fact** if unchanged; else
`UPDATE person` + one fact with `reason = 'manual'`.

Timestamps: `occurred_at` = `received_at` for intake facts (request
receipt time — a future webhook adapter may carry the source's own time)
and `Utc::now()` in the command for assign/stage; `recorded_at` is the DB
default. `correlation_id` is generated per command execution on the
server; the request span logs both it and the request id.

## 5. HTTP contracts

All endpoints authenticate via the existing `AuthContext`; `{"error":
code}` envelope; 401 without a valid session and 503 `unavailable` on
database failure exactly as Slice 001. Path parameters that are not UUIDs
→ 400 `malformed_request`. `ApiError` gains variants additively. CORS
method list (GET/POST/DELETE) is unchanged.

| Endpoint | Request | Success | Errors |
|---|---|---|---|
| `POST /api/inquiries` — intake; the simulated dev ingress and the manual-entry form are the same route | `{"source": "zillow", "payload": {…any JSON…}, "assign_to_user_id"?: uuid}`; JSON content type; body ≤ 256 KiB; `source` lowercased/trimmed and must match `^[a-z0-9_]{1,64}$` | **201** first delivery: `{"status":"resolved","inquiry_id","person_id","person_created":bool,"routing_strategy","assigned_user_id":uuid\|null,"duplicate":false}` or `{"status":"unresolved","raw_payload_id","reason","duplicate":false}`; **200** same shapes with `"duplicate":true` when the payload was already stored and processed | 400 `malformed_request`, 422 `invalid_assignee`, 500 `internal_error` (decrypt failure; Organization without stages), **503 `intake_busy`** (`Retry-After: 2`) when this Organization's intake lock could not be acquired within the retry budget — distinct from `unavailable`: the database is up, this Organization's intake is contended; the request was never stored as `pending` by this attempt if it was already `pending` from an earlier one, and a retry (or a client's own retry logic honoring `Retry-After`) resumes cleanly |
| `GET /api/people` | — | 200 `{"people":[{id, first_name, last_name, display_name, stage:{id,name}, assigned_user:{id,display_name}\|null, primary_email, primary_phone, inquiry_count, last_inquiry_at, created_at}], "truncated": bool}` ordered `created_at DESC, id`; cap 500 | |
| `GET /api/people/{id}` | — | 200 `{person:{…as above}, contact_methods:[{id,kind,value}] (by created_at), inquiries:[{id,source,source_external_id,message,received_at}] (received_at DESC), history:[{kind,id,occurred_at,recorded_at,actor:{id,display_name}\|null,origin,correlation_id,detail:{…per kind, below}}]}` | 404 `not_found` (identical for other Organizations' ids) |
| `POST /api/people/{id}/assignment` | `{"assigned_user_id": uuid\|null}` | 200 `{"person": {…summary}, "changed": bool}` | 404 `not_found`, 422 `invalid_assignee` (identical for nonexistent and other-Organization users) |
| `POST /api/people/{id}/stage` | `{"stage_id": uuid}` | 200 `{"person": {…summary}, "changed": bool}` | 404 `not_found`, 422 `invalid_stage` (same non-leaking behavior) |
| `GET /api/stages` | — | 200 `{"stages":[{id,name,position}]}` in position order | |
| `GET /api/intake/unresolved` | — | 200 `{"items":[{id,source,received_at,resolution,reason,byte_len}], "truncated": bool}` ordered `received_at DESC`; cap 500. *Amended by SLICE_007e (declared, AGENTS.md §11): the queue lists `resolution IN ('pending','unresolved')` — `discarded` rows (new in 007e) do not appear; shape unchanged.* | |

`primary_email` / `primary_phone` = the earliest-created contact method of
that kind, or null. `display_name` is derived server-side so both lanes
agree: `first_name` and `last_name` joined by a space (whichever are
present), else `primary_email`, else `primary_phone` (a Person always has
at least one contact method, so it is never empty).

**History entries.** `kind` is one of `inquiry_received`,
`routing_decision`, `assignment_changed`, `stage_changed` — plus
`contact_attempted` (`kind_rank` 4, `detail {channel, outcome}`), added
by SLICE_003 §5 as a declared additive change (AGENTS.md §11), and
`call_completed` (`kind_rank` 5, `detail {call_id, outcome,
talk_seconds, answered_at}`), added by SLICE_006 §2 the same way;
`contact_attempted.detail` gains `call_id`, `corrects_id`, `superseded`
by SLICE_006c §2 (a correction is a new row with `corrects_id`; in
history it is placed at its `recorded_at`, i.e. after the call).
`detail` per kind (ids always accompanied by resolved names):

- `inquiry_received`: `{inquiry_id, source, person_created, matched_by}`
- `routing_decision`: `{inquiry_id, strategy, assignee: {id, display_name} | null}`
- `assignment_changed`: `{from: {id, display_name} | null, to: {id, display_name} | null, reason}`
- `stage_changed`: `{from_stage: {id, name} | null, to_stage: {id, name}, reason}`

Ordering is `occurred_at, recorded_at, kind_rank, id`, where `kind_rank`
is the fixed sequence above (inquiry_received = 0 … stage_changed = 3),
computed in the UNION query. This is required, not cosmetic: intake's
four facts share `occurred_at` (= `received_at`) **and** `recorded_at`
(`now()` is the transaction-start time, identical for rows inserted in
one transaction), so without `kind_rank` the §1 timeline order would be
undefined. The frontend does not re-sort.

A 200 duplicate body's `assigned_user_id` is the Person's *current*
assignee. A re-POST that resolves a previously `pending` row is a first
delivery → 201, `duplicate: false`.

The 201/200 split replaces an earlier 201/202/200 scheme: `unresolved` is
a committed, terminal outcome in this slice, so 202 ("accepted for
further processing") would be wrong.

**Why an authenticated endpoint is the dev ingress** (not a binary or
script): it is the single command path (D-008), testable through the
existing `oneshot` harness, usable from the UI and through the tunnel,
and the future webhook hostname (D-016 §4) adds a second *adapter*
(signature auth, `actor_kind = 'system'`, `origin = 'webhook'`, raw body
bytes, provider delivery-id idempotency) onto the same `receive_inquiry`.

## 6. Authorization and tenant isolation

- Organization enters every query only from `AuthContext` →
  `PersonVisibilityScope` / `CommandContext`. Client-supplied Organization
  ids in query, header, or body are ignored (tested, as in Slice 001).
- Composite FKs `(person_id, organization_id)` and `(stage_id,
  organization_id)` make a cross-Organization contact method, inquiry,
  person-stage, or stage-change fact unrepresentable at the database
  level, independent of application checks. `assigned_user_id` is
  deliberately **not** composite-FK'd to `organization_membership`: that
  would make membership deletion fail while anyone is assigned and
  pre-decide O-004.
- `not_found`, `invalid_assignee`, and `invalid_stage` are byte-identical
  for nonexistent and other-Organization ids — no existence leak.
- The `raw_payload` idempotency unique key is Organization-scoped; the
  same bytes delivered to two Organizations are two rows.
- Unresolved-queue metadata is visible to every member (D-005 spirit);
  loosening or tightening that later is a privacy decision.

## 7. Encryption and content hash

- Crate `chacha20poly1305` 0.10 (RustCrypto, same family as the existing
  `argon2`/`hmac`/`sha2`), **XChaCha20-Poly1305**: 24-byte random nonce
  from `rand::rng().fill_bytes` (as `session.rs`), safe without counters
  or rotation. AAD = `organization_id.as_bytes() ‖ raw_payload.id.as_bytes()`
  so a ciphertext cannot be re-pointed to another row or Organization.
- Stored: `nonce`, `ciphertext` (tag appended), `content_hmac`,
  `byte_len`. The fact keeps `raw_payload_id` + `content_hmac` — D-015
  §4's "pointer plus content hash".
- **The content hash is keyed.** A plain SHA-256 of a small, guessable
  `{first_name, last_name, email, phone, message}` document sitting in an
  immutable row would let anyone holding the history table (backups, a
  future replica) confirm a candidate email after the Person row and blob
  are erased. `content_hmac = HMAC-SHA256(k_hash, plaintext)` with
  `k_hash = HMAC-SHA256(CRM_RAW_PAYLOAD_KEY, "crm-raw-payload-content-hash-v1")`.
  Still deterministic (the idempotency unique key works), still an
  integrity binding for the key holder, same rotation constraint as the
  ciphertext. Recorded here as the D-015 §4 hash algorithm.
- Config: `CRM_RAW_PAYLOAD_KEY` in `.env.example`, exactly 64 hex chars →
  `RawPayloadKey([u8; 32])` with a redacted `Debug` like `SessionSecret`;
  the API refuses to start if it is missing, the wrong length, or not
  hex. Added to `AppState`. No key-version column now; rotation later adds
  one defaulting to 1.
- Plaintext bytes for this endpoint = `serde_json::to_vec(&payload)`.
  `serde_json::Value` maps are `BTreeMap` unless `preserve_order` is
  enabled, so serialization is key-sorted and the hash is canonical. (
  `indexmap` itself is present in `backend/Cargo.lock` — pulled in by
  `sqlx-core`, unrelated to this slice — but `serde_json` has no edge to
  it and no crate in the workspace enables `preserve_order`; confirmed
  with `cargo tree -e features -i serde_json`.) A unit test pins the
  sorted-key assumption (§13) because a future dependency enabling
  `preserve_order` through feature unification would silently change
  every idempotency key. The webhook adapter will hash raw body bytes
  instead.

## 8. Observability

`#[tracing::instrument(skip_all, fields(organization_id, actor_id,
correlation_id, source, raw_payload_id, outcome))]` on the three commands;
the request span is unchanged. Each command is a thin wrapper around a
private implementation: the wrapper is the single point that records
`outcome` on every return, success or failure, so every error path is
observable — `outcome` is `"resolved"` / `"unresolved"` / `"duplicate"`
on success or `error_kind`'s value (`CommandError`'s variant name, e.g.
`database`, `crypto`, `no_stages_configured`, `intake_busy`) on failure,
logged via `tracing::warn!`. **Never logged**: payload bytes or
plaintext, names, emails, phones, `message`, the key, ciphertext, or the
content hash — `CommandError::Database`'s inner `sqlx::Error` is
identified by variant name only, never its `Display`/`Debug` text.
`ParsedLead` and payload types get a hand-written `Debug` that prints
only `has_email` / `has_phone`.

## 9. Failure behavior

- Database down → 503 `unavailable` on every new endpoint; `/api/health`
  stays 200.
- Phase A committed, Phase B fails → 503; the row stays `pending`,
  visible in the queue; re-POST retries idempotently (`FOR UPDATE` on the
  stored row).
- Decrypt failure (wrong key, tampered ciphertext) → 500
  `internal_error`, row stays `pending`, logged with ids only.
- Organization without stages → 500 `internal_error` (misconfiguration;
  seed and fixtures always create them).
- Two identical simultaneous deliveries → one row; the second sees
  `resolved` under the row lock → `duplicate: true`.
- Two different first-contacts sharing a contact value concurrently → the
  per-Organization advisory lock serializes them → one Person.
- One Organization's intake lock held past the 3-second retry budget
  (§3) → 503 `intake_busy`, `Retry-After: 2`; the row stays `pending`
  (or is created `pending` by this attempt) either way, exactly as any
  other Phase B failure; a retry resumes cleanly. Empirically verified
  this does **not** propagate to other Organizations: a burst against an
  unrelated Organization during a 7-second external hold of the busy
  Organization's lock completed in ~500ms, unaffected.
- Invalid assignee → 422 and **no** `raw_payload` row.
- Invalid or missing `CRM_RAW_PAYLOAD_KEY` → process refuses to start.

## 10. Frontend (D-017)

**Visual style is governed by `docs/design/UI_STYLE.md`** (accepted
2026-08-21 from user-supplied reference screens). The frontend lane
implements its tokens as the Tailwind v4 `@theme` and its control specs
as the PrimeVue `pt` objects and `DataTable.vue`; a screen that deviates
from it is a review finding. Summary: white hairline-bordered cards on a
very light gray page, one deep-indigo accent for primary actions,
sentence-case muted table headers, 56 px rows, Inter self-hosted, Lucide
outline icons, no shadows except floating surfaces, no dark mode yet.

Packages (pin at implementation; `pnpm install` updates the lockfile,
`bootstrap` keeps `--frozen-lockfile`): `tailwindcss` + `@tailwindcss/vite`
(v4, `@import "tailwindcss"` in `src/style.css`); `primevue` v4 with
`app.use(PrimeVue, { unstyled: true })`, used sparingly (`Select` for
stage/assignee, `Button`, `ToggleSwitch`) with local `pt` objects;
`@tanstack/vue-table` v8; `@tanstack/vue-query` v5; `vue-router` (current
major — five routes and a deep link to a Person justify it);
`@fontsource-variable/inter`; `lucide-vue-next`.

Layout under `web/src/`:

```text
main.ts                    PrimeVue, VueQueryPlugin, router
router.ts                  /login, /people, /people/:id, /intake/unresolved, /intake/new
api/client.ts              resolveApiBaseUrl (moved from App.vue); apiFetch<T> with
                           credentials:'include'; throws ApiError { status, code }
api/types.ts               §5 shapes
api/queries.ts             key factory ['org', orgId, 'people' | ['person', id] | 'stages'
                           | 'unresolved' | 'members']; useMe(); useQuery hooks;
                           useMutation hooks invalidating ['org', orgId]
components/AppShell.vue
components/DataTable.vue   TanStack Table wrapper
views/{LoginView, PeopleView, PersonDetailView, UnresolvedView, NewInquiryView}.vue
App.vue                    shell + <RouterView/>
```

Screens, per `UI_STYLE.md` §6–7: **People** — page header with "New
lead" as the primary action, one card containing the table (name over
primary email, stage badge, assignee, inquiry count, last inquiry, row
click → detail). **Person detail** — identity header card (display name,
stage `Select`, assignee `Select`, both mutating inline with a receipt),
then "Contact methods", "Inquiries", and "History" cards. **New lead** —
a card stack: source, name pair, email, phone, message, assignee;
primary "Add lead" right-aligned. **Unresolved** — one table card
(source, received, resolution badge, reason, size) with an empty state.
**Login** — a single centered card. The sidebar groups are `Work`
(People) and `Intake` (New lead, Unresolved); the user menu with logout
sits at the sidebar's bottom.

Router `beforeEach` gates on the `me` query (401 → `/login`); logout
calls `queryClient.clear()`. The raw `fetch` + `ref` pattern in the
current `App.vue` is fully replaced. The gate (lint, typecheck, build) is
unchanged in shape. No Organization-wide inquiry list: the Person detail
already shows that Person's inquiries and nothing on the thesis §16 chain
needs more (this cuts a `GET /api/inquiries` endpoint and view from the
planner draft).

## 11. sqlx offline mode

- **New** application code uses `query!` / `query_as!`; Slice 001's
  runtime `sqlx::query` calls are left alone (converting them is
  unrelated refactoring, AGENTS.md §13). Test fixtures in `tests/` keep
  runtime queries so `prepare` need not compile test targets.
- A **repo-root** `.cargo/config.toml` sets `[env] SQLX_OFFLINE = "true"`
  for every cargo invocation. Root, not `backend/`: cargo discovers
  `.cargo/config.toml` by walking up from the *current working
  directory*, not from `--manifest-path`, and `scripts/dev-api`,
  `dev-seed`, and `db-migrate` run `cargo` from the repo root while
  `check`/`check-db` run from `backend/`; the root file covers both (and
  rust-analyzer). This is not optional: sqlx-macros-core 0.8.6 falls back
  to `dotenvy::dotenv_iter()`, which walks up from the working directory,
  so `dev-api`'s `cargo run` would otherwise find `/…/crm/.env` and
  compile online against the drifting dev database. `scripts/sqlx-prepare`
  and the `check-db` prepare step override with a process-level
  `SQLX_OFFLINE=false`, which takes precedence over the config `[env]`.
- New `scripts/sqlx-prepare`: sources `.env`, derives a throwaway URL from
  `MIGRATION_DATABASE_URL` (database `crm_sqlx_prepare`), then `cargo sqlx
  database create` → **`cargo sqlx migrate run --source
  crates/crm-api/migrations`** (the CLI migrator, which needs no
  compilation — running the Rust `migrate` binary here would be circular,
  since building it compiles the lib's new `query!` macros, which at that
  moment can resolve neither online (empty DB) nor offline (cache not yet
  written); the CLI shares the file format and `_sqlx_migrations`
  bookkeeping with `sqlx::migrate!` 0.8, and the DB is dropped afterwards
  so no cross-tool state survives) → `DATABASE_URL=<tmp> SQLX_OFFLINE=false
  cargo sqlx prepare --workspace` (writes `backend/.sqlx/`, committed;
  contains only query text and column types) → `cargo sqlx database drop
  -y`. Never prepares against the dev database. `scripts/db-migrate` keeps
  using the Rust binary.
- `scripts/check` gains nothing new once `.sqlx/` is committed:
  `SQLX_OFFLINE` comes from the cargo config, so a missing or stale cache
  entry fails the gate. `check` stays free of sqlx-cli.
- `scripts/check-db` gains a first step running the same throwaway-DB
  flow with `SQLX_OFFLINE=false cargo sqlx prepare --check --workspace`
  (catches schema and nullability/type drift that an offline compile
  cannot); the subsequent `cargo test … -- --ignored` stays offline.
- A query change requires re-running `sqlx-prepare` before the next
  build; the README Development section says so.
- **sqlx-cli is pinned to the locked crate minor**: `cargo install
  sqlx-cli --version 0.8.6 --locked --no-default-features --features
  postgres,rustls`. The machine currently has 0.9.0; `scripts/sqlx-prepare`
  asserts the CLI minor equals the workspace's locked `sqlx` minor and
  stops otherwise. The workspace stays on 0.8 — bumping to 0.9 is an
  unrelated dependency change for a separate review. `bootstrap` checks
  the CLI **version**, not just presence (non-fatal warning).
- **Contract change, recorded per AGENTS.md §11**: Slice 001 §2 said
  sqlx-cli is not a bootstrap requirement. After this slice it is
  required for `scripts/sqlx-prepare` and `scripts/check-db` (not for
  `check`, `dev-api`, or `bootstrap`).

## 12. Explicit exclusions

- Manual resolve/discard of unresolved or stuck-`pending` payloads (next
  intake slice).
- Person name / contact-method editing endpoints and UI (ordinary CRUD,
  additive later).
- Erasure/redaction, any DELETE grant, retention TTL (D-015 §5/§7 —
  runbook first).
- Real webhook hostname and signature verification (D-016 §4); provider
  adapters; routing rules/round-robin; stage administration (D-019).
- Pagination (the `truncated` flag is the only concession); key
  rotation; `contracts/` directory (this document §5 is the contract of
  record).
- Today, realtime, Operator (Slices 003/004).

## 13. Checks and tests

### Acceptance criteria

1. Fresh database: migrations apply; `crm_app` grants are exactly §2,
   tested table by table, including both sides of the column grant on
   `raw_payload` (`UPDATE … SET resolution = resolution WHERE false`
   succeeds; `SET ciphertext = ciphertext WHERE false` is denied).
2. As `crm_app` (grant) **and** as `crm_migrator` (trigger), UPDATE and
   DELETE on each fact table fail. The trigger half must target an
   **existing** fact row (insert one via intake or fixture first): a
   `FOR EACH ROW` trigger never fires on the zero-row `… WHERE false`
   pattern criterion 1 uses, so copying that pattern would pass
   vacuously.
3. Intake of a new lead creates Person, contact methods, Inquiry, a
   `resolved` raw_payload, and four facts sharing one `correlation_id`,
   with `assignment_changed.causation_id` = the routing decision and
   `stage_changed.to_stage_id` = the Organization's position-1 stage.
4. Intake matching an existing **assigned** Person by email (and,
   separately, by phone) creates no Person, no stage or assignment fact,
   preserves the first Inquiry's source, and records
   `routing_decision.strategy = 'kept_existing'` with the 201 body
   showing the real assignee. Matching an existing **unassigned** Person
   writes exactly one `assignment_changed` from NULL.
5. When email and phone match different Persons, email wins.
6. Payload with no contact method → 201 `unresolved` with reason; zero
   Person/Inquiry/fact rows; the ciphertext does not contain the email
   bytes.
7. Identical payload twice → one raw_payload, one Inquiry, one set of
   facts; second response 200 with `duplicate: true`.
8. Cross-Organization: the same email in A and B yields two Persons; B
   gets 404 on A's Person for GET/assignment/stage; A using B's stage or
   member gets 422 and writes no fact; `GET /api/people`, `GET
   /api/stages`, and `GET /api/intake/unresolved` are each
   Organization-scoped; client-supplied Organization ids are ignored.
9. `AssignPerson` / `ChangePersonStage` write exactly one fact when
   changed and none when unchanged; detail history ordering is stable.
10. A `pending` row (fixture-inserted, encrypted with the test key)
    resolves on re-POST of the same bytes with `duplicate: false`.
11. Wrong key / tampered ciphertext → 500; the row remains `pending` and
    appears in the queue.
12. Invalid assignee → 422 and no `raw_payload` row.
13. API refuses to start on a missing/short/non-hex
    `CRM_RAW_PAYLOAD_KEY`; `Debug` redacts it.
14. Seed run twice: nine stages per Organization in D-019 order, two
    members per Organization, no duplicates.
15. `./scripts/check` green with no services; `./scripts/check-db` green
    including `prepare --check`.
16. Manual: the §1 walkthrough locally and through the tunnel.
17. Concurrency: two concurrent intakes for the same new email → one
    Person; two concurrent identical deliveries → one Inquiry, one 201
    and one 200.
18. An explicit `assign_to_user_id` on an already-assigned Person →
    `kept_existing`, no fact, and the 201 body reports the existing
    assignee, not the requested one.
19. `GET /api/people` and `GET /api/intake/unresolved` return
    `truncated: true` when more than 500 rows exist and `false`
    otherwise.

### Required tests — service-free (main gate)

Config parsing for the key (missing, 63/65 chars, non-hex, redaction);
seal/open round-trip, tamper, wrong AAD, distinct nonces; `content_hmac`
determinism and key-dependence; a test that `serde_json::to_vec` of a
`Value` built with reverse-ordered keys yields sorted output (guards the
hashing contract against `preserve_order`); parser and normalization
tables (email case/whitespace, 10/11-digit US phones, garbage); each
`UnresolvedReason`; `PersonVisibilityScope::organization_id`; 401 without
a cookie on all seven endpoints; 400 on malformed/oversized/non-JSON
bodies, bad `source`, and non-UUID path params; 503 per endpoint against a
closed loopback port; redacted `Debug` of `ParsedLead`; existing Slice
000/001 tests still pass (they need the new config variable — see §14a).

### Required tests — DB-backed (`check-db`)

Criteria 1–14 and 17–19. The two races in criterion 17 use
`tokio::join!` on two `oneshot`s over the default pool (max 10
connections). Criterion 10's fixture needs `raw_payload::crypto` to
expose seal + content-HMAC as `pub`.

### Manual

Criterion 16.

## 14. Safe defaults adopted (overridable at approval)

1. **Dedup policy**: Organization-scoped exact match on normalized email,
   then phone; ambiguous → earliest Person; matched Person gains new
   contact methods, names untouched; stage unchanged on a repeat inquiry;
   assignment unchanged if one exists, otherwise assigned per routing.
   Mirrors Follow Up Boss; affects only future intakes; cheap to change.
2. Unresolved queue shows metadata only, to all members.
3. Facts reference erasable rows by bare UUID (consequence of D-015, §2).
4. Keyed content hash (HMAC-SHA256 with a derived key) instead of plain
   SHA-256 (§7).
5. Content-hash idempotency is the generic dev ingress's key only; webhook
   adapters will use provider delivery ids (§3).
6. 201 for any first delivery, 200 for a duplicate (§5).
7. Per-Organization intake advisory lock (§3).
8. Session-authenticated intake records the actor-claimed `source` as-is
   with `origin = 'web_session'`; `source` is a free-form validated
   string, not a closed list.
9. Default assignee is the actor.
10. Seed gains a second member per Organization (changes Slice 001's
    "at least one User per Organization" statement; recorded here).
11. `inquiry.message` is plaintext in the erasable CRUD set, truncated to
    4 KiB; the encrypted raw payload keeps the full text.
12. XChaCha20-Poly1305 with a 64-hex-char key; 256 KiB body limit;
    500-row list caps with a `truncated` flag; no retention TTL in dev.
13. sqlx-cli pinned to 0.8.6; workspace stays on sqlx 0.8;
    `SQLX_OFFLINE` defaulted through `backend/.cargo/config.toml`.
14. Vue Router; no Organization-wide inquiry list this slice.

Implementation details (not decisions): composite FK mechanics,
`hashtextextended` vs `hashtext`, `primary_email`/`primary_phone`
definition, the optional client `submission_id`, `AppState` test
constructor, index choices, `updated_at` maintenance.

## 14a. Implementation notes (from independent review)

- `tests/session.rs` and `tests/health.rs` build `Config::from_source`
  with only `CRM_SESSION_SECRET`, and `tests/db_identity.rs::build_router`
  constructs `AppState` by struct literal; all three need
  `CRM_RAW_PAYLOAD_KEY` / the new field. Prefer an
  `AppState::for_tests(pool, &config)`-style constructor so future fields
  stop touching every test file.
- `hashtextextended('intake:' || org_id::text, 0)` — the `::text` cast
  is required; hash collisions only cause extra serialization, never
  incorrectness.
- `INSERT … ON CONFLICT DO NOTHING` needs only INSERT; `RETURNING` needs
  SELECT (granted). On the conflict path the freshly encrypted copy is
  discarded and Phase B decrypts the stored row under the stored id —
  consistent with the AAD construction.
- Add `chacha20poly1305` to `backend/Cargo.toml`; no other new backend
  dependencies are expected.
- Lane B's step 2 (screens) must not start until §5 is frozen by approval
  of this document.

## 15. Lane ownership and sequencing

Two worktrees after a short sequential head, contracts frozen by §5 of
this approved document; any §5 change goes through AGENTS.md §11.

- **Lane A — backend** (owns `backend/**`, `scripts/**`, `.env.example`,
  `README.md` Development section, and **all migrations**):
  1. migrations + grants + trigger + `stage::seed_defaults`;
  2. root `.cargo/config.toml`, sqlx-cli pin check, `scripts/sqlx-prepare`,
     `check-db` prepare step — **before any `query!` macro is written**,
     otherwise every build of new macro code goes online via the `.env`
     walk against the drifting dev database, the very hazard this slice
     removes;
  3. `CRM_RAW_PAYLOAD_KEY` config + crypto + content HMAC;
  4. envelope, facts, `PersonVisibilityScope`, the three commands, with
     service-free tests (re-run `sqlx-prepare` as queries are added;
     commit `.sqlx/`);
  5. endpoints;
  6. DB-backed suite including the two races;
  7. seed extensions (stages, second member).
- **Lane B — frontend** (owns `web/**`):
  1. migrate the existing login/me/members shell onto Tailwind /
     PrimeVue-unstyled / TanStack Query / Vue Router — no backend
     dependency, can start immediately;
  2. screens against §5, started only after approval, run locally
     against Lane A's branch once endpoints land; merges after Lane A.
- **Coordinator** owns `docs/**` and `PROJECT_STATE.md`.

No file overlap exists between lanes. Sequential execution would cost
roughly two days for no safety gain.

Effort: backend 3–4 focused days, frontend 1.5–2, integration and review
1. Risk concentrates in `receive_inquiry` (two-phase idempotency,
unresolved semantics, the two concurrency tests), the sqlx-cli/crate
version discipline, and PrimeVue-unstyled + Tailwind v4 setup churn.
Review attention goes to `receive_inquiry`, the grants/trigger tests, and
the tenant-isolation suite.

## 16. Likely Slice 003

Today + realtime on this substrate: a deterministic, explainable Today
read model (D-010) over Persons/Inquiries/assignment; Centrifugo
application integration (token minting, per-Organization channels,
Person-invalidation events) with TanStack Query cache invalidation on the
web (D-017 §5) and reconnect recovery by refetch (D-011); the first
realtime reconnect/recovery tests. Manual resolve/discard for the
unresolved queue may ride along if small. Operator retrieval is Slice 004.
