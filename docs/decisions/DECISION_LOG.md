# Decision Log

This file is the highest-authority document in the repository.

Accepted decisions here override every other document. Open decisions listed
here must not be guessed by an implementation or planning agent.

Format: each accepted decision records the decision, date, and source.
Open decisions record what is unresolved and what they block.

---

## Accepted decisions

The following decisions were accepted before the 2026-08-20 repository reset
and are carried forward. Their canonical statements live in `AGENTS.md`;
this log records them as accepted so they are not re-litigated.

### D-001 — Technology stack

Accepted. The fixed technology choices in `AGENTS.md` §3 (Rust, Axum, Tokio,
SQLx, PostgreSQL/CloudNativePG, Vue 3 + TypeScript + Vite, Swift/SwiftUI,
Kotlin/Jetpack Compose, ZITADEL, Centrifugo OSS, application-owned APNs/FCM,
self-hosted LiveKit with Telnyx SIP, OpenTelemetry to OpenObserve, OVH bare
metal with Talos/Kubernetes/Cilium, Cloudflare tunnels, provider-neutral AI
inference with Groq as initial preferred provider) are accepted and must not
be replaced during ordinary implementation work.

Note: secrets are a local `.env` file in development (D-013) and OpenBao in
production (D-014).

### D-002 — Modular application, not microservices

Accepted. Start with a modular Rust application and a small number of
deployable workloads. Split a service only for a real scaling, failure,
security, deployment, or technology boundary. (`AGENTS.md` §4.1)

### D-003 — Authentication and authorization are separate

Accepted. ZITADEL authenticates identities; the Rust application owns business
authorization. A valid token does not authorize access to any Organization
resource. (`AGENTS.md` §4.2)

### D-004 — Organization is the tenant boundary

Accepted. Every tenant-owned query and mutation enforces an Organization
boundary. Cross-Organization access is forbidden and must be tested.
(`AGENTS.md` §4.3)

### D-005 — Initial Person visibility is Organization-wide

Accepted. All active Organization members may view the Organization's People.
Assignment controls responsibility, not visibility. A server-side
`PersonVisibilityScope` abstraction is preserved with only
`Organization(active_organization_id)` implemented. (`AGENTS.md` §4.4)

### D-006 — Person, Inquiry, Deal, and Identity are distinct

Accepted. A Person may have multiple Inquiries; later Inquiries never
overwrite original source attribution; a CRM Person does not automatically
receive an authenticated account. (`AGENTS.md` §4.5)

### D-007 — Hybrid persistence

Accepted. Durable immutable history for facts where historical meaning,
auditability, or reconstruction matters (inquiries, attribution, assignment,
routing, stage changes, consent, calls, number ownership, future Deal
lifecycle). Ordinary relational CRUD elsewhere. Do not event-source
everything. (`AGENTS.md` §4.6)

### D-008 — One typed command layer for all clients

Accepted. Web, native clients, public API, automation, integrations, and the
AI Operator all use the same typed Rust application commands. No second
mutation path for the Operator, and no direct database access for the
Operator. (`AGENTS.md` §4.8, §5.1)

### D-009 — Application-enforced Operator action risk

Accepted. The application, not the model, classifies action risk (read-only /
low-risk reversible / consequential-or-bulk) and enforces confirmation.
(`AGENTS.md` §5.4)

### D-010 — Deterministic, explainable Today ranking

Accepted. Initial Today ranking is deterministic; the model may explain
ranking but never secretly decides priority. (`AGENTS.md` §5.5)

### D-011 — Realtime is delivery, not truth

Accepted. Centrifugo carries realtime events; PostgreSQL and the application
remain authoritative; reconnecting clients must recover correct state.
(`AGENTS.md` §6.1)

### D-012 — Migration fidelity is a product capability

Accepted. Migrations preserve raw source payloads, record source versions,
are rerunnable and idempotent, produce reconciliation results, support delta
import, avoid silent data loss, and report inaccessible source data.
(`AGENTS.md` §8)

### D-013 — Development secrets use a local .env file (2026-08-20)

Accepted. Development secrets are stored in a local, gitignored `.env` file
loaded by the development workflow. No secrets-manager product is integrated
for development. `.env` files must never be committed; a names-only
`.env.example` may be committed.

### D-014 — Production secrets manager is OpenBao (2026-08-20)

Accepted. OpenBao is the production secrets and key authority. Infisical is
not used. Integration happens in a future production-deployment slice; no
secrets-manager integration exists before that slice. Together with D-013,
this resolves former open decision O-001 (the OpenBao-vs-Infisical
documentation conflict).

