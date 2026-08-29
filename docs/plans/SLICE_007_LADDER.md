# Slice 007 — Email lead intake: the ladder

Status: RUNGS a–h1 ALL COMPLETE AND MERGED (007a…007h1, last merge
`105f730`, 2026-08-25; see docs/plans/PROJECT_STATE.md ledger). h2+
(one rung per additional pinned email format) remains open, driven by
fixtures as they arrive. Round-robin routing, mentioned below as a
possible later rung, shipped separately as Slice 008 (D-041).
Original status: PLANNED + REVIEWED (planner + reviewer 2026-08-23;
007c split applied).
Source: DECISION_LOG O-014 (all groundwork), D-012, D-021, D-028/D-034.
Rule (user, 2026-08-23): **small rungs, each fully tested and walked
through before the next.** Each rung is its own spec (`SLICE_007x.md`),
its own PR, its own `check`/`check-db` gate and live walkthrough — the
006/006a/006b/006c pattern. Rung specs are written when the previous
rung merges, not in advance, so they can absorb what was learned.

## Facts about today's intake that shape the ladder

- `receive_inquiry` (crm-app `domain/commands/receive_inquiry.rs`):
  Phase A seals + inserts `raw_payload` (`payload_format` hard-coded
  `generic_v1`; idempotency `UNIQUE (organization_id, source,
  content_hmac)`), Phase B locks the row and parses JSON; anything else
  lands Unresolved. Raw email today would be `invalid_json`.
- Every command has a mandatory user actor (`CommandContext`,
  `FactEnvelope::for_command` = `ActorKind::User`); routing falls back to
  the actor. The DB already permits a system actor; `routing_decision.
  strategy` CHECK does not yet know an unattended strategy. Unassigned
  People are invisible on Today. → unattended intake needs a system
  actor **and** a routing default (rung c), never before.
- Unresolved is metadata-only, no detail/retry/discard (SLICE_002 §12
  deferred them). `unresolved_reason` is free TEXT.
- `organization` has no slug/settings; `create_organization` is the
  only creation path (platform route, `crm-admin create-organization`;
  dev seeding drives the platform route via `scripts/seed_dev.py`).
- Webhook precedent: `routes/livekit_webhook.rs` — outside CORS,
  signature verified in crm-app, Organization derived server-side.
- Provider seam: `crm_operator::InferenceProvider`; crm-app must not
  depend on crm-operator (D-034 fence). Only crm-api sees both.

## The rungs

