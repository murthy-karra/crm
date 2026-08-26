# Slice 007h1 — Forwarded-wrapper unwrapping (Gmail inline)

Status: APPROVED (user, 2026-08-25). Independently reviewed
(ready-with-fixes; all fixes applied: S-1 verdict-home supersession,
S-2 detect-first order, I-1..I-6). First rung of the 007h family
(`docs/plans/SLICE_007_LADDER.md`): formats are pinned as real
fixtures arrive, and this is the one fixture family the user can
generate today by forwarding any email from Gmail.

Decisions in force: D-035 (routing), D-036 (forged-mail posture),
D-038 (LLM scope), D-039 (receiving path), **D-040** (forwarded mail
may match pinned formats, with typed provenance — this rung's
charter). Amendment declared per AGENTS §11: SLICE_007d §4's
"forwarded copies of form mail are not the pinned flow" is superseded
by D-040 (the direct-path subject-equality pin in `cypress_bay.rs`
stands as the *Direct*-arm pin; forwarded copies now reach the format
via the unwrapper).

## 1. User-visible outcome

An agent forwards a lead email from their own mailbox (Gmail "Fwd:")
to their org's intake address. Today the CRM sees the forwarding
decoration as the message. After this slice:

- a forwarded copy of a **pinned-format** mail (Cypress Bay form
  today; portals later) parses deterministically — Person + Inquiry
  on Today via D-035, no Groq call, no human review;
- a forwarded copy of an **unknown-format** lead goes to LLM
  extraction seeing the clean inner message (inner subject, inner
  sender domain, inner text) instead of banner noise — better
  extractions, same D-038 scope;
- anything unrecognizable as a forward behaves byte-identically to
  today (whole-message fallback; the status quo is the floor).

## 2. In / out of scope

**In:** Gmail inline forwards, English locale: the
`---------- Forwarded message ---------` banner, a `From:/Date:/
Subject:/To:` block, blank line, inner body — in both text/plain and
HTML-only mails (via the existing HTML→text fallback). A
`ForwardStyle` mini-registry (static slice, first match, mirroring
`format.rs`) so later styles are additive S-rungs. Iterative unwrap
with **depth cap 3** (innermost view wins; deeper → last view, no
panic). The `SenderTrust` type threaded through `detect()` and
`EmailFormat::matches()`.

**Out (explicit):** Outlook inline (`-----Original Message-----`) —
later h-rung. Attachment-style `message/rfc822` forwards — later
h-rung (requires a `mime.rs` fence extension; noted: the only style
whose inner DKIM survives for re-verification — the future strong
path). Reply chains, signature/disclaimer stripping, non-English
locales. `authenticated_sender`/verdict extraction (D-036's third
layer — first consumer is a portal rung). Forwarder attribution /
routing to the forwarding agent (recorded deferred decision; D-035's
territory; needs a verified agent-address mapping that doesn't
exist). No migration, no new persisted state, no frozen-contract
changes (`/inbound/email` envelope, `POST /api/inquiries` untouched).

## 3. Architecture — a pre-processing layer, not a format

New `email/forward.rs` in crm-app (pure text processing on
`ParsedMail`; **no `mail_parser` import** — the directory-walk fence
test in `email/mod.rs` passes unchanged):

- `forward::resolve(mail: ParsedMail) -> (ParsedMail, SenderTrust)`.
  Conservative trigger: banner present AND a parseable inner `From:`
  address AND non-empty inner body — else no-op (fake-"Fwd:" subjects
  with no real banner, and banner-with-empty-inner-body, fall through
  untouched). Inner view: inner from_addr lowercased (as the mime
  wrapper does), inner subject, inner body = remainder after the
  header block. NUL-stripping already happened at the fence.
  Linear-time; pathological many-banner bodies stay bounded (depth
  cap).
- **Detect-first order (reviewer S-2):** `detect` runs on the
  **Direct** view FIRST; `forward::resolve` runs only when no format
  matched directly, and `detect` then re-runs on the ForwardedClaim
  view. This preserves 007d behavior byte-for-byte for every
  currently-matching direct mail — in particular a genuine form mail
  whose `Message:` field quotes a pasted forward banner must NOT be
  unwrapped away from its deterministic parse (fixture-pinned). Cost:
  one extra registry pass on the no-direct-match path only.