### D-015 — Event-sourcing scope and PII handling (2026-08-20)

Accepted; resolves O-005. Reconciles the event-sourcing research with D-007
hybrid persistence:

1. The research document's ten transaction/compliance aggregates (Escrow/
   TrustLedger, Offer/PurchaseContract, DisclosurePackage, Property/Listing,
   Licensee/Brokerage, AgencyRelationship, ComplianceCase, CommissionOr
   Referral, FairHousingReview, RetentionPolicy/LegalHold) are deferred.
   They regulate activities the product does not perform. Compliance that
   attaches to activities the product does perform stays in scope at the
   slice where the activity ships (messaging consent, recording consent,
   privacy erasure).
2. Day-one disciplines for all immutable-history tables: standard envelope
   (actor, on-whose-behalf, origin, occurred_at and recorded_at,
   correlation IDs); append-only enforced at the database; corrections are
   fix-forward rows referencing the corrected row, never edits.
3. History rows are PII-free: they carry person/inquiry IDs only. The
   Person table (ordinary CRUD, plaintext, searchable per D-007) is the
   sole correlation of ID to name/contact PII.
4. Where content inherently is PII (raw lead webhook payloads, migration
   exports, recordings, future transcripts), it is stored as an encrypted,
   deletable blob; the immutable record keeps pointer plus content hash.
   Raw payloads live in a dedicated encrypted `raw_payload` table in
   PostgreSQL (large media moves to object storage when recordings ship).
   A payload is stored before parsing, then correlated to its Person/
   Inquiry; payloads that fail to resolve stay in a visible unresolved
   queue — never silently unlinked, since unlinked PII cannot be erased.
5. Erasure: delete the Person correlation row, delete/shred the person's
   payload and content blobs, and write a redaction event so read models,
   search indexes, and rebuilds never resurrect the data. History remains
   intact with orphaned IDs. Deletable-by-design raw payloads are the
   accepted reconciliation of D-012 raw-source preservation with privacy
   erasure; the deletion is itself recorded, so there is no silent loss.
6. No per-person field encryption on the Person table. This is reversible:
   CRUD columns can be migrated to encrypted-with-blind-index later if
   compliance demands it.
7. A written erasure runbook (CRUD rows, blobs, read models, search
   indexes, logs, backup expiry) is required before the first real
   design-partner data enters the system.
8. First history-bearing slice implements exactly four typed fact tables —
   InquiryReceived, RoutingDecision, AssignmentChanged, StageChanged — not
   a generic event store or replay framework.

### D-016 — Development environment (2026-08-20)

Accepted. All development happens on the developer's MacBook Pro (M1 Max,
64 GB, macOS/Apple Silicon), replacing the previous shared-Linux-server/
Caddy model:

1. The Rust/Axum API and the Vite/Vue dev server run as local services.
2. PostgreSQL and Centrifugo run as local Docker containers.
3. Development authentication is locally stored username/password — no
   ZITADEL in dev. It must sit behind the same session/identity
   abstraction that ZITADEL fills in production; no second auth or
   mutation path may be baked in.
4. External connectivity uses a Cloudflare tunnel on `tarams.org` with
   real certificates. Cloudflare Access (long-lived sessions) protects the
   dev hostname. Future webhook endpoints get a separate hostname that
   bypasses Access and is verified by the application (signatures/tokens).
   Access is dev scaffolding for the customer-facing surface; keeping it
   for internal/admin surfaces in production is a later decision.
5. LiveKit SFU and Egress run on separate boxes with public IPs and full
   UDP access when the calling slice arrives; media never routes through
   the tunnel (consistent with AGENTS.md §7).
6. APNs and FCM developer accounts exist and are used at the mobile
   slices.
7. `.env.example` carries a names-only inventory of every required key
   (Groq, tunnel token, etc.) per D-013.
8. Production remains the Kubernetes cluster per D-001; no development
   choice here constrains production topology.
9. No CI for now; checks run locally. No OpenObserve for now; development
   uses console logging (OpenTelemetry instrumentation still lands in code
   per AGENTS.md §10 so a collector can be added without rework).
10. A committed names-only `.env.example` is the single inventory of every
    required environment variable.

### D-017 — Web frontend styling and data stack (2026-08-21)

Accepted. The Vue 3 web client (D-001) uses:

1. Tailwind CSS for styling.
2. PrimeVue in unstyled mode (Tailwind pass-through) for standard interactive
   components: forms, modals, overlays, date pickers, autocomplete.
