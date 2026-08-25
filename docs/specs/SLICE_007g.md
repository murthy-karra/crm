# Slice 007g — Real receiving on elysianfeld.com

Status: APPROVED (user, 2026-08-25; planner pass + live provider
research + independent review same day, reviewer verdict "ready
with amendments" — every factual claim verified against the code, no
blocking findings; all amendments applied: the worker's derived
~1.4 MiB size threshold + explicit 4xx matrix, the conditional
Authentication-Results escalation (approval amends D-036's "provider
adapter from 007g" wording), the node-tested base64 chunker, the
completed fact-table envelope, ladder/007a stale-pointer fixes, the
rotation tracing-capture test, runbook preconditions.)
Ladder: docs/plans/SLICE_007_LADDER.md rung g. Builds on 007b–007f
(all merged; `32239dc`). Targets **D-039** (local-part scheme;
Cloudflare Email Routing + Email Worker relay — resolves cross-rung
decisions 1 and the 007g half of 2), D-016/D-024/D-025 (the dev
tunnel; the dashboard is the routing source of truth), D-036 (the
SPF/DKIM item — see §6's declared deviation), D-037 (admin-only
intake-address surface), SLICE_007a (scheme-neutral storage,
`parse_recipient` accepts both forms), SLICE_007b (the frozen
`/inbound/email` contract — untouched and now the real receiving
endpoint), AGENTS.md §11/§13.

## 1. User-visible outcome

A real email sent from any mailbox to an Organization's intake
address — now rendered as **`<slug>-<token>@leads.elysianfeld.com`**
— arrives through real DNS and Cloudflare Email Routing into the same
pipeline 007b–007f built: a pinned format becomes a Person on Today;
an unknown format goes through LLM extraction; junk becomes "Not a
lead"; everything is preserved raw. Additionally, an org admin can
**rotate** the intake token from Manage → Intake (break-glass for a
leaked/spammed address); the old address stops working immediately
and the page shows the new one. `scripts/inbound-email` and the
frozen `/inbound/email` endpoint remain the dev/test path, unchanged
— they are also the production receiving endpoint, fed by the worker.

Live walkthrough (§10 runbook first): send a real email from a
personal mailbox to a seeded org's copied address → the Person
appears on the default assignee's Today (or the mail lands in
Unresolved per its format) within seconds; send to a forged-token
address → nothing anywhere; rotate the token → mail to the old
address vanishes (200-rejected at the endpoint), mail to the new
address flows; the walkthrough captures one real message's stored
headers into the 007h notes (the `Authentication-Results` fact).

## 2. In / out of scope

In: the committed Cloudflare **Email Worker** relay
(`infra/email-worker/`), the config flip to `local_part` (env values;
`.env.example` comments incl. the D-025 drift fix), **token
rotation** end-to-end (migration: UPDATE grant + the
`intake_token_rotated` fact table; domain command; `POST
/api/organization/intake-address/rotate`; the Manage → Intake Rotate
button + confirm dialog; audit), the ops runbook (§10), tests, the
live walkthrough.

Out: any change to the frozen `/inbound/email` envelope or
`receive_inbound_email`; a crm-api provider-adapter route or provider
trait (none is needed — D-039; returns only if a third-party inbound
provider is adopted); SPF/DKIM *consumption* (007h; see §6);
dual-token rotation grace (rejected, §7); spam auto-discard;
production (k8s) ingress; more formats (007h).

## 3. The Email Worker relay (`infra/email-worker/`)

A small committed Worker (`worker.js` + `wrangler.toml`), deployed by
hand with `wrangler` (runbook §10):

- Handler: `async email(message, env)` — reads the raw RFC 822 bytes
  from `message.raw` (chunked base64 encoding; never `btoa` on a
  megabyte string), takes the **envelope recipient** from
  `message.to` (the RCPT TO — not the To: header), and POSTs the
  frozen contract `{"recipient", "raw"}` to
  `env.CRM_INBOUND_API_URL` (`https://api.tarams.org/inbound/email`
  in dev) with `Authorization: Bearer env.CRM_INBOUND_EMAIL_SECRET`
  (a Worker secret — the same value as the API's; the worker is the
  "provider", authenticated exactly as cross-rung decision 2's
  deployment-bearer posture).
