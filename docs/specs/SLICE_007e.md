# Slice 007e — Unresolved workbench (detail, Try again, Discard)

Status: APPROVED (user, 2026-08-25; planner pass + independent review
same day, reviewer verdict "ready with amendments" — all
documentation-level, no blocking findings, all six amendments applied;
§5 contracts and §13 safe defaults accepted as written, including:
rescue routes per D-035 not to the clicking admin, discarded bytes
never resurrect, discard retains ciphertext until O-013. Reviewer
verified no fact-table constraint rejects a system-actor fact with
`on_behalf_of_user_id` set or `origin='web_session'`.)
Ladder: docs/plans/SLICE_007_LADDER.md rung e. Builds on 007d (pinned
email format; merged `7bc88b7`). Targets **D-037** (raw unresolved
content is org-admin-only, on demand), D-015 §4 (visible queue, never
silently unlinked; deletable-by-design), D-012 (raw preserved, parsing
re-runnable), D-035 (unattended routing — a rescued lead routes per the
org default, NOT to the admin who clicked), O-012 (parked; nothing here
may preclude the per-Person-key upgrade), SLICE_002 §12 (the deferred
detail/retry/discard), AGENTS.md §9 (never log content), §11 (contract
discipline).

## 1. User-visible outcome

An Organization admin opens an Unresolved row and sees, decrypted **on
demand**, what actually arrived — for an email row: subject, from
(display name + address), the mail's own Date header, and the text
body; for a generic JSON row: the pretty-printed JSON. Content is
never in the list response and never in any log. Two actions:

