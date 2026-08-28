# Slice 009 — Correspondence capture v1 (CC/BCC-first, metadata-only)

Status: DRAFT, independently reviewed 2026-08-27 (READY-WITH-FIXES;
all folded in: future-Date clamp, digest token lookup, terminal-
status PII nulling, forward-dedup message_id rule, link semantics +
transition matrix, client_replied precedence + SLICE_003 §5
declaration, SLICE_004 mint declaration + mint-if-absent, parser
alphabet pin, blast-radius additions, crypto-signature note, five
new criteria, trim-levers-amend-D-042). APPROVED (user, 2026-08-27).
D-042 accepted. Executes O-014 #2 under the recorded CC/BCC-first
direction. Decisions in force: D-042 (the six scoping decisions),
O-014 (direction + mitigations incl. retroactive forwarding), D-012,
D-015 (metadata compatibility; §3 PII-free fact rows — declared
tension in §4), D-038 (capture raw must be structurally outside the
extraction sweep), D-040/007h1 (unwrapper reuse), O-012 (bodies
parked — nothing here may preclude the later upgrade), O-008 (no AI
this slice).

## 1. User-visible outcome

An agent finds their personal capture address in a new "Email
capture" view and CCs it on client/lead threads (BCC works too).
From then on: every captured email appears on the Person's timeline
(direction + agent + time — no content); an outbound CC counts as a
contact attempt and clears the Person from Today; a client reply
that reaches us (reply-all) re-arms Today as "client replied" for
the assignee, in realtime. Missed mail is repairable: forward the
old emails to the same address and they land at their ORIGINAL date
in the timeline, correct direction, no duplicates. Mail whose
correspondent isn't a known Person goes to the agent's private
"unmatched" list to link or dismiss. Agents can rotate their address
any time.

## 2. In / out of scope

**In:** per-agent capture addresses (mint at migration for active
members + on membership activation; self-service rotation);
receiving via the EXISTING Cloudflare catch-all + relay + frozen
`/inbound/email` (internal dispatch only); metadata capture
(direction ladder, matching, Message-ID/thread dedup); retroactive
forwards via the 007h1 unwrapper (inner Date placement, backdated
flag); `correspondence` timeline entries (org-wide visible);
auto-`contact_attempted` on outbound; `client_replied` Today arm +
realtime; per-agent unmatched held queue (link/dismiss); encrypted
raw retention with NO read surface; new web "Email capture" view +
timeline/Today rendering.

**Out (explicit):** message bodies/subjects anywhere readable
(O-012); OAuth/mailbox access of any kind (later opt-in per O-014);
attachment-style rfc822 forwards (007h1 exclusion stands);
non-English forward banners; SPF/DKIM verdict consumption (stored
raw carries headers; `SenderTrust::Direct` is the future home —
v1 consumes none); AI suggestions (O-008); sending; held-queue
realtime; admin views over capture; any change to intake behavior,
the `/inbound/email` envelope, or `POST /api/inquiries`.

## 3. Receiving & addresses