- Response handling: 2xx → done (accepted/duplicate/rejected are all
  200 by design — the endpoint is the oracle-free judge); network
  error or 5xx → **rethrow** so Cloudflare's mail layer temp-fails
  and the sending MTA retries (at-least-once; our
  `(org, source, content_hmac)` idempotency absorbs redelivery). The
  exact retry semantics of a throwing Email Worker are a live-verify
  item in §10 — if throwing turns out to hard-bounce,
  the fallback posture is `message.setReject(...)` on permanent
  failures only and rethrow otherwise, adjusted during the
  walkthrough and recorded.
- Oversize: the endpoint's 2 MiB cap is on the **HTTP body**, and
  base64 + JSON inflate raw bytes by ~4/3 — so the worker's reject
  threshold is **the largest raw size whose encoded body fits the
  endpoint cap (~1.4 MiB, a derived constant with the derivation in a
  comment, not a magic number)**. Over it →
  `message.setReject("message too large")` — an honest bounce beats a
  silent 413.
- 4xx handling (explicit, neither silent loss nor infinite retry):
  413/400 from the endpoint → `setReject` (an honest bounce — the
  mail can never succeed); 401/403 → **rethrow** (a misconfigured or
  rotated-out-of-sync bearer; retry while a human fixes the secret).
- The worker holds no other logic: no parsing, no filtering, no
  logging of content (Worker logs may carry message ids/sizes only).

## 4. Persistence — rotation (one migration, `20260902000001_intake_token_rotation.sql`)

```sql
GRANT UPDATE (intake_token) ON organization TO crm_app;

CREATE TABLE intake_token_rotated (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organization (id),
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    actor_user_id UUID REFERENCES app_user (id),
    CHECK ((actor_kind = 'user') = (actor_user_id IS NOT NULL)),
    on_behalf_of_user_id UUID REFERENCES app_user (id),
    origin TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correlation_id UUID NOT NULL,
    causation_id UUID,
    corrects_id UUID REFERENCES intake_token_rotated (id)
);
CREATE INDEX intake_token_rotated_org_occurred_idx
    ON intake_token_rotated (organization_id, occurred_at);
CREATE INDEX intake_token_rotated_org_correlation_idx
    ON intake_token_rotated (organization_id, correlation_id);
GRANT SELECT, INSERT ON intake_token_rotated TO crm_app;
-- append-only + no-TRUNCATE triggers via reject_mutation(), the
-- standard fact-table pattern. NEVER a token value, old or new.
```

The 007a migration deliberately withheld the `intake_token` UPDATE
grant ("rotation is a later rung") — this is that rung. The fact row
carries the standard envelope (who rotated, when, correlation), never
token material; the 007a admins-only read rule (D-037/7a) already
covers who can see the current address.

## 5. Domain & HTTP — rotation

`domain/intake/address.rs` (or a sibling) gains:

```rust
pub async fn rotate_intake_token(
    pool, publisher_not_needed, ctx: &CommandContext,
) -> Result<(), RotateError>
// mint_intake_token() (the 007a mint, [a-z2-7]{8}) → UPDATE
// organization SET intake_token = $new WHERE id = ctx.organization_id
// + insert the intake_token_rotated fact (FactEnvelope::for_command)
// in ONE transaction. No realtime event (nothing queue-visible
// changes); the web mutation invalidates the address query. Another
// admin's open tab shows the old address until refetch — accepted.
```

`POST /api/organization/intake-address/rotate` — `OrgAdminContext`
(the 7a rule), empty body, 200 returning the same
`{"address","scheme"}` shape as the existing GET (the *new* address);
401/403/503 as elsewhere. Additive; no frozen contract touched.

**Rotation semantics — single-token, immediate invalidation** (safe
default, veto-able; the trade-off is customer-visible): rotation is
break-glass for a leaked/spammed address, and a grace window would
keep the leaked token working, defeating the purpose. Consequence
stated plainly in the confirm dialog and here: mail sent to the old
address after rotation is **silently discarded** (the endpoint
200-rejects it; senders get no bounce — including mail accepted by
Cloudflare before the rotation but relayed after it) until every
forwarding rule is updated to the new address. The dual-token grace alternative
(`previous_intake_token` + expiry, two constant-time compares with
uniform dummy work) was analyzed and rejected: it roughly doubles the
lookup path's security surface for a break-glass feature. Rotation
touches zero code in `receive.rs` — the lookup simply reads the new
value.

## 6. SPF/DKIM (`authenticated_sender`) — declared deviation