| Rung | Outcome | Size |
|---|---|---|
| **007a** Organization intake address | Admin sees "Forward lead notifications to `leads-k7f3q2wd@acme-realty.elysianfeld.com`" (copy button). No mail yet. Stored as `(intake_slug, intake_token)`; rendered by config (`CRM_INTAKE_MAIL_DOMAIN`, `CRM_INTAKE_ADDRESS_SCHEME` = subdomain \| local_part); recipient parser accepts both forms so the scheme can flip later. | S |
| **007b** Inbound endpoint → encrypted raw → Unresolved, zero parsing | `POST /inbound/email` (bearer `CRM_INBOUND_EMAIL_SECRET`, outside CORS, 2 MiB) `{recipient, raw: base64 RFC 822}` → tenant from the recipient token only → `raw_payload` (`source=email`, `payload_format=rfc822_v1`, `origin=webhook`) → Unresolved `email_unparsed` + realtime. **Phase-A-only entry point** (`receive_inbound_email` takes no `CommandContext`, writes no facts, publishes the Unresolved event itself) — so it does NOT pull the system actor forward from 007c. Unknown recipient → 200 rejected, nothing stored. `scripts/inbound-email` + first `.eml` fixtures. | S–M |
| **007c** System actor + unattended routing | `IntakeActor::System` / `FactEnvelope::for_system` (origin `webhook`), routing strategies `organization_default` / `unassigned` (persistence-contract change, declared), org setting `intake_default_assignee_user_id` + `PUT /api/organization/intake-settings` + the settings-page dropdown. **Testable alone**: today's `generic_v1` JSON fed through the system-actor path → Person on the default assignee's Today, `inquiry_received` with no actor. No email parsing yet. | S–M |
| **007d** One pinned deterministic format → real inquiries | MIME layer (`mail-parser`, wrapped in `email/mime.rs`), `EmailFormat` registry, the cypressbayrealty.com contact-form template (we author it) → Person, Inquiry (`source=website`), facts, Today via 007c's routing. `complete_intake` refactor for reuse. Unknown format → Unresolved `email_unrecognized_format`. | M |
| **007e** Unresolved workbench | Admin opens a row: subject/from/date/text (decrypted on demand, never listed, never logged); **Try again** (re-run Phase B — also rescues SLICE_002's stuck `pending`); **Discard** (`resolution=discarded`, actor+time on the row). Members keep metadata only. | M (2 lanes) |
| **007f** LLM extraction for unrecognised formats | crm-app `LeadExtractor` trait; crm-api adapter over `crm_operator::InferenceProvider` (D-028 §5 pattern, no new crate/edge); background worker (`SKIP LOCKED`, backoff, max attempts); strict schema; anti-hallucination (every email/phone must appear in the input text); `confidence ≥ 0.7`; `is_lead=false` → `not_a_lead`; PII-free `intake_extraction` ledger. Provider down → waits, never lost. | M–L |
| **007g** Real receiving on elysianfeld.com | Per D-039: Cloudflare Email Routing on the registered `leads.` subdomain, catch-all → a committed Email Worker relaying raw MIME to the frozen `/inbound/email` (same bearer; no provider adapter route or signature scheme — none exists on this path), config flip to local-part, `.env.example`; **token rotation**. Mostly ops. | S–M |
| **007h** Portal parsers as fixtures arrive | One rung per format (Zillow, Realtor.com, Homes.com, forwarded-wrapper unwrapper): fixture `.eml` + `formats/<name>.rs` + tests incl. "LLM path not invoked". Repeatable. Sub-rungs numbered `007h1..`; **007h1 = forwarded-wrapper (Gmail inline)** per D-040 — `docs/specs/SLICE_007h1.md`, S–M (the SenderTrust type + trait-signature change push it past plain S). | S each (h1: S–M) |

Dependencies: a → b → c → d → e → f; g after b (before h so
design-partner mail can flow); h parallelisable. (Reviewer 2026-08-23:
the original c bundled two rungs — split into c and d.)

## Cross-rung decisions

| # | Decision | Class | Default / when |
|---|---|---|---|
| 1 | Address scheme final form | **DECIDED — D-039** (user, 2026-08-25) | The wildcard check found no free/incumbent wildcard-subdomain inbound path → flipped to local-part: `<slug>-<token>@leads.elysianfeld.com`, received via Cloudflare Email Routing (registered subdomain, catch-all → Email Worker → the frozen `/inbound/email`). Token 8 chars `[a-z2-7]`, plaintext to admins, never logged. Slug immutable. |
| 2 | Inbound endpoint auth | SAFE_DEFAULT (007g half resolved by D-039) | Deployment bearer secret on the provider-neutral endpoint (007b). With the Email Worker relay there is no third-party webhook to verify — the worker authenticates with the same bearer; a provider-signature adapter returns only if a third-party inbound provider is ever adopted. Tenant from the recipient token only. |
| 3 | Where the extraction LLM call lives | SAFE_DEFAULT | crm-app trait + crm-api adapter (as ToolBackend). Revisit → shared `crm-inference` crate only if a third consumer appears. |
| 4 | Sending inbound lead mail to Groq | **DECIDED — D-038** (user, 2026-08-25) | Blessed with scope: text-only, ≤16 KiB, subject + sender domain, no org/agent identifiers, no tools; Groq on the subprocessor list. |
| 5 | System actor + new routing strategies | SAFE_DEFAULT, declared contract change in 007c | `IntakeActor::System`, `FactEnvelope::for_system`, CHECK extension. |
| 6 | Who receives unattended email leads | **DECIDED — D-035** (user, 2026-08-24) | Org-admin-set default assignee; unset → Person created unassigned (People, not Today) + settings warning. Round-robin later. |
| 7 | Who may read raw unresolved content | **DECIDED — D-037** (user, 2026-08-25) | Org admins only, on demand. |
| 7a | Who may read the intake address/token | SAFE_DEFAULT (adopted 2026-08-23) | Org admins only (endpoint + page agree); the token is the anti-forgery secret. |
| 8–13 | Backfill in migration; `mail-parser` dep (wrapped); 2 MiB/413; `received_at` = receipt time; `inquiry.source` = detected source while `raw_payload.source` stays `email`; rotation/spam auto-discard/round-robin/O-012 key migration | IMPL / SAFE / LATER | as stated |

**Confirmation to record at 007d:** O-014 says forged mail "must land in
Unresolved, never silently create". A forged message that *matches* a
pinned format with a valid contact method **will** create a Person —
that is what a valid format means. Mitigations: the unguessable token;
restricting each pinned format's `matches()` to its real sender domain;
sender authentication (SPF/DKIM) deferred per SLICE_007g §6: the
stored raw bytes are the carrier, and the 007g live walkthrough
(2026-08-25) RESOLVED the known-issue caveat — Email Routing → Worker
delivery does stamp `Authentication-Results` from mx.cloudflare.net
(observed live: dkim=pass, spf=pass, dmarc=pass, arc=pass), so 007h
may rely on those verdicts; `ParsedMail.authenticated_sender` lands
with its first consumer.

## Explicitly not in this ladder

Correspondence capture (O-014 #2, OAuth, CASA), sending (#3, O-006),
transactional mail (#4), migration reconstruction (#5), O-012/O-013,
round-robin/rules routing, SMS intake, Operator tools over intake,
inbound calls, any mailbox for the CRM.

Round-robin note (user, 2026-08-24, during 007c spec discussion): the
intake settings card is expected to grow into a routing-*mode* picker
(single default assignee | round-robin | rules later). Round-robin
becomes **its own rung after 007d**, once unattended leads actually
flow — not part of 007c and not before email parsing exists. 007c's
single routing decision point and free-text `routing_decision.strategy`
leave the seam open (a `round_robin` strategy is additive).