- **Try again** re-runs Phase B on the stored bytes. This is also the
  manual rescue for stuck `pending` rows (SLICE_002's crash window and
  007d's lock-deferred rows). A row whose bytes still don't parse
  fails again honestly, same reason.
- **Discard** flips the row to `resolution='discarded'`, recording
  which admin and when on the row itself; it leaves everyone's queue
  in realtime. Discard is not deletion: the ciphertext is retained
  (erasure waits on the D-015 §7 runbook / O-013).

Members keep exactly today's metadata-only table.

Live walkthrough: (1) deliver `plain.eml` → "Unrecognized email
format"; as alice (admin) open the row → subject/from/date/body
render; as a member, the row is not clickable and the content endpoint
403s. (2) Try again → still unresolved, same reason. (3) Hold the
org's intake advisory lock in `psql` — `SELECT
pg_advisory_lock(hashtextextended('intake:' || '<org-uuid>', 0));`
(session-level; same keyspace as the code's
`pg_try_advisory_xact_lock`, the `db_intake.rs` held-lock pattern) —
deliver `cypress_bay_contact.eml` → the row appears Pending (007d
DeferredPending); release the lock (`pg_advisory_unlock` or close the
session), Try again → the Person appears on the default assignee's
Today and the row leaves the queue live — the stuck-`pending` rescue
end-to-end. (4) Deliver
`cypress_bay_forged_sender.eml`, open, Discard (confirm) → the row
disappears from the queue in realtime; the DB row carries
`discarded_by_user_id`/`discarded_at`. (5) Re-deliver step 4's exact
bytes → 200 accepted, the row stays discarded, nothing resurrects.

## 2. In / out of scope

In: one migration (resolution CHECK widening + discard columns +
grants), `unresolved_detail` / `retry_intake` / `discard_raw_payload`
domain functions, `GET /api/intake/unresolved/{id}` + `POST
…/{id}/retry` + `POST …/{id}/discard` (all admin-only per D-037), the
queue-filter change (discarded rows leave `list_unresolved` — a
declared amendment to SLICE_002 §5's queue definition), the
`duplicate_outcome` `discarded` arm (without it, redelivery of
discarded bytes hits an `unreachable!` — a handler panic), the small
declared `IntakeActor::System` / `FactEnvelope::for_system` extension
(`on_behalf_of_user_id`), the web detail dialog + actions, tests
including tracing capture.

Out: LLM extraction and any background/auto-retry worker (007f owns
background rescue); bulk retry/discard; un-discard; content deletion,
DELETE grants, or erasure (O-013; D-015 §7 runbook first — if
discard-should-shred is wanted, that is a separate decision, rejected
here as out of scope); O-012 key migration; token rotation (007g); new
formats (007h); Operator awareness; any change to the frozen
`/inbound/email` or `POST /api/inquiries` envelopes (the latter's
`duplicate_outcome` path is extended additively only).

## 3. Persistence

One migration, `20260830000001_raw_payload_discard.sql`, owned by
Lane A:

```sql
ALTER TABLE raw_payload
    ADD COLUMN discarded_by_user_id UUID NULL REFERENCES app_user (id),
    ADD COLUMN discarded_at TIMESTAMPTZ NULL;
ALTER TABLE raw_payload DROP CONSTRAINT raw_payload_resolution_check;
ALTER TABLE raw_payload ADD CONSTRAINT raw_payload_resolution_check
    CHECK (resolution IN ('pending', 'resolved', 'unresolved', 'discarded'));
ALTER TABLE raw_payload ADD CONSTRAINT raw_payload_discard_fields_check
    CHECK ((resolution = 'discarded') = (discarded_at IS NOT NULL)
       AND (resolution = 'discarded') = (discarded_by_user_id IS NOT NULL));
GRANT UPDATE (discarded_by_user_id, discarded_at) ON raw_payload TO crm_app;
```

- The current CHECK is the inline constraint in
  `20260821000003_raw_payload.sql` (auto-named
  `raw_payload_resolution_check`; verify with `\d raw_payload` before
  relying on the DROP — the 007c precedent). DROP+ADD is the
  established widening pattern (`20260829000001`). The pair-CHECK
  mirrors the fact tables' `(actor_kind='user') = (actor_user_id IS
  NOT NULL)` discipline. Discard is admin-only and human-only, so
  `discarded_by_user_id` is a plain user FK.
- `crm_app`'s existing UPDATE grant covers `(resolution,
  unresolved_reason, resolved_at, inquiry_id)`; the two new columns are
  the entire new write surface. `ciphertext` stays untouchable.
- **Queue**: `list_unresolved` currently filters `resolution <>
  'resolved'` and would keep listing discarded rows — it changes to
  `resolution IN ('pending', 'unresolved')`. Declared amendment to
  SLICE_002 §5's queue definition (pointer line added there at
  approval). The web's `UnresolvedResolution` union is already exactly
  `'pending' | 'unresolved'` — no type change.
- **Redelivery of discarded bytes stays discarded** (safe default,
  veto-able): `insert_pending`'s ON CONFLICT re-selects the discarded
  row; `complete_intake` sees non-`pending` and calls
  `duplicate_outcome`, which gains a `"discarded"` arm returning the
  stored reason as `Unresolved { duplicate: true }` — `/inbound/email`
  maps that to `Duplicate` (200 accepted, no publish, no
  resurrection); a `/api/inquiries` replay gets the existing
  `{"status":"unresolved","duplicate":true}` shape. A row discarded
  while still `pending` (a 007d deferral or crash-window row) has
  `unresolved_reason NULL` — the arm uses the existing
  `no_contact_method` fallback decode, same as the unresolved arm
  (`/inbound/email` drops the reason anyway). An admin's
  explicit decision is not silently undone by a re-send; the
  byte-identical-new-lead case is the already-accepted hmac-key
  limitation. Without this arm the current code panics
  (`unreachable!` on unknown resolutions) — criterion 13 pins no-panic.
- **Discarded rows are retained, hidden PII** — stated explicitly, not
  silent: no UI lists them and nothing deletes them until the erasure
  runbook/O-013. D-015 §4's "never silently unlinked" is honored: the
  discard is explicit, attributed, and timestamped on the row.

## 4. Domain

New `domain/intake/workbench.rs` (admin commands with a
`CommandContext` that orchestrate `complete_intake`):

```rust
pub struct UnresolvedDetail {
    pub id: Uuid, pub source: String, pub payload_format: String,
    pub received_at: DateTime<Utc>, pub resolution: String,
    pub unresolved_reason: Option<String>, pub byte_len: i32,
    pub content: UnresolvedContent,   // redacted Debug (ParsedMail precedent)
}
pub enum UnresolvedContent {
    Email { subject: Option<String>, from_display: Option<String>,
            from_addr: Option<String>, date: Option<DateTime<Utc>>,
            text: Option<String>, truncated: bool },
    Text  { text: String, truncated: bool },
}
pub async fn unresolved_detail(pool, key, ctx: &CommandContext, id: Uuid)
    -> Result<UnresolvedDetail, WorkbenchError>;
pub async fn retry_intake(pool, key, publisher, ctx: &CommandContext, id: Uuid)
    -> Result<ReceiveInquiryOutcome, WorkbenchError>;
pub async fn discard_raw_payload(pool, publisher, ctx: &CommandContext, id: Uuid)
    -> Result<DiscardOutcome, WorkbenchError>;   // Discarded | AlreadyDiscarded
```

**Detail** (decrypt on demand): a new org-scoped store read (the
existing `LockedRawPayload` lacks `payload_format` / `received_at` /
`byte_len`; `payload_format` is written today but read nowhere — this
rung is its first reader, as 007b §3 predicted). Only `pending` and
`unresolved` rows are visible; `resolved`/`discarded` → 404
byte-identical with nonexistent. Decrypt via `crypto::open` (the O-012
chokepoint — nothing else touches ciphertext). Per format:
`rfc822_v1` → `mime::parse` → `ParsedMail` (this rung is the first
consumer of its `from_display` and `date` fields); an unparseable
email row falls back to `Text` with `String::from_utf8_lossy`.
`generic_v1` → `serde_json::to_string_pretty`, lossy-raw fallback for
`invalid_json` rows. Unknown `payload_format` → lossy-raw `Text`
(fail-open for display only; retry fails closed, below). Every text
field capped at **64 KiB** with `truncated: true` (raw can be
~1.5 MiB; the workbench needs enough to decide, not the whole blob).
Decrypt failure (a corrupted row) → a distinct error the UI renders as
"cannot be decrypted; Try again will not help; Discard is the remedy"
— not a silent 500 collapse (the HTTP status is still 500
`internal_error`; the distinction is UI copy on that path).

**Try again** — two steps, `complete_intake` unchanged:

1. Own transaction: `SELECT … FOR UPDATE` org-scoped. `resolved` →
   return the stored outcome (`duplicate: true` — the graceful
   two-admin race); `discarded` → 409. Otherwise `UPDATE … SET
   resolution='pending', unresolved_reason=NULL, resolved_at=NULL`
   (existing column grant) and commit. A stuck-`pending` row matches
   as a no-op — same path.
2. Call `complete_intake` with a parse closure reconstructed from the
   row's `payload_format`: `rfc822_v1` → **the same closure the
   delivery path uses** — extracted into a named `pub(crate)` fn in
   `domain/intake/email/` so retry and delivery cannot drift;
   `generic_v1` → `parse::parse` + `Source::parse(&row.source)` (the
   row's stored source, re-validated — `Corrupt` if it no longer
   parses); unknown format → fail closed (`Corrupt` → 500).

Concurrency composes: the step-1 row lock and `complete_intake`'s own
per-iteration `lock_for_processing` serialize retry-vs-retry and
retry-vs-redelivery (whichever loses sees `resolved` →
`duplicate_outcome`); the per-org advisory lock still guards
identify/create with the shared `ADVISORY_LOCK_BUDGET`. Two accepted
interleavings, stated: (i) a discard committing between step 1 and
step 2 makes the retry surface the discarded arm as 200
`{"status":"unresolved","duplicate":true}` rather than step 1's 409 —
benign, the discard's own publish still updates the queue; (ii) after
the reset, a concurrent byte-identical `POST /api/inquiries` replay
can win the `pending` row and process it under the POSTing *user's*
actor (routing `actor_default`, pre-existing SLICE_002 pending-row
semantics) — "the rescue routes per D-035" holds when the retry's own
`complete_intake` wins, which is the only case outside a deliberate
race. Still exactly one Person/Inquiry either way. Failure
behavior: parse fails again → same reason re-recorded, `resolved_at`
updated, one `intake_unresolved_changed` (all already inside
`complete_intake`). `IntakeBusy` → **503 `intake_busy` +
`Retry-After: 2` to the admin** (an interactive caller — deliberately
unlike `/inbound/email`); the row stays `pending` with its reason
cleared (the same "Pending, reason —" queue presentation as 007d's
deferral). Crypto failure → 500, row stays `pending` (crash-window
rule; UI directs to Discard). Two verification-pass hardenings
(adversarial findings, folded in at implementation): every post-reset
error path publishes one ids-only `intake_unresolved_changed` (the
reset committed; without an event all connected clients keep the stale
"Unresolved" row) and the web retry mutation also invalidates the
queue on error; and unretryable rows (unknown `payload_format`, a
stored source that no longer validates) fail closed **before** the
reset, so the stored diagnostic reason survives a retry that could
never succeed.

**Actor semantics** (safe default, and one consequence to be aware
of): the retry runs as **`IntakeActor::System`** with a fresh
correlation id and `origin = web_session` — the rescue completes
*unattended* intake, so the lead routes per **D-035** to the org
default assignee (or unassigned), **not to the admin who clicked**.
(`IntakeActor::User` would silently assign every rescued lead to the
clicking admin via `actor_default` — wrong.) For auditability the
admin is recorded in **`FactEnvelope.on_behalf_of_user_id`** — the
field D-015 §2 created for exactly this, written `None` everywhere
today. Declared additive extension (AGENTS.md §11):
`IntakeActor::System` gains `on_behalf_of_user_id: Option<Uuid>` and
`FactEnvelope::for_system` the matching parameter; every existing
caller passes `None`, behavior unchanged (SLICE_007c §4's signature
amended by this spec's approval).

**Discard** — one transaction: `FOR UPDATE` org-scoped;
`pending`/`unresolved` → set `discarded` + attribution; already
`discarded` → idempotent 200; `resolved` → 409. **No fact row** (the
ladder puts attribution on the row; discard is not deletion, so
D-015 §5's "deletion is recorded" is not yet triggered). One
`intake_unresolved_changed` after commit (ids-only, fresh correlation
id) so the row leaves every member's queue live.

## 5. HTTP contract (frozen at approval; the inter-lane contract)

All under the CORS'd `/api` router in `routes/intake.rs`;
`OrgAdminContext` (D-037); non-UUID path id → 400 `malformed_request`;
every query binds `(id, active_organization_id)` so cross-org ids 404
byte-identically.

| Endpoint | Success | Errors |
|---|---|---|
| `GET /api/intake/unresolved/{id}` | 200 `{id, source, payload_format, received_at, resolution, reason, byte_len, content}` where `content` = `{"kind":"email", subject, from_display, from_addr, date, text, truncated}` \| `{"kind":"text", text, truncated}` (all content fields nullable except `kind`/`truncated`) | 401 `unauthenticated`; 403 `forbidden` (member); 404 `not_found` (unknown / cross-org / `resolved` / `discarded`, byte-identical); 500 `internal_error` (decrypt failure); 503 `unavailable` |
| `POST /api/intake/unresolved/{id}/retry` (empty body) | 200 `{"status":"resolved", inquiry_id, person_id, person_created, routing_strategy, assigned_user_id, duplicate}` or `{"status":"unresolved", reason, duplicate}` — the SLICE_002 §5 vocabulary reused; `duplicate: true` when another writer resolved it first | 401/403; **404 = unknown/cross-org only** (a `resolved` row → 200 duplicate; a `discarded` row → 409); 409 `discarded`; 503 `intake_busy` + `Retry-After: 2` (row stays `pending`); 500 `internal_error` (decrypt); 503 `unavailable` |
| `POST /api/intake/unresolved/{id}/discard` (empty body) | 200 `{"status":"discarded"}` (idempotent on repeat; attribution unchanged — first writer wins) | 401/403; **404 = unknown/cross-org only**; 409 `already_resolved` (`resolved` rows); 503 `unavailable` |
| `GET /api/intake/unresolved` | shape unchanged; discarded rows no longer appear (declared SLICE_002 §5 queue amendment) | unchanged |

`ApiError` gains two additive variants: `Discarded` → 409
`{"error":"discarded"}`, `AlreadyResolved` → 409
`{"error":"already_resolved"}`.

## 6. Realtime

- Discard publishes `intake_unresolved_changed` (required; nothing
  else fires).
- A successful retry publishes `person_changed{inquiry_received}` via
  `complete_intake` — and the web client **already invalidates the
  unresolved queue on that event** (`web/src/realtime/events.ts`), so
  connected members drop the row live with no extra publish. This
  corrects SLICE_007d §4e/§12(m)'s "stale until refetch" caveat, which
  the client code contradicts (note added to SLICE_007d at approval;
  Lane A also fixes the same stale claim in `receive.rs`'s
  DeferredPending comment, which it owns and touches anyway).
- A failed retry publishes `intake_unresolved_changed` (inside
  `complete_intake`, unchanged).
- The acting admin's own client additionally invalidates on mutation
  success (TanStack convention; resolved retries also invalidate
  people/today).

## 7. Web (Lane B)

`UnresolvedView.vue`: rows become clickable **for admins only**
(`me.organization.role === 'admin'`, the established gate); the member
rendering stays byte-identical to today. Detail surface: a **dialog**
(the repo's `ConfirmDialog`/`LogContactDialog` precedent; no drawer
exists), fetching `GET …/{id}` only on open — content never
prefetched, never cached beyond the dialog. It shows the metadata
header plus subject/from/date/body (email) or `<pre>` JSON (text),
`truncated` notice when set, and two actions: **Try again** (primary)
and **Discard** (danger, behind the confirm dialog). Outcomes: retry
resolved → success note linking the Person, queue invalidated (+
people/today); retry unresolved → "still unresolved" with the reason;
retry 503 `intake_busy` → "intake is busy, try again shortly"; decrypt
500 → the §4 corrupted-row copy. New `types.ts` entries
(`UnresolvedDetailResponse`, retry/discard responses), a
`useUnresolvedDetail` fetch and two mutations invalidating
`queryKeys.unresolved(orgId)`. No new routes.

## 8. Authorization & tenant isolation

Content read, retry, and discard: **org admins only** (D-037), via
`OrgAdminContext`; members 403 on all three and keep the metadata
list. Cross-org and unknown ids 404 byte-identically on every
endpoint. Decrypted content exists only in per-row on-demand response
bodies — never in the list, never in spans/logs (tracing-capture
test), redacted `Debug` on `UnresolvedContent`. O-012 non-preclusion:
decryption stays behind the single `crypto::open` chokepoint; no
plaintext is persisted anywhere new.

## 9. Observability

Spans `intake.unresolved_detail` / `intake.retry` / `intake.discard`:
`organization_id`, `actor_id` (the admin — ids are safe),
`raw_payload_id`, `payload_format` (detail and retry; the discard span
omits it — a discard parses nothing and its row read does not carry
the column), `outcome` (static tags:
`shown` | `retried_resolved` | `retried_unresolved` | `retried_busy` |
`discarded` | `already_discarded` | `already_resolved` | error-variant
names). Never: subject, sender, body text, JSON content, decrypted
bytes. `complete_intake`'s inner discipline is unchanged.

## 10. Acceptance criteria

1. The migration applies to a populated DB; the CHECK accepts
   `discarded` and rejects unknown values; the pair-CHECK rejects
   `discarded` without both attribution fields and attribution fields
   without `discarded`; `crm_app` can UPDATE the two new columns and
   remains denied on `ciphertext`/`nonce`/`content_hmac`.
2. Admin GET on an `unresolved` `rfc822_v1` row returns
   subject/from_display/from_addr/date/text matching the fixture; on a
   `generic_v1` row, pretty-printed JSON; on an `invalid_json` or
   unparseable-email row, the lossy-raw text fallback; a **`pending`**
   row is also viewable (the stuck-pending rescue starts by opening
   one); a >64 KiB body sets `truncated: true` with the cap applied at
   a **UTF-8 character boundary** (the `truncate_to_bytes` discipline —
   naive byte slicing panics mid-code-point).
3. Member GET/retry/discard → 403 `forbidden`; the metadata list stays
   member-visible and unchanged.
4. Unknown id, cross-org id, `resolved` row, and `discarded` row →
   byte-identical 404 on GET.
5. Retry on a stuck-`pending` in-format row (delivered under an
   externally-held advisory lock, then released) → Person on the
   default assignee's Today; all facts `actor_kind='system'`,
   `actor_user_id NULL`, **`on_behalf_of_user_id` = the admin**,
   `origin='web_session'`, routing per D-035; one
   `person_changed{inquiry_received}`; the response carries the
   resolved shape.
6. Retry on an `email_unrecognized_format` row whose bytes still don't
   match → 200 `{"status":"unresolved"}`, same reason re-recorded,
   `resolved_at` updated, one `intake_unresolved_changed`.
7. Retry on a `generic_v1` row re-runs the JSON parse with the row's
   stored `source` (closure-reconstruction unit test + one DB case).
8. Two concurrent retries of one row → exactly one Person/Inquiry/
   publish; the loser returns the stored outcome `duplicate: true`.
9. A retry racing a byte-identical redelivery → one Person, one
   Inquiry (row lock + advisory lock compose).
10. Retry with the org advisory lock held past budget → 503
    `intake_busy` + `Retry-After`, row stays `pending`, no Person.
11. Retry on a corrupted-ciphertext row → 500, row stays `pending`,
    still discardable.
12. Discard sets `resolution`/`discarded_by_user_id`/`discarded_at`,
    removes the row from every member's queue, publishes one
    `intake_unresolved_changed`; a repeat discard (including by a
    different admin) → 200 idempotent with the **original attribution
    unchanged** (first writer wins); discard on `resolved` → 409
    `already_resolved`; retry on `discarded` → 409 `discarded`.
13. Redelivery of a discarded row's exact bytes → 200 accepted, row
    stays `discarded`, zero new rows, zero publishes, **no panic**
    (the `duplicate_outcome` `discarded` arm).
14. A `/api/inquiries` replay hitting a discarded row → 200
    `{"status":"unresolved","duplicate":true}` (envelope unchanged).
15. Tracing capture over detail/retry(success + failure)/discard: no
    subject, sender, body, JSON content, or decrypted bytes in any
    span or log line.
16. Frozen envelopes unchanged: `/inbound/email` byte-for-byte;
    `POST /api/inquiries` additively extended only on the
    duplicate-replay path; the existing `db_inbound_email*` and
    `db_intake*` suites pass (unmodified except where this spec
    declares).
17. Web: admin sees clickable rows, the dialog (fetch on open), both
    actions with their outcome states; the member rendering is
    byte-identical to today (Vitest-pinned); labels pinned.
18. `./scripts/check`, `./scripts/check-db`, web
    lint/typecheck/test/build green; live walkthrough per §1.

## 11. Tests

DB-backed: new `tests/db_intake_workbench.rs` (recording publisher;
the external-advisory-lock helper pattern from `db_intake.rs`; the
tracing-capture pattern from `db_inbound_email.rs` with its
global-subscriber caveat; fixtures reused from
`tests/fixtures/email/`). Service-free: 401/403/404/400 shapes on a
stateless app; 503 DB-down. Unit: content caps + truncation,
pretty-print and lossy fallbacks, per-format closure reconstruction
(incl. unknown-format fail-closed and `Source::parse` revalidation),
the `duplicate_outcome` `discarded` arm, `UnresolvedContent` redacted
`Debug`, `for_system` `on_behalf_of` field construction. Web: Vitest
for member-vs-admin rendering, dialog fetch-on-open, and the outcome
states.

## 12. Lanes and checks

Two lanes (the ladder's "M (2 lanes)"), SLICE_002/003 pattern:

- **Lane A — backend**: owns `backend/**` (the migration — sole owner,
  domain, routes, `ApiError` variants, envelope extension, all backend
  tests). Merges first.
- **Lane B — web**: owns `web/**` only; builds against the frozen §5
  table + §6 realtime statements without waiting on a running
  backend. No shared files.

Gates: `./scripts/check` + `./scripts/check-db` green (Lane A), web
lint/typecheck/test/build green (Lane B), live walkthrough per §1
after integration. Reminders: `./scripts/db-migrate` after checkout
(crm_dev never auto-migrates); the dev API runs orphaned — rebuild and
restart it by exact PID for the walkthrough; `cargo sqlx prepare`
refresh (new queries).

## 13. Safe defaults adopted (reviewer/user may veto)

(a) Redelivery of discarded bytes stays discarded — no resurrection;
(b) retry runs as `IntakeActor::System` with `on_behalf_of_user_id` =
the acting admin and `origin='web_session'` — so a rescued lead routes
per D-035 to the org default (or unassigned), NOT to the clicking
admin; (c) the retry reset clears `unresolved_reason`/`resolved_at`;
(d) detail text capped at 64 KiB with `truncated`; (e) lossy-raw
fallback for unparseable/invalid content; unknown `payload_format`
displays as raw text but retry fails closed; (f) detail 404 for
`resolved` and `discarded` rows (byte-identical with nonexistent);
(g) discard is idempotent; 409 only against `resolved`; (h) a retry of
a concurrently-resolved row returns the stored outcome
`duplicate: true`; (i) the detail surface is a dialog, not a
route/drawer; (j) no fact row for discard — attribution lives on the
row (the ladder's wording; discard is not deletion, D-015 §5 not
triggered); (k) retry surfaces `intake_busy` as a 503 to the admin
(interactive caller — deliberately unlike `/inbound/email`); (l) the
`IntakeActor::System`/`for_system` `on_behalf_of_user_id` extension is
the declared envelope change (existing callers pass `None`, behavior
unchanged); (m) discarded rows are retained, UI-invisible ciphertext
until the erasure runbook/O-013 — stated, not silent.