The ladder's D-036 note promised "`ParsedMail.authenticated_sender`
carried from the provider adapter from 007g on". With the D-039 path
there is no provider payload carrying verdicts — whatever Cloudflare
records lands (if anywhere) as headers **inside the delivered raw
MIME**, which we store byte-for-byte. Nothing consumes verdicts yet
(007d's `matches()` takes only `ParsedMail`; tightening is 007h
work). Therefore: **007g adds no `authenticated_sender` code**. The
verdict data — if present — is durably in the stored bytes, and 007h
(the first consumer) extends `mime.rs` to extract
`Authentication-Results` filtered by a configured trusted
authserv-id. **Known risk, researched (2026-08)**: an open Cloudflare
issue reports that Email Routing → Worker delivery can arrive with NO
`Authentication-Results` header (only `arc=none`), even though the
docs say the MX stamps one — i.e. the "durable in stored bytes"
premise may be false on exactly this path. The §10 walkthrough
captures one real stored message's headers so 007h starts from facts,
not guesses; **if the header is absent, that is recorded in the
ladder as a conditional blocker on D-036's SPF/DKIM tightening**
(options then: wait on the Cloudflare fix, or an alternate verdict
source) — escalated before 007h relies on it, never silently
dropped. Approving this spec explicitly amends D-036's and the
ladder's "carried from the provider adapter from 007g on" wording to
this deferred-with-escalation posture (fallback design if headers are
absent: DKIM re-verification from the stored bytes, or accepting
token + sender-domain matching as the permanent gates — a 007h-or-
later decision). This preserves
D-036's substance (verdicts available to tighten domain claims, from
real-mail arrival onward, because the raw bytes are kept) while
moving the field to the rung with its first consumer — declared here,
per AGENTS.md §11.

## 7. Config & env

