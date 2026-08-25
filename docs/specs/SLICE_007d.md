# Slice 007d — One pinned email format → real inquiries

Status: APPROVED (user, 2026-08-25; planner pass + independent review
same day, reviewer verdict "ready with amendments" — no blocking
findings, all seven amendments applied, including the declared 007b
criteria 3/14/18 supersessions and the route-file ownership; §12 safe
defaults accepted as written)
Ladder: docs/plans/SLICE_007_LADDER.md rung d. Builds on 007b (inbound
endpoint; merged `4b3462a`) and 007c (system actor + unattended
routing; merged `fe0b99b`). Targets O-014 #1 (deterministic template
parsers for pinned formats), D-012 (raw preserved, parsing
re-runnable), D-035 (unattended routing), **D-036** (forged-mail
posture: in-format mail from a valid address creates a Person; the
defenses are the token, per-format sender-domain matching, SPF/DKIM at
007g), SLICE_002 (Phase A/B mechanics), AGENTS.md §11 (contract
discipline), §13 (no new dependency without a concrete requirement —
`mail-parser` is the ladder-blessed exception, wrapped).

## 1. User-visible outcome

An email in the pinned cypressbayrealty.com contact-form format,
delivered to an Organization's intake address (still via
`scripts/inbound-email` — real DNS is 007g), becomes — with no human
in the loop — a Person with normalized contact methods, an Inquiry
with `source='website'`, the four intake facts attributed to
**System** (`origin='webhook'`), routed per 007c: onto the default
assignee's Today, or created unassigned with the settings-page warning.
Any other mail still lands in Unresolved, now labeled **"Unrecognized
email format"**. The frozen `/inbound/email` response reveals nothing
new.

Live walkthrough: (1) Manage → Intake: default assignee = bob (007c
setting). (2) `scripts/inbound-email <copied address>
cypress_bay_contact.eml` → as bob, the Person appears on Today; Person
detail shows "Inquiry received" (source `website`) attributed to
System. (3) `scripts/inbound-email <addr> plain.eml` → an Unresolved
row "Unrecognized email format". (4) Re-run step 2's exact command →
200 accepted, nothing new anywhere. The script needs no changes.

## 2. In / out of scope

In: the `mail-parser` dependency (crm-app only, wrapped in
`domain/intake/email/mime.rs`, fence-tested), the `EmailFormat`
registry with exactly one format (the cypressbayrealty.com contact
form — a template we author), the `complete_intake` extraction from
`receive_inquiry` for reuse, Phase B on `receive_inbound_email`
(`IntakeActor::System`, `Origin::Webhook`), the
`email_unrecognized_format` unresolved reason + two-line web label,
the new `.eml` fixtures, the declared amendments to 007b's tests
(§10), the live walkthrough.

Out: retrying pre-existing terminal `unresolved` rows (007e's
Try-again owns rescue; 007d processes only new deliveries and
redeliveries of non-terminal rows); Unresolved detail/discard (007e);
LLM extraction and any background worker (007f — a `pending`-deferred
row waits for redelivery or 007e, nothing polls it); provider
adapters, SPF/DKIM, token rotation, real DNS (007g); more formats
(007h); round-robin; any change to the frozen `/inbound/email`
envelope or to `POST /api/inquiries` behavior; any migration; storing
Message-ID as an external id.

## 3. Persistence

**No migration.** New free-text values only:
`unresolved_reason='email_unrecognized_format'`; `inquiry.source=
'website'` (detected source) while `raw_payload.source` stays
`'email'` — the ladder's settled split. `routing_decision.strategy`
was already widened by 007c (`20260829000001`). `crm_app` already
holds every grant the email Phase B needs (it is `receive_inquiry`'s
existing write set). Idempotency keyspace unchanged: `(org, 'email',
content_hmac)`.

## 4. Domain

### 4a. MIME wrapper — `domain/intake/email/mime.rs`

The only module in the workspace allowed to `use mail_parser`
(Stalwart's `mail-parser`; current stable pinned at implementation
time):

