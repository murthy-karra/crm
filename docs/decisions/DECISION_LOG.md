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
   *(Status 2026-08-23: done — Slice 006a, `docs/specs/SLICE_006a.md`.
   One step narrower than "non-Axum auth": the session layer stayed in
   crm-api; only the token format check and `AuthContext` moved.)*
   *(Status 2026-08-23, D-034: the `crm-operator -> crm-app` edge is
   deferred again — 006b ships `start_call` through the ToolBackend
   seam with no new Cargo edge; the edge question returns at the
   second mutation tool.)*

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

### D-032 — Agents may correct a call's outcome after the call (2026-08-22)

Accepted (user, after the Slice 006 live walkthrough). Amends D-031's
"Rejected: prompting the agent for an outcome". Observed: the system
cannot tell a person answering from voicemail, a full mailbox, or a
carrier message (a first live call was "answered" in 2.8 s without the
phone ringing). Decision: the automatic attempt written by D-031 stays
exactly as it is (Today never waits on the agent); in addition, when a
call ends the panel offers an optional outcome — talked to them /
voicemail / no answer / busy / wrong number — pre-selected from what the
system observed, with optional notes. A chosen outcome is recorded as a
*correction* fact (`corrects_id` on the existing envelope; history stays
append-only; the original auto-logged row remains visible as superseded).
Skipping the prompt changes nothing. Notes: deferred (user, 2026-08-22,
option 1) — facts stay free of free text (D-015 §3, SLICE_003 §2); a
Note-on-a-Person feature is its own later slice, so 006c ships the
outcome picker only. Today may treat "voicemail" as an
attempt made but resurface the Person sooner than "talked to them" —
the exact rule is for the slice spec. Not taken: answering-machine
detection at the carrier (unreliable, and a Telnyx feature the app would
have to hold credentials for). Scheduled as Slice 006c (after 006a
`crm-app` extraction; before or alongside 006b Operator calling).

### D-033 — The agent must choose every call's outcome; until then the call is incomplete and the Person stays on Today in a lowest "outcome needed" tier (2026-08-23)

Accepted (user, after the Slice 006c live walkthrough). Amends D-031
and D-032. The system is unreliable at telling a person from voicemail,
busy, or a carrier message, so its observation is never presented as
the outcome. Rules:

1. After every call in which the callee was reached or rang (i.e. an
   automatic attempt exists), the post-call prompt requires a choice —
   talked to them / voicemail / no answer / busy / wrong number — with
   **no default and no Skip**. (Calls where nothing reached the callee
   need no choice.)
2. Until the agent chooses, the call is **incomplete**: the timeline
   shows "Call — <duration> · outcome needed" with a **Set outcome**
   action. The automatic attempt row stays as the system's evidence
   ("answered" / "no answer") and still advances Today at answer time
   (D-031's reason stands: the call happened, Today never waits).
3. The Person additionally stays on the **caller's** Today in a new,
   lowest tier — priority `low`, reason `call_outcome_needed`,
   recommended action `set_outcome` — sorted under every other item,
   until an outcome is chosen. Safe default: it is shown to the caller
   (who owes the answer), not the assignee, when they differ.
4. Storage is unchanged from 006c: the agent's choice is the row on top
   of the automatic row (`corrects_id`); "correction" is no longer the
   user-facing word — it is the outcome. Multiple choices chain; the
   timeline shows the current one.

Rejected: writing nothing until the agent chooses (a closed tab would
lose the attempt and Today would wait on paperwork); pre-selecting the
system's guess (it is wrong exactly in the cases that matter).

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

### O-012 — PII content blobs: per-Person keys and crypto-shred (OPEN)

Recorded 2026-08-23 (user, while reviewing the Slice 006c timeline).
Call summaries, transcripts, recordings, and free-text notes will land
on the Person timeline. D-015 §3/§4 already settle *where* such content
lives: never in a history row (IDs and tags only), always in a
separate encrypted, deletable blob; the fact row keeps a pointer and a
content hash, and after erasure it renders as "erased on <date>". What
is not yet decided:

1. Key hierarchy. Slice 002's raw-payload scheme is one master key in
   `.env` with no per-payload keys (accepted then as a simplification)
   — it cannot shred one Person: deleted blobs survive in backups.
   Proposed: a per-Person key (wrapped by the master key; OpenBao in
   production per D-014), and per-blob data keys wrapped by the
   Person's key, so erasing a Person = deleting one key (every copy,
   including backups, becomes unreadable; history rows untouched) and
   a single recording can be shredded alone. This is the
   ARCHITECTURE_BASELINE "Telephony" direction generalised to all PII
   content. Raw payloads must migrate onto it so there is one mechanism.