- `.env` (values; runbook): `CRM_INTAKE_ADDRESS_SCHEME=local_part`,
  `CRM_INTAKE_MAIL_DOMAIN=elysianfeld.com` (the render becomes
  `<slug>-<token>@leads.elysianfeld.com`; the `leads.` label is the
  scheme's fixed prefix in code). No new API env vars — the worker
  reuses `CRM_INBOUND_EMAIL_SECRET`'s value as a Worker secret.
- `.env.example`: comment updates only — the scheme note, and the
  pre-existing D-025 drift fix (the "External connectivity" comment
  still claims the tunnel is file-managed; two lines, declared here).
- Worker config (`wrangler.toml` + secrets): `CRM_INBOUND_API_URL`
  (var), `CRM_INBOUND_EMAIL_SECRET` (secret, set via
  `wrangler secret put`).

## 8. Web

Manage → Intake (`IntakeSettingsView.vue`): a **Rotate address**
button beside the existing address card, behind a `ConfirmDialog`
whose copy states the immediate-invalidation consequence ("the
current address stops working immediately; update every forwarding
rule to the new address"). On success: the address query invalidates
and the card shows the new address. Vitest pins the dialog copy and
the confirm→POST→invalidate flow. Nothing else changes (the address
render already follows the server's `scheme`).

## 9. Authorization, isolation, observability, criteria, tests

- Rotate: `OrgAdminContext`; member 403; org-B admin rotating never
  touches org A (org solely from the session); pinned by DB tests.
- After rotation: delivery to the old address → 200 rejected, nothing
  stored; to the new address → flows end-to-end. Pinned via
  `/inbound/email` DB tests (no Cloudflare needed).
- The rotation span records org id + actor id, never tokens; the
  fact table carries no token material (schema-enforced — there is no
  column for it).
- Acceptance criteria:
  1. Migration applies; `crm_app` can UPDATE `intake_token` (and
     still nothing else new); the fact table is append-only both
     directions + TRUNCATE.
  2. Rotate mints a fresh `[a-z2-7]{8}` token ≠ the old one (the
     implementation re-mints on the 2^-40 collision so this can never
     flake), writes exactly one fact row (user actor, envelope
     fields), returns the new address in the GET shape; repeated
     rotation works.
  3. Old-token delivery → 200 rejected, nothing stored; new-token
     delivery → stored/processed (the 007b criteria re-pinned across
     a rotation).
  4. Member 403; org-B admin's rotate changes only org B; 401/503
     shapes.
  5. The web Rotate flow (confirm copy, POST, invalidation)
     Vitest-pinned; member rendering unchanged.
  6. `local_part` render pinned (already unit-tested in address.rs —
     re-asserted with the new env values in a config test).
  7. The worker artifact exists with the derived size threshold,
     envelope-recipient use, the §3 response-handling matrix — and its
     one bug-prone pure computation, the chunked base64 encoder, is an
     exported function covered by a `node --test` file beside it
     (node is already required for the web gates; wired into
     `./scripts/check`'s web section). Corrupt base64 would be
     silently 200-accepted and file garbage — the one failure mode a
     walkthrough can miss, hence the unit test.
  8. `./scripts/check` + `./scripts/check-db` + web gates green; the
     §10 runbook executed; the live walkthrough per §1 passed,
     including the real personal-mailbox email and the header capture
     for 007h.
- Tests: DB (`db_intake_rotation.rs`): criteria 1–4 plus a
  tracing-capture test over the rotate path (no token material in any
  span or log — the 007b precedent); service-free: 401/403 shapes;
  web Vitest: criterion 5; `node --test` for the worker's chunked
  base64 (criterion 7). The walkthrough owns the live half.

## 10. Ops runbook (user + coordinator, by hand — the spec's checklist)

1. Preconditions: the `elysianfeld.com` zone is in the user's
   Cloudflare account (confirm; if not, add/transfer it first — user
   action); `.env`'s `CRM_INBOUND_EMAIL_SECRET` is set (≥ 32 bytes)
   and the API restarted with it — an unset secret 401s every worker
   delivery into the retry branch.
2. Cloudflare dashboard → Email Routing on the elysianfeld.com zone:
   register the subdomain `leads.elysianfeld.com` (Email Routing →
   Settings → Subdomains); Cloudflare adds the MX/SPF records for it.
3. Deploy the worker: `cd infra/email-worker && wrangler deploy`,
   then `wrangler secret put CRM_INBOUND_EMAIL_SECRET` (the same
   value as `.env`'s). Set `CRM_INBOUND_API_URL` to
   `https://api.tarams.org/inbound/email`.
4. Email Routing → Routes: **catch-all on the
   `leads.elysianfeld.com` subdomain → Send to Worker** (the deployed
   worker). (The catch-all is correct: token validation is the
   endpoint's job; unknown/forged recipients must 200-reject there,
   not bounce distinguishably at the edge.)
5. `.env`: set the two intake-mail values (§7); restart the dev API
   (kill by exact PID — it runs orphaned). The Manage → Intake page
   now renders the local-part address.
6. Tunnel: no new route needed — the dashboard catch-all
   `api.tarams.org → localhost:3000` carries all paths in principle
   (the 007b walkthroughs exercised `/inbound/email` on loopback
   only, so the tunnel hop for this specific path is proven by THIS
   walkthrough); `./scripts/check-tunnel` first for the plumbing.
7. Live walkthrough per §1: real mail (a pinned-format message, an
   unknown-format lead, junk), forged token, rotation, the
   stored-header capture for 007h; verify the worker's throw-on-5xx
   behavior by stopping the API for one send and confirming redelivery
   (record what Cloudflare actually does — §3's live-verify item).

## 11. Lane and checks

Single lane, branch `slice-007g-real-receiving`, one writer, sole
migration owner. Gates: `./scripts/check`, `./scripts/check-db`, web
gates, independent review + adversarial testing, then the §10
runbook + live walkthrough (interleaved with the user's console
steps).

## 12. Safe defaults adopted (reviewer/user may veto)

(a) Rotation = single-token immediate invalidation (the silent-discard
window stated in the UI); (b) rotation audited as a minimal
append-only `intake_token_rotated` fact (envelope only, no token
material); (c) no provider trait / no crm-api adapter route (D-039 —
the worker IS the adapter; revisit only with a third-party provider);
(d) SPF/DKIM deferred to 007h with the raw bytes as the carrier (§6's
declared deviation); (e) the worker rejects oversize with a bounce
and rethrows on 5xx for MTA-level retry (live-verified, adjusted if
Cloudflare's semantics differ); (f) catch-all routing at the edge —
the endpoint stays the only judge of recipients (no edge-level
oracle); (g) no realtime event on rotation; another tab's stale
address until refetch accepted; (h) the subdomain-form addresses stay
parseable forever (`parse_recipient` unchanged) though unroutable
once MX exists only on `leads.elysianfeld.com` — stated, accepted
(no real subdomain-form mail ever flowed; the form existed only in
dev walkthroughs).
