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
   **Superseded 2026-08-22 by D-024: Access is removed from the dev
   tunnel.** The tunnel and its TLS termination are unaffected.
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

### D-024 — Cloudflare Access removed from the dev tunnel (2026-08-22)

Accepted (user). Amends D-016 §4. The Cloudflare Access application
gating `app.tarams.org`/`api.tarams.org` in development (one
`self_hosted` app, one Allow-by-email policy, created when D-016 was
accepted) is deleted from the Cloudflare account. The tunnel itself —
routing, TLS termination, DNS — is unchanged; only the Cloudflare-level
login wall in front of it is gone.

Rationale: the app already has a real authentication boundary (D-016
§3's session-cookie login, the same abstraction ZITADEL fills in
production). Access was a second, redundant login layer whose
per-hostname session scoping was a recurring source of friction
(documented in the README's former troubleshooting section and in
PROJECT_STATE: logging in at one hostname didn't authenticate the
other, and an expired per-hostname Access session silently broke the
realtime WebSocket, presenting as a stuck "reconnecting" indicator with
no obvious cause).

Consequence, stated plainly: `app.tarams.org` and `api.tarams.org` are
now reachable by anyone on the public internet up to the app's own
login screen. There is no login rate-limiting or lockout today (a
pre-existing gap, not introduced by this decision — Argon2id's
per-attempt cost is the only friction). Acceptable for a dev
environment holding only synthetic seed data; revisit before real
customer data reaches this environment.

Production is unaffected: D-016 §4's original wording — that whether
to front internal/admin surfaces with Access-equivalent protection in
production is a later decision — still stands. This decision is
dev-tunnel-only.

### D-025 — The dev Cloudflare Tunnel is dashboard-managed, not file-managed (2026-08-22)

Discovered and recorded (user + agent, live). D-016 §4 and the README
documented `crm-dev` as a locally-managed tunnel: ingress rules in the
committed `infra/development/cloudflared/config.yml`, chosen specifically
to avoid Cloudflare's dashboard-managed mode (warned to be a one-way,
irreversible migration). In practice this was already false: `cloudflared`
ignores `config.yml`'s `ingress:` section entirely and applies whatever
routes are configured in the Cloudflare Zero Trust dashboard instead —
confirmed by restarting `cloudflared` (no effect) and by its own log
showing a server-pushed "Updated to new configuration" event whose rules
did not match the local file. Consequence: the Slice 003 realtime
WebSocket path rule, though correctly written into `config.yml` at
implementation time, was never actually in effect, and the realtime
connection silently 404'd through the tunnel until diagnosed and fixed
live during this session (Today still worked via the 60 s poll backstop,
D-011 — no data was lost or wrong, only the live-push path was dark).

Fix: three routes added/reordered directly in the dashboard (Tunnels →
`crm-dev` → Routes): `app.tarams.org` → `:5173`; `api.tarams.org` path
`/connection/websocket` → `:8000`; `api.tarams.org` (catch-all) → `:3000`
— the path-specific route ordered above the catch-all, since dashboard
routes are evaluated top-to-bottom, first match wins, identical to the
file's own rule. Verified live: a raw WebSocket upgrade to
`wss://api.tarams.org/connection/websocket` returns `101`, and a full
two-browser cross-session realtime walkthrough passed over the real
tunnel (a new lead appeared on the assignee's Today in under a second;
reassigning it live-moved it to a second, separately logged-in user's
Today).

`config.yml`'s `ingress:` section is retained as documentation of intent
(and its `tunnel:`/`credentials-file:` lines remain genuinely
load-bearing) but is not authoritative; the dashboard is, and must be
kept in sync with it by hand going forward. Whether a path back to true
local-file management exists is unconfirmed and not attempted — Cloudflare
describes the transition as one-way. Dev-only; D-018's production ingress
(in-cluster `cloudflared` → Cilium Gateway API `HTTPRoute`s) does not use
Cloudflare Tunnel dashboard routing and is unaffected.

---

### D-026 — Organization admin continuity: last-admin protection, platform-admin recovery, members unaffected (2026-08-22)

Accepted (user, during Slice 004 pre-planning). Refines D-021.