- `SenderTrust` (crm-app, intake email module):
  `Direct` — the RFC 822 From of the message as delivered; the future
  home of Authentication-Results verdicts. `ForwardedClaim { depth }`
  — the From line is quoted body text; **no field can carry
  verdicts** (D-040's compile-error guarantee).
- The trait splits the match decision by trust arm:
  `EmailFormat::matches_direct(&ParsedMail)` and
  `matches_forwarded(&ParsedMail, depth)` — two required methods, so
  every format (present and future portal rungs) must answer BOTH
  arms explicitly; D-040's "separate explicit decision per format" is
  enforced by the trait shape, not by convention (reviewer I-4). An
  internal crm-app trait change, declared, mechanical for
  `cypress_bay`: under D-040 it applies the same domain + subject
  rules on both arms against the given view. Existing tests are
  mechanically re-targeted to `matches_direct` (call sites change;
  assertions don't), and the `formats/cypress_bay.rs` "forwarded
  copies are not the pinned flow" comment is rewritten per the D-040
  supersession.
- **Verdict home (reviewer S-1, declared supersession):** whenever
  Authentication-Results extraction lands (a later rung), verdicts
  populate `SenderTrust::Direct` — NEVER a `ParsedMail` field. This
  amends SLICE_007g §6's and the ladder's older
  "`ParsedMail.authenticated_sender`" wording per AGENTS §11: a
  verdict on `ParsedMail` would be structurally inherited by the
  unwrapped inner view and defeat D-040's compile-error guarantee.

**One resolve, two consumers (the critical seam):**
1. `email::parse_payload` (`email/mod.rs`): `mime::parse` →
   `forward::resolve` → `detect` → extract/normalize. Delivery
   (`receive.rs` Phase B) and 007e workbench retry get unwrapping
   automatically.
2. The 007f extraction worker resolves through the **same**
   `forward::resolve` before `build_input` (the worker calls resolve
   on its re-parsed mail and passes the resulting view — `build_input`
   ownership/signature is a lane detail, reviewer I-1). The seam is
   pinned two ways (reviewer I-2): behaviorally — the same forwarded
   fixture yields the inner subject/domain/text through BOTH
   `parse_payload` and the worker's input builder — and by a
   fence-style source check that no second banner-matching
   implementation exists outside `forward.rs`.

Nesting: `Fwd(Fwd(pinned))` unwraps toward the innermost message; a
matched inner format then runs exactly as 007d Phase B (System actor,
facts, D-035 routing, idempotency over the raw OUTER bytes —
unchanged).

## 4. Security & trust (D-040 pins)

1. Inner content never inherits outer authentication —
   structurally: verdicts can only ever live on `Direct`.
2. No verdict extraction/consumption this rung.
3. When a later rung tightens a format's Direct arm with SPF/DKIM,
   its ForwardedClaim arm is a separate explicit per-format decision;
   the two-arm `matches()` signature forces each format to answer
   both.
4. Accepted blast radius (restated from D-040): a forged forward can
   create one bogus recognizable lead AND stamp the matched format's
   source label on the immutable `inquiry_received` fact. The LLM
   path keeps `source='email'` as today.
5. Observability: spans gain `forwarded` (bool), `forward_style`
   (static name), `forward_depth` — never subjects, senders, or
   body text (inner or outer). Declaration sites (reviewer I-5 —
   tracing silently drops undeclared fields, the 007d §4d pitfall):
   the inbound route's `#[instrument]` in crm-api and the extraction
   worker's `intake.extract` span; both named here so neither is
   forgotten.

## 5. LLM interplay (D-038 — interpretation declared, no amendment)

Unwrap succeeded, no pinned inner match → `ExtractionInput` is built
from the inner view: inner subject, inner claimed sender **domain**,
inner text (≤ 16 KiB, truncation flagged — shape unchanged, values
switch). Anti-hallucination stays consistent because validation runs
against the same input the model saw. Safe default: inner text only —
an agent's preamble note above the banner is dropped (rare
contact-info-in-preamble edge accepted; alternative "preamble +
inner body" available for veto). Unwrap failure / fake-Fwd /
non-English → whole-message extraction exactly as today. No prompt
change, no `ExtractionInput` shape change, no crm-operator touch.
Unresolved reasons: reuse `email_unrecognized_format` and
`no_contact_method` — no new reason, so the 007f eligibility partial
index and worker predicate are untouched.

Known accepted edge (observed in the real-fixture reconciliation,
2026-08-25): Gmail places the FORWARDER's own signature after the
quoted message at the same plain-text level, so it is part of the
inner body — the LLM could attribute the forwarder's phone to the
lead. Accepted for this rung (the anti-hallucination gate still
requires the value to appear in the input; a mis-picked contact is
within LLM-path tolerance); the HTML `gmail_quote` structure could
separate signature from quote — a later-rung refinement, recorded
here so it is a decision, not a surprise.

## 6. Acceptance criteria

1. A real Gmail forward of the pinned Cypress Bay form mail →
   Person + Inquiry end-to-end (System actor, D-035 routing), **LLM
   path not invoked** (the ladder's required test).
2. A Gmail forward of an unknown-format lead → extraction input is
   the inner subject/domain/text (unit test on the shared seam).
3. Fake-"Fwd:" subject with no real banner, AND a real banner with an
   empty inner body (`gmail_fwd_empty_inner.eml`) → unwrapper no-op;
   behavior byte-identical to today. A genuine DIRECT form mail whose
   `Message:` field quotes a forward banner still parses
   deterministically via detect-first (S-2 fixture).
4. Forged inner headers (inner From claims cypressbayrealty.com,
   hand-crafted) → creates the D-040-accepted lead via the
   ForwardedClaim arm; never a trust upgrade; type-level pin that
   `ForwardedClaim` cannot carry verdicts.
5. Nested forwards unwrap to the innermost within depth 3; deeper →
   last view, bounded work, no panic.
6. HTML-only Gmail forward: banner survives the HTML→text fallback
   and unwraps (fixture-proven).
7. Size limits unchanged (2 MiB endpoint, 16 KiB LLM input);
   many-banner pathological body stays linear.
8. Duplicate/redelivery idempotency (HMAC over raw outer bytes),
   tenant isolation, and span-content rules re-pinned over the new
   paths; 007e retry on a pre-existing forwarded row resolves
   through the unwrapper.
9. Frozen contracts untouched: `/inbound/email` envelope,
   `POST /api/inquiries` suites pass unmodified.
10. Live walkthrough: a real Gmail forward through the real 007g
    receiving path (leads.elysianfeld.com) lands on Today.

## 7. Required tests & fixtures

Fixtures under `backend/crates/crm-api/tests/fixtures/email/`,
user-generated (Gmail → forward → Show original → download .eml),
sanitized of real third-party PII before commit:
`gmail_fwd_cypress_bay.eml`, `gmail_fwd_unknown_lead.eml`,
`gmail_fwd_html_only.eml`, `fake_fwd_subject.eml`,
`gmail_fwd_forged_inner.eml` (hand-crafted), `gmail_fwd_nested.eml`,
`gmail_fwd_empty_inner.eml`, and the S-2 pin
`cypress_bay_banner_in_message.eml` (hand-crafted direct form mail
whose Message field quotes a forward banner). Unit tests in `forward.rs` +
`formats/cypress_bay.rs`; seam test binding `parse_payload` and
`build_input` to one resolve; db tests extending
`db_inbound_email_intake.rs` / `db_intake_extraction.rs` for criteria
1, 2, 4, 8.

## 8. Lane and checks

Single lane, one writer, branch `slice-007h1-forwarded-wrapper` off
`main`. No migration. Gates: `./scripts/check` + `./scripts/check-db`;
independent review + adversarial testing; live walkthrough BEFORE the
commit gate (Phase 9 approvals as always).

## 9. Safe defaults adopted (reviewer/user may veto)

(a) Gmail-inline-only, English markers, `ForwardStyle` registry;
(b) depth cap 3; (c) conservative unwrap trigger with whole-message
fallback; (d) reuse `email_unrecognized_format` (no migration);
(e) inner-text-only LLM input (preamble dropped); (f) provenance in
spans only, nothing persisted; (g) no `authenticated_sender` work;
(h) the split `matches_direct`/`matches_forwarded` trait methods and
the detect-first-then-resolve pipeline order (both internal,
declared);
(i) forwarder attribution/routing deferred (recorded, D-035
territory); (j) size S–M noted against the ladder's "S each"
estimate.