3. TanStack Table (headless) for CRM data grids, rather than PrimeVue's
   built-in DataTable. CRM tables need heavy per-view customization (inline
   edit, bulk actions, saved/custom views) that a headless table supports
   more directly than a fixed component.
4. TanStack Query owns all server state (fetching, caching, invalidation),
   replacing ad hoc `fetch` + `ref` calls.
5. Centrifugo realtime events (D-011) trigger TanStack Query cache
   invalidation/updates and never carry authoritative state themselves.
   Reconnect recovery is a TanStack Query refetch, consistent with D-011 and
   `AGENTS.md` §6.1.

### D-018 — Production ingress: Cloudflare Tunnel to Cilium Gateway (2026-08-21)

Accepted. In the OVH/Talos/Kubernetes/Cilium production cluster (D-001):

1. `cloudflared` runs in-cluster as a Deployment (2+ replicas), not a single
   local process; Cloudflare's edge load-balances and fails over across
   whichever replicas are connected.
2. Credentials are a Kubernetes Secret sourced from OpenBao (D-014), using
   token-based `cloudflared tunnel run --token`, not a mounted credentials
   file (the model the locally-managed dev tunnel uses).
3. cloudflared's tunnel ingress routes every hostname to a single Cilium
   Gateway API entry point (a `Gateway` Service), rather than one ingress
   rule per backend Service. No separate nginx/Traefik ingress controller is
   introduced.
4. Host-based routing (app hostname, api hostname, future webhook hostname)
   is implemented as `HTTPRoute` resources against that `Gateway`, not in
   cloudflared's own config.
5. TLS terminates at Cloudflare's edge; cloudflared-to-Gateway traffic is
   internal and does not require its own public certificate. Origin mTLS is
   a possible later hardening step, not required now.
6. Prerequisite: Cilium's Gateway API support (Gateway API CRDs,
   `gatewayAPI.enabled=true`) must be enabled during cluster bootstrap.
7. LiveKit media continues to bypass this path entirely (D-016 item 5,
   `AGENTS.md` §7); it is unaffected by this decision.

### D-019 — Person stages are a per-Organization list, not a fixed enum (2026-08-21)

Accepted. Person stages are rows in an Organization-scoped `stage` table,
not a hardcoded Rust enum. `StageChanged` facts (D-015 §8) reference a
`stage_id`; the application validates that the stage belongs to the actor's
Organization (D-004).

Rationale: Follow Up Boss — the reference product and the initial
customer's current system — lets teams rename, reorder, and add stages, so a
fixed vocabulary would lose migrated data (D-012) and force a rewrite of
immutable history (D-015 §2) the first time a custom stage appears.

Each Organization is seeded with Follow Up Boss's nine defaults, in order:
Lead, Hot Prospect, Nurture, Active Client, Pending, Closed, Past Client,
Sphere, Trash. No stage-administration UI ships with the first slice;
editing stages is a later broker-administration feature.

### D-020 — Hot Prospect carries a stage marker; no other stage does (2026-08-21)

Accepted. The Hot Prospect stage renders a 16 px Lucide `Flame` in `danger`
red before its name everywhere a stage name appears: the People table's
stage badge, and the Person detail stage `Select`'s value and its options.
One component owns it (`web/src/components/StageLabel.vue`) so the three
surfaces cannot drift apart.