```rust
pub struct ParsedMail {
    pub from_addr: Option<String>,   // first From mailbox, lowercased
    pub from_display: Option<String>,
    pub subject: Option<String>,
    pub date: Option<DateTime<Utc>>, // header Date; informational only
    pub text_body: Option<String>,   // text/plain preferred; parser's
                                     // HTML→text conversion as fallback
}
pub fn parse(raw: &[u8]) -> Option<ParsedMail>   // None = email_unparsed
```

Owned std/chrono types only; redacted `Debug` (the `ParsedLead`
precedent). Dependency mechanics: `mail-parser` enters
`[workspace.dependencies]` in `backend/Cargo.toml` with `workspace =
true` in crm-app (repo convention), default features (charset decoding
is wanted for real mail), no `serde` feature. `mail-parser` is
extremely lenient (`MessageParser::parse` returns `Some` for almost
anything), so the wrapper defines "unparseable" explicitly: the parser
returned `None`, **or** the result has no headers and no body —
keeping `email_unparsed` a real, tested path (a garbage-bytes fixture
pins it; expect iteration crafting bytes the parser actually rejects).
`ParsedMail.date` is parsed but never used for `received_at` (receipt
time, per the ladder). A **directory-walk fence test** over
`domain/intake/email/` (`fs::read_dir`, not `include_str!` — the
future leak site is a `formats/*.rs` file that doesn't exist yet)
asserts only `mime.rs` mentions `mail_parser`.

### 4b. Format registry — `domain/intake/email/format.rs` + `formats/cypress_bay.rs`

```rust
pub struct ExtractedLead {
    first_name: Option<String>, last_name: Option<String>,
    email: Option<String>, phone: Option<String>,
    message: Option<String>, source: &'static str,
}
pub trait EmailFormat: Send + Sync {
    fn name(&self) -> &'static str;               // "cypress_bay_contact_v1"
    fn matches(&self, mail: &ParsedMail) -> bool; // sender domain AND
                                                  // template marker, both
    fn extract(&self, mail: &ParsedMail) -> ExtractedLead;
}
pub fn detect(mail: &ParsedMail) -> Option<&'static dyn EmailFormat>
// static slice, declaration order, first match wins
```

`extract` is deliberately **infallible**: a labeled-field scan cannot
structurally fail (missing fields are `None`); the only downstream
failure is "no normalizable contact method", which reuses the
**existing** `no_contact_method` reason and label — zero new
vocabulary, zero web work for that branch.

**The pinned template** (we author the cypressbayrealty.com form, so
the spec and fixtures define it): From `"Cypress Bay Realty"
<forms@cypressbayrealty.com>`; Subject `New contact form submission`;
plain-text body of labeled lines `Name:`, `Email:`, `Phone:`,
`Message:` (Message consumes the remainder, multi-line). `matches()` =
the From address's **domain** equals `cypressbayrealty.com`
(case-insensitive; domain not full-address, since we control the form
but the mailer's local part may vary) AND the subject **equals the
marker after trim, exact case** (the form mails directly; a
"Fwd: "-prefixed subject deliberately does not match — forwarded
copies of form mail are not the pinned flow). Both conditions
required, so the right template from the wrong sender domain does NOT
match (D-036's mitigation, criterion 9). The body scan: labels are
**line-anchored, exact case** (`Name:` at start of line), values
trimmed. Name splits on first whitespace (single word → `first_name`
only). `ExtractedLead →
ParsedLead` conversion mirrors `parse.rs`: `contact::normalize_email`
/ `normalize_phone`, raw values preserved, message truncated at the
same `MESSAGE_MAX_BYTES` (4096), `external_id: None`, source
`website`.

### 4c. The reuse seam — `complete_intake`

`receive_inquiry`'s Phase-B loop (row lock → duplicate short-circuit →
decrypt → parse → advisory-lock bounded retry → identify → Person +
contact methods → routing → Inquiry → four facts → `mark_resolved` →
one `person_changed` publish; parse failure → `mark_unresolved` +
`intake_unresolved_changed`) is extracted verbatim into a
`pub(crate)` function in the same file (minimal churn; no new module):

```rust
pub(crate) struct CompleteIntake<'a> {
    raw_payload_id: Uuid, content_hmac: &'a [u8],
    received_at: DateTime<Utc>, assign_to_user_id: Option<Uuid>,
}
pub(crate) async fn complete_intake<F>(
    pool: &PgPool, key: &RawPayloadKey, publisher: &Publisher,
    actor: &IntakeActor, params: CompleteIntake<'_>, parse: F,
) -> Result<ReceiveInquiryOutcome, CommandError>
where F: Fn(&[u8]) -> Result<(Source, ParsedLead), UnresolvedReason>
```

Shared: everything after Phase A, including the advisory lock, dedup /
identify, the 007c routing matrix, fact writes, both publishes,
`duplicate_outcome`. Per-caller: the parse closure (generic:
`parse::parse` + the request's `source`; email: `mime::parse` →
`detect` → `extract` → normalize → `Source::parse("website")`),
`assign_to_user_id` (always `None` for email), the actor, and the
**span `outcome` records**: the loop's current
`Span::current().record("outcome", …)` calls move out of the extracted
function to each caller, derived from the returned outcome
(behavior-identical for `/api/inquiries`; the email caller records
§8's vocabulary on `intake.inbound_email` — re-recording inside the
shared function would double-stamp fields across two differently-named
span vocabularies). `MESSAGE_MAX_BYTES` becomes `pub(crate)` so the
email normalization shares it.
`inquiry.source` and the `inquiry_received` fact take the closure's
detected source. Ordering preserved: parse runs **before** the
advisory-lock attempt, so unresolved outcomes never contend for the
lock (as today). This extraction is a pure refactor for the
`/api/inquiries` caller — its behavior is byte-for-byte unchanged and
the full existing `db_intake.rs` + `db_intake_system_routing.rs`
suites pass unmodified (criterion 16).

`UnresolvedReason` gains `EmailUnparsed` and `EmailUnrecognizedFormat`
variants (`as_str` `email_unparsed` / `email_unrecognized_format`) —
extending the existing enum, not a parallel email type.
`duplicate_outcome`'s reason decode (today defaulting unknown strings
to `NoContactMethod`) gains both, so a duplicate replay decodes
faithfully. (A `/api/inquiries` replay hitting an email row requires
bytes simultaneously valid JSON and RFC 822 — probability ~0, the
007b §3 accepted posture, restated.)

### 4d. `receive_inbound_email` grows Phase B

The existing recipient-parse → org-resolve → seal → `insert_pending`
sequence is unchanged (as-built signatures kept — `received_at`
computed internally; the rejection variants as merged). After it, the
007b-era `mark_unresolved_if_pending` call is replaced by
`complete_intake` with `IntakeActor::System { organization_id, origin:
Origin::Webhook, correlation_id: Uuid::new_v4() }` and the email parse
closure. `mark_unresolved_if_pending` becomes dead code and is
deleted (its behavior is subsumed and re-pinned by §9; it has no unit
test on main to retire). `lock_for_processing`'s pending check subsumes 007b's
duplicate detection and upgrades the stuck-`pending` rescue: a
redelivery finding a `pending` row now attempts the **full parse**
(007b criterion 14 superseded — an in-format stuck row resolves
end-to-end). Terminal rows (`resolved` or `unresolved`, including
007b-era `email_unparsed` rows) short-circuit through
`duplicate_outcome`: 200 accepted, no reprocessing, no second Person,
no publish — historical rows are 007e's business, not redelivery's.

`InboundEmailOutcome` (domain type; the HTTP envelope never changes)
becomes: `Completed { person_id, inquiry_id, raw_payload_id }` |
`Unresolved { raw_payload_id, reason }` | `DeferredPending {
raw_payload_id }` | `Duplicate` | `Rejected`. The route
(`crm-api/src/routes/inbound_email.rs`) therefore changes too — its
outcome `match` and its `#[instrument]` span-field declarations (§8's
new fields must be declared there or tracing drops them silently) —
while every response body stays byte-identical.

**Declared 007b supersessions** (AGENTS.md §11 hygiene; the substance
is argued in §4f): 007b criterion 3 ("the intake advisory lock is
never taken") and criterion 18 ("no changes to `receive_inquiry`") are
invalidated by this rung, alongside criterion 14 (the stuck-`pending`
rescue now runs the full parse) and the §1/§10 `email_unparsed`
expectations for the two existing fixtures. All four are superseded by
this spec's approval, not silently.

### 4e. Failure ladder

Row already `pending`; the HTTP response is 200 `{"status":"accepted"}`
in every row of this table:

| Step | Row outcome | Publish |
|---|---|---|
| `mime::parse` → unparseable (garbage; rare by design) | `unresolved` / `email_unparsed` (reason and label unchanged from 007b) | `intake_unresolved_changed` |
| Valid MIME, no format matches — including the right template from a wrong sender domain (D-036 mitigation) | `unresolved` / `email_unrecognized_format` | `intake_unresolved_changed` |
| Format matches, no normalizable email or phone | `unresolved` / `no_contact_method` (existing reason + label) | `intake_unresolved_changed` |
| Format matches, valid contact method | `resolved`; Person / Inquiry / facts; 007c routing | one `person_changed{inquiry_received}` |
| Advisory-lock budget exhausted (`CommandError::IntakeBusy`) | stays `pending` (queue-visible, reason "—") | `intake_unresolved_changed` (ids-only invalidation, the delivery's correlation id; the queue lists `resolution <> 'resolved'`, so connected clients see the row now, reconnecting ones by refetch). Known limitation, accepted: when a later redelivery completes this row, only `person_changed` fires — a connected queue viewer keeps the stale row until refetch (pre-existing behavior for `/api/inquiries` retries; SLICE_003's refetch-recovery convention) |
| Crypto/DB error mid-Phase-B | stays `pending` (SLICE_002 crash-window rule) | none; 500/503 envelope as 007b |

### 4f. The advisory lock — key design point

The email path creates People, so it MUST take the per-org intake
advisory lock (identify + insert must serialize, or two concurrent
leads sharing a contact value create two Persons — SLICE_002 §3's
exact rationale; criterion 15 pins it). This does **not** break 007b's
"can never return `intake_busy`" promise, which is an HTTP-response
property: on `CommandError::IntakeBusy` the email path catches the
error and returns 200 accepted with the row left `pending` — exactly
SLICE_002's IntakeBusy row state ("untouched, as if the attempt never
happened"). The lead is never lost: the row is queue-visible,
byte-identical redelivery re-attempts the full parse, and 007e's
Try-again will rescue it manually. Alternatives rejected: a 503 to
the mail provider breaks the frozen contract and invites retry
storms; an in-process background retry pulls 007f's worker forward.
Handler wall-time under contention stays bounded by the existing
shared `ADVISORY_LOCK_BUDGET` (~3 s) — acceptable for webhook
timeouts; the constant stays shared.

## 5. HTTP contract

**No change anywhere.** The frozen 007b `/inbound/email` envelope
holds byte-for-byte: 200 `{"status":"accepted"}` for completed,
unresolved, deferred-pending, and duplicate alike — nothing about the
parse outcome, no ids; 200 `{"status":"rejected"}` unchanged;
401/413/400/503/500 unchanged. `POST /api/inquiries` is byte-for-byte
unchanged. No new endpoints.

## 6. Web

Two lines plus a Vitest pin: `web/src/api/types.ts`'s
`UnresolvedReason` union gains `'email_unrecognized_format'`;
`web/src/lib/labels.ts` gains `email_unrecognized_format:
"Unrecognized email format"`. Nothing else — Unresolved's `source`
cell renders raw text (`website` never appears there;
`raw_payload.source` stays `email`), Person detail already renders the
System actor and the 007c routing labels, and realtime invalidation is
already wired.

## 7. Authorization & tenant isolation

Unchanged from 007b: the token is the only tenant credential; the
Organization comes solely from `(slug, token)`. New pinned properties:
a valid-format email to org A creates rows only in org A, and the new
Person is visible to org A members and never to org B; the
forged-format-wrong-domain case lands Unresolved and never creates a
Person (D-036); all facts carry `actor_kind='system'`,
`actor_user_id NULL`, `origin='webhook'`, one correlation id (the DB
CHECK pair makes the alternative unrepresentable).

## 8. Observability

`intake.inbound_email` span: `outcome` grows to `completed` |
`unresolved_unparsed` | `unresolved_unrecognized_format` |
`unresolved_no_contact_method` | `deferred_busy` (plus the existing
vocabulary); new fields `format` (the static `name()`, only when a
format matched) and `person_id` / `inquiry_id` on completed (ids are
safe). Never in any span/log: subject, sender, body, recipient, slug,
token — the 007b tracing-capture test extends over the new paths.
The `/api/inquiries` caller keeps the existing `receive_inquiry` span
discipline through the refactor.

## 9. Acceptance criteria

1. Valid bearer + valid recipient + the canonical cypress-bay fixture
   → 200 accepted; the `raw_payload` row has `source='email'`,
   `payload_format='rfc822_v1'`, `origin='webhook'`,
   `resolution='resolved'`, `inquiry_id` set.
2. Exactly one Person (name split correct; email and phone contact
   methods normalized with raw values preserved), one Inquiry
   `source='website'`, message = the form's Message truncated at
   4096 bytes, `received_at` = receipt time (not the Date header).
3. All facts from one delivery: `actor_kind='system'`,
   `actor_user_id NULL`, `origin='webhook'`, one shared correlation
   id; the `inquiry_received` fact's `source='website'`.
4. Routing per 007c: default set + active → `organization_default`,
   the Person on that member's Today and nobody else's,
   `assignment_changed` NULL→default with `causation_id` = the routing
   decision id; default unset → `unassigned`, no assignment fact, no
   member's Today.
5. Exactly one `person_changed{inquiry_received}` publish per
   completed delivery (recording publisher), ids-only, `v:1`.
6. A second in-format email from the same lead address with a
   different body → a second Inquiry on the same Person (matched by
   email), no second Person; `kept_existing` when already assigned.
7. Byte-identical redelivery of a `resolved` row → 200 accepted, zero
   new rows anywhere, zero publishes.
8. Valid MIME in no known format (the existing `plain.eml` and
   `multipart.eml` fixtures) → `unresolved` /
   `email_unrecognized_format`, no Person/Inquiry/fact, the advisory
   lock never taken, one `intake_unresolved_changed`. **Declared
   amendment**: 007b's tests pinned `email_unparsed` for these
   fixtures; those assertions change in this slice (an internal
   free-text value evolving, not a frozen contract — SLICE_007b §1/§10
   superseded on this point).
9. The correct template from a non-cypressbayrealty.com sender →
   `email_unrecognized_format`, no Person (D-036 mitigation pinned).
10. An in-format email with no normalizable email or phone →
    `unresolved` / `no_contact_method`.
11. Garbage bytes (unparseable per §4a's definition) → `unresolved` /
    `email_unparsed`.
12. Pre-existing terminal `unresolved` rows (any reason) are never
    reprocessed by redelivery; a stuck-`pending` row IS fully parsed
    on redelivery — an in-format stuck row resolves end-to-end
    (007b criterion 14 upgraded).
13. With the org's advisory lock held externally past the budget →
    200 accepted, row stays `pending`, no Person, one
    `intake_unresolved_changed`; a later redelivery completes it;
    `/inbound/email` can never return `intake_busy` (re-pinned).
14. Two concurrent byte-identical in-format POSTs → exactly one
    Person, one Inquiry, one publish (the 007b criterion-19 race
    pattern).
15. Two concurrent different-content in-format emails sharing one
    contact value → one Person, two Inquiries (the lock's reason to
    exist).
16. `POST /api/inquiries` byte-for-byte unchanged; the full existing
    `db_intake.rs` and `db_intake_system_routing.rs` suites pass
    unmodified (regression gate for the `complete_intake` refactor).
17. The frozen `/inbound/email` envelope is unchanged, including
    rejection/401/413/400 shapes; success bodies reveal nothing about
    the parse outcome.
18. Tracing capture over completed / unrecognized-format / unparsed /
    no-contact paths: no subject, sender, body, recipient, slug, or
    token in any span or log line; the `format` field carries only the
    static format name.
19. Tenant isolation: org A's valid-format mail writes zero rows in
    org B; the created Person is invisible to org B.
20. The fence holds: `mail_parser` is referenced only in
    `domain/intake/email/mime.rs` (fence test); the dependency is
    added to crm-app only; no other new dependencies.
21. Web: the "Unrecognized email format" label renders
    (Vitest-pinned).
22. `./scripts/check`, `./scripts/check-db`, web
    lint/typecheck/test/build all green; live walkthrough per §1.

## 10. Tests

DB-backed: extend `tests/db_inbound_email.rs` (amending the **three**
declared reason assertions — the criterion-1 shape, the queue reason,
and the criterion-14 stuck-pending rescue, which under the full-parse
rescue now expects `email_unrecognized_format`) plus a sibling
`db_inbound_email_intake.rs` for the completion cases (criteria 1–15,
19), reusing the `db_today.rs` helpers and the recording publisher.
Service-free: the 007b `tests/inbound_email.rs` suite passes
unchanged (no HTTP-contract changes; the route's outcome mapping and
span fields do change per §4d/§8). Unit: the mime wrapper
(plain / multipart / HTML-only / garbage; Debug redaction; header-date
ignored for `received_at`), the cypress-bay `matches()` matrix (right
domain wrong subject, right subject wrong domain, case-insensitivity),
`extract()` (multi-line Message, missing fields, single-word name),
`ExtractedLead → ParsedLead` normalization, `UnresolvedReason`
round-trip including the new variants, the fence test. Web: one
Vitest label line.

Fixtures (under `backend/crates/crm-api/tests/fixtures/email/`), all
synthesized: `cypress_bay_contact.eml` (canonical),
`cypress_bay_contact_html_only.eml` (HTML-fallback path),
`cypress_bay_contact_no_contact.eml`, `cypress_bay_forged_sender.eml`,
`garbage.eml`; the existing `plain.eml` / `multipart.eml` become the
unknown-format fixtures.

## 11. Lane and checks

Single lane, branch `slice-007d-email-format`, one writer, owning:
crm-app (the `email/` module, `receive.rs` Phase B, the
`complete_intake` extraction, the `UnresolvedReason` extension, the
workspace dependency entry), `crm-api/src/routes/inbound_email.rs`
(outcome mapping + span fields), crm-api tests + fixtures, and the
two-line web touch. No migration exists to own. Gates: `./scripts/check` and
`./scripts/check-db` green, web lint/typecheck/test/build green, live
walkthrough per §1. Reminder: `./scripts/db-migrate` is NOT needed (no
migration), but the dev API must be rebuilt onto the branch (it runs
orphaned; restarting Docker does not restart it).

## 12. Safe defaults adopted (reviewer/user may veto)

(a) `extract()` is infallible; contact-method failure reuses the
existing `no_contact_method` reason/label; (b) Message-ID is not
stored as an external id this rung; (c) the email path shares the
existing `ADVISORY_LOCK_BUDGET`; (d) `complete_intake` stays
`pub(crate)` inside `receive_inquiry.rs` (minimal churn, no new
module); (e) the registry is a static slice, first match wins, in
declaration order; (f) `UnresolvedReason` is extended, not
paralleled; (g) `ParsedMail.date` is parsed but unused for
`received_at`; (h) label "Unrecognized email format"; (i) `matches()`
uses sender-**domain** equality (not full address — we author the
form; the mailer's local part may vary); (j) on `IntakeBusy` the row
stays `pending` and one ids-only `intake_unresolved_changed` is
published (connected clients see the queue row now; reconnecting ones
recover by refetch); (k) `mark_unresolved_if_pending` is deleted as
dead code after the refactor; (l) the 007b test assertions pinning
`email_unparsed` for `plain.eml`/`multipart.eml` change to
`email_unrecognized_format`, and 007b criteria 3, 14, and 18 are
superseded as declared in §4d — spec-approved amendments, not silent
drift; (m) a completed previously-deferred row publishes only
`person_changed` (queue staleness until refetch accepted, §4e); (n)
`mail-parser` with default features, no `serde`, via a workspace
dependency entry.