`save-<token12>@leads.elysianfeld.com` per agent (D-042.5): token =
12 chars of the intake mint alphabet (~60 bits; thread-visible
addresses warrant more than intake's 40). Grammar disjointness from
intake is STRUCTURAL: intake requires a final 8-char token segment
(`address.rs` `TOKEN_LEN=8`), capture requires exactly `save-` +
12 token chars — neither parser can accept the other's addresses
(no reserved-slug list needed; pinned by unit tests both ways).
The capture parser validates ALL 12 token chars against the
hyphen-free mint alphabet — required for the disjointness proof (a
length-only check would collide with a hypothetical org slug
`save-abc`); the hyphenated-`save-*`-slug edge is pinned in the
disjointness tests both directions. Dispatch in `receive.rs`:
intake parse first (byte-identical behavior), else capture parse,
else the existing 200-rejected. The
frozen SLICE_007b §5 contract is reused byte-for-byte — declared
here as an internal routing extension, not a contract change.
Cloudflare: ZERO changes (catch-all already relays every recipient;
the worker uses envelope RCPT TO, which is why BCC and reply-all
both deliver; multi-RCPT double-delivery is absorbed by dedup, §4).
Known limitation (stated): the 2 MiB endpoint cap bounces
attachment-heavy mail — an honest agent-visible bounce (007g §3),
never silent loss.

Token hygiene: `CaptureToken` newtype in the `IntakeToken` mold (no
Display/PartialEq, redacted Debug, constant-time `verify`). Lookup:
indexed SELECT by a **deterministic digest column**
(`token_lookup` = SHA-256 of the token; unkeyed suffices — B-tree
early-exit timing on digest bytes carries no information about
token-byte prefixes), then constant-time verify of the full token
against the fetched row + dummy compare on miss. This restores
intake-grade lookup uniformity: intake's B-tree probe is on the
public slug, never token bytes, and capture must not be weaker just
because its token is the address (reviewer finding — the earlier
"parity" framing conflated storage posture with lookup timing).
Plaintext token stays stored for re-display, exactly like
`organization.intake_token`. Rotation: per-agent,
immediate-invalidation, `capture_token_rotated` append-only fact
(007g §5 template). Deactivated member → token stops resolving
(200 rejected, nothing stored); reactivation restores it (O-004
pointer). Tokens never in spans/logs (tracing-capture pinned).

## 4. Persistence (one migration, single owner, `20260904000001`)

1. `capture_address` (id, organization_id, user_id, token,
   token_lookup, created_at; UNIQUE token_lookup, UNIQUE
   (organization_id, user_id)); backfill-mint for existing ACTIVE
   members; minted-IF-ABSENT on activation thereafter — a
   reactivated member gets their EXISTING address back, mint happens
   only when no row exists (the (org,user) UNIQUE makes
   unconditional mint an error; reconciles §3). Extending
   accept_invitation / set_member_status this way is a **declared
   additive SLICE_004 change** (pointer amendment rides with this
   spec).
2. `correspondence_raw` (id, organization_id, received_at, nonce,
   ciphertext, content_hmac, byte_len, processed bool; UNIQUE
   (organization_id, content_hmac)) — sealed with the same org‖id
   AAD construction via a generalized/sibling `crypto` signature
   (`seal` is currently typed to `RawPayloadId`; a correspondence
   id must NEVER be smuggled through that type — reviewer note); NO read endpoint exists (D-042.6); NOT
   `raw_payload` — structural immunity from the 007f extraction
   sweep (D-038) and from workbench/D-037 lifecycle assumptions.
3. `correspondence_captured` — envelope-style fact table (the
   `intake_token_rotated` template: actor columns + CHECK,
   on_behalf_of, origin, occurred_at, recorded_at, correlation_id,
   causation_id, corrects_id, append-only + TRUNCATE-reject
   triggers) plus: person_id, agent_user_id NOT NULL, direction
   CHECK ('inbound','outbound'), message_id TEXT NULL (normalized:
   brackets stripped, capped, NUL-stripped), thread_key TEXT NULL
   (first References entry else self Message-ID), via CHECK
   ('cc','forward'), correspondence_raw_id, backdated bool.
   Indexes: (organization_id, person_id, occurred_at); dedup
   partial UNIQUE (organization_id, person_id, message_id) WHERE
   message_id IS NOT NULL.
4. `capture_message` (held queue): org, agent_user_id,
   correspondence_raw_id, counterparty_email (plaintext — visible
   ONLY to the attributed agent, D-042.3), direction_hint NULL,
   captured_at, status CHECK ('held','linked','dismissed') —
   UPDATE grant limited to status transitions + nulling
   counterparty_email; no DELETE. Transitions: held→linked and
   held→dismissed ONLY (both terminal; re-link after linked → 409,
   dismiss after dismissed → idempotent no-op).
   **`counterparty_email` is NULLED on every terminal transition**
   (link and dismiss): a dismissed row must not retain third-party
   PII forever in a no-DELETE table (D-015 §4; reviewer finding).
5. `capture_token_rotated` fact table (007g §4 shape).
6. Grants: SELECT/INSERT on the new tables (+ the status UPDATE on
   capture_message; token UPDATE on capture_address for rotation).

Timestamps: `occurred_at` = message time (CC: header Date, fallback
receipt; forward: unwrapped inner Date, fallback receipt +
`backdated=false`), **clamped to `min(message time, receipt time)`**
— the Date header is sender-controlled, and an unclamped future date
could suppress a Person from Today for years (forged outbound → an
auto-attempt "newer" than every future inquiry) or plant a
`client_replied` nudge no attempt can clear (reviewer's clamp
finding; criterion-pinned). Backdating is unaffected — only future
dates clamp. `recorded_at` = capture time. The history sort
`(occurred_at, recorded_at, kind_rank, id)` places backdated rows
correctly with zero read-model mechanics. This is the declared,
O-014-recorded divergence from intake's receipt-time rule (007d
§4a pointer carried here).