1. **Members never stop working.** An Organization with no active admin
   stays fully operational for its members: leads, Today, contact
   attempts, realtime, and every non-admin action behave exactly as
   before. "Admin" is an authorization fact about who may change
   membership and settings, not a liveness condition for the
   Organization. The only thing an admin-less Organization cannot do is
   admin-only actions.
2. **Last-admin protection on self-service paths.** An Organization
   admin cannot demote themselves or remove their own membership if they
   are the last active admin; the application rejects the command with
   a clear message (promote someone else first). This is an application
   invariant enforced in the domain layer, not a database constraint.
3. **Platform-admin recovery, both ways, always.** For any Organization
   the platform admin may (a) promote an existing member to admin, and
   (b) invite a new admin who is not yet in the system — the same
   invitation mechanism D-021 ships. Both actions are always available;
   neither is gated on the other (a newly appointed outsider is a normal
   case). The platform-admin view may list members for convenience but
   does not have to.
4. **Scope of platform-admin power stays narrow.** Day-to-day membership
   management inside an Organization is the Organization admin's job;
   the platform admin steps in only to create Organizations and to
   restore admin continuity. The platform admin is not a superuser over
   Organization data (D-003, D-021).
5. **Surfacing, not pushing, in Slice 004.** Admin-less Organizations
   are surfaced as "needs attention" in the platform-admin view. A
   newly created Organization whose first-admin invitation is still
   pending is "pending first admin", not an error; it becomes "needs
   attention" only if that invitation expires. No email/Slack/push
   notification subsystem is built for this in 004; pushed alerts to
   the platform admin are a later slice once a delivery channel exists.

Blocks: nothing. Feeds the Slice 004 plan (membership roles, the
promote/demote commands, invitation expiry, platform-admin listing).

### D-027 — Membership deactivation instead of removal; Organization suspension reserved (2026-08-22)

Accepted (user, during Slice 004 planning). Answers the question D-021
left open ("may an Organization admin revoke memberships in 004").

1. **Removal is not a domain concept.** Memberships, users, and
   Organizations are never deleted by an administrative action.
   "Inactive" is the terminal membership state. Hard deletion, if ever,
   is a future data-retention/compliance policy, not an admin button.
2. **Member deactivation ships in Slice 004.** `organization_membership`
   gains `status ∈ {active, inactive}`. An Organization admin may
   deactivate and reactivate a member. Deactivation blocks login to that
   Organization, invalidates existing sessions on the next request, and
   disconnects the member's realtime connection (the hook SLICE_003 §14a
   reserved). All history, People, and contact attempts are retained and
   remain attributed to the inactive member.
3. **Attribution stays visible.** Because an inactive member's assigned
   People no longer appear on anyone's Today, the Members view shows the
   count of People still assigned to each inactive member, and an admin
   may reassign them with the existing per-Person assign command. Bulk
   reassignment and any richer departure workflow remain with O-004.
4. **Last-admin protection (D-026) counts active admins only.**
   Deactivating or demoting the last active admin on the self-service
   path is rejected. The platform admin's recovery path (promote an
   active member, or invite a new admin) is unchanged.
5. **Organization suspension is reserved, not shipped.** `organization`
   gains `status` with only `active` permitted in 004 so suspension
   later is a domain rule and a UI, not a migration. Its semantics
   (non-payment vs legal hold; whether intake keeps accepting leads;
   what members see) are customer-visible and open — see O-009.

Blocks: nothing. Feeds the Slice 004 specification.

### D-028 — The AI Operator is an in-process crate, not a separate service (2026-08-22)

Accepted (user, during Slice 005 planning). Applying D-002's test to the
Operator: it has no technology boundary (inference providers are HTTP
APIs, trivially called from Rust), no scaling boundary yet (load tracks
user count, the cost is I/O wait), a failure boundary that bounded
concurrency and timeouts inside one process already cover, and a
**security boundary that argues against splitting** — a separate
service would have to reach the typed commands/queries over the network
with a second, service-to-service auth path carrying actor and
Organization, which is exactly the kind of trust boundary D-008 and
`AGENTS.md` §5.2 exist to avoid.