This is a deliberate single exception to `docs/design/UI_STYLE.md` §5
("Monochrome … never a filled or multicolor icon") and §9 ("no per-stage
colors"), and both sections were amended to record it. No other stage gets
an icon or a color; a second stage marker is a new decision, not a
precedent set by this one.

Because stages are per-Organization rows with no semantic key (D-019), the
marker matches the seeded stage *name* — `"Hot Prospect"`, compared
case- and whitespace-insensitively. An Organization that renames the stage
simply stops seeing the flame. Making the marker survive a rename needs a
column on `stage` and an API contract change (`AGENTS.md` §11); that is
explicitly not part of this decision.

### D-021 — Slice 004 is administration: platform admin, invitations, no direct database writes (2026-08-21)

Accepted. After Slice 003 (Today + realtime), Slice 004 is the first
administration slice. It ships:

1. A **platform administrator** surface for the operator of the product
   (initially the developer alone) that creates Organizations and invites
   each Organization's first admin.
2. An **Organization-admin** surface that invites agents into that
   Organization. Both invitations use one mechanism; only the
   authorization check differs.
3. The first Organization **roles** on membership, so "admin" is a fact
   the application can enforce (D-003).

Operator retrieval moves to Slice 005 and calling to Slice 006 (product
thesis §16).

Principle, applying from Slice 004 onward: **no data is created or changed
by writing to the database directly.** Organizations, users, memberships,
and invitations are created only through the application's domain
functions, exposed via the API and, for local bootstrap, via a CLI that
calls the same functions. `seed.rs`'s direct `INSERT`s are replaced by
calls to those functions. Migrations are the only code that writes to the
database outside the application path. Rationale: direct writes bypass
validation, authorization, auditing, and default-stage seeding, and every
slice so far has had to note hand-entered rows as a caveat.

The platform administrator is a **separate actor**, not a member of every
Organization: the platform surface may create Organizations and issue
invitations but may not read tenant CRM data (People, Inquiries, facts).
Support or impersonation access into a tenant is a later decision with
its own consent and audit requirements, not a consequence of this one.

Administrative actions (Organization created, invitation issued,
invitation accepted, role granted) are recorded as typed fact rows in the
same style as D-015 §8, per product thesis §12.8.

The remaining design choices are listed as safe defaults in O-007 and are
confirmed or changed when Slice 004 is planned.

### D-022 — A contact attempt is a recorded fact and the unit of response for Today (2026-08-21)

Accepted (user choice among three options during Slice 003 planning).
Slice 003 adds one typed fact table, `contact_attempted` — the fifth,
in the same envelope and append-only discipline as the four D-015 §8
prescribed for the *first* history-bearing slice — written by a typed
command `LogContactAttempt` (channel: call / text / email / other;
outcome: reached / no answer / left message / sent). It is PII-free and
carries no free text.

Today (D-010) uses it as the unit of response: a Person assigned to the
viewer is on Today while their latest Inquiry has no contact attempt at
or after it. A contact attempt by **any** member of the Organization
counts as the response; a later Inquiry puts the Person back. Stage does
not remove a Person from Today (D-019 stages have no semantic key and
D-020 forbids a second name-based hinge), and there is no done / snooze /
dismiss — the product thesis §8 defers those semantics and this decision
does not pre-decide them. Until they are specified, a junk lead leaves
Today only by logging a contact attempt, which is itself recorded.

Rationale: the alternatives were a pure read model with no exit other
than stage change, reassignment, or time decay (which conflates "I
contacted them" with "their stage changed" and silently drops work), and
an explicit per-item done/snooze/dismiss state (which pre-decides the
deferred semantics and creates a second truth the Operator would have to
explain). A contact attempt is a real-world event with historical
meaning — first-response time is the broker's metric — and is exactly
what the calling slice (Slice 006, D-021) will record automatically.

### D-023 — Realtime model: Organization channel, server-side subscriptions, ids-only events (2026-08-21)

Accepted with the Slice 003 specification (`docs/specs/SLICE_003.md` §6,
§7, §9, §11). Applies D-011 and D-017 §5 concretely:

1. One Centrifugo channel per Organization (`org:<organization_id>`).
   The application mints short-lived HS256 connection tokens whose
   `channels` claim subscribes the connection server-side to exactly the
   session's active Organization; clients never choose channels and the
   namespace denies client-initiated subscriptions. Token refresh goes
   through the application, which re-verifies membership (401 ends the
   connection).
2. Events are ids-only invalidation hints (`person.changed`,
   `intake.unresolved_changed`) — never state, never PII. Clients respond
   by re-fetching authoritative data over the normal authenticated API.
3. Publishing is best-effort after commit, off the request path; a failed
   publish is logged, never a failed command. Recovery is by refetch:
   reconnect invalidates everything, plus interval and focus refetches.
   No transactional outbox, no Centrifugo history, no per-user channels
   until an event needs semantics beyond invalidation or multi-pod
   deployment arrives.
4. Development transport: the WebSocket is path-routed under the API
   hostname (`api.tarams.org/connection/websocket`) in the committed
   cloudflared ingress, behind Cloudflare Access; production expresses
   the same as an `HTTPRoute` path match (D-018). Routing realtime to a
   hostname that bypasses Access is a security-boundary change and is
   not adopted without an explicit decision.

---

## Open decisions

### O-001 — RESOLVED

Resolved by D-013 (development: local `.env`) and D-014 (production:
OpenBao).

### O-005 — RESOLVED

Resolved by D-015.

### O-006 — Outbound messaging consent policy (OPEN)

O-002 covers call-recording consent only. Consent policy for outbound SMS/
email (TCPA, Do-Not-Call scrubbing, quiet hours, opt-out handling) is
covered by no decision. Blocks: the SMS/outbound-messaging slice.

### O-002 — Call recording consent policy (OPEN)

Recording and consent rules are explicitly open (`AGENTS.md` §7). Blocks:
recording features in the calling slice.

### O-003 — Autonomous AI calling (OPEN)

Whether and how the Operator may place calls autonomously is open
(`AGENTS.md` §7). Blocks: autonomous outbound communication features.

### O-004 — Conversation and data ownership on agent departure (OPEN)

Ownership of communication history and client data when an agent leaves the
Organization is open (`AGENTS.md` §7; research doc "open questions"). Blocks:
reassignment/departure workflows beyond simple reassignment.

### O-005 — Role of the event-sourced compliance model (OPEN)

`docs/research/event-sourced-crm-aggregates-and-events.md` proposes a broad
event-sourced aggregate model (Party, ConsentRecord, Licensee, Offer,
Escrow, DisclosurePackage, ComplianceCase, and others). Much of that scope
(transaction management, escrow, listings, contracts) is deliberately
deferred by the product thesis, and D-007 rejects event-sourcing everything.
The document is research (precedence level 6), not accepted architecture.
Open question: which, if any, of its aggregate boundaries and envelope
practices are adopted for the initial product's immutable-history areas.
Blocks: nothing immediately; informs the architecture baseline for
history-bearing facts.

### O-007 — Slice 004 administration design defaults (OPEN — confirm at planning)

D-021 fixes scope and principle. These defaults were proposed on
2026-08-21 and are adopted unless the Slice 004 plan changes them:

1. **Platform admin model**: a `platform_admin` allowlist table keyed by
   `app_user.id`, a separate `/platform/...` route group with its own
   auth extractor, and handlers that take the Organization id explicitly —
   never from the session's `active_organization_id`.
2. **Initial role set**: `admin` and `member` on `organization_membership`.
   Broker/team-leader/office nuance is deferred.
3. **Invitation record**: `(organization_id, email, role, token_hash,
   expires_at, invited_by, accepted_at)`; token random, hashed at rest,
   single-use, expiring; acceptance requires the authenticated email to
   match; responses never disclose whether an email already exists.
4. **IdP-agnostic acceptance**: acceptance is "authenticate however the
   current identity provider does, then claim the token". In dev the
   authenticate step sets a local password; with ZITADEL it becomes a
   login/registration round-trip. Only that step changes (D-016 §3).
5. **No email delivery**: the admin UI shows the invitation link for the
   inviter to send by hand. A transactional email provider is a later
   increment.
6. **Grants**: `crm_app` gains `INSERT` on `organization`, `app_user`,
   `organization_membership`, `invitation`, and `stage` (default stages
   are seeded through the application path, no longer only by the
   migrator). The unique index on `organization.name` and email
   trim/normalization (PROJECT_STATE backlog) land here.
7. **Surface**: same API binary and same web app, gated area; in
   production Cloudflare Access may additionally front it (D-016 §4),
   which is not decided here.

Blocks: nothing before Slice 004 planning. Questions for that plan: whether
an Organization admin may revoke memberships in 004 or only invite; whether
the platform surface lists/suspends Organizations or only creates them.

### O-008 — AI next-step suggestions after each communication and daily (OPEN — intent recorded, design open)

Product intent recorded 2026-08-21 (user): after every communication
with a Person is attempted or completed — call, email, SMS, chat — the
system automatically runs that Person's communication history through an
AI and asks it to suggest next steps for the agent. The same pass also
runs on a schedule, once a day, so People nobody has contacted still get
fresh suggestions.

Constraints already fixed by accepted decisions, so the design cannot
drift from them: the AI *suggests*; Today's ranking stays deterministic
and explainable (D-010) and the model may explain but never secretly
decide priority; any proposed action executes only through the typed
command layer with application-enforced risk classification and
confirmation (D-008, D-009); message bodies, transcripts, and emails are
untrusted content and must never be interpreted as instructions
(`AGENTS.md` §5.3); only the minimum necessary history is sent (§5.3);
inference goes through the provider-neutral abstraction (D-001, Groq
initially). "Autonomous AI nurture" remains deferred (thesis §11) — this
is suggestion, not autonomous outreach.

Open: trigger mechanism (after the communication fact is committed; the
daily pass as a scheduled job), where suggestions are stored and how
they appear (Today reasons? Person detail? Operator?), cost/latency
budget, and what "communication history" includes once calls, SMS, email,
and chat exist. No work before the communication slices (D-021
sequencing: 006 calling onward). Blocks: nothing.