Declared D-015 §3 tension: message_id/thread_key are third-party
technical identifiers on a fact row — not name/contact PII; O-013
decides their redaction-on-erasure posture later.

Actor model: `Actor::System` + `on_behalf_of_user_id = agent` (the
agent's CC caused the unattended capture); `agent_user_id` is the
queried attribution column, mirrored into on_behalf_of.

## 5. Capture pipeline

Store-raw-first (crash window rescued by redelivery — the raw
UNIQUE absorbs byte-identical retries), then one transaction:
parse (mime, §7) → optional unwrap (007h1 `forward::resolve`; fires
only when the banner trigger holds; `via='forward'`) → direction
ladder → match → insert fact row(s) (+ auto-attempt) → mark raw
processed → publish realtime.

Direction/attribution ladder (deterministic, v1 — D-042 + planner
§4; attribution is ALWAYS the token's owner):
1. From equals an active member's login email → outbound.
2. Else From matches a Person (org-scoped normalize + match; match-
   never-create) → inbound from that Person (recipient Persons get
   no rows on inbound — stated).
3. Else any recipient (To/Cc minus the capture address, capped 25)
   matches → outbound; one row per matched recipient Person.
4. Else → unmatched → held queue (counterparty = inner-or-outer
   From for presumed-inbound, first non-member recipient otherwise).
Forwards apply the same ladder to the INNER From/To/Cc.
Mailing-list/assistant edge (From matches nothing but recipients
do): classified outbound — semantically imperfect, structurally
consistent, stated. Multi-agent threads: per-RCPT copies attribute
per token; the per-person Message-ID unique makes cross-agent
attribution first-writer-wins (accepted, stated). Matching skips
the intake advisory lock: capture never creates; a racing intake
either matches or the mail lands held — benign (stated reasoning).

Auto-attempt (D-042.4): outbound rows also write
`contact_attempted` (channel Email, outcome Sent — existing
variants), System actor on behalf of the agent, `causation_id` =
the correspondence fact id, `occurred_at` = message time.
Consequences stated: CC'ing clears Today; retroactive outbound
forwards clear historically (correct); forged outbound can clear
one Today item (visible, recoverable).

Dedup layers: raw (org, content_hmac); fact (org, person,
message_id) — where, for `via='forward'`, the stored `message_id`
is the FORWARDED ORIGINAL's id derived from the outer References
chain (last entry; `thread_key` takes the first), NOT the forward's
own Message-ID — that derivation is what lets the same UNIQUE
constraint back both re-forward dedup and
forward-of-already-CC-captured dedup. A client that strips
References defeats it (accepted, walkthrough-verified), and
Message-ID-LESS mail dedups only at the raw byte layer —
different-byte duplicates of id-less mail can double-post
(accepted, stated). Forged-Message-ID suppression (a forged row
with id X eats the later genuine X for that Person) is part of the
§9 blast radius.

## 6. Today & realtime

New third candidate arm in `today/queries.rs`: Person assigned to
the viewer with an inbound correspondence `occurred_at` later than
every effective contact_attempted AND every outbound correspondence
for that Person → reason `client_replied {occurred_at}`
(`waiting_since` = that occurred_at). Cleared by any at-or-after
attempt or captured outbound. Existing `latest.id IS NOT NULL`
(Inquiry) constraint kept (every Person has one; stated).
Ranking (default, veto-able): fresh (<24 h) reply → High tier, else
Normal; recommended-action rule unchanged. Old backdated forwards
arm Today only if genuinely unanswered; transient arming during
out-of-order thread forwarding accepted (stated).

Precedence when a Person qualifies for both an existing arm and
`client_replied`: `client_replied` wins the reason slot
(`waiting_since` = the inbound's occurred_at) — the reply is the
newer, more actionable signal. The `GET /api/today` `reasons[]`
vocabulary gaining `client_replied {occurred_at}` (+ the
fresh-reply ranking rule) is a **declared additive SLICE_003 §5
change** (pointer amendment rides with this spec, following the
006c precedent).

Realtime: reuse `person.changed` with new variant
`PersonChange::CorrespondenceCaptured` (declared additive SLICE_003
§6 vocabulary change; the web handler already invalidates
person/people/today for every person.changed, so old clients
degrade correctly). No new event type; no held-queue realtime (v1).

## 7. MIME & unwrapper extensions

`mime.rs` (inside the fence; additive `ParsedMail` fields):
`message_id`, `in_reply_to: Vec<String>`, `references: Vec<String>`,
`to_addrs`/`cc_addrs: Vec<String>` (lowercased bare addresses, no
display names; NUL-stripped; caps ~25 recipients / ~512-byte ids);
redacted Debug extended (counts only). Intake consumers unaffected.

`forward.rs` (`GmailInline`): (a) inner To/Cc extraction — REQUIRED
for outbound retroactive forwards — including continuation-line
folding for Gmail's wrapped headers (the current per-line loop has
none); (b) inner Date parsing — new English-locale parser for
Gmail's quoted format incl. the U+202F narrow no-break space;
unparseable → receipt-time fallback + backdated=false. English-only
(rung scope stands); attachment-style forwards stay out.

## 8. HTTP & web

New endpoints (additive; member-self `AuthContext`, NOT admin):
- `GET /api/capture/address` → `{address}` (render like intake's).
- `POST /api/capture/address/rotate` → `{address}` (mint-and-return,
  007g §5 semantics).
- `GET /api/capture/unmatched` → agent-only held list (cap 200 +
  truncated flag): id, counterparty_email, captured_at,
  direction_hint, status.
- `POST /api/capture/unmatched/{id}/link` `{person_id, add_contact
  _method: bool}` → writes the correspondence row. Direction is
  DETERMINISTIC and needs no fallback: linking asserts "the held
  counterparty address belongs to this Person", so the stored
  counterparty was the From → inbound, else outbound — inferred
  from the held row itself, not from re-matching current state.
  A link-created OUTBOUND row writes the auto-attempt exactly as
  the live pipeline does (D-042.4 consistency). Optionally adds the
  email contact method. Idempotent re-link with the SAME person →
  no-op 200; different person after linked → 409.
- `POST /api/capture/unmatched/{id}/dismiss` → status transition;
  idempotent.

`GET /api/people/{id}` history gains kind `correspondence`
(kind_rank 6): `detail {direction, agent: UserRef, captured_at,
via, backdated}` — NO addresses/subject/message-id (D-042.1/2).
Declared additive SLICE_002 §5 change; shape frozen here (types.ts
is the lane boundary).

Web: new "Email capture" view (address card + copy button + connect
instructions + reply-all etiquette & signature snippet per O-014
mitigation 1 + Rotate behind the consequence-stating ConfirmDialog
+ the unmatched list with link/dismiss). Deliberately NOT on
IntakeSettingsView (that is admin/tenant surface; this is the
agent's own credential). PersonDetailView timeline rendering +
icon/summary; TodayView `client_replied` label. Realtime TS union
gains the variant.

## 9. Authorization, isolation, observability, failure

- All capture endpoints: member-self; held list/link/dismiss
  strictly attributed-agent (404 others, admins included — D-042.3);
  timeline rows org-wide (D-042.1); raw readable by nobody.
- Isolation pins: org-A token never matches org-B Persons; same
  client email in two orgs → row only in the token's org; cross-org
  held/link/dismiss 404 probes; zero org-B writes.
- Forged-mail blast radius (stated honestly, post-adversarial):
  INBOUND forgery is always ONE false row/nudge (the ladder's step 2
  matches a single From-Person). OUTBOUND forgery (a forged
  member-From) creates one row + one auto-attempt PER MATCHED
  RECIPIENT — up to RECIPIENT_CAP (25) Today items cleared by one
  message (pinned; the mechanism is correct for legitimate mass-CC
  and repeated sends grant the same reach). Plus forged-Message-ID
  suppression of one later genuine row. No content exposure, no
  privilege, never a Person; occurred_at is clamped to
  [2000-01-01, receipt] (§4 upper clamp + adversarial floor), so no
  effect ranks beyond the present or renders degenerate history.
  Rotation is the remedy. Spam → held noise, agent-dismissible,
  FLOOD-CAPPED at 500 live held rows per agent (beyond it: raw
  stored encrypted, no held row, span outcome
  capture_held_overflow — keeps un-listable plaintext counterparty
  addresses from accumulating past the agent's reach, D-015 §4).
  Cross-tenant: the link endpoint org-validates person_id via
  lock_person (adversarial H1), and the Today person-joins carry
  org predicates as defense-in-depth.
- Observability: `capture.inbound_email` span — outcome vocabulary
  (`captured|capture_duplicate|capture_unmatched|rejected|...`),
  direction, matched flag, forwarded/style/depth, byte_len, own ids
  only. NEVER subjects, addresses, message-ids, tokens.
  Tracing-capture tests over matched/unmatched/forward paths.
- Failure: DB down → 503 → worker throw → MTA retry; constraint-
  backed dedup under concurrency (two identical deliveries → one
  row, one publish); crash-after-raw rescued by redelivery.

## 10. Acceptance criteria

1. Real Gmail CC (agent → client) → outbound `correspondence` row
   on the Person timeline + auto `contact_attempted` clearing the
   Today item — within seconds of arrival.
2. Real client reply-all → inbound row + `client_replied` Today item
   for the assignee + realtime invalidation observed.
3. Retroactive forward of an older exchange → row at the INNER-Date
   position with correct direction + backdated flag; re-forward → no
   duplicate; forward of an already-CC-captured message → no
   duplicate (References dedup) — all live-verified.
4. Unmatched correspondent → held row visible ONLY to the attributed
   agent; link → timeline row (+ contact method when requested);
   dismiss → gone; admins/others 404.
5. Forged/rotated-out token → 200 rejected, nothing stored; rotation
   flips live (old dies, new flows).
6. Address-grammar disjointness unit-pinned both directions; intake
   suites pass UNMODIFIED (explicit criterion).
7. Tenant-isolation probes (§9); tracing-capture clean.
8. Deactivated member's token stops resolving; reactivation restores
   the SAME address (mint-if-absent pinned); backfill-mint pinned on
   a populated fixture.
9. Future-date clamp pinned: a Date far in the future lands at
   receipt time (both directions — no years-long Today suppression,
   no uncleareable nudge).
10. Concurrency: two identical simultaneous deliveries → one raw,
    one fact row, one publish (007b race-test pattern).
11. History detail for `correspondence` contains NO subject, address,
    or message-id KEY (shape pin — D-042.2's testable half); held
    list caps at 200 with the truncated flag.
12. Gates: `./scripts/check` + `./scripts/check-db` + web suite; live
    walkthrough per §11.

## 11. Live walkthrough sketch

Needs a second real mailbox as the "client" (a consumer Gmail or
the eospia/choravia fixture domains). Agent = seeded alice with her
minted capture address (note: ladder step 1 — From equals a member
login email — fires live only if alice's login email is a real
sendable mailbox; otherwise walkthrough (a) exercises step 3 and
step 1 is DB-test-covered — either is acceptable, stated): (a) CC a real outbound mail to a known
Person → timeline + Today cleared; (b) client replies-all →
`client_replied` on Today in realtime; (c) forward last week's
thread → backdated rows, no duplicates; (d) CC a mail to a brand-new
address → held queue → link → timeline; (e) rotate → old address
dead, new flows.

## 12. Lane, size, checks

Single lane (backend dominates; the frozen §8 shapes make a later
web split possible but not planned), branch
`slice-009-correspondence-capture`, sole migration owner
(`20260904000001`). Size M–L. Trim levers if needed (in order):
drop rotation; held queue → drop+counter; drop auto-attempt — BUT
each lever cuts an accepted D-042 decision (5/3/4 respectively), so
exercising one requires a D-042 amendment at that time, not a
silent spec change.
Gates: both check scripts + web; independent review + adversarial
testing; live walkthrough BEFORE the commit gate; Phase 9 approvals.

## 13. Safe defaults adopted (reviewer/user may veto)

(a) `save-` prefix (cosmetic; disjointness is by token length);
(b) 12-char token; (c) `client_replied` High-if-fresh ranking;
(d) no held-queue realtime; (e) deactivated-token behavior;
(f) first-writer-wins dual-capture attribution; (g) inbound rows
only for the From-Person; (h) English-only inner-Date parsing with
receipt fallback; (i) held cap 200; (j) rotation included;
(k) matching without the intake advisory lock (never-create);
(l) mailing-list edge classified outbound; (m) `via` vocabulary
('cc','forward'); (n) backfill-mint addresses for existing active
members.