1. The Operator lives in a new workspace crate, `crates/crm-operator`,
   compiled into the same `crm-api` binary. It depends on the
   application layer (typed commands, queries, `Actor`, Organization
   context) and **not** on Axum, the HTTP/session layer, or SQLx
   directly; the compiler enforces "approved typed tools only".
2. The crate owns the provider-neutral inference abstraction (D-001,
   Groq first), the tool registry and tool definitions, the tool-loop
   runner, prompt assets, and Operator turn auditing. `crm-api` exposes
   it through one route group with its own bounded concurrency and
   timeouts so a slow or failing model call cannot starve other
   handlers.
3. The Operator-tool contract is a shared contract (`AGENTS.md` §9):
   changes are explicit, never silent.
4. Revisit criteria for splitting into its own workload: self-hosted
   model inference; a voice/push-to-talk pipeline with its own runtime
   (LiveKit audio, a non-Rust agent framework); or measured Operator
   load starving the API. If any occurs, the crate is wrapped in its
   own binary behind an internal API — the crate boundary is where the
   service boundary would go.
5. **Refinement (2026-08-22, user-accepted).** There is no standalone
   application-layer crate yet: `domain/`, `auth/`, `realtime/` all
   live inside `crm-api`, and `crm-api` must depend on `crm-operator`
   to mount its routes, so `crm-operator` cannot depend on `crm-api`
   (package cycle). For Slice 005 the dependency is **inverted**:
   `crm-operator` defines `OperatorContext` and a `ToolBackend` trait
   naming exactly the approved read tools; `crm-api` implements the
   trait over its existing `domain::` queries and injects it. The only
   Cargo edge is `crm-api -> crm-operator`; the trait is the complete
   data surface reachable by the tool loop. Extracting a `crm-app`
   crate (domain + realtime + non-Axum auth) is deferred and is a
   **prerequisite for the first slice that gives the Operator mutation
   tools**; it ships as its own refactor PR with no behavior change.

Blocks: nothing. Feeds the Slice 005 specification.

### D-029 — Operator turns are audited as a PII-free ledger; transcripts are not stored (2026-08-22)

Accepted (user, during Slice 005 planning). Every Operator turn,
including failed ones, writes an append-only ledger row: Organization,
actor, origin, timing, outcome, provider/model, token counts, and one
row per tool call (tool name, outcome, duration, Person ids touched).
The user's message, the model's reply, tool arguments (including search
strings), and conversation history are **not** persisted and are never
logged. Rationale: transcripts are customer content with retention and
erasure implications (D-015 §3, §5) that the product has not yet
defined; the ledger alone satisfies "Operator actions must be
explainable later" (thesis §13) for a read-only Operator. Revisit when
quality evaluation, O-008 suggestions, or persisted conversations need
the text — at which point transcripts follow the D-015 §4 encrypted,
deletable-blob discipline.

Blocks: nothing. Feeds the Slice 005 specification.

### D-030 — Slice 006 is human-initiated outbound browser calling; Operator calling is 006b (2026-08-22)

Accepted (user, during Slice 006 planning). Slice 006 proves "an agent
places a real phone call from the CRM and the CRM records it": a member
clicks Call on a Person, the browser joins a self-hosted LiveKit room
over WebRTC, the API dials the Person's phone through LiveKit SIP →
Telnyx → PSTN, the call lifecycle is recorded PII-free, and Today /
realtime advance. Sequencing:

- **006** — web calling from the Person page (this slice).
- **006a** — the `crm-app` crate extraction (D-028 §5), a pure refactor
  with no behaviour change.
- **006b** — the first Operator mutation tool, `start_call`, with the
  D-009 preview → confirm → receipt flow, on top of the same `StartCall`
  command.

Out of 006: inbound calls, native mobile, recording/egress/transcription
(O-002 blocks), Telnyx API or webhooks in the application, per-
Organization numbers, SMS (O-006), streaming/voice Operator (declared
deferral of SLICE_005 §16's "arrive with the calling work"). Nothing in
006 touches O-003: every call is started by a person in a session.

Development telephony runs on a **public Linux host** (small VPS or the
OVH box): LiveKit server + SIP service + Redis from a committed compose,
signaling at `wss://livekit.tarams.org` through a `cloudflared` route on
that host, media directly to the host's public IP over UDP (D-016 §5,
AGENTS §7). One US number and a credential SIP connection are purchased
on the user's Telnyx account and configured once as a LiveKit outbound
trunk; the application holds no Telnyx credentials, only the trunk id.
LiveKit Cloud was rejected (D-001 self-hosting); Mac-local LiveKit cannot
receive PSTN media behind NAT.