2. Where summaries come from. An LLM-written summary means the
   transcript leaves the system to the model provider — a
   data-processing boundary interacting with O-002 (recording consent).
3. Operator discipline: a decrypted summary may be read at request time
   to answer a question, but the Operator ledger (D-029) and logs never
   hold the text.
4. Timeline rendering: pointer rows fetch/decrypt on demand through a
   dedicated endpoint; never embedded in `history[]` JSON.

Blocks: call summaries/transcripts, recordings (with O-002), free-text
notes (deferred from D-032). Must be resolved, and the raw-payload
migration done, before any of those slices is planned.


### O-013 — "Delete my data": Person erasure, suppression, and who fields the request (OPEN — must be addressed, not immediately)

Recorded 2026-08-23 (user, after comparing with Follow Up Boss). FUB's
model, from its help center and privacy notice: any member can move a
person to a *Trash* stage (hidden, fully recoverable); only the owner or
admin can *Delete*, which FUB documents as unrestorable — yet its
backups are retained 35 days continuously plus daily copies for one
year at a second provider, so a deleted person stays readable in
backups for up to a year. There is no per-artifact erasure (whole
record only), no suppression list (the same lead re-arriving from a
source is simply recreated), and no consumer-facing flow: leads are
redirected to Zillow's privacy notice; in practice the brokerage
(controller) performs the delete. CCPA requests go to
privacy@followupboss.com with identity verification and no stated SLA;
GDPR is not mentioned.

We can do better than FUB, and the shape is already fixed by D-015
(append-only facts, IDs and tags only in history rows) and O-012
(per-Person keys, crypto-shred). O-012 supplies the mechanism; this item
is the product and policy layer on top of it. Open questions:

1. **What survives a shred.** Proposed: the non-PII skeleton stays
   (that events happened, counts, stages, outcomes, assignments) for
   reporting and audit; identifiers (name, phones, emails, addresses)
   and all content blobs become unreadable; the Person renders as
   "erased on <date>". Whether identifiers live under the Person key
   (so the skeleton survives) or are physically removed is for the
   slice spec.
2. **Soft-remove vs. erase.** Match FUB's two tiers: a recoverable
   "archived/trashed" state any member can apply (a stage or a flag —
   interacts with O-007 stages), and an admin-only, confirmed,
   irreversible erasure. Erasure is a fact on the ledger (who, when,
   reason: consumer request / policy / duplicate).
3. **Suppression.** After erasure keep a hashed-identifier suppression
   record so a re-imported or re-arriving lead (migration delta, Zillow
   /Realtor webhook) does not silently reappear with history; the
   Unresolved queue should surface it as "previously erased". FUB has
   nothing here.
4. **Who fields the request.** Default: the brokerage is the controller
   and erases from the product; we (processor) act only on account
   offboarding or a verified request the customer cannot handle.
   Document this in the customer-facing privacy posture; no
   consumer-facing self-service flow in the first cut.
5. **Statutory clocks.** CCPA/CPRA 45 days, GDPR 30 days: an erasure
   request needs a recorded received-at and a due-by somewhere an admin
   can see it (could be a Today item for the admin).
6. **Backups.** Crypto-shred answers the backup problem only if backups
   never hold the Person keys in the clear — the key store (OpenBao,
   D-014) must be backed up separately from the database, with its own
   deletion propagation.
7. **Third parties.** What we have sent outward (LLM provider for
   summaries — O-012 §2, telephony provider call records, email/calendar
   syncs) is outside the shred; record what each receives and its
   retention so the erasure report is honest, as the migration report
   is (PRODUCT_THESIS §4.1 "never silently discard").

Not blocking any current slice. Sequence: O-012 resolved and raw-payload
migration done → this item specified → a slice that ships archive,
admin erasure with suppression, and the erasure fact. Do before the
first external customer holds real consumer data.

### D-034 — 006b adds no `crm-operator -> crm-app` edge; mutation tools go through the ToolBackend seam (2026-08-23)

