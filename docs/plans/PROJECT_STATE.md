# Project State

Last updated: 2026-08-26 (NEW TRACK: type-safety hardening ladder
surveyed and accepted as the working plan —
`docs/design/type-safety-hardening.md` (full-backend planner sweep at
`105f730`): 8 chunks, S1 (Resolution/PayloadFormat enums) → N1
(OrganizationId) → N2 (UserId) → A1 (Actor enum) → N3/N4 (id
clusters) → V1 (NormalizedEmail/Phone) → S2 (fact fields +
IntakeToken + Publication). Each chunk behavior-preserving, own
branch, gates green, wire/DB byte-identical; sqlx strategy = bind
`.0`, `.sqlx` untouched; crm-operator seam stays bare Uuid (D-028
fence). CHUNK S1 IMPLEMENTED on `hardening-s1` (Option A: Sonnet
implementation subagent under Fable coordination — worked well; file
list verified against git status, both deviations disclosed and
sound). Independently reviewed: READY, no blockers; deferred minors:
F1 (row-boundary `sqlx::Error::Decode` surfaces as 503 not 500 —
fold a `Decode → Corrupt` mapping into S2) and F2 (no direct
`duplicate_outcome(Pending)` pin — needs a crm-app db harness that
doesn't exist; parse-level pins cover it). Gates verified green by
the coordinator (check + check-db 285/0, `.sqlx` byte-untouched).
S1 MERGED + PUSHED (`main` `dc27ae6`). CHUNK N1 (`OrganizationId`)
IMPLEMENTED on `hardening-n1` (same Option A workflow): new
`crm-app/src/ids.rs` (serde/repr-transparent, no implicit
conversions, compile_fail,E0308 doctests), 49 files swept —
CommandContext/FactEnvelope/IntakeActor/AuthContext/visibility/
realtime (channel_for, mint, events)/crypto AAD all typed; ~96 binds
via `.0`, SQL + `.sqlx` byte-untouched (proven by live
sqlx-prepare-check); crm-operator zero-diff (D-028 fence); AAD/
channel/JWT byte-identity verified. Reviewer: READY (20 conversion
sites audited authoritative; doctest error-code pin applied).
N1 MERGED (`565f997`); N2 (`UserId` + both N1 carry-overs closed:
session layer typed, org DTOs typed) MERGED + PUSHED (`main`
`4a75279`) — reviewer ready-with-fixes (record-level only: the
same-role residuals — determine_routing's two Option<UserId> params,
platform TwoUuidPathIds — are recorded in the ladder doc as LATER;
a shared newtype cannot distinguish same-role params). Ladder 3/8 done (S1, N1, N2). BATCHED-AUTONOMY MODE (user,
2026-08-26): A1 and N3 run without per-chunk approvals; each commits
on its own branch when review+gates pass; merge+push approval batched
at the end. A1 (`Actor` enum + `RoutingAssignees` params struct —
absorbing the determine_routing residual) DONE: reviewed READY (zero
actionable findings), gates independently green, committed
`hardening-a1` `76b49d4`. N3 (PersonId/InquiryId/RawPayloadId/
StageId) DONE on `hardening-n3` (stacked on a1): 39 files, both AAD
halves now typed (bytes unchanged), axum extractors renamed
PersonIdPath/RawPayloadIdPath wrapping typed ids (orphan rule),
call_changed's call/person adjacency cross-typed (dead one chunk
early); reviewed READY (one doc-comment minor, folded in); gates
independently green (285/0) twice. A1+N3 MERGED+PUSHED (`main` `f942d99`).
N4 and V1 ran as PARALLEL WORKTREE LANES (user-approved;
crm-wt-n4/crm-wt-v1) with hard ownership boundaries — held perfectly:
zero cross-lane edits, zero merge conflicts; V1 correctly
STOPPED at the identify() retype (sole caller receive_inquiry.rs was
N4-shared) and reported instead of breaching; completed at
integration on hardening-v1 after merging hardening-n4 (by-value
typed identify params + call site + idempotence pin, reviewer-
blessed shape). Both lanes reviewed READY; serial full check-db per
lane 285/0 each. N4+V1 MERGED+PUSHED (`main` `af71668`;
worktrees removed). S2 (FINAL chunk) IMPLEMENTED on `hardening-s2`:
typed fact vocabulary (3 micro-enums, write-only, bootstrap variant
deliberately absent), AttemptOutcome (13 variants matching the CHECK
exactly; is_transport exhaustive, no wildcard), IntakeToken (no
Display/PartialEq, redacted Debug, constant-time-only verify; the
receive.rs auth path line-audited by a security-weighted review —
timing structure unchanged, dummy branch byte-identical),
constructor-only Publication (channel AND event private after review
minor), Decode→Corrupt completed across ALL THREE error types
(Command/Workbench/AdminCommand). Reviewed READY; review minors all
folded in; gates green three times (285/0). LADDER COMPLETE 8/8
pending the final commit+merge gate; closing state written into
docs/design/type-safety-hardening.md. Feature slices may interleave.

Prior update, 2026-08-25 (SLICE_007h1 COMPLETE, MERGED, PUSHED —
`main` `105f730`. Detail below; SLICE_007h1 IMPLEMENTED on
`slice-007h1-forwarded-wrapper` (off `main` `9604f76`), uncommitted.
Built: email/forward.rs (SenderTrust Direct/ForwardedClaim,
ForwardStyle registry, gmail_inline_v1, resolve with depth cap 3),
trait split matches_direct/matches_forwarded + detect(mail, trust),
detect-first parse_payload, worker resolve-before-build_input, span
fields both sites, banner fence widened to the whole intake module,
8 authored fixtures (self-approximated Gmail format — reconcile
against the user's real .eml when provided) + 12 new db tests across
inbound/extraction/workbench suites + adversarial From-line panic pin.
Implementation review: ready-with-fixes — F1 worker-seam executable
pin (db test capturing the fake extractor's input: inner
subject/domain/text), F2 fence scope, F3 forwarded
redelivery/convergence pins, F4 bracket rejection in is_addr_shaped —
ALL APPLIED. Adversarial testing: NO critical/high (panic-free slicer
probed with multibyte/RTL/nested brackets; linear work; tenant-safe);
its gaps (worker db pin, retry-on-forwarded-row, two-forwarder
convergence, forwarded cross-tenant, From-line panic pin) ALL
APPLIED. LIVE WALKTHROUGH PASSED (2026-08-25): the user forwarded the earlier
Maya sample from Gmail through leads.elysianfeld.com → unwrapped →
extracted (0.99, 1.1 s) → converged on the existing Maya Lindqvist as
her SECOND inquiry (no duplicate person); an unplanned newsletter
forward was correctly binned not_a_lead 0.99. REAL-FIXTURE
RECONCILIATION DONE: the user's three real .eml forwards (plain,
HTML-heavy, nested — PII, never committed; probed via a temporary
runtime test, then deleted) all unwrap (depths 1/1/2, true inner
domains recovered); real Gmail's banner/QP/U+202F/multipart structure
matches the matcher byte-for-byte, and a SANITIZED replica fixture
(gmail_fwd_real_structure.eml) + fast-gate structural test pin it.
Known accepted edge recorded in spec §5: the forwarder's trailing
signature is part of the inner body in plain text (LLM could pick the
forwarder's phone); HTML gmail_quote separation is a later rung.
Final gates re-run after the fixture addition; next: commit gate →
merge gate.

SLICE_007g COMPLETE, MERGED, PUSHED (2026-08-25): `main` `9604f76` on
origin carries planning `7ef2277` + implementation `c601363` + merge.
The 007g detail below is history. The
§10 ops runbook ran end-to-end with the user: worker
`crm-inbound-relay` deployed (`workers_dev = false` added to
wrangler.toml — email-only worker, no HTTP URL, which also sidesteps
the workers.dev-subdomain registration error on first deploy) with
`CRM_INBOUND_EMAIL_SECRET` set via `wrangler secret put`; the user
registered subdomain `leads.elysianfeld.com` in Email Routing (MX
live) and the enabled catch-all routes to the worker; `.env` gained
`CRM_INTAKE_ADDRESS_SCHEME=local_part` (domain already defaults to
elysianfeld.com) and dev-api was restarted. Live results, all real
Gmail → Cloudflare → worker → tunnel → `/inbound/email`: (1) freeform
inquiry to the org address → 200 in 35 ms → unresolved → Groq
extraction ~5 s → Person created at stage Lead with phone, assigned
via round-robin; (2) forged-token address → worker Ok, endpoint 200,
silently discarded — no person, no unresolved row; (3) live rotation
via the admin API → old address dead (same discard path), fresh mail
to the minted address → second Person created. WALKTHROUGH FINDING
that RESOLVES the §6 escalation: Email Routing → Worker delivery DOES
carry `Authentication-Results` from mx.cloudflare.net (observed live:
dkim=pass, spf=pass, dmarc=pass, arc=pass) — the known-issue caveat is
moot and 007h MAY rely on SPF/DKIM verdicts. Verified via a temporary
verdict-headers-only console.log in the worker, removed and
clean-redeployed afterward; the committed worker.js logs nothing.
Implemented and verified: migration `20260902000001`
(intake_token UPDATE grant + the append-only `intake_token_rotated`
envelope fact table with corrects_id/indexes), `rotate_intake_token`
(one tx: FOR UPDATE → re-mint-on-collision → UPDATE → fact; returns
the minted token so the response can never misreport a completed
rotation), the admin-only rotate route (slug pre-read; ids-only span),
the web Rotate button behind the consequence-stating ConfirmDialog,
the Email Worker relay `infra/email-worker/` (derived 1.4 MiB
threshold, chunked 3-aligned base64, envelope recipient, explicit
2xx/400/413-bounce/401+5xx-throw matrix, 30 s AbortSignal timeout, no
content logging) with 9 `node --test` cases wired into
`./scripts/check`, `.env.example` D-025-drift + scheme comments, the
declared 007a/007c grant-pin amendments. Independently reviewed
("ready with fixes" — all applied: criterion-6 config-render test,
TRUNCATE/migrator-DELETE pins, doc-comment fixes, scheme assert) and
adversarially tested (no blocking issues; M1 worker-matrix tests,
L1 return-token refactor, L6 fetch timeout all applied). ALL GATES
GREEN: ./scripts/check (incl. worker tests) + ./scripts/check-db
(db_intake_rotation 4/4 incl. the token-leak capture).

Ops side notes from the 007g runbook: the user's Cloudflare API token
in `.env` was re-minted (zones + Email Routing Rules + DNS edit; it
still cannot read Email Routing *settings/subdomains* endpoints —
rules + catch-all suffice); the walkthrough rotation means the seeded
org's live intake address is the rotated one, visible in Intake
Settings; `slice-007g-real-receiving` still exists locally (deletion
not yet approved).)

Planning phase, earlier (Slice 007g: D-039 accepted
and recorded — the mandated wildcard check found no free/incumbent
inbound path accepting arbitrary `*.elysianfeld.com` (Cloudflare Email
Routing: ≤30 registered subdomains, no wildcard; SendGrid: named
hosts, unsigned webhooks; Mailgun: wildcard works but a new paid-ish
vendor), and the user chose Cloudflare Email Routing + the
007a-prepared local-part scheme: `<slug>-<token>@leads.elysianfeld.com`,
catch-all on the registered subdomain → a committed Email Worker
relaying raw MIME to the frozen `/inbound/email` with the existing
bearer (no new crm-api route; resolves ladder cross-rung decisions 1
and the 007g half of 2). `docs/specs/SLICE_007g.md` drafted (worker
artifact, token rotation end-to-end with an append-only
`intake_token_rotated` fact, config flip, ops runbook); independent
review in progress. Research caveat carried in the spec: a known
Cloudflare issue may deliver Worker mail WITHOUT Authentication-
Results headers — the walkthrough captures real stored headers and
escalates before 007h relies on SPF/DKIM.)

Prior update, same day (SLICE_007f implemented on
`slice-007f-extraction` off `main` `c3cbc5a`: migration
`20260901000001` (extraction-state columns + the PII-free
`intake_extraction` ledger, append-only), the `LeadExtractor` seam +
full validation gauntlet in crm-app (strict schema, inclusive 0.7
gate both verdicts, subject+text anti-hallucination with the 10-digit
phone floor and separator-only stripping, reply-field control-char
sanitation), the sweep-pattern worker (claim lease, guarded reset,
un-reset on any post-reset error, two-class failure accounting), the
Groq adapter + injection-hardened prompt, `ChatRequest.response_format`
in crm-operator (declared additive; wire byte-identical when None),
`not_a_lead`/`email_extraction_failed` end-to-end, config with the
lease invariant. Independently reviewed (`crm-reviewer`: no blockers;
F1 span-instrument fix applied) and adversarially tested
(`crm-tester`: CRITICAL C1 + HIGH H1 found and FIXED — a model reply
echoing NUL into name/message, or any deterministic post-reset
failure, was classed as an unbounded wait-forever retry paying for a
Groq call every 60 s; now reply fields are sanitized and all
deterministic failures are counted/bounded → terminal
"Extraction failed" after 3, only genuine provider outages wait
forever — both pinned by tests incl. the empty-script no-paid-calls
proof). All lesser findings folded in (drain continuation, full span
counts, build_request no-tools pin, TRUNCATE pin, eligibility
negatives, failed-attempt capture, from_config keyless test).
`./scripts/check` + `./scripts/check-db` green (14 db_intake_extraction
tests). Live walkthrough against REAL Groq (`openai/gpt-oss-120b`):
the leftover 007e plain.eml row auto-extracted at confidence 0.99 in
1.2 s (Jordan → Carol's Today); `unrecognized_lead.eml` → Priya
Natarajan extracted 0.99/771 ms onto Carol's Today with normalized
contacts and system/webhook facts; `spam.eml` → "Not a lead" 0.99;
the outage demo (dead CRM_OPERATOR_BASE_URL, kill-by-PID restarts) →
`provider_unavailable` then, after restore, the same row's ledger
reads provider_unavailable→extracted and Sasha Reyes appeared — a
lead is never lost; zero lead content in any dev-api log. Committed,
fast-forward merged to `main`, and pushed with explicit user approval
2026-08-25. crm_dev gained the walkthrough rows; dev-api runs the
merged binary.)

Planning phase, earlier (Slice 007f: D-038 accepted
and recorded (inbound lead mail → Groq blessed with the fixed scope:
text-only ≤16 KiB, subject + sender domain only, no org/agent/
recipient identifiers, no tools, Groq on the subprocessor list — the
ladder's LAST blocking decision), `crm-planner` pass done,
`docs/specs/SLICE_007f.md` drafted, independent review in progress.
Key planner findings baked into the draft: the telephony sweep is the
worker template (spawn/run_once/interval — no worker built from
scratch); `InferenceProvider` has no structured-output support, so
`ChatRequest` gains a declared additive `response_format` field;
"backoff+max attempts" vs "provider down waits forever" reconciles
only via a two-class failure taxonomy (transport failures never count
or go terminal; quality failures cap at 3 → `email_extraction_failed`);
anti-hallucination needs normalized matching (digit-sequence phones),
not raw substring; extraction model default must be
`openai/gpt-oss-120b` (llama retired); the IntakeBusy path inside the
worker must UN-reset the row or it strands as pending.)

Prior update, same day (SLICE_007e implemented on
`slice-007e-workbench` off `main` `5720f54`: migration `20260830000001`
(discarded resolution + attributed discard columns, pair-CHECK,
column grants), `domain/intake/workbench.rs` (detail decrypt-on-demand
with 64 KiB UTF-8-safe caps, guarded-reset retry via the shared
`complete_intake` as System actor + `on_behalf_of` admin, attributed
idempotent discard), the `duplicate_outcome` discarded arm (fixes the
latent redelivery panic), queue filter → `pending|unresolved`, three
admin-only routes (400-before-auth id extractor, 409 `discarded`/
`already_resolved`), the web workbench dialog (fetch-on-open, cleared
on close, ConfirmDialog discard, intake-busy copy) + DataTable
`onRowClick`. Independently reviewed (`crm-reviewer`: "ready with
fixes") and adversarially tested (`crm-tester`: no security/tenancy
blockers) in parallel; all findings fixed and pinned: B1 the dialog
rendered unstyled (`dialogPt` passed uncalled — the one-char fix);
MEDIUM-1 failed retries committed the reset but published nothing
(now one ids-only invalidation + web onError refetch, test-pinned);
MEDIUM-2 unretryable rows (unknown payload_format) destroyed the
stored reason before failing (now fail-closed BEFORE the reset,
test-pinned); the missing criterion-14 test (/api/inquiries replay of
a discarded row) added; cross-org retry/discard 404 probes,
deterministic resolved-retry duplicate test, nonce/content_hmac
denial, `payload_format` on the retry span, and the declared unit
tests all added. Spec §4/§9/§13 amended to record the hardenings.
`./scripts/check` + `./scripts/check-db` green after fixes (184 unit,
18 db_intake_workbench + 2 service-free new; 259+2 web). Live
walkthrough against the real dev stack (dev-api rebuilt onto the
branch, PID 4838): admin detail render + member 403; retry-unchanged
same reason; delivery deferred under a psql-held advisory lock →
Pending row → Try again rescued it end-to-end ("Morgan Hale" on
Carol's Today, `organization_default`, facts system/on_behalf_of=
alice/web_session); forged eospia.com mail opened + discarded with
attribution; byte-identical redelivery stayed discarded. Committed,
fast-forward merged to `main`, and pushed with explicit user approval
2026-08-25. crm_dev gained the walkthrough rows; dev-api runs the
merged binary.)

Planning phase, earlier the same day (Slice 007e: D-037 accepted
and recorded (raw unresolved content is org-admin-only, on demand —
resolves ladder cross-rung decision 7), `crm-planner` pass done,
`docs/specs/SLICE_007e.md` drafted, independent review
(`crm-reviewer`) in progress. Notable planner findings baked into the
draft: adding `resolution='discarded'` without extending
`duplicate_outcome` would make redelivery of discarded bytes hit its
`unreachable!` — a handler panic; `list_unresolved`'s `<> 'resolved'`
filter must become `IN ('pending','unresolved')` (declared SLICE_002
§5 queue amendment); "re-run Phase B" needs a guarded reset-to-pending
step; retry runs as System actor with `on_behalf_of_user_id` = the
acting admin (declared envelope extension) so rescued leads route per
D-035 to the org default, NOT to the clicking admin; discarded rows
are retained, UI-invisible ciphertext until O-013 — stated, not
silent; 007d's §4e "stale queue" caveat is factually wrong (the web
already invalidates the unresolved queue on `person_changed`) and gets
corrected at approval.)

Prior update, same day (SLICE_007d implemented on
`slice-007d-email-format` off `main` `d491175`: `mail-parser` 0.11.8
wrapped in `domain/intake/email/mime.rs` (directory-walk fence test,
redacted Debug, NUL stripped at the fence), `EmailFormat` registry with
`cypress_bay_contact_v1` (domain + exact-subject matching per D-036),
`complete_intake` extracted from `receive_inquiry` and shared by both
intake entry points, Phase B on `receive_inbound_email`
(`IntakeActor::System`, `Origin::Webhook`; IntakeBusy → 200 + row left
`pending`), `email_unrecognized_format` reason + web label, five new
`.eml` fixtures, the three declared 007b test amendments. Two safe-
default refinements disclosed: "unparseable" = no From + no Subject +
no body (mail-parser headers even garbage, so the spec's literal
definition was unreachable — probed empirically); NUL bytes stripped in
the mime wrapper (see below). Independently reviewed (`crm-reviewer`:
"ready to commit", extraction verified verbatim against HEAD) and
adversarially tested (`crm-tester`) in parallel; the tester found one
HIGH blocking issue — NUL bytes in attacker-crafted in-format mail
reached Postgres TEXT (22021) → attacker-triggerable 503 + permanently
stuck queue-invisible `pending` rows — fixed at the mime fence with
unit + DB pins (`nul_bytes_in_an_in_format_email_complete_without_
error_or_poison_row`). Reviewer's four smaller items all folded in:
capture test extended over unparsed/no-contact paths, registry
source-validity check made real, email-path default-unset test,
multipart reason assertion. `./scripts/check` + `./scripts/check-db`
green after fixes (db_inbound_email 13, db_inbound_email_intake 17,
regression gates db_intake 22 + db_intake_system_routing 7 unmodified).
Live walkthrough against the real dev stack (dev-api rebuilt onto the
branch, old orphaned PID killed): carol reactivated + set as Acme's
default assignee via the real 007c endpoints; `scripts/inbound-email`
with the cypress fixture → 200 accepted → "Jordan Ellis" on carol's
Today with all four facts System-attributed, routing
`organization_default` → Carol, inquiry `source=website` with the
multi-line message; `plain.eml` → Unresolved "email_unrecognized_
format"; byte-identical redelivery → still one Person, one unresolved
row; wrong token → rejected. Committed (`a75b9a8`), fast-forward
merged to `main`, and pushed with explicit user approval 2026-08-25. Dev-api left running on the branch
binary; crm_dev now has carol active, the default-assignee setting,
and the walkthrough rows.)

Planning phase, earlier the same day (Slice 007d: D-036 accepted
and recorded (forged-mail posture — in-format mail from a valid intake
address creates a Person; defenses are the address token, per-format
sender-domain matching, SPF/DKIM at 007g; everything else lands in
Unresolved), `crm-planner` pass done, `docs/specs/SLICE_007d.md`
drafted, independent review (`crm-reviewer`) in progress. Notable
planner findings: no migration needed (`unresolved_reason` and
`inquiry.source` are free text); `mail-parser` confirmed absent from
the workspace (the ladder-blessed new dep, wrapped in
`domain/intake/email/mime.rs` with a fence test); the email path must
take the per-org advisory lock since it now creates People, and on
`IntakeBusy` returns 200 with the row left `pending` (007b's
"never `intake_busy`" holds; redelivery or 007e rescues); a declared
amendment: 007b's tests pinning `email_unparsed` for
`plain.eml`/`multipart.eml` change to `email_unrecognized_format`.)

Previous update, 2026-08-24/25 (Slice 007c implemented on
`slice-007c-system-routing`, off `main` `778c2e2`: system actor,
unattended routing, `GET`/`PUT /api/organization/intake-settings`,
`crm-admin receive-inquiry`, the Unattended-lead-routing web card.
Independently reviewed by `crm-reviewer` and adversarially tested by
`crm-tester` in parallel against the real diff — both found the
implementation spec-faithful with no blocking issues; both
independently converged on the same real gap (§7's "system-actor
intake into org A writes zero rows in org B" pin was unwritten,
though the code was already correctly scoped); `crm-tester`
additionally flagged an untested explicit-null-vs-absent-key PUT wire
case. Both fixed with new tests (`db_intake_system_routing.rs`'s
`system_actor_intake_into_org_a_writes_zero_rows_in_org_b`,
`IntakeSettingsView.test.ts`'s "choosing Unassigned PUTs an explicit
null"). `./scripts/check` and `./scripts/check-db` both green after
the fixes. Full live walkthrough run against the real dev stack (real
Postgres, real dev-api rebuilt onto this branch, real headless
Chromium against the real Vite dev server) — see Completed work below.
Committed (`57ef058`, `fe0b99b`), fast-forward MERGED to `main`, and
PUSHED to `origin/main` (`fe0b99b`), all with explicit user approval
2026-08-25.)

## Current phase

**Slice 007h1 (forwarded-wrapper, Gmail inline) PLANNING: D-040
accepted, spec drafted, independent review in progress. See the
header block.** Prior: Slice 007g COMPLETE + MERGED + PUSHED
(`main` `9604f76`): real Gmail → Cloudflare → worker → lead live;
forged token silently dropped; rotation verified;
Authentication-Results PRESENT (§6 escalation resolved).

Spec phase, for reference: spec APPROVED (user, 2026-08-25); D-039;
D-036's SPF/DKIM wording amended via that approval.

Prior phase: **Slice 007f (LLM extraction) COMPLETE: implemented,
reviewed, adversarially tested (CRITICAL+HIGH found, fixed, pinned),
checks green, live real-Groq walkthrough passed, committed, merged to
`main`, and pushed — all with explicit user approval 2026-08-25.**

Spec phase, for reference: **spec APPROVED (user, 2026-08-25).** D-038 accepted.
Spec: planner pass, independent review (no blocking findings, eight
amendments applied incl. the non-Clone-ParsedLead closure fix and the
race-safe ledger seq), user-approved as written incl. the declared
additive ChatRequest.response_format extension.

Prior phase: **Slice 007e (Unresolved workbench) COMPLETE: implemented,
reviewed, adversarially tested (all findings fixed + pinned), checks
green, live walkthrough passed, committed, merged to `main`, and
pushed — all with explicit user approval 2026-08-25.**

Spec phase, for reference: **spec APPROVED (user, 2026-08-25).** D-037 accepted.
Spec: planner pass, independent review (no blocking findings, six
documentation-level amendments applied), user-approved as written
incl. the three highlighted safe defaults (rescue routes per D-035 not
to the clicking admin; discarded bytes never resurrect; discard
retains ciphertext until O-013). Amendment pointers placed: SLICE_002
§5 (queue lists pending|unresolved only), SLICE_007d header (stale-
queue caveat corrected; duplicate_outcome discarded arm; for_system
on_behalf_of extension).

Prior phase: **Slice 007d (one pinned email format → real inquiries)
COMPLETE: implemented, reviewed, adversarially tested (one HIGH
finding fixed + pinned), checks green, live walkthrough passed,
committed (`a75b9a8`), merged to `main`, and pushed — all with
explicit user approval 2026-08-25.** D-036 accepted. Spec `docs/specs/SLICE_007d.md`: planner
pass, independent review (no blocking findings), seven amendments
applied, user-approved as written. The SLICE_007b supersession pointer
(criteria 3/14/18 + the fixture-reason change) is in place.

Prior phase: **Slice 007c (system actor + unattended routing)
COMPLETE, MERGED to `main`, and PUSHED to `origin/main` (`fe0b99b`).**
007b is COMPLETE
and MERGED to `main` (`4b3462a`). D-035 accepted (resolves ladder
cross-rung decision 6). Spec `docs/specs/SLICE_007c.md`: planner pass,
independent review (no
blocking findings), amendments applied, user-approved as written. The
user gave the implementation go-ahead 2026-08-24/25 ("Implement 007c
now") after an earlier "Not yet" on the same gate report. Branch
`slice-007c-system-routing` was created off `main` `778c2e2`;
implementation, tests, independent review, adversarial testing, fixes,
full-suite verification, and a live dev walkthrough all completed
there, then committed (`57ef058`, `fe0b99b`), fast-forward merged to
`main`, and pushed to `origin/main` (`fe0b99b`) with explicit user
approval 2026-08-25. The branch itself no longer exists (confirmed
deleted 2026-08-25; only `main` remains locally).

Planner findings that shaped the draft: the fact tables already permit
`actor_kind='system'` (no fact-table migration); the Rust
`RoutingStrategy::from_str` fails closed, so the two new strategies are
a declared additive change to the frozen `POST /api/inquiries`
`routing_strategy` vocabulary (reachable via `duplicate: true` replays
of system-routed rows); the walkthrough trigger is a new `crm-admin
receive-inquiry` subcommand (D-021-sanctioned domain-function path,
`Publisher::Disabled`); a deactivated default assignee routes
`unassigned` fail-safe with the setting retained (D-027 interaction);
`is_organization_member` lacking a status filter on the manual
explicit-assign path is a pre-existing D-027/O-004 gap, flagged in the
spec's exclusions, not fixed in 007c.

Prior phase, for reference: **Slice 006c (call outcome) COMPLETE and
MERGED to `main` (`58ecad8`).** Scope grew during the live walkthrough into three
user decisions, all implemented, reviewed and adversarially tested:
one timeline line per call (Follow Up Boss style); D-033 — the agent
must choose every call's outcome (no default, no Skip), an unchosen
call is "outcome needed" in the timeline, and the Person stays on the
caller's Today in a new lowest `low` tier (`call_outcome_needed` /
`set_outcome`) until chosen; O-012 recorded (PII content blobs,
per-Person keys, crypto-shred — blocks summaries/recordings/notes).
Review fixes in `fafaf95` (notably: the agent's first choice is always
written even when it equals the system's guess — otherwise the call
stayed incomplete forever; Today query keeps its assignee index
predicate; Operator `ORDERING_RULE`/`Ahead` extended for `low`; two new
indexes `20260826000002`). Gates: `check` green (236 Vitest), `check-db`
green (`db_calls` 49). Live: two real calls corrected on 2026-08-23, and the D-033 UI (forced
choice, "outcome needed" row, Today low tier, Set outcome clearing
both) walked through live on 2026-08-23 — works. Note: the dev API runs
as an orphaned process; restarting Docker services does not restart it
(stale binary masked the fixes once). Follow-ups: rotate the Telnyx SIP password; busy/ring-out never
proven live; `placing` sweep horizon vs slow mic prompts; orphaned
"outcome needed" calls when a caller is deactivated (O-004 territory);
OrbStack hung twice. Hostname is `livekit1.tarams.org`.

## Current slice

Slice 007h1 — Forwarded-wrapper (Gmail inline) —
`docs/specs/SLICE_007h1.md` — DRAFT under independent review; D-040
accepted. Previous: Slice 007g — Real receiving —
`docs/specs/SLICE_007g.md` — COMPLETE, MERGED to `main` and pushed
(`9604f76`); live receiving operational on leads.elysianfeld.com. Previous: Slice 007f — LLM extraction —
`docs/specs/SLICE_007f.md` — COMPLETE, MERGED to `main` and pushed. Previous: Slice 007e — Unresolved
workbench — `docs/specs/SLICE_007e.md` — COMPLETE, MERGED to `main`
and pushed. Previous: Slice 007d — One pinned
email format → real inquiries — `docs/specs/SLICE_007d.md` —
COMPLETE, MERGED to `main` and pushed.
Previous: Slice 007c — System actor + unattended routing —
`docs/specs/SLICE_007c.md` — COMPLETE, MERGED + PUSHED `fe0b99b`.
Before that: Slice 007b — Inbound email endpoint —
`docs/specs/SLICE_007b.md` — COMPLETE, MERGED `4b3462a`. Before that:
Slice 007a — Organization intake address — `docs/specs/SLICE_007a.md`
— COMPLETE, MERGED `81af77f`.

Earlier history, for reference:
Slice 006c — Call outcome (D-032, D-033) — `docs/specs/SLICE_006c.md`
(+ §5a) — COMPLETE, MERGED `58ecad8`. Next: Slice 006a — `crm-app`
extraction — `docs/specs/SLICE_006a.md` — COMPLETE, MERGED `a17aed3`
(reviewed + adversarially tested, no blocking findings; check +
check-db green post-fixes). Known flake (pre-existing, not
006a): `db_calls::a_second_correction_chains_onto_the_first_with_
strictly_increasing_recorded_at` can misorder around `call_completed`
under full-suite load — the history sort ties on microsecond
timestamps (person/queries.rs `(occurred_at, recorded_at, kind_rank,
id)`); passed 8/8 isolated full-suite reruns on both main and the
branch. Previous:
Slice 006 — Calling — `docs/specs/SLICE_006.md` (MERGED `332e78a`). Previous:
Slice 005 — Operator retrieval — `docs/specs/SLICE_005.md` (APPROVED). Read-only AI Operator: `crm-operator` crate with a
`ToolBackend` trait (five tools: `search_people`, `get_person`,
`get_today`, `get_next_work_item`, `explain_priority`), Groq via a
provider-neutral trait, `POST /api/operator/turns` (stateless,
non-streaming, bounded), append-only `operator_turn` /
`operator_tool_call` ledger, web **Ask** drawer. Closes the thesis §16
proof chain. Slice 004 is complete and merged (see History).

## Current branch

`main` (`fe0b99b`, pushed to `origin/main`). `slice-007c-system-routing`
fast-forward merged into it and left in place, not yet deleted (both
branches point at `fe0b99b`).

## Last accepted decision

2026-08-25, user-accepted (Slice 007g planning):
- D-039 — final intake address scheme is local-part
  (`<slug>-<token>@leads.elysianfeld.com`); receiving via Cloudflare
  Email Routing on the registered `leads.` subdomain, catch-all → a
  committed Email Worker relaying raw MIME to the frozen
  `/inbound/email` with the existing bearer. No new vendor, no new
  crm-api route; a provider-signature adapter returns only if a
  third-party inbound provider is adopted. Resolves ladder cross-rung
  decisions 1 and the 007g half of 2.

Previous, same day (Slice 007f planning):
- D-038 — inbound lead-email content may be sent to Groq for
  extraction, scope fixed: text-only ≤16 KiB, subject + sender
  domain only (never the full address/recipient/org/agent
  identifiers), no tools; the reply is untrusted, schema-validated,
  anti-hallucination-checked, confidence-gated; Groq on the
  subprocessor list. Resolves ladder cross-rung decision 4 — the
  ladder's last blocking decision.

Previous, same day (Slice 007e planning):
- D-037 — raw unresolved content (the decrypted email/JSON), Try
  again, and Discard are Organization-admin-only, on demand, per row,
  never in the list, never logged; members keep the metadata-only
  queue. Resolves ladder cross-rung decision 7. Widening later remains
  a future decision.

Previous, same day, user-confirmed (Slice 007d planning):
- D-036 — forged-mail posture for pinned email formats. Mail reaching
  a valid intake address (unguessable token) that matches a pinned
  format's template AND claims that format's real sender domain WILL
  create a Person and route per D-035, without human review — that is
  what a pinned format means. Defenses layered: the token, per-format
  sender-domain `matches()`, SPF/DKIM from 007g. Everything failing
  any gate lands in Unresolved with raw mail preserved (D-012).
  Accepted blast radius: one obvious bogus lead row. Records the
  confirmation the ladder mandated at rung d.

Previous, 2026-08-24, user-accepted (Slice 007c planning):
- D-035 — unattended intake (system-actor path, email from 007d on)
  routes to an admin-set Organization default assignee
  (`intake_default_assignee_user_id`); unset → the Person is created
  unassigned (visible in People, on nobody's Today) and the intake
  settings page warns. Round-robin/rules routing stay outside the 007
  ladder. Resolves ladder cross-rung decision 6.

Previous (Slice 005 planning), 2026-08-22, user-accepted:
- D-028 — the AI Operator is an in-process workspace crate
  (`crates/crm-operator`) compiled into `crm-api`; §5 refinement: the
  dependency is inverted (`crm-api → crm-operator` only; `ToolBackend`
  trait is the whole data surface); the `crm-app` extraction is a
  prerequisite for the first Operator mutation slice.
- D-029 — Operator turns are audited as a PII-free ledger; no
  transcripts, no logged message/reply/argument text.

Previous (Slice 004 planning):
2026-08-22, user-accepted (Slice 004 planning):
- D-027 — membership deactivation (status active/inactive) ships in
  004 instead of removal; removal is not a concept; data retained and
  attribution stays visible with per-Person reassignment; last-admin
  rule counts active admins; `organization.status` reserved with
  suspension semantics OPEN as O-009.
- D-026 — Organization admin continuity. Admin-less Organizations stay
  fully operational for members; an Organization admin cannot remove
  the last admin (themselves); the platform admin can always both
  promote an existing member and invite a brand-new admin; admin-less
  Organizations are surfaced as "needs attention" in the platform-admin
  view (pending first-admin invitation is not an error); no push
  notifications in 004. Feeds the Slice 004 plan.

Earlier the same day, user-accepted:
- D-024 — Cloudflare Access removed from the dev tunnel. The app's own
  session login is the only gate now; the tunnel and its TLS are
  unchanged. Amends D-016 §4. Executed live (the `crm-dev` Access
  application deleted via the Cloudflare API and confirmed gone);
  README and SLICE_003 §11/§10 updated to match.
- D-025 — the dev tunnel turned out to be dashboard-managed, not
  file-managed as D-016 §4 documented; `config.yml`'s ingress was never
  actually applied, so the Slice 003 realtime WebSocket silently 404'd
  through the tunnel since it was written. Fixed live by adding/ordering
  the three routes in the Cloudflare dashboard; verified with a real
  101 Switching Protocols upgrade and a full two-browser cross-session
  walkthrough over the actual tunnel. README, `config.yml`'s comments,
  and this file updated to point at the dashboard as the real source of
  truth.

2026-08-21, user-accepted:
- D-023 — realtime model: one Organization channel with server-side
  subscriptions in API-minted short-lived tokens; ids-only events;
  best-effort publish after commit with recovery by refetch; dev
  WebSocket path-routed under the API hostname behind Access.
- Slice 003 specification approved as written, including the §14 safe
  defaults and the declared additive history-kind change to SLICE_002
  §5 (pointer line added there).
- D-022 — a contact attempt is a recorded fact (`contact_attempted`, the
  fifth typed fact table) and the unit of response for Today; any
  member's attempt counts; stage does not remove a Person from Today;
  done/snooze/dismiss remain unspecified (thesis §8).
- D-021 — Slice 004 is administration (platform admin + invitations +
  roles) and, from 004 on, nothing outside migrations writes to the
  database directly: seed/CLI/API all go through the same domain
  functions. Design defaults recorded as O-007 for confirmation when 004
  is planned.
- D-017 — web frontend stack: Tailwind, PrimeVue (unstyled), TanStack
  Table, TanStack Query; Centrifugo events drive Query invalidation.
- D-018 — production ingress: in-cluster `cloudflared` → Cilium Gateway
  API, `HTTPRoute` per hostname, no separate ingress controller.
- D-019 — Person stages are an Organization-scoped table seeded with
  Follow Up Boss's nine defaults, not a fixed enum.
- D-020 — Hot Prospect carries a red `Flame` marker wherever its name is
  shown; the one exception to `UI_STYLE.md` §5/§9, matched on the seeded
  stage name because D-019 stages have no semantic key.
- Slice 002 scope (not log entries): raw-payload key is one `.env`
  value, no DEKs/rotation; routing is "assign to a named member", no
  rules engine; list view + frontend stack + Vue Router land in 002;
  sqlx offline mode adopted in 002.

## Completed work

- 2026-08-22: Slice 005 Lane A implemented and verified on
  `slice-005-operator`. `./scripts/check` green (fmt, clippy `-D
  warnings`, 244 service-free Rust tests incl. 52 in `crm-operator`,
  web lint/typecheck/57 Vitest/build — note: `pnpm` needs
  `~/.nvm/nvm.sh` sourced in a non-interactive shell). `./scripts/check-db`
  green (sqlx `prepare --check`, 116 DB-backed tests incl. 21 new in
  `tests/db_operator.rs`). Live walkthrough §1 steps 1–7 on loopback
  against real Groq, model `openai/gpt-oss-120b`, 0.3–2.0 s per turn
  (1–3 tool calls): next-call answer with card matching `GET /api/today`,
  "why is she first?" citing tier/reasons/rule/ahead counts, find/tell-me
  with cards, bob's cross-Organization ask → "couldn't find" with a
  `search_people → ok`, zero-id ledger row, a polite refusal to "call her",
  dead provider port → 503 `operator_unavailable` in ~100 ms with a
  `provider_error` row (one retry, `model_call_count = 2`).
  Review/tester findings applied: `Usage::add` now saturates (a hostile
  endpoint could panic the spawned turn → 500 and no ledger row); a
  tool in flight at the turn deadline is recorded; NUL/invisible
  characters are stripped from the model's search query (was a fake
  `tool_error` outage via Postgres 22021) and from `UntrustedText`
  (bidi/zero-width); `TodayView.truncated` honours the tool `limit`;
  request body limit 256 KiB; redacting `Debug` on `TurnInput` /
  `TurnOutput` / `ChatRequest`; Groq response body capped at 1 MiB;
  mutex poisoning no longer wedges the in-flight set; the crate fence
  test also catches `package = "sqlx"` / `[dependencies.sqlx]`; new
  DB tests for ledger-insert failure (200 + no row, one transaction),
  LIKE escaping + contact match + foreign contact values, the
  append-only triggers against the owner, and validation boundaries.

- 2026-08-20: Repository initialized; initial commit of carried-over
  documents (AGENTS.md, CLAUDE.md, README.md, product thesis, event-sourcing
  research).
- 2026-08-20: Documentation bootstrap committed (`e5af54c`): canonical
  `docs/` structure, decision log (D-001–D-012, O-001–O-005), architecture
  baseline, this state file.
- 2026-08-20: D-013 accepted and recorded; AGENTS.md/README secrets
  statements reconciled; `.gitignore` added.
- 2026-08-20: D-014/D-015/D-016 accepted; `docs/specs/SLICE_000.md` drafted,
  independently reviewed (ready with minor amendments, all applied), and
  user-approved (`b3bd051`).
- 2026-08-20: Slice 000 implemented on `slice-000-foundation`: Cargo
  workspace with `crm-api` (Axum health/ready, request-id, graceful
  shutdown, typed config with loopback/timeout-bound validation, lazy SQLx
  pool); Vue 3 + Vite web shell with `/api` proxy; Docker Compose for
  PostgreSQL 18 and Centrifugo v6 (loopback-only); `scripts/bootstrap`,
  `dev-services`, `dev-api`, `dev-web`, `dev-tunnel`, `check`,
  `check-services`; README Development section rewritten to match D-016.
- 2026-08-20: Independent review (`crm-reviewer`) and adversarial test
  analysis (`crm-tester`) run against the implementation; both verdicts
  were "ready to merge." Applied fixes: `dev-tunnel` now scopes only the
  tunnel token into `cloudflared`'s environment instead of the whole
  `.env`; dropped an unused `tower-http` Cargo feature; clippy lints set
  to `deny` (matching the spec's wording, not just the check script's `-D
  warnings` flag); readiness failures now log a specific reason (timeout
  vs. query error) instead of none; per-request tracing (method, path,
  latency, status) is now visible under the documented default `RUST_LOG`
  (tower-http's span/request/response levels were DEBUG, silently
  filtered by the default `info` filter); added a test proving the
  readiness timeout bound holds against a peer that completes the TCP
  handshake but never responds (previously only "connection refused" was
  tested); corrected a test comment that mischaracterized sqlx's
  connect-refused retry/backoff as "failing fast."
- 2026-08-20: Slice 000 merged to `main` (`e5182d1`, pushed);
  `slice-000-foundation` deleted.
- 2026-08-20: Slice 001 planned (narrow-cut and no-trait decisions
  user-accepted), spec drafted, independently reviewed (14 findings
  applied — notably: DB-backed tests now exercise the router as `crm_app`
  not the schema owner; `crm_migrator` made the dev database's owner so
  the first migration doesn't fail; login always mints a fresh token
  against session fixation; logout leaves the cookie in place on a
  persistence failure), and user-approved (`b6e8764`).
- 2026-08-20: Slice 001 implemented on `slice-001-identity`: migrations
  harness (`sqlx::migrate!`, hand-authored SQL, embedded `migrate`
  binary) with the `crm_migrator`/`crm_app` role split provisioned by
  `dev-services up`; `organization`/`app_user`/`local_credential`/
  `organization_membership`/`user_session` schema; session/auth core
  (Argon2id, HMAC-SHA256 session tokens, `AuthContext` extractor
  re-verifying membership every request); `POST`/`DELETE /api/session`,
  `GET /api/me`, `GET /api/organization/members`; `seed` binary +
  `scripts/dev-seed` (two Organizations, idempotent); web login UI
  (conditional rendering, no router/Pinia); `scripts/db-migrate`,
  `check-db`.
- 2026-08-20: Independent review (`crm-reviewer`) and adversarial test
  analysis (`crm-tester`) run in parallel against the implementation;
  both independently found the same two real security bugs. Fixed:
  Argon2id password verification now runs under `tokio::task::
  spawn_blocking` (it was running synchronously on the async runtime,
  so a burst of login attempts — including the dummy-hash path, which
  needs no valid account — could stall every other concurrent request
  on that worker thread); the session-fixation revoke-on-relogin now
  fires only after login has actually succeeded and only when the
  presented cookie belongs to the same user who just authenticated
  (previously, presenting *any* leaked session token — even a different
  account's — during one's own login would silently revoke it with no
  ownership check; manually verified fixed by having one seeded user
  log in while presenting a second seeded user's real cookie and
  confirming the second user's session survives). Also strengthened the
  `crm_app`-grants test to check all four read-only tables (previously
  only spot-checked one) and added the members-list assertion a
  differently-named test had promised but not made, a body-based
  org-id-ignored probe, and a real second-logout-call idempotency test —
  15 DB-backed tests total, all passing after the fixes.
- 2026-08-20: Slice 001 merged to `main` (`587a087`, pushed);
  `slice-001-identity` deleted.
- 2026-08-20: Created a dedicated `crm-dev` Cloudflare tunnel (separate
  from the account's other tunnels — notably an existing `k8s-crm`
  tunnel with live traffic, deliberately left untouched) after the user
  chose `app.tarams.org` (browser) / `api.tarams.org` (API) as two
  separate hostnames with the browser calling the API directly. Added,
  on `tunnel-cors-config`: two new optional config variables
  (`CRM_CORS_ALLOWED_ORIGIN`, `CRM_SESSION_COOKIE_DOMAIN`; both unset by
  default, preserving Slice 001's exact original no-CORS/host-only-cookie
  behavior for loopback dev and any single-hostname tunnel); a CORS layer
  added to the API only when configured, using `AllowOrigin::list` (not
  the single-`HeaderValue` form, which — caught by a new test —
  unconditionally echoes the configured origin regardless of the
  request's actual `Origin`, rather than only when it matches); the
  session cookie's `Domain` attribute now configurable; the web shell
  now detects an `app.*` hostname at runtime and calls `api.*` directly,
  leaving loopback/Vite-proxy behavior untouched. 9 new tests (2 config
  parsing, 2 cookie-domain, 3 CORS behavior at the HTTP level, plus
  fixing 2 pre-existing cookie tests whose expectations didn't account
  for the `cookie` crate's RFC 6265 leading-dot normalization). Full
  suite green (34 unit + 17 integration + 15 DB-backed).
- 2026-08-21: Live tunnel setup and end-to-end verification, working
  through real obstacles rather than the originally-planned dashboard
  flow:
  - `crm-dev` turned out to be a **locally-managed** tunnel (Cloudflare's
    dashboard Public Hostname UI only works for dashboard-managed
    tunnels, and migrating is irreversible); pivoted to a committed
    `infra/development/cloudflared/config.yml` declaring both ingress
    rules instead, authenticating via the credentials file `cloudflared
    tunnel create` already wrote outside the repo — no token needed.
    `scripts/dev-tunnel` and `.env.example` updated accordingly
    (`CLOUDFLARED_TUNNEL_TOKEN` removed as unused).
  - The Access application was created via the Cloudflare API (a
    dashboard token with "Access: Apps and Policies" Edit scope could
    create the application but got `auth.forbidden` on both the nested
    and top-level policy-creation endpoints; the policy was ultimately
    attached by including it inline in the same `PUT` that updates the
    application, which the same token *could* do).
  - **Cloudflare Access scopes its login session per hostname, not per
    Application** — authenticating at `app.tarams.org` does not also
    authenticate `api.tarams.org`, even under one Application with one
    policy, contrary to the original assumption when this design was
    chosen. Visiting each Access-protected hostname once directly
    (a real top-level page load) establishes a session for that
    hostname; only after both are established does the browser's
    background `fetch()` from `app.*` to `api.*` carry a valid session.
    Documented for anyone extending this pattern later.
  - CORS preflight (`OPTIONS`) requests were being intercepted by Access
    itself (redirected to its login page) before ever reaching the
    API's own CORS layer; fixed via the Access application's
    `options_preflight_bypass: true` setting, which lets preflight
    requests through to the origin.
  - Found and fixed a real bug while wiring this up: `web/src/App.vue`'s
    `fetch()` calls all used `credentials: 'same-origin'`, which does
    not send or store cookies on the genuinely cross-origin `app.*` →
    `api.*` calls this design requires. Changed to `credentials:
    'include'` (correct and harmless for the same-origin/loopback case
    too).
  - End-to-end login through the tunnel confirmed working by the user
    after these fixes.
- 2026-08-21: Tunnel work merged to `main` (`3b6df76`, `122c7e4`).
- 2026-08-21: Frontend stack (D-017), production ingress (D-018), and
  stage model (D-019) decided with the user and logged.
- 2026-08-21: Slice 002 planned with `crm-planner` (scope decisions
  fixed by the user first), plan independently reviewed by
  `crm-reviewer` (17 findings — notably: a per-contact-value advisory
  lock would miss mixed email+phone payloads, so intake takes one
  per-Organization lock; sqlx macros would silently compile online
  against the drifted dev DB via dotenvy's parent-directory walk, so
  `SQLX_OFFLINE` is defaulted through a cargo config; a plain SHA-256 of
  a tiny PII payload in an immutable row is a dictionary oracle after
  erasure, so the content hash is a keyed HMAC; the planner's
  Organization-wide `/inquiries` list was scope creep and is cut),
  spec drafted, spec re-reviewed by the same reviewer (12 further
  items — notably: the cargo config must live at the repo root because
  cargo walks from cwd not `--manifest-path`; the `sqlx-prepare` script
  as first written was circular and now migrates the throwaway DB with
  the CLI; intake's four facts share both `occurred_at` and
  `recorded_at` so history ordering needs an explicit `kind_rank`; the
  trigger test must hit a real row or passes vacuously; history `detail`
  shapes and `display_name` derivation were unspecified for the
  frontend lane). All findings applied as safe defaults; the reviewer
  classified zero items as blocking decisions.
- 2026-08-21: Slice 002 implemented in two parallel lanes (backend on
  `slice-002-intake`, frontend on `slice-002-web` worktree; sqlx-cli
  0.9.0 replaced with the locked 0.8.6). Both self-verified green
  (`check`/`check-db` backend; lint/typecheck/build frontend), then sent
  through independent review and adversarial testing against the real
  diffs. Review: contract cross-check between backend response bodies
  and frontend types found zero discrepancies; a few IMPLEMENTATION_DETAIL/
  LATER nits. Adversarial testing found real issues, all fixed:
  - **An undisclosed file**: the backend lane had built
    `dump_raw_payloads.rs` + `scripts/dump-raw-payloads`, a tool that
    decrypted every Organization's raw lead payloads into one permanent,
    unscoped plaintext table — never in the spec, never in the
    implementer's own "files changed" report, caught only by diffing
    actual `git status` against the report. Deleted; a standing memory
    now says to always cross-check implementer-reported file lists
    against real git state.
  - A `TRUNCATE` gap in the append-only trigger (row-level triggers
    don't fire on `TRUNCATE`; `crm_migrator` could empty a fact table)
    — fixed with a second `FOR EACH STATEMENT` trigger + test.
  - `routing_decision.strategy` had no CHECK constraint backing a
    Rust `unreachable!()` — added the constraint, changed to a typed
    error.
  - Three `raw_payload` queries skipped the `organization_id` predicate
    every other query in the slice includes — added for defense in
    depth.
  - No logging on intake failure paths — added, keyed by error variant
    name only, never error text.
  - A cross-tenant availability bug: one Organization's intake burst
    could exhaust the shared, unconfigured connection pool and 503
    every *other* Organization's logins/reads (empirically reproduced:
    12 concurrent requests against a held lock). User chose to fix the
    actual mechanism over containment or a PgBouncer-based approach
    (which wouldn't have helped — transaction-mode pooling still binds
    a connection for an open transaction's full duration): replaced the
    blocking `pg_advisory_xact_lock` with a bounded `pg_try_advisory_xact_lock`
    retry loop (3s budget, jittered backoff, releases the connection
    between attempts) failing closed to a new 503 `intake_busy` +
    `Retry-After` rather than parking a connection indefinitely.
    Re-reproduced the original exploit as a test: busy Organization
    fails in ~3.2s (not the full 7s external hold), an unrelated
    Organization's 5-request burst during that same hold completes in
    ~500ms, all succeeding.
  - Frontend: a 401 encountered outside the router guard (session
    expiring while idle on a page) didn't redirect to `/login`,
    contradicting the code's own comment — fixed with a global
    `QueryCache`-level handler, mutexed against the guard's own redirect.
  - History timeline rows were ~64px against `UI_STYLE.md`'s specified
    56px — fixed.
  Spec `§3`/`§5`/`§8`/`§9` updated to describe the as-built bounded-retry
  mechanism and the `intake_busy` error code.
- 2026-08-21: Full live walkthrough run — both dev servers together
  (backend from the main checkout, frontend from the worktree, real
  Postgres), driven with a headless Playwright browser script (no
  project `run` skill existed for this repo; none created, since
  the setup — two servers in two locations, real login — was specific
  to this verification, not a reusable pattern yet). One real
  environment hazard found and fixed along the way: `.env` was left in
  tunnel-mode config (`CRM_SESSION_COOKIE_DOMAIN=tarams.org`,
  `CRM_SESSION_COOKIE_SECURE=true`) from earlier tunnel testing, which
  a loopback browser correctly refuses (a `Domain=tarams.org` cookie
  from a `127.0.0.1` response is invalid) — login 200'd but the cookie
  never stuck; temporarily switched to loopback values for the test,
  restored exactly afterward. Also found and killed a stale Vite
  process serving the *old* Slice 001 frontend on the same port,
  which the readiness check had passed against by coincidence.
  Walkthrough covered: login; new lead → Person detail with all four
  history facts; stage change; reassignment; repeat lead with the same
  email → dedup, `kept_existing` routing confirmed live; unresolved
  lead (no contact method); duplicate delivery; logout + cross-Organization
  isolation (second Organization's login shows zero of the first's
  data). Zero console or page errors. Found and fixed one cosmetic nit
  live (`DataTable`'s footer said "1 unresolved leads"; added proper
  singular/plural support) and re-verified.
- 2026-08-21 (`fac3413`, fast-forwarded to `main` from `post-002-ui-fixes`;
  root-level screenshots ignored in `e6dbd50`), user-reported from live use
  through the tunnel: three fixes, no slice open.
  1. **Stage marker (D-020)**: `components/StageLabel.vue` +
     `lib/stages.ts`; wired into the People table's stage badge and the
     Person detail stage `Select` (`#value` and `#option` slots).
     `UI_STYLE.md` §5 and §9 amended to record the exception.
  2. **`Select` menu overflowed its own panel**: `lib/controls.ts`'s
     `selectPt()` put `max-h-64 overflow-auto` on the `list` `<ul>`, but
     PrimeVue sets an inline `max-height: {scrollHeight}` (14 rem) on the
     `listContainer` and, unstyled, ships no `overflow` for it — so with
     nine stages (D-019) the 368 px `<ul>` spilled out below the panel's
     background and border and painted over the page beneath it (the user
     saw "Trash" sitting on top of the Inquiries card). Scroll moved to
     `listContainer`; measured after the fix, the container clips the
     139 px of overflow. Affects every `Select`, not just stage. `root`
     also gained `gap-2` so the value cannot touch the chevron.
  3. **Permanent "Loading…" whenever `me` failed with anything but a 401**:
     the router guard deliberately lets a non-401 `me` failure through so
     "the view's own query can show an error state" (`router.ts`), but each
     view derives `orgId` from `me` and keeps its queries `enabled: false`
     until it resolves — and a *disabled* TanStack query reports
     `isPending` forever. The intended error state was therefore
     unreachable: every screen sat on "Loading…" with nothing to click.
     `AppShell.vue` now renders the `me` error plus a "Try again" button in
     place of the routed view, guarded on `me` having no cached data at all
     so a failed background refetch cannot replace a working screen.
- 2026-08-21 (environment, same session): the tunnel was broken and the
  cause was `.env` drift, not the app. `CRM_CORS_ALLOWED_ORIGIN` was empty,
  so the API attached no CORS layer and the browser discarded every
  cross-origin response from `api.tarams.org` to the `app.tarams.org` page
  — the API logged twelve `/api/me` 200s the client never saw, and
  `/api/people` was never requested (this is what surfaced fix 3 above).
  Restored to `https://app.tarams.org` and the API restarted;
  `/api/session` and `/api/people` immediately 200'd from the user's
  browser. Note that the Slice 002 walkthrough entry above claims the
  tunnel-mode values were "restored exactly afterward" — they were not:
  all three (`CRM_CORS_ALLOWED_ORIGIN`, `CRM_SESSION_COOKIE_DOMAIN`,
  `CRM_SESSION_COOKIE_SECURE`) were left at loopback settings. See Pending
  work item 4.
- 2026-08-22: D-024 (Cloudflare Access removed from the dev tunnel) and
  D-025 (the tunnel is actually dashboard-managed; the real WebSocket
  routing fix and its verification) — full narrative in the decision
  log. Net effect verified live over the real tunnel with two separate
  headed-Chrome sessions (Alice, Carol): a brand-new lead posted
  directly to the API appeared on the assignee's Today in under a
  second with no reload; reassigning it from the Person-detail page
  removed it from Alice's Today and added it to Carol's, live, in a
  second real browser window, in under a second. This is the first time
  the Slice 003 realtime path has been proven working through the
  actual public tunnel rather than only on loopback.

- 2026-08-22: Slice 004 implemented in two parallel lanes (backend on
  `slice-004-admin` in the main checkout, owning the migration; web on
  `slice-004-web` in the `../crm-web` worktree, built against the
  frozen §5 contract without waiting on a running backend). Both
  self-verified green, then independently reviewed and adversarially
  tested against the real diffs:
  - `crm-reviewer` found zero blocking issues; confirmed the one
    coordinator-approved deviation from the spec's literal grant block
    (`GRANT SELECT ON invitation TO crm_app;` — every sibling table in
    that line already had SELECT from an earlier migration, `invitation`
    was the one omission, and no narrower column-level grant works
    because `token_hash` is used in the same `WHERE` clauses that
    authenticate a token) as a safe mechanical correction, not a policy
    decision. Two IMPLEMENTATION_DETAIL findings: a stale `stage.rs` doc
    comment (fixed) and a defensible, non-fixed query-invalidation
    narrowing in `web/src/api/queries.ts`.
  - `crm-tester` found no exploitable tenant-isolation or
    authorization-bypass bug. One real low-severity finding: `IssueInvitation`
    racing a concurrent `AcceptInvitation` for the same `(org, email)`
    could surface a bare 503 instead of a clean error, or leave a
    harmless dangling invitation row, under READ COMMITTED's
    per-statement snapshot — fixed with a re-check immediately before
    insert plus a `check_violation` retry. Two test-coverage gaps
    closed: a true concurrent double-accept test (mirroring the
    existing last-admin race test's `tokio::spawn`/`tokio::join!`
    pattern, not sequential accept-then-accept) and platform-route
    403-for-org-admin coverage extended to all five routes (previously
    three of five).
  - Both lane file lists cross-checked against real `git status`/`git
    diff --stat` at every hand-off — no undisclosed files, no repeat of
    the Slice 002 lesson.
  - Live headless-browser walkthrough of spec §1 steps 2–9 (real
    backend on :3000, real frontend, real Postgres/Centrifugo): 46/46
    assertions passed — platform-admin Organization creation and
    first-admin invitation, promote/demote/deactivate/reactivate with
    the last-admin invariant enforced through real UI actions, revoke/
    re-issue, and every tenant-isolation and platform-vs-tenant
    authorization boundary probed directly (403/404/401 exactly as
    contracted). Zero real application console errors (the walkthrough
    logged Chrome's automatic "failed to load resource" lines for
    intentional negative-path 401/403/404/409 probes and the anonymous
    session-check on public routes — not unhandled errors).
  - Final counts, verified by direct re-run rather than trusted from
    agent reports: backend 143 lib unit tests + 133 integration tests
    (38 service-free via `./scripts/check`, 95 DB/Centrifugo-backed via
    `./scripts/check-db`, including 24 new in `db_admin.rs`); web 57
    Vitest tests across 5 files. All green.
  - Found live (not a slice defect, an environment-tooling gap):
    `./scripts/dev-services down` does not drop the Postgres named
    volume, so it does not give a true clean slate on its own — needs
    `down -v` or an explicit volume prune. Noted in Pending work below.

- 2026-08-22: Slice 004 spec §1 steps 2–9 re-walked-through through the
  real Cloudflare tunnel (`https://app.tarams.org`/`https://api.tarams.org`,
  not loopback), specifically because this project's history (D-024/D-025)
  has repeatedly found tunnel-only bugs that loopback testing misses (the
  Slice 003 realtime WebSocket 404'd silently through the tunnel for weeks
  before D-025 caught it). Fresh `dev-bootstrap` seed data
  (`owner@platform.test`, Acme/Best); Cedar Realty and Erin/Frank/Gina
  created live during the walkthrough per the spec script. Headless
  Playwright against `https://app.tarams.org` (adapted from the loopback
  walkthrough script, with `apiFetch` resolving explicitly against
  `https://api.tarams.org` since the app's own `resolveApiBaseUrl()` does
  that at runtime and a bare relative fetch from an `app.*` page would
  otherwise wrongly resolve against `app.*` itself): **48/48 assertions
  passed** (the original 46, plus 2 new ones instrumenting step 6's
  realtime disconnect directly). `./scripts/check-tunnel` confirmed
  plumbing first (app 200, API health 200, WebSocket upgrade 101).
  - **Step 6 (Frank deactivates Erin) got explicit WebSocket lifecycle
    instrumentation** via Playwright's `page.on('websocket')`, tracking
    Erin's live Centrifugo connection (`wss://api.tarams.org/connection/
    websocket`) with open/close timestamps, attached before any
    navigation. Confirmed live, with no page reload: Erin's WebSocket
    closed **179ms after** Frank's deactivation request resolved, and her
    tab's URL had not changed at that moment (ruling out the close being
    a side effect of a forced navigation rather than a genuine server-
    initiated disconnect). Cross-checked against the API's own log: zero
    `realtime disconnect failed` warnings anywhere in the run (the
    `disconnect_user` codepath in `realtime/publisher.rs` only warns on
    failure; success is `debug!`-only and filtered by the default log
    level, so silence plus the observed close is the expected signature
    of success). This is the first time this exact codepath (Centrifugo's
    HTTP disconnect API call, followed by the client's WebSocket actually
    closing) has been proven over the real tunnel rather than loopback.
  - A first run of the instrumented script scored 46/48 — a bug in the
    script, not the app: the WebSocket tracker was attached *after*
    navigating to `/today` and waiting, by which point the connection
    (opened within about a second of mount) had already been missed
    (`page.on('websocket')` only fires for connections opened after the
    listener is registered). Fixed by attaching the tracker at page
    creation, before any navigation, and re-run clean; required a full
    `dev-services down -v && up && db-migrate && dev-bootstrap` reset
    first since the first run's Cedar Realty/Erin/Frank/Gina data would
    otherwise collide with the second run's unique-name/invitation-state
    assumptions.
  - No other tunnel-specific behavioral differences from the loopback
    run: CORS, the `Secure` cross-origin cookie, and the WebSocket upgrade
    path all held up unchanged. Zero unhandled console/page errors; all
    observed "Failed to load resource" console lines were the anonymous
    session-check on public routes or the walkthrough's own deliberate
    401/403/404/409 negative-path probes.
  - Backend/frontend/tunnel processes stopped after the run; dev-services
    (Postgres, Centrifugo) left running per instruction.

- 2026-08-24: Dev seeding moved from `crm-admin seed-dev` (in-process
  domain-function calls) to `scripts/seed_dev.py` (the live HTTP API),
  on a new branch `dev-seed-via-api` off the same commit as
  `slice-007a-intake-address` (kept separate since it's dev tooling, not
  part of 007a's feature scope). `scripts/dev-bootstrap` now wipes and
  recreates the database on every run instead of being idempotent — user
  choice ("I always want to start from a baseline"). Found and fixed
  along the way: an Organization created before the 007a migration ran
  fell into its backfill path and got a generic `org-<uuid8>` intake
  slug instead of a name-derived one; a genuinely blank database (this
  new flow's starting point) never hits that path. `./scripts/check`
  equivalent (fmt, clippy `-D warnings`, `cargo check --workspace`, full
  `cargo test --workspace`) green; both rewritten DB tests
  (`db_identity.rs::platform_bootstrap_flow_rejects_repeat_creation`,
  `db_people.rs::organization_creation_seeds_nine_ordered_stages_and_two_members`)
  pass against a real Postgres instance. `dev-bootstrap` run live
  end-to-end against the real dev database: wiped, migrated, reseeded
  Acme/Best Realty with correct name-derived slugs (Cedar Realty and
  Cypress Bay Realty, prior manual test data, were lost in the wipe —
  expected and disclosed before running). Both branches reviewed +
  amended + merged to `main` (`b91dcb9` and `110bd95`); origin/main
  pushed through `24377c4`.

- 2026-08-24: Slice 007b implementation started (partial). Domain layer
  complete: `receive_inbound_email` function in `domain/intake/receive.rs`
  (Phase-A-only: parse recipient → resolve org by slug+token with
  constant-time compare → seal → insert pending → mark unresolved
  email_unparsed → publish realtime event). Config type complete:
  `InboundEmailSecret` in crm-app `config.rs` (32-byte minimum, redacted
  Debug, parse/as_bytes accessors). Both committed on
  `slice-007b-inbound-email`, merged to `main` `110bd95`.

- 2026-08-24: Slice 007b completed on `slice-007b-inbound-email-api`
  (branched off `main` `ca89d70`): `POST /inbound/email`
  (`crm-api/src/routes/inbound_email.rs`) — bearer-first constant-time
  auth, then JSON, then base64; mounted outside CORS like
  `livekit_webhook.rs`; own `DefaultBodyLimit::max(2 MiB)`; new additive
  `ApiError::PayloadTooLarge` (413, keeps the shared envelope). Config/
  state wiring: `Config.inbound_email_secret: Option<InboundEmailSecret>`
  (`CRM_INBOUND_EMAIL_SECRET`, unset/short handled per §6),
  `AppState.inbound_email_secret`. `.env.example` documents the new
  variable name-only. Web: `UnresolvedReason` gains `'email_unparsed'`,
  `UNRESOLVED_REASON_LABEL` gains `"Unparsed email"`, one Vitest line.
  Tests: `tests/db_inbound_email.rs` (13, criteria 1–9/11–15/19–20 incl.
  the concurrency race, the stuck-pending rescue, and the §9 tracing-
  capture leak test across accepted/rejected/malformed/bad-bearer) and
  `tests/inbound_email.rs` (5, service-free: no-bearer 401, secret-unset
  401, unreachable-DB 503, no-CORS-headers, oversize-body-with-bad-bearer
  413-with-envelope). Two synthesized `.eml` fixtures under
  `crm-api/tests/fixtures/email/`. `scripts/inbound-email` (bash,
  `demo-leads`-style: sources `.env`, secret never on argv, GNU/BSD
  base64 no-wrap handled).

  Independent review (`crm-reviewer`) and adversarial test analysis
  (`crm-tester`) run in parallel against the real diff; both
  independently found the same two issues, both pre-existing in the
  already-merged `receive.rs` (not introduced by this session), both
  fixed:
  - The unknown-slug lookup path short-circuited via `.ok_or(...)?`
    before ever calling `constant_time_eq`, so it skipped the dummy
    8-byte compare §4 step 2 and criterion 7 explicitly require — the
    wrong-token and unknown-slug paths did not have identical work
    profiles as documented, though the HTTP response was already
    byte-identical (no observable oracle; `crm-tester` and `crm-reviewer`
    both rated this low-severity/IMPLEMENTATION_DETAIL). Fixed: the
    `None` branch now runs `constant_time_eq` against a fixed
    `DUMMY_TOKEN` before returning `OrgNotFound`. Added unit tests for
    `constant_time_eq` (receive.rs had none) — single-bit near-miss
    coverage, length-mismatch, and the dummy-token length invariant.
  - `crm-reviewer` additionally noted the DB-unset 503 path in the route
    never recorded `span.record("outcome", …)` unlike the DB-unreachable
    503 path — fixed with one `"unavailable"` record.
  - `crm-tester` additionally flagged that criteria 1 and 11's explicit
    "`received_at`/`occurred_at` = receipt time" and "fresh
    `correlation_id`" claims weren't directly asserted — fixed by adding
    before/after timestamp-window and correlation-id assertions to the
    existing storage and publish tests.
  - `receive_inbound_email` computing `received_at = Utc::now()`
    internally rather than accepting it as a parameter (deviating from
    SLICE_007b.md §4's pseudocode signature) was flagged by both agents
    as harmless — `Utc::now()` runs before any DB work, functionally
    equivalent to receipt time for this rung's synchronous delivery
    path — and left as a LATER note, not fixed (matches production
    behavior; only 007g's queued/retried receiving path could ever make
    this observable, and no test today would catch a regression there).

  `./scripts/check` and `./scripts/check-db` both green after the fixes
  (156 service-free + 236 DB-backed backend tests incl. the 13 new
  `db_inbound_email` + 5 new `inbound_email`; web lint/typecheck/248
  Vitest/build). Live walkthrough: `scripts/inbound-email` against the
  real dev stack (real Postgres, real dev-api rebuilt onto this branch)
  — first delivery to Acme Realty's real intake address (`leads-
  yhbchrw7@acme-realty.elysianfeld.com`) → 200 accepted, one row
  (`source=email`, `payload_format=rfc822_v1`, `origin=webhook`,
  `resolution=unresolved`, `unresolved_reason=email_unparsed`);
  byte-identical re-delivery → 200 accepted, still one row; wrong token
  → 200 rejected, nothing stored; unknown slug (post-fix) → 200
  rejected, nothing stored; `GET /api/intake/unresolved` as
  `alice@acme.test` shows the row with `reason: "email_unparsed"`,
  matching the web label. Not covered by this walkthrough: a live
  browser screenshot of the Unresolved table (no Playwright runner in
  this repo; the API-level check plus the Vitest-pinned label were used
  instead — noted as a gap, not a blocker, consistent with prior
  slices' web-verification posture where no browser tooling exists).

  Not yet committed to `main`: awaiting user review of the diff summary
  and explicit commit approval (AGENTS.md Phase 9).

- 2026-08-24/25: Slice 007c implemented on `slice-007c-system-routing`
  (branched off `main` `778c2e2`), single lane per the spec's §11
  ownership: migration `20260829000001_intake_settings.sql` (the
  nullable `intake_default_assignee_user_id` FK, its scoped `crm_app`
  UPDATE grant, the widened `routing_decision.strategy` CHECK);
  `FactEnvelope::for_system` (`domain/envelope.rs`); `IntakeActor`
  (`domain/intake/mod.rs`) — `User(CommandContext)` /
  `System { organization_id, origin, correlation_id }` with
  `organization_id()`/`origin()`/`correlation_id()`/`actor_kind()`/
  `user_actor_id()`/`envelope()` accessors; `receive_inquiry`'s actor
  parameter and routing matrix refactored onto it (`determine_routing`
  is now async, doing the System-actor default-assignee lookup inside
  the same Phase-B transaction via three new `admin_queries` functions
  — `intake_default_assignee_user_id`, `update_intake_default_assignee`,
  `is_active_member`, `active_intake_default_assignee`); two new
  `RoutingStrategy` variants (`organization_default`, `unassigned`) with
  `from_str`/`as_str` round-trips; the `assignment_changed` NULL→NULL
  no-op fact suppressed for `unassigned`. `GET`/`PUT
  /api/organization/intake-settings` (`routes/organization.rs`) — the
  PUT parses the body as a raw `serde_json::Value` specifically because
  serde's derive implicitly defaults *any* `Option<T>`-typed field to
  `None` on a missing key, which would have silently equated "key
  omitted" with "explicit null" and defeated the contract's required
  400-vs-clear distinction. `crm-admin receive-inquiry` subcommand (the
  walkthrough trigger; validates the Organization exists before Phase A,
  re-serializes the payload exactly as `routes/intake.rs` does before
  sealing so `content_hmac` dedup matches `POST /api/inquiries`); a new
  standalone `config::raw_payload_key_config` factored out of
  `Config::from_source` (the `intake_mail_config` precedent) so the CLI
  doesn't need a full `Config`. Web: `IntakeSettingsResponse`/`Request`
  types, `RoutingStrategy` union extended, `useIntakeSettings`/
  `useUpdateIntakeSettingsMutation` hooks, the "Unattended lead routing"
  card in `IntakeSettingsView.vue` (active-members-only dropdown +
  explicit Unassigned option, unset/deactivated warnings);
  `PersonDetailView.vue` needed only two new `ROUTING_STRATEGY_LABEL`
  entries — the "System" actor label fell out of the existing
  `row.actor?.display_name ?? 'System'` fallback with no code change,
  since a null actor was previously unreachable and is now exactly what
  a system fact produces.

  New tests: `tests/db_intake_settings.rs` (5: tenant isolation +
  role-gating, byte-identical 422 across nonexistent/foreign/inactive,
  explicit-null-clears/absent-key-400/malformed-UUID-400/
  GET-survives-deactivation, the `crm_app` grant scoped to the new
  column, the CHECK accepting both new strategies and rejecting an
  unknown one); `tests/db_intake_system_routing.rs` (7: criteria 3–9 —
  default set routes `organization_default` with all facts sharing one
  correlation id/system shape and landing on the default's Today only,
  no default routes `unassigned` with no assignment fact, a
  since-deactivated default routes `unassigned` with the setting
  retained, `kept_existing` writes no new assignment fact, a
  matched-but-previously-unassigned Person picks up a newly-set default
  with exactly one fact, a system-routed payload's exact-byte re-POST
  via `POST /api/inquiries` reports `organization_default` with
  `duplicate: true`, and — added after adversarial review, see below —
  a system-actor intake into one Organization writes zero rows into
  another); `tests/intake_settings.rs` (4, service-free: 401/503 for
  both routes); 2 new unit tests each in `receive_inquiry.rs` (the
  `RoutingStrategy` round-trip and fail-closed) and `domain/intake/
  mod.rs` (`IntakeActor` accessors/envelope construction, both actor
  kinds); Vitest additions in `IntakeSettingsView.test.ts` (6: unset/
  active/deactivated warning states, dropdown lists active members
  only, choosing a member PUTs the value, and — added after adversarial
  review — choosing Unassigned PUTs an explicit `null`, not an omitted
  key) and `PersonDetailView.test.ts` (2: the two new routing-strategy
  label pins, actor "System"). The full existing `db_intake.rs`
  suite (18 tests) re-run unmodified and green as the criterion-8
  regression gate.

  Independent review (`crm-reviewer`) and adversarial test analysis
  (`crm-tester`) run in parallel against the real diff (not a
  description of it — both re-ran `cargo check`/`clippy`/the full test
  suites themselves). Both verdicts: spec-faithful, no blocking issues,
  no exploitable tenant-isolation or authorization bypass, no way to
  write an invalid assignee to the column, no way to produce a
  NULL→NULL `assignment_changed` fact, and the PUT's byte-identical-422
  guarantee holds structurally (one static error tuple regardless of
  cause). Both independently converged on the same real gap — §7
  explicitly requires a pinned test that "a system-actor intake into
  org A writes zero rows in org B," and no such test existed (the code
  was already correctly scoped by inspection: `IntakeActor::System`'s
  `organization_id` is server-supplied and every new query, including
  `active_intake_default_assignee`'s join, is scoped by it — but this
  is exactly the kind of copy/paste parameter-order mistake a test
  should catch, not inspection). `crm-tester` additionally flagged the
  one untested wire-format case that matters most given the PUT's
  absent-key-is-400/explicit-null-clears contract: the web test suite
  asserted a PUT body for setting a value but not for clearing one via
  the "Unassigned" option (which the code already handled correctly —
  `IntakeSettingsRequest`'s field is required, not optional, so
  `JSON.stringify` always includes the key). Both fixed with new tests
  (listed above); no application code changed as a result. `crm-tester`
  also noted, informationally only, that the diff deletes a stale
  `.sqlx` cache file for an unrelated Slice-004-era query no longer in
  source — expected fallout of the required `cargo sqlx prepare`, not a
  regression.

  `./scripts/check` and `./scripts/check-db` both green after the fixes
  (160 crm-app + 58 crm-operator unit tests, 256 Vitest, plus the new
  DB-backed and service-free integration tests above; `cargo sqlx
  prepare --check --workspace` confirmed no offline/schema drift).

  Live walkthrough against the real dev stack: killed the orphaned
  stale-binary `dev-api` process (the known "dev-api runs orphaned"
  hazard) and restarted it onto this branch; `./scripts/dev-bootstrap`
  for a clean Acme/Best seed. Ran the full §1 script end-to-end over
  real HTTP/CLI against the real Postgres: (1) set Carol (Acme member)
  as the default assignee via `PUT /api/organization/intake-settings`;
  (2) `crm-admin receive-inquiry --organization <acme> --source website
  --payload-file lead1.json` → `intake resolved: ... routing
  organization_default -> <carol>`; (3) as Carol, `GET /api/today` shows
  the new Person, `GET /api/people/{id}` history shows all four facts
  with `actor: null`/`origin: "cli"` (renders as "System" in the UI) and
  the `organization_default` routing decision naming Carol; (4) cleared
  the setting, delivered a second lead → resolved `unassigned`, visible
  in `GET /api/people`, on neither Carol's nor Alice's Today; (5) tried
  setting the default to Carol while she was already deactivated → 422
  `invalid_assignee`; reactivated her, set her as default, deactivated
  her again, confirmed `GET` still returned her id (retained), delivered
  a third lead → resolved `unassigned`, nothing errored. Also drove a
  real headless Chromium (via a throwaway Playwright script, since
  neither `chromium-cli` nor a project `run` skill exists in this repo —
  a gap worth `/run-skill-generator` if browser walkthroughs recur)
  against the already-running real Vite dev server: logged in as Alice,
  navigated to `/manage/intake`, screenshotted the card in the
  deactivated-default state (dropdown blank, red "deactivated" warning,
  visually consistent spacing/borders with the address card above it),
  then selected "Unassigned" in the real dropdown and screenshotted the
  resulting unset-warning state — confirmed via a follow-up `GET` that
  the click genuinely persisted through the real PUT. One console 401
  observed, matching every prior slice's documented signature (the
  anonymous session-check on the public `/login` route before
  authentication) — not an application error. `dev-web` (already
  running, pre-existing) and `dev-services` left running per the
  established convention from prior walkthroughs; the restarted
  `dev-api` also left running.

  Not yet committed: awaiting user review of the diff summary and
  explicit commit approval (AGENTS.md Phase 9).

## Pending work

1. Resolved 2026-08-22 (D-024): the Cloudflare Access application was
   deleted rather than documented more durably — the user decided the
   app's own login is sufficient for the dev tunnel and Access was
   redundant friction. No longer applicable.
2. User-side (whenever convenient, not blocking): fresh-clone walkthrough
   of Slice 000.
3. Not blocking: the local dev database (`crm_dev`) now has a few
   Slice-002-testing rows from review/adversarial/manual verification
   (e.g. "Ada Lovelace", "Grace Hopper", an unresolved "website" entry).
   Harmless local data; `./scripts/dev-services down` + `up` + re-migrate
   + re-seed resets it if a clean slate is wanted before real use.
6. Resolved 2026-08-22: `./scripts/dev-services down` did not drop the
   `crm_postgres_data` named volume, so `down && up` alone was not a
   true clean slate (discovered during the Slice 004 live walkthrough —
   a prior review session's "Cedar Realty" Organization was still
   present). Fixed: `down` now accepts an optional `-v`/`--volumes` flag
   (`compose down -v`); plain `down` still keeps data (needed for the
   restart-only use cases already documented — Centrifugo HMAC-secret
   changes, Docker Desktop clock drift). README's dev-services section
   documents both forms. Used immediately after fixing to get a real
   clean slate (`down -v && up && db-migrate && dev-bootstrap`) before
   the tunnel walkthrough below.
5. Not blocking, dev-only (D-025): the dev tunnel's real routing config
   lives only in the Cloudflare dashboard now, not in the repo — a fresh
   clone or a recreated tunnel would need the three dashboard routes
   (and their order) set up by hand per the README, since `config.yml`'s
   `ingress:` section is documentation only and is not applied. Worth
   revisiting if this needs to survive an account change or if Cloudflare
   ever offers a path back to file-managed mode.
4. Resolved 2026-08-21 (user: "I always go through the tunnel"): `.env` is
   committed to tunnel mode — `CRM_CORS_ALLOWED_ORIGIN=https://app.tarams.org`
   and `CRM_SESSION_COOKIE_SECURE=true`. `CRM_SESSION_COOKIE_DOMAIN` stays
   **unset**, deviating from what README step 5 used to prescribe: only
   `api.*` is ever called with credentials, so a host-only cookie is
   sufficient, and adding a `Domain` cookie while a host-only one already
   exists sends two same-named cookies (older, host-only one first), which
   the server reads in preference while logout clears only the newer — a
   401 loop with no way out but clearing cookies by hand. README's variable
   table and step 5 were corrected to say this, and step 5 now states that
   `CRM_CORS_ALLOWED_ORIGIN` is not optional and names the symptom of
   forgetting it. Loopback dev still works with these values (Chromium
   accepts `Secure` cookies on `http://127.0.0.1`; Safari would not).

## Blocking decisions

None for Slice 000, 001, or 002, or their merges. O-006 (outbound
messaging consent) blocks the SMS slice; O-002 (recording consent)
blocks recording features.

## Safe defaults adopted

- Slice 004: `GRANT SELECT ON invitation TO crm_app;`, added by Lane A
  beyond SLICE_004 §2's literal grant block and confirmed by both
  `crm-reviewer` and the coordinator as a mechanical correction of an
  editing omission (every sibling table in the same GRANT line already
  had SELECT), not a policy decision — no narrower grant is possible
  since `token_hash` backs the same `WHERE` clauses that authenticate a
  token, and no `token_hash` value is ever returned to a client.
- The event-sourced aggregates document is classified as research
  (precedence level 6), not accepted architecture, because its scope
  conflicts with the thesis's deferred capabilities and D-007. Recorded as
  D-015 (resolving O-005).
- Toolchain pins refreshed to current stable at implementation time, per
  spec §9 default 6: Rust 1.98.0 (not the estimated 1.97.1), pnpm 11.22.0
  (not 11.18.0).
- TypeScript pinned to 6.0.3, not the newer 7.0.2: TypeScript 7 is a new
  native-compiler major that `vue-tsc` 3.3.10 cannot yet load (`typescript/
  lib/tsc` is not exported under its new package layout). Revisit when
  vue-tsc adds support.
- PostgreSQL 18's official image changed its data-directory convention
  (single mount at `/var/lib/postgresql`, not `.../data`); `compose.yaml`
  mounts accordingly.
- Centrifugo v6's `health` config key is a nested object (`{"enabled":
  true}`), not the boolean used by older versions; `config.json` written
  accordingly.

## Latest verification

2026-08-21, post-merge fixes on `main` (stage marker, `Select` overflow,
session-unavailable state), web-only — no backend file changed, so the
cargo half of `./scripts/check` was not re-run:
- `pnpm lint`, `pnpm typecheck`, `pnpm build` green.
- Live headless-Playwright walkthrough against the running dev stack
  (real API, real Postgres, logged in as the seeded `alice@acme.test`):
  People list badge and the Person detail `Select` both show the flame on
  Hot Prospect only; the stage menu is clipped inside its own panel with
  the last row cut as a scroll affordance (DOM measurement: `<ul>`
  overflows its container by 139 px, `overflow: auto` on the container).
- The permanent-"Loading…" defect was reproduced first (abort `/api/me`,
  which is what a discarded cross-origin response looks like to the
  client) and then re-run against the fix: both People and Person detail
  now render "Could not reach the server…" and a "Try again" button.
- Not covered: no automated web test suite exists (no vitest/Playwright
  runner in `web/`), so all three fixes are pinned by manual verification
  only. Worth a real frontend test setup before the web surface grows.

2026-08-21, Slice 002, on `slice-002-intake` + `slice-002-web`, after
implementation, independent review, adversarial testing, and fixes, all
local (no CI yet):
- Backend `./scripts/check` and `./scripts/check-db` (run twice) both
  green: 109 unit/service-free tests, 46 DB-backed tests (append-only
  via grants and via the TRUNCATE-statement trigger; both concurrency
  races — same-email dedup and duplicate-delivery — plus the new
  advisory-lock-contention race proving an unrelated Organization is
  unaffected; encryption round-trip, tamper, and wrong-key failure
  modes; tenant isolation on every endpoint in both directions;
  `crm_app`/`crm_migrator` grants including the column-level
  `raw_payload` grant and its trigger backstop; seed idempotency run
  three times manually plus in-suite).
- Frontend `pnpm lint`/`typecheck`/`build` green; no test framework
  exists yet for this project (noted, not blocking — a future decision
  if frontend tests are wanted).
- Live walkthrough (both processes together, real Postgres, headless
  browser): login; new lead → Person detail with 4 correctly-ordered
  history facts; stage change and reassignment, each producing exactly
  one new fact; repeat lead by email → no new Person, `kept_existing`
  routing confirmed on screen; unresolved lead (no contact method);
  duplicate delivery; full cross-Organization isolation on logout/relogin.
  Zero console or page errors throughout.

2026-08-20, on `slice-001-identity`, after both reviews' fixes, all local
(no CI yet):
- `./scripts/check` — cargo fmt, clippy (`deny`, `-D warnings`), cargo test
  (25 unit + 15 integration passing, 15 DB-backed tests correctly
  `#[ignore]`d), web lint, typecheck, build — all green with zero
  services running.
- `./scripts/check-db` — all 15 DB-backed tests pass against fresh
  ephemeral databases: full login→me→members→logout→replay lifecycle;
  wrong-password/unknown-user/no-local-credential all return identical
  401; expired session, tampered token, revoked membership all 401;
  re-login rotates the token and revokes the old one; logout is
  idempotent against an already-revoked session; zero-membership login
  is 403 with no session row created; two-Organization isolation in
  both directions; client-supplied Organization ID (query string,
  header, and login-request body) ignored; multi-membership picks the
  earliest deterministically and its members list is provably scoped to
  only that Organization (a second, later-org-only person does not
  leak in); `crm_app`/`crm_migrator` `current_user` checks; `crm_app`
  denied DDL and INSERT/UPDATE on all four read-only tables, has
  exactly its `user_session` grants (SELECT/INSERT/UPDATE, DELETE
  denied); the real `seed` binary run twice via subprocess produces no
  duplicates.
- Manual, against the real dev database through the real Vite proxy:
  login sets a correctly-attributed cookie (`HttpOnly`, `SameSite=Lax`,
  `Path=/`, `Max-Age=604800`); `/api/me` and `/api/organization/members`
  return correctly-scoped data; a second Organization's members are
  invisible; logout returns a matching clearing cookie and the old
  cookie is rejected afterward; wrong-password and unknown-user timing
  are comparable (~350–550ms, both dominated by Argon2id), confirming no
  enumeration oracle; the cross-account revoke exploit found by review
  (finding: presenting a leaked session cookie during an unrelated
  login silently revoked it) is confirmed fixed — a second seeded user
  logging in while presenting the first user's real cookie no longer
  disturbs the first user's session.
- Not yet performed: the fresh-clone walkthroughs and the Cloudflare
  tunnel negative-Access check (both Slices' spec §8/§9) — require the
  user's machine/dashboard, called out as pending work above.

2026-08-24/25, Slice 007c, on `slice-007c-system-routing`, after
implementation, independent review, adversarial testing, and the two
resulting test additions — full detail in Completed work above; summary
here:
- `./scripts/check` green: fmt, clippy `-D warnings`, cargo check,
  crate-boundary fences, 160 crm-app + 58 crm-operator unit tests
  (including new `IntakeActor`/`RoutingStrategy` unit tests), web
  lint/typecheck/256 Vitest/build.
- `./scripts/check-db` green: `cargo sqlx prepare --check --workspace`
  against a live migrated database (no offline/schema drift), the
  `db_intake.rs` regression suite (18 tests, unmodified, criterion 8),
  the two new DB-backed files (`db_intake_settings.rs` 5,
  `db_intake_system_routing.rs` 7 including the post-review cross-org
  isolation test), and `intake_settings.rs` (4, service-free).
- Live walkthrough against the real dev stack (real Postgres, real
  dev-api rebuilt onto this branch after restarting the previously
  orphaned stale-binary process, real headless Chromium against the
  already-running Vite dev server): the full §1 script end-to-end —
  set/clear/re-set the default assignee via the real HTTP API,
  `crm-admin receive-inquiry` for both the `organization_default` and
  `unassigned` outcomes, Today/People/history verified per-assignee via
  real logged-in sessions, the deactivated-before-set 422 and
  deactivated-after-set self-healing-to-unassigned cases both proven
  live — plus a real-browser screenshot of the settings card in both
  its deactivated-warning and unset-warning states, with the dropdown
  interaction's PUT confirmed to actually persist via a follow-up GET.

## Backlog (deferred, not blocking)

- **O-014 (user, 2026-08-23): Email epic** — five products: lead-intake
  via forwarded notification mail (recommended first; no OAuth, no
  O-012), metadata-only correspondence capture (bodies wait on O-012;
  visibility + scope are blocking product decisions), send (O-006),
  transactional, migration reconstruction. Gmail restricted-scope
  CASA assessment is the schedule-driver — start paperwork early.
  User holds test domains. Full note in the decision log.

- **O-013 (user, 2026-08-23): "Delete my data"** — Person erasure on top
  of O-012 crypto-shred: archive/trash tier vs admin-only erasure, what
  skeleton survives, hashed-identifier suppression so re-arriving leads
  don't resurrect, brokerage-as-controller posture, CCPA/GDPR clocks,
  key-store backup separation, third-party disclosure in an erasure
  report. Must be addressed before the first external customer holds
  real consumer data; not immediate. Full note in the decision log.

- **O-008 (user intent, 2026-08-21): AI next-step suggestions** — after
  every communication attempt/completion (call, email, SMS, chat) and
  once a day, run the Person's communication history through an AI and
  suggest next steps. Reminder only; design open; no work before the
  communication slices. Full note in the decision log.

From the Slice 001 reviews — appropriately low priority per both
reviewers' own framing (dev-only script, latent/library-internal, or
requires a self-registration flow that doesn't exist yet):
- `seed.rs`'s `find_or_create_organization` is not race-safe (no unique
  constraint on `organization.name`); two concurrent `dev-seed` runs
  could create duplicate Organizations. Local dev-bootstrap script only,
  recoverable by hand. **Scheduled for Slice 004** (D-021/O-007 §6),
  where `seed.rs` is rewritten onto the application path anyway.
- Cookie-parsing edge cases (duplicate same-name cookies, percent-encoded
  values) are handled correctly today per `axum-extra`/`cookie`'s
  internals, but pinned only by library behavior, not an explicit test —
  latent risk on a future dependency bump.
- No trimming/Unicode-normalization on login email input; will matter
  once a registration flow exists (none does yet — all emails today are
  fixed seed literals). **Scheduled for Slice 004** (O-007 §6), which
  introduces the first user-entered emails via invitations.
- `migrate`/`seed` binaries propagate connection errors via `Debug`,
  which doesn't currently echo a DSN/password in practice but isn't
  provably safe for every error variant; a generic-message wrapper on
  connect failure would close this defensively.
- `logout()` doesn't format-check the cookie before its DB call (unlike
  `AuthContext`); spec-compliant since logout must accept even invalid
  sessions to stay idempotent, just a possible-but-optional round-trip
  saving for obviously-garbage cookies.

From the Slice 000 review — deferred until `/internal/ready` carries
real operational weight:

Raised by the `crm-tester` adversarial pass; reasonable to defer until
`/internal/ready` carries real operational weight, since a proper fix
means either a hand-rolled Postgres wire-protocol mock or real signal
delivery in a spawned subprocess — disproportionate for this scaffolding
slice:
- No automated test exercises `crm_api::run()`/`shutdown_signal()`
  directly (real TCP bind, real SIGTERM) — currently covered only by the
  manual walkthrough.
- No test for a connection that is *acquired* (fully authenticated) and
  then hangs mid-query, as distinct from a hung/refused *connection
  attempt* (which the new test now covers).
- No test for concurrent `/internal/ready` load or a burst-then-recover
  transition (a manual stress check found this clean under current
  dependency versions, but it is unpinned by the test suite).
- `CRM_API_BIND_ADDR` accepts IPv6 loopback (`[::1]`) silently; untested
  and asymmetric with the Vite dev server's IPv4-only assumption.
- Client-supplied `x-request-id` headers are trusted and echoed verbatim
  (tower-http's standard behavior) — undecided whether that trust is
  correct once the tunnel/Access sits in front of this in earnest.
- `DATABASE_URL=""` (empty-but-set) and a wrong-scheme URL (e.g.
  `mysql://...`) are untested edge cases; both currently fail at a
  reasonable point but with no test pinning the behavior.

## Next recommended action

1. **Slice 007 — email lead intake**, as a ladder of small rungs
   (`docs/plans/SLICE_007_LADDER.md`, user rule: each rung fully
   tested + walked through before the next): 007a intake address →
   007b inbound endpoint → 007c system actor + unattended routing →
   007d first pinned format → 007e Unresolved workbench → 007f LLM
   extraction → 007g real DNS/receiving → 007h portal parsers. O-012
   parked (user). 007a spec `docs/specs/SLICE_007a.md` reviewed; safe
   default adopted: intake address/token readable by org admins only.
   Genuine decisions land at their rungs: default lead recipient
   (007c), who reads raw mail (007e), lead mail → Groq (007f), final
   address scheme (007g).
2. Smaller follow-ups: Telnyx SIP password rotation (user action,
   still pending); the pre-existing db_calls timeline-sort flake;
   deterministic history tiebreaker. Noted: 006b confirm executes
   inline (accepted); dev DB needs ./scripts/db-migrate after branch
   switches.
2. Rotate the Telnyx SIP password; update the trunk (user action).

## Approval currently required

**Resume the 007g ops runbook** (user's choice to pause,
2026-08-25): the code is complete and verified, uncommitted on
`slice-007g-real-receiving`. Next action on resume: runbook step 1
(confirm the elysianfeld.com zone in the user's Cloudflare account),
then steps 2–4 (Email Routing subdomain, wrangler deploy + secret,
catch-all → worker), then the .env flip + live walkthrough, then the
Phase 9 commit gate. No other approvals outstanding.

Slice 007f: nothing outstanding — committed, merged, pushed, branch
deleted, all with explicit user approval 2026-08-25.

Slice 007e: nothing outstanding — committed, merged, pushed, branch
deleted, all with explicit user approval 2026-08-25.

Slice 007d: nothing outstanding — committed (`a75b9a8`), merged,
pushed, branch deleted, all with explicit user approval 2026-08-25.

Slice 007c: nothing outstanding — implemented, verified, committed
(`57ef058`, `fe0b99b`), fast-forward merged to `main`, and pushed to
`origin/main` (`fe0b99b`) with explicit user approval 2026-08-25; the
branch is deleted.

dev-seed-via-api review outcome (crm-reviewer, 2026-08-24): the two
blocking findings (refusal guard failed open against a stale
listening-but-unready API; guard probed CRM_DEMO_API_URL while the
API binds CRM_API_BIND_ADDR) were resolved by redesign per user
direction ("when I ask for a wipe, do a wipe"): dev-bootstrap now
wipes unconditionally and seeds through the already-running dev-api
(liveness-checked via /api/health before the wipe; /internal/ready
reconnect wait after), removing the temporary-API machinery and with
it the remaining temp-API findings. LATER items deliberately left:
`set_local_password` has no test coverage (only caller is `crm-admin
set-password`, still a SLICE_004 §4 contract row); no test executes
the `crm-admin` binary anymore.