Safe defaults adopted with this decision: one deployment-level caller
number (per-Organization numbers and number-ownership facts belong with
thesis §5.3); the LiveKit webhook is a path under `api.tarams.org`
(amends D-016 §4's "separate hostname bypassing Access", which D-024
made moot); one active call per user; org-wide read of a call, caller-
only control.

### D-031 — A call becomes the contact attempt at answer/failure time (2026-08-22)

Accepted (user, during Slice 006 planning). Refines D-022 for calls:
the automatic `contact_attempted` is written in the same transaction as
the transition that settles whether the callee was reached, not at call
end — so Today never depends on the end-of-call signal. Answered →
`call, reached` at `answered_at`. Busy / declined / ring-out (and a
cancel after ringing started) → `call, no_answer`. Agent never joined,
provider error, expiry, or a cancel before ringing → no attempt (nothing
reached the callee). Voicemail is indistinguishable from a person
answering and reads `reached`; the agent may log `left_message` with the
existing dialog. The attempt's envelope names the caller as actor and
carries the call's `correlation_id`, with `causation_id = call.id`.
Rejected: prompting the agent for an outcome after each call (makes
"automatic" conditional), and writing only at call end.

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

### O-009 — Organization suspension semantics (OPEN)

D-027 §5 reserves `organization.status`. Open: the distinct cases
(non-payment, legal hold, voluntary pause) and for each whether inbound
intake continues to accept and store leads, what members see on login,
whether the platform admin may act inside the Organization during a
hold, and retention after a terminal suspension. Blocks: any
platform-admin suspend action; nothing in Slice 004.

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

### O-010 — Search: fuzzy matching and a search layer (OPEN — deferred until needs are known)

Recorded 2026-08-22 after the Slice 005 walkthrough: the Operator's
`search_people` is an exact substring match (SLICE_005 §2), so a
misspelt name ("Okafore" for "Okafor") returns nothing. The user chose
to defer fuzzy search until the product's search needs are clearer
(properties, locations, notes, a site-wide search box), rather than
fix the one case now.

Direction recorded for when it is picked up, so the first fix is step
one of a layered design rather than a one-off:

1. **Matching primitive** — `pg_trgm` similarity (typo-tolerant names,
   addresses, identifiers) as a reusable `domain::search` helper that
   each entity's own Organization-scoped query calls. Never a generic
   cross-entity query that bypasses `PersonVisibilityScope` (D-005).
2. **Search projection** — a `search_document` table (organization_id,
   entity_kind, entity_id, title, body) with a trigram index on `title`
   and full-text (`tsvector`) on `body`, written by the same domain
   commands (D-021) and rebuildable from history; powers a global search
   box, the Operator's search tool, and entity pickers. Built when the
   second searchable entity lands.
3. **Semantic search** — `pgvector` embeddings on the same table for
   meaning-based queries (inquiry messages, notes, property
   descriptions), combined with 1–2 as hybrid retrieval. Embedding text
   means sending PII to an embedding provider or running a local model:
   a privacy decision to log explicitly (D-029 spirit), not an
   implementation detail. Not the starting point — it does not solve
   the typo case.

Explicitly rejected: sending the full People roster to the model and
asking it to match (does not scale past demo data, puts the maximum
rather than the minimum untrusted text in the prompt, non-deterministic).

Stopgap available at any time without a contract change: a prompt rule
telling the model to retry a name search with a shorter prefix before
answering "not found". Blocks: nothing.

### O-011 — Outbound calling compliance (OPEN)

O-006 scopes itself to SMS/email. No decision covers outbound *calling*
compliance: Do-Not-Call scrubbing, quiet hours, caller-ID rules, state
consent-to-record interplay with O-002. Slice 006 is human-dialed to
People who inquired, so nothing is built; recorded so it is not
forgotten. Blocks: autonomous or bulk outbound calling (with O-003).