Accepted (user, during Slice 006b planning). SLICE_006a §3 predicted
the `crm-operator -> crm-app` Cargo edge would arrive in 006b. Planning
showed 006b does not need it: the `start_call` tool only *proposes*
(one new `ToolBackend` method implemented by crm-api's adapter over
existing queries), and the confirm endpoint is a plain crm-api route
calling `crm_app::domain::commands::start_call` directly. Keeping
crm-operator unable to name `AuthContext`, `CommandContext`, or any
domain type preserves D-028 §1's compile-time fence: fabricating an
actor/Organization stays a compile error, not a discipline. The
`operator_deps.rs` fence gains "crm-operator must not depend on
crm-app" to freeze this; the edge question genuinely returns when a
second mutation tool wants shared app types. Amends SLICE_006a §3's
one-line prediction and the D-028 §5 status note.

Blocks: nothing. Feeds the Slice 006b specification.

### O-014 — Email: intake first, then capture; mailbox access models (OPEN — expected to be a major epic)

Recorded 2026-08-23 (user, in discussion after 006b). "Email" is five
products and must not be planned as one slice:

1. **Lead intake via email** — Zillow/Realtor notification mail parsed
   into `receive_inquiry` (raw email preserved as the raw payload).
   Needs NO agent-mailbox access: one inbound address per Organization
   on a domain we control, sources set up a forward once. Reuses
   intake/dedupe/routing/Today wholesale. **Recommended first email
   slice.**
2. **Correspondence capture** — agent ↔ Person mail on the timeline.
   Metadata-only (direction, timestamps, thread id, matched Person) is
   compatible with D-015/D-029 today and already carries the operating-
   loop value ("they replied" → Today). Message bodies (and arguably
   subjects) are PII content blobs → **blocked on O-012** (parked).
   Capture design must not preclude the later O-012 body upgrade or
   migration reconstruction (thesis §4.1).
3. **Send from the CRM** — after capture; outbound policy is O-006.
4. **Transactional email** (invitations) — separate, small (D-021
   deferred it).
5. **Migration reconstruction** from Gmail/M365 — with the migration
   epic.

Access models for #2 (recorded so the slice starts from facts):

- **Google (Workspace + consumer Gmail)**: Gmail API. Per-agent OAuth
  (refresh token per mailbox) covers both; Workspace-only alternative
  is domain-wide delegation (admin grants a service account, bulk, big
  trust). `watch` + Pub/Sub push, `history.list` incremental. **The
  schedule-driver: Gmail read scopes (`gmail.readonly`, even
  `gmail.metadata`) are "restricted" — OAuth app verification plus an
  annual third-party security assessment (CASA). Real cost/lead time;
  start the paperwork before the slice. Re-verify current requirements
  at planning time.**
- **Microsoft 365 + Outlook.com**: Microsoft Graph, delegated
  `Mail.Read` per agent (tenant may require one-time admin consent);
  bulk alternative is application permissions scoped by an application
  access policy. Change notifications + delta queries. Publisher
  verification only — much lighter than Google's.
- **Long tail**: IMAP (Yahoo/AOL, iCloud app passwords, hosting-bundled
  mail) — do not build early. **Universal fallback: BCC/forwarding
  capture** to a per-org/per-agent address on our domain — works with
  every provider, no OAuth, no verification programs; relies on agent
  discipline for correspondence but is fully reliable for forwarded
  lead notifications (#1 uses exactly this).
- **Aggregators (Nylas, Aurinko, Unipile…)**: one API over all three,
  they carry the verification burden — but per-mailbox pricing and a
  third party processing all client mail, against the D-015/O-012/O-013
  posture. Default: direct integrations.

Defaults proposed (to confirm at slice planning): per-agent OAuth (not
admin bulk-connect) for v1 capture; Gmail vs Graph first is the user's
stack call; visibility of captured mail (assigned agent vs broker
continuity) is a BLOCKING product decision for #2, as is capture scope
(matched-People threads only — never whole-mailbox — proposed).

Test infrastructure (2026-08-23): **elysianfeld.com is the CRM's own
domain** — the per-Organization inbound-intake addresses live there.
Leading candidate (user, 2026-08-23): a **per-brokerage subdomain**,
`leads@<org-slug>.elysianfeld.com`, served by one wildcard MX record so
org creation never touches DNS. Caveats recorded for slice planning:
the receiving path must support wildcard subdomains (own SMTP receiver
does trivially; hosted inbound services vary — this couples the two
choices); fold an unguessable token into the local part
(`leads-x7f3@<slug>...`) since the bare form is guessable and intake
addresses accept mail from anyone (forged mail must land in Unresolved,
never silently create); the subdomain is a stable org slug minted at
creation, not the display name (renames must not break forwards). The
alternative (single subdomain, org in the local part,
`acme-x7f3@leads.elysianfeld.com`) works with any receiving provider. The user also holds test
domains — **eospia.com**, **choravia.com**, **cypressbayrealty.com**
(more available) — for the *other* side of the flow:
cypressbayrealty.com as the fictional brokerage brand (its "agents" and
forwarding rules; matches the existing Cypress Bay Entra tenant from
the parked federation work, a candidate M365 test tenant for capture
later); eospia.com/choravia.com as fake lead-source senders (a pretend
portal) or additional test brokerages for multi-org isolation tests.

Accounts needed, by slice: **#1 intake needs no Google or 365 accounts**
— only DNS/MX control on one test domain plus an inbound receiving path
(own SMTP receiver vs an inbound-email service vs registrar/CDN email
routing — an IMPLEMENTATION choice at slice planning); lead-source
fixtures can be sent from any mailbox or synthesized. **#2 capture**
later needs: one Google Workspace tenant on a test domain (paid,
per-user) for the Workspace path, one or two consumer @gmail.com
accounts (free) for the consumer path, and an M365 tenant with Exchange
(check whether the Cypress Bay tenant has mailboxes; otherwise a
Microsoft 365 developer/test tenant).

Extraction approach for #1 (user suggestion 2026-08-23): **hybrid,
leaning LLM**. Deterministic template parsers only for the few pinned
high-volume formats (portal notifications are rigid templates); **LLM
extraction as the general path** for everything else — extraction only,
into a strict validated schema (no tools, no side effects; the
application decides), raw email preserved first so extraction is
re-runnable/auditable (D-012), low confidence or validation failure →
Unresolved, never an invented lead. Injection blast radius is "wrong
field values", absorbed by validation + Unresolved. To confirm at slice
planning: sending inbound lead-email content to the inference provider
is a data-processing flow to bless explicitly — not a new boundary
(Operator turns already send Person PII to Groq, SLICE_005) but a new
kind: unsolicited third-party content, automatic, at volume (interacts
with O-012 §2). Intake is async, so provider-down = queue and retry,
never a lost lead.

Fixture strategy for #1 (2026-08-23; the user is not an agent and has
no listings): synthesize per-source fixtures from published format
samples (portal lead emails are machine-generated templates; CRM
vendors document the formats they parse); generate real end-to-end
mail from sources we own — a contact form on cypressbayrealty.com (the
"website" source) and hand-forwarded mail (the generic-forward path);
harden against reality later via design partners forwarding actual
notification mail. Architectural consequence, already supported:
parsers MUST assume unknown formats — unparseable mail lands in the
Unresolved queue with the raw email preserved (D-012, raw-payload
store), never dropped, and each new real format becomes a fixture +
parser case. Optional pre-slice task: a throwaway catch-all mailbox on
a test domain purely to collect sample notifications; never part of
the architecture (production receiving is a mailbox-less stream:
inbound-service webhook or our own SMTP receiver).

Blocks: nothing yet. #1 can be planned independently of every open
item; #2 waits on O-012 for bodies (not for metadata) and on the
visibility decision; #3 waits on O-006.

### D-035 — Unattended intake routes to an admin-set Organization default assignee (2026-08-24)

Accepted (user, during Slice 007c planning). Resolves the ladder's
cross-rung decision 6 (`docs/plans/SLICE_007_LADDER.md`).

When intake runs with no human actor (the 007c system-actor path;
email intake from 007d on), the Person is assigned to the
Organization's default assignee — an org-admin-set setting
(`intake_default_assignee_user_id`) — and therefore lands on that
member's Today. When the setting is unset, the Person is created
**unassigned**: visible in the People list, on nobody's Today, and the
intake settings page shows a warning that unattended leads are going
unassigned. Round-robin and rules-based routing remain later work,
explicitly outside the 007 ladder.

Blocks: nothing. Feeds the Slice 007c specification.

### D-036 — Forged-mail posture for pinned email formats (2026-08-25)

Confirmed (user, during Slice 007d planning). Records the confirmation
the ladder required at rung d, reconciling O-014's "forged mail must
land in Unresolved, never silently create" with what a pinned format
means.

Mail that (1) reaches a valid intake address — which embeds the
unguessable 8-character token — **and** (2) matches a pinned format's
template **and** (3) claims that format's real sender domain WILL
create a Person and route it per D-035 (default assignee's Today)
without human review. That is the point of a pinned format; requiring
review for format-matching mail would keep every email lead off Today.

Layered defenses, in order: the address token (mail without it never
resolves to an Organization); each pinned format's `matches()` is
restricted to its real sender domain, never content alone; from 007g
on, the provider adapter carries real SPF/DKIM sender-authentication
results to tighten domain claims. Everything failing any gate lands in
Unresolved with the raw mail preserved (D-012). Accepted blast radius
of a successful forgery: one bogus, quickly recognizable lead row —
no data access, no privilege.

Amended 2026-08-25 (user, via SLICE_007g approval): D-036's third
defense layer — "SPF/DKIM from 007g on" — becomes
deferred-with-escalation per SLICE_007g §6: the D-039 receiving path
has no provider payload carrying verdicts; the stored raw bytes are
the carrier IF Cloudflare stamps Authentication-Results on Worker
delivery (a known open issue says it may not). The 007g walkthrough
captures real stored headers; if absent, a decision is raised before
007h relies on SPF/DKIM (fallbacks: DKIM re-verification from stored
bytes, or token + sender-domain matching as the permanent gates).

Blocks: nothing. Feeds the Slice 007d specification.

### D-037 — Raw unresolved content is readable by Organization admins only (2026-08-25)

Accepted (user, during Slice 007e planning). Resolves the ladder's
cross-rung decision 7.

Opening an Unresolved row to read its decrypted raw content (the
actual email or JSON that arrived), and the Try-again and Discard
actions, are restricted to Organization admins — always on demand, per
row, never in the list response, never logged. All active members keep
the metadata-only queue (source, reason, size, received time) they
have today. Rationale: unresolved content is unvetted third-party
material; the narrowest sensible surface ships first, and widening
later is easy while walking back is not. Widening to members (in
either the read or the retry-only form) remains a future decision.

Blocks: nothing. Feeds the Slice 007e specification.

### D-038 — Inbound lead-email content may be sent to Groq for extraction, under a fixed scope (2026-08-25)

Accepted (user, during Slice 007f planning). Resolves the ladder's
cross-rung decision 4 — the last blocking decision in the 007 ladder.

Emails that fail every pinned format may be sent automatically to the
inference provider (Groq) for lead extraction, with this scope fixed:

- text only — no attachments, no raw HTML (the mime wrapper's text
  conversion), total input ≤ 16 KiB (truncated, flagged);
- subject and the sender's **domain** accompany the text — never the
  full sender address, never the recipient/intake address, never the
  Organization's name, and never any agent identifier;
- no tools: extraction is a pure question-in/answer-out call; the
  model's reply is untrusted data, strictly schema-validated, with
  anti-hallucination checks (every extracted contact value must appear,
  normalized, in the input) and a confidence gate before anything is
  created — low confidence or validation failure lands in Unresolved,
  never an invented lead (O-014);
- Groq goes on the subprocessor list for the SOC 2/DPA work.

Context: not a new boundary (Operator turns already send Person PII to
Groq, SLICE_005) but a new kind of flow — unsolicited third-party
content, automatic, at volume, no human per call. Injection blast
radius is wrong field values, absorbed by validation + Unresolved.

Blocks: nothing. Feeds the Slice 007f specification.

### D-039 — Final intake address scheme is local-part; receiving via Cloudflare Email Routing + an Email Worker relay (2026-08-25)

Accepted (user, during Slice 007g planning). Resolves the ladder's
cross-rung decision 1 (deferred from 007a to here by design) and the
007g half of decision 2.

The mandated pre-007g check found no free/incumbent inbound path that
accepts mail for arbitrary `*.elysianfeld.com` subdomains (Cloudflare
Email Routing: up to 30 individually registered subdomains, no
wildcard; SendGrid Inbound Parse: named hosts only, unsigned webhooks;
Mailgun supports wildcard inbound but means a new vendor account). The
user chose **no new vendor**: the scheme flips to the 007a-prepared
local-part form —

    <slug>-<token>@leads.elysianfeld.com

(`CRM_INTAKE_ADDRESS_SCHEME=local_part`; storage was always
scheme-neutral and `parse_recipient` accepts both forms, so any
previously shared subdomain-form address keeps working at the parser —
though mail to it will no longer be routable once MX exists only on
`leads.elysianfeld.com`).

Receiving path: Cloudflare Email Routing enabled on the registered
subdomain `leads.elysianfeld.com` (the elysianfeld.com zone lives in
the user's Cloudflare account), catch-all route → a committed
Cloudflare **Email Worker** that relays the raw RFC 822 bytes to the
frozen `POST /inbound/email` endpoint with the existing
`CRM_INBOUND_EMAIL_SECRET` bearer. Decision 2's "provider signature
verification on the adapter" resolves accordingly for this path: the
sender of the webhook IS our own worker, so the receiving hop's
transport auth is our own deployment bearer — no third-party
signature scheme exists or is needed; no new crm-api route is built.
A provider-signature adapter returns as a design only if a
third-party inbound provider is ever adopted.

Blocks: nothing. Feeds the Slice 007g specification.

### D-040 — Unwrapped forwarded mail may match pinned formats, with typed provenance (2026-08-25)

Accepted (user, during Slice 007h1 planning). D-036 posture extension
for forwarded mail.

When an agent forwards a lead email (e.g. a Gmail
"---------- Forwarded message ---------" inline forward) to their
org's intake address, the intake pipeline unwraps the forwarding
decoration and the inner message MAY satisfy a pinned format's
sender-domain match and deterministically create a Person — same
D-035 routing, no human review — even though the inner
From/Subject/body are quoted text under the forwarder's control, not
authenticated headers.

Rationale: no format consumes SPF/DKIM verdicts yet, so today's
"direct" domain match and a forwarded claim carry equal evidence; the
LLM fallback would create the same lead from the same unauthenticated
text anyway (at Groq cost, D-038); deterministic parsing is strictly
more accurate and keeps PII in-house. Accepted blast radius is
unchanged from D-036 (one bogus, recognizable lead), plus: a forged
forward can stamp the matched format's source label on
`inquiry_received` (immutable, D-006).

Structural requirement: sender trust is TYPED. A direct message
carries `SenderTrust::Direct` (the future home of
Authentication-Results verdicts); an unwrapped view carries
`SenderTrust::ForwardedClaim`, which has no capacity for verdicts —
inner content inheriting outer authentication must be a compile
error. When a later rung tightens a format's Direct arm with SPF/DKIM,
that format's ForwardedClaim arm is a separate explicit decision
(match on content-equality grounds, or reject); tightening never
silently extends to or bypasses the forwarded path.

Amends D-036: inline forwards are permanently outside SPF/DKIM
tightening (only attachment-style message/rfc822 forwards preserve a
re-verifiable inner DKIM signature; that style is a future rung's
strong path). Supersedes SLICE_007d §4's sentence that "forwarded
copies of form mail are not the pinned flow": the direct-path
subject-equality pin stands, but forwarded copies now reach the same
format via the unwrapper, declared per AGENTS §11.

Also records the 007g walkthrough resolution feeding this decision:
Cloudflare Email Routing → Worker delivery DOES stamp
`Authentication-Results` (dkim/spf/dmarc/arc verdicts observed live
2026-08-25), so D-036's deferred third defense layer is available
from the stored raw bytes whenever its first consumer lands.

Blocks: nothing. Feeds the Slice 007h1 specification.

### D-041 — Intake routing modes: three-mode picker with round-robin (2026-08-26)

Accepted (user, during Slice 008 planning). Extends D-035; resolves
the ladder's deferred round-robin item.

The Organization's intake settings carry an explicit
`intake_routing_mode`: **`default_assignee` | `round_robin` |
`unassigned`** (the user chose the three-mode picker over folding
"unassigned" into the default-assignee dropdown; the dropdown's old
"Unassigned" entry moves up to become the mode). Round-robin pool =
**all active members, admins included** (matches the dropdown's
population; per-member opt-outs are rules-engine territory, later).
Fairness = **continue-anchored, never reset**: canonical order is
membership join order (`created_at, user_id`); next = first active
member strictly after the last-assigned member's (retained)
membership position, wrapping; deactivated members are skipped, the
pointer survives its member's deactivation, new members join at the
end of the cycle. The pointer advances only when round-robin actually
assigns. Explicit `assign_to_user_id` still overrides in every mode
and does not consume a rotation turn (D-035 (h) unchanged; strict
rotation, not least-loaded).

Migration mapping (deterministic under the three-mode choice):
existing orgs with a default assignee → `default_assignee`; with
NULL → `unassigned`; new orgs default to `unassigned` (mirrors
today's initial no-default state). `default_assignee` mode requires
a non-null active assignee at PUT time; a later-deactivated assignee
still falls back to the unassigned OUTCOME + warning at routing time
(007c behavior). The `routing_decision` fact vocabulary gains
`round_robin` (declared persistence-contract change); the fact
always records the actual outcome (`unassigned` on an empty pool).

Blocks: nothing. Feeds the Slice 008 specification.
