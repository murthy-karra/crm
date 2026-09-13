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

**Amended 2026-08-29 (user, docs-freshness audit):** `.env.example` may
carry names AND non-credential defaults (e.g. the Groq base URL/model,
intake mail domain, timeout bounds) — blessing the drift the file had
already accumulated. Credentials and machine-specific values stay empty;
the never-commit-a-real-credential rule is unchanged.

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

**Visual policy amended by D-045 (2026-09-06):** muted tints are now
authorized for Lead, Active Client, Nurture and Hot Prospect. Name matching
and neutral treatment of custom stages remain unchanged.

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
   **Superseded in part 2026-08-22 by D-024/D-025:** Access is removed
   from the dev tunnel entirely (D-024), and the tunnel turned out to be
   dashboard-managed — the committed ingress file is documentation, not
   the applied routing (D-025). The path-routing shape itself stands.

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

> **Note:** accepted decisions D-034 through D-044 continue BELOW,
> appended chronologically among the open items in the next section.
> D-033 is not the latest accepted decision.

## Open decisions

### O-001 — RESOLVED

Resolved by D-013 (development: local `.env`) and D-014 (production:
OpenBao).

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

### O-005 — Role of the event-sourced compliance model (RESOLVED by D-015)

*Resolved 2026-08-20; duplicate RESOLVED stub removed 2026-08-29. D-015
answers each question below: the ten aggregates are deferred (§1), the
envelope practices are adopted (§2), and the first history-bearing slice
ships exactly four typed fact tables (§8). The original question is kept
as the record of what was asked.*

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

### O-007 — Slice 004 administration design defaults (RESOLVED — confirmed by Slice 004, D-026, D-027)

*Resolved 2026-08-22 (status recorded 2026-08-29): every default below
shipped in migration `20260823000001_administration.sql` and the Slice
004 implementation; the two closing questions were answered by D-027
(revocation → deactivation) and D-026 §5 + D-027 §5 (platform listing /
suspension reservation).*

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

Blocks: call summaries/transcripts, recordings (with O-002). Must be
resolved, and the raw-payload migration done, before any of those slices
is planned. *Amended by D-053 (2026-09-08):* free-text notes (deferred
from D-032) no longer wait on this decision; Slice 015 ships note bodies
as plaintext in the erasable CRUD set, and note bodies migrate onto the
key hierarchy when this decision lands (D-053 states the constraints that
keep that a column move).


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

*Status 2026-08-29: #1 (lead intake) SHIPPED as the 007 ladder,
007a–007h1 (D-035–D-040); #2 (correspondence capture) v1 SHIPPED as
Slice 009 (D-042, which supersedes the matched-only scope below).
Remaining: #3 send (blocked on O-006), #4 transactional, #5 migration
reconstruction.*

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

**Direction updated 2026-08-27 (user): CC/BCC-first, OAuth
later-opt-in — flips the paragraph above.** V1 capture = agent-
addressed: agents CC (preferred over BCC, so client reply-alls are
also captured) a capture address on our domain for lead/client
threads. Rationale: agent control makes the privacy boundary
structural (nothing enters the system except what an agent
deliberately addressed to it — the whole-mailbox concern dissolves),
it is provider-agnostic (Gmail/M365/anything), needs no OAuth
verification or CASA (the external clock vanishes), and reuses the
proven 007 receiving stack. Accepted limitation, eyes open: BCC
captures outbound only; CC catches inbound only on reply-all — so
the "they replied" Today signal is PARTIAL in v1. Per-agent mailbox
OAuth (metadata-only scopes: gmail.metadata / Mail.ReadBasic)
remains the recorded LATER opt-in upgrade for automatic two-way
capture — same pipeline, second feed, per-agent choice.

Mitigations for the partial-inbound limitation (user, same
discussion):

1. **Reply-all as etiquette**: agents ask clients/leads to always
   reply-all (the visible CC address makes this natural); the product
   can reinforce it (connect instructions, a suggested signature
   snippet) — slice-planning detail.
2. **Retroactive forwarding**: missed correspondence must be
   forwardable AFTER the fact — the agent picks the missed emails,
   forwards them to the capture address, and the system receives,
   processes, and inserts them into the timeline CORRECTLY. Design
   requirements this sets for the capture slice: reuse the 007h1
   forwarded-wrapper unwrapping (`SenderTrust` machinery) to recover
   the ORIGINAL message — the inner From identifies the client and
   the direction (inner From = client → inbound; = agent →
   outbound); timeline placement honors the ORIGINAL correspondence
   timestamp (the unwrapped inner Date), with capture time recorded
   separately — a conscious, declared divergence from intake's
   received_at-is-receipt-time rule (007d §4a); dedup by
   Message-ID/thread ids so a re-forward, or a forward of something
   already CC-captured, never duplicates a timeline entry.

Also recorded from the same discussion: if/when OAuth capture is
built, match-on-headers-then-discard with provider-level body-less
scopes is the standard to hold (transient header processing for
filtering, stored only on a matched-Person hit).

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

### D-042 — Correspondence capture v1: the six scoping decisions (2026-08-27)

Accepted (user, during Slice 009 planning). Executes O-014 #2 under
the CC/BCC-first direction recorded 2026-08-27; the metadata-only
posture (bodies parked on O-012) stands throughout.

1. **Visibility**: captured metadata rows (direction + agent + time)
   are visible org-wide on the Person timeline — consistent with the
   existing org-wide Person history model and broker continuity. The
   unmatched held queue is attributed-agent-only; raw content is
   readable by NOBODY (no endpoint exists in v1).
2. **Subjects are not stored** outside the encrypted raw. Timeline
   and history JSON carry no subject, address, or message-id.
3. **Unmatched correspondents → per-agent held queue**: the
   attributed agent sees the counterparty address, can link-to-Person
   (adding the contact method and writing the timeline row) or
   dismiss. Never auto-creates a Person (match-never-create holds;
   forged mail to a leaked token must not mint People). This narrowly
   and deliberately supersedes O-014's original "matched-People
   threads only" scope — blessed here.
4. **Outbound capture auto-writes `contact_attempted`(email, sent)**
   — System actor on behalf of the agent, caused by the
   correspondence fact, at message time — so a CC'd email clears the
   Today item like a logged call. Accepted edge: a forged "outbound"
   can clear one Today item (visible, recoverable).
5. **Per-agent token capture address** on the existing subdomain:
   `save-<token12>@leads.elysianfeld.com`, 12-char token (the address
   is thread-visible, so higher entropy than intake's 8), grammar
   structurally disjoint from intake addresses by token length;
   attribution is the address itself; per-agent self-service rotation
   included. Zero Cloudflare changes (existing catch-all + relay).
6. **Raw kept, encrypted, in a NEW `correspondence_raw` table** with
   no read surface — structurally separate from `raw_payload` so
   client correspondence can never enter the Groq extraction sweep
   (D-038's scope stays airtight). Preserves the O-012 body upgrade
   and migration reconstruction; O-012 key-hierarchy migration debt
   noted.

Blocks: nothing. Feeds the Slice 009 specification.

### D-043 — Smart lists are first-class and become Today's configurable feed (2026-08-28)

Accepted (user, during Slice 011 planning). A deliberate amendment
to the thesis §8 posture, with the user's rationale recorded:
switchers must be able to reproduce Follow Up Boss's genuinely great
points — smart lists foremost — and "people are notoriously unable
to change"; equally, lists give agents A SENSE OF CONTROL over what
appears on Today, rather than the CRM hardcoding it.

1. **Smart lists are first-class, FUB-shaped**: saved, dynamic,
   filterable People lists. Personal lists are created and edited
   freely by each agent; shared lists are admin/team-curated and
   visible org-wide (agents duplicate rather than edit the shared
   original) — the FUB permission shape (re-verify details at spec
   time).
2. **Lists feed Today**: a list can be marked as a work source; its
   members surface on Today with the list as the explainable reason.
   Today remains the primary "what should I do next" surface — its
   INPUTS become visible and configurable instead of compiled-in.
3. **Today's built-in logic becomes tweakable**: the existing
   deterministic reasons (new inquiry, no contact attempt, client
   replied, call outcome needed) become system-provided defaults
   expressed in the same filter vocabulary — adjustable per org —
   rather than hardcoded magic. Determinism and explainability stay
   (thesis §8's "no secret priority" holds; O-008's AI layer remains
   separate and later).

Consequence: the filter model IS the Today configuration language —
one vocabulary serves ad-hoc People filtering, saved lists, and
Today's feeds. Tags (no model today) are an early dependency
question for FUB-parity filters; building them also re-opens part of
the parked Slice 010 migration ladder.

Blocks: nothing. Feeds the Slice 011 ladder plan (phased, minimum 4
rungs per the user's standing sizing rule).

### D-044 — Elysium CRM identity and Threshold E logo direction (2026-09-03)

Accepted (user, after review of the logo-system concept and vector
direction).

1. **The customer-facing product name is Elysium CRM.** Product chrome,
   browser metadata, install surfaces, and future native clients use that
   name. The existing `elysianfeld.com` intake-mail domain remains
   infrastructure and is not renamed by this decision.
2. **The Threshold E is the approved identity direction.** A continuous
   geometric ribbon forms an abstract `E`; its open edge and negative space
   suggest a doorway without using literal real-estate or AI iconography.
3. **The primary identity is one-color-first.** Deep indigo `#1E1B4B` on
   light surfaces, white reversed on indigo, and black for one-color production.
   The mark has no gradient, glow, bevel, texture, or decorative shadow.
4. **The system includes distinct standard and micro marks.** The optically
   simplified micro mark is used below 32 px. Horizontal, stacked, app-tile,
   favicon, reversed, and monochrome forms share the same core geometry.
5. **SVG is authoritative.** Wordmarks are outlined paths with no runtime font
   dependency; raster and platform exports are derived artifacts. Canonical
   masters and usage rules live under `docs/design/branding/`.

This is a visual/product-chrome decision only. It changes no HTTP, realtime,
Operator-tool, or persistence contract.

Blocks: nothing. Authorizes the web-brand integration and future native asset
exports from the same masters.

### O-015 — Correspondence/payload blob size, storage location, and retention (OPEN)

*Question 1 resolved by D-056 (2026-09-09): the cap is Cloudflare's own
25 MiB inbound ceiling (endpoint body limit 34 MiB), delivered by Slice
017 with a streaming relay. Questions 2 (storage location) and 3
(retention) remain open.*

Recorded 2026-08-29 (user, in discussion after the perf baseline work).
Three coupled questions about the encrypted raw blobs
(`raw_payload.ciphertext`, `correspondence_raw.ciphertext`). Most of
the analysis is settled below so the eventual slice starts from facts;
what remains genuinely open is marked.

**Ground truth today.** There is NO object storage anywhere in the
system: raw MIME — attachments included, since they are part of that
MIME — is stored as an encrypted `BYTEA` in PostgreSQL. Hard caps: the
Email Worker bounces mail over **1.4 MiB** raw
(`infra/email-worker/worker.js`, an explicit "honest bounce beats
silent truncation" choice), `POST /inbound/email` caps HTTP bodies at
2 MiB, generic intake JSON at 256 KiB. D-015 §4 already anticipates the
exit: "large media moves to object storage when recordings ship."

1. **Size cap (nearest-term, product-visible).** 1.4 MiB is small for
   real-estate mail: disclosure packets, signed contracts, and
   inspection reports routinely run 5–20 MB. Under Slice 009 capture, a
   client emailing a signed PDF to an agent's capture address is
   BOUNCED — the thread silently misses the timeline and the client
   sees a bounce from our domain. Raising the worker threshold and the
   endpoint limit (a two-constant change) is available today,
   independent of questions 2 and 3, at a Postgres-bytes cost that is
   negligible at design-partner scale. **OPEN: what cap?**

2. **Storage location.** Follow-up 2026-09-11: D-062 accepts keeping email
   bulk content outside PostgreSQL; backend selection and implementation remain
   future work. The whole-message preservation rationale below still applies.
   Original recommendation, reasoned through: when
   recordings force object storage into existence, move the **whole
   encrypted raw MIME** there with a pointer row in Postgres — NOT
   per-attachment extraction. Whole-message relocation preserves D-012
   raw preservation byte-for-byte, keeps `content_hmac` (the delivery
   idempotency key, `UNIQUE (organization_id, source, content_hmac)`)
   unchanged, removes ALL blob bytes from WAL rather than the
   attachment fraction, and needs no MIME surgery. Nothing in the
   product serves individual attachments today
   (`correspondence_raw` has no read endpoint at all, D-042.6;
   `raw_payload`'s only reader is the admin workbench), so part-level
   extraction buys nothing until a "download this attachment" feature
   exists. Write the object BEFORE committing the row (a crash then
   orphans a reapable object rather than leaving a pointer to nothing).
   Blob bytes are incompressible (encrypted), so under CNPG each stored
   byte is ~1 byte of WAL × N replicas + archive — the multiplier
   object storage removes.

   **Junk stripping (signature logos, tracking pixels, inline GIFs) is
   REJECTED, not deferred.** The error asymmetry is unacceptable
   (keeping junk costs fractions of a cent; dropping a scanned
   signature page is silent data loss in a system of record), the
   `Content-Disposition` field that should classify it is unreliable
   across mail clients, and stripping before hashing freezes the
   stripper into a permanent versioned contract — retuning it changes
   `content_hmac` and breaks redelivery dedup. Storage economics
   (~8x lever, order-of-$100/month at 200-org scale) do not justify a
   permanent correctness hazard. Storing raw preserves the option to
   strip later; stripping forecloses recovery.

3. **Lifecycle and retention.** Tiering: prefer an
   INFREQUENT-ACCESS class (immediate retrieval, small read fee) over
   deep-archive/Cold-Archive classes, whose asynchronous
   minutes-to-hours retrieval would infect the workbench and any
   re-processing path with a restore state machine — the extra savings
   are tens of dollars a month against real permanent complexity.
   Implement transitions as BUCKET LIFECYCLE RULES (pure configuration,
   pointers unaffected since the key does not change), not application
   logic; state-based tiering (archive as soon as a payload is resolved
   or discarded — nothing reads those) is a later refinement needing
   code. Note the elegant composition with O-012 crypto-shred: with a
   per-object/per-person key in Postgres, erasure deletes a key row and
   the archived ciphertext becomes permanently unreadable in place —
   no retrieval, no early-deletion fee, no cross-system delete
   coordination. Also verify object-storage backup/versioning is
   aligned with Postgres PITR, or a point-in-time restore yields rows
   pointing at since-changed objects.

   **OPEN, and the part with legal weight: retention.** "Archive after
   a year" implies "keep forever", which is itself a policy choice —
   brokerage transaction-record retention obligations (multi-year,
   state-dependent) push one way; CCPA/GDPR deletion schedules push the
   other. Determines whether the lifecycle rule ends in "transition" or
   "transition, then expire". Sequenced with O-013 (erasure), which
   owns the same territory; O-012 (per-Person keys) is the mechanism.

Blocks: nothing immediately. Question 1 is actionable now; questions 2
and 3 are forced by the recordings slice (O-012 territory), which
should absorb whole-message relocation in the same slice rather than
building object storage twice.


### D-045 — Attio-inspired white workspace with selective glass (2026-09-06)

Accepted (user, after iterative screenshot review; explicitly requested a
checkpoint commit before applying). Approved reference:
`docs/design/concepts/people-attio-glass-approved.png`.

- Preserve a minimal white/black foundation. Use compact headings, muted but
  readable text, fewer redundant labels, fine borders and flat data tables.
- Add Apple-inspired glass selectively to navigation selection, controls,
  badges and the floating person inspector: translucent white, reflective
  edges and shallow shadows. No colorful backgrounds or heavy refraction.
- Stage badges may use muted blue (Lead), sage (Active Client), lavender
  (Nurture), and clay (Hot Prospect). Hot Prospect retains its flame. This
  supersedes D-020's no-other-stage-color restriction; normalized-name matching
  still applies, with neutral custom/renamed stages. No stage schema changes.
- People gains a preview alongside the table without losing list context;
  full-profile links remain available. Use current data and capabilities only.
- Black primary controls and the existing black D-044 brand assets are
  authorized. The underlying Threshold E identity remains unchanged.
- This supersedes UI_STYLE's previous fixed typography, card-only layout and
  shadow/no-gradient restrictions for the approved glass surfaces. Functional
  specifications, authorization and shared wire contracts are unchanged.

Implementation brief: `docs/tasks/UI_REFRESH.md`.

### D-046 — Saved-list privacy and separate personal/shared limits (2026-09-06)

Accepted (user, answering both pending Slice 011b questions during kickoff).
Refines D-043 and resolves the two open items in PROJECT_STATE.

1. **Personal saved lists are creator-only, including from admins.** Only
   the creator can discover, read, edit, or delete the personal definition.
   There is no admin visibility or management exception. This protects the
   list's name and filter criteria; it does not change Organization-wide
   Person visibility (D-005) or grant access to People through a shared list.
2. **Separate limits:** at most **200 shared lists per Organization** and
   **50 personal lists per creator per Organization**, with **no combined
   Organization cap**. These count saved definitions, not matching People.
   They replace the ladder's ambiguous "200/org, 50/owner" wording.

Shared lists remain admin-curated and visible Organization-wide; agents
duplicate a shared definition into their own list rather than editing the
shared original (D-043). No other 011b contract or implementation is approved
by this decision; the concrete draft still receives its planned review.

Blocks: nothing. Feeds the Slice 011b specification and implementation brief.

### D-047 — Today source limit and partial availability (2026-09-06)

Accepted by the user while specifying Slice 011c, in two explicit answers:

1. Each agent may connect **up to five saved lists as Today sources** in their
   active Organization. This is separate from D-046's saved-definition limits.
2. If one source cannot load, Today **shows available work with a clear
   notice**: preserve built-in reminders and results from working sources.
   An unavailable source must not be presented as an empty list or a complete
   Today result. This does not require pretending work is available when the
   underlying database or built-in query cannot be read.

The concrete status, query, command and client contracts belong to the 011c
specification and its review. These answers approve the two product choices,
not an as-yet-unreviewed implementation or a push/deployment.

Blocks: nothing. Feeds the Slice 011c specification.

Follow-up, 2026-09-06: after independent READY review and isolated performance
evidence, the user explicitly approved the complete `SLICE_011c.md` plan for
implementation, including its declared contracts and acceptance thresholds.
This authorizes implementation/testing, not commit, merge, push or deployment.

Follow-up, 2026-09-06 (late evening): after the Claude takeover finished
verification, the user approved three things in one answer: (1) the fourth
transaction-local planning change in `SLICE_011c.md` §8 (`SET LOCAL
enable_mergejoin = off` in the Today read transaction), which pins the fast
plan for the built-in query's per-Person contact probe against a
visibility-map-dependent planner flip measured in Phase B; (2) acceptance of
the Phase B evidence with its one recorded limitation (the concentrated book's
zero-source HTTP pairing rests on run 1, run 2's frozen baseline series for it
being disrupted by the same hazard); (3) the local commit and merge of the
slice. Push and deployment were not authorized. The planner hazard itself is a
pre-existing Slice 003/009 property; a durable fix is queued in PROJECT_STATE.

### D-048 — A saved list's sort order is part of its definition (2026-09-06)

Accepted by the user while specifying Slice 011b-sort. A saved People list's
sort (one of Added, Name, Stage, Assignee, ascending or descending, applied by
the server before the 500-row cap) is stored with the list, changes bump the
list's revision, and editing it needs the list's write rights: the creator for
a personal list, a current admin for a shared list. Everyone opening a shared
list therefore sees the same first 500 rows; a member who wants another order
duplicates the list, as for criteria. A per-viewer sort preference was
declined for v1 (it needs a second table and mutation path and makes "which
500" viewer-dependent). The user also accepted the default control: clickable
column headers plus a compact "Added" column, a cheap-to-reverse UI choice.

The concrete contracts belong to `docs/specs/SLICE_011b_SORT.md`. This
decision does not authorize implementation, commit, merge, push or deployment.

Blocks: nothing. Feeds the Slice 011b-sort implementation gate.

### D-049 — Slice 011d ships as one L rung with parallel backend and web lanes (2026-09-06)

Accepted by the user while planning Slice 011d. The ladder pre-declared a
d1/d2 split (two person-state feeds first, the call-state axis second) if the
rung ran past M. Planning against the code showed that seam does not exist:
the three compiled-in Today arms are one SQL statement sharing one precedence
rule and one 201-row cap, so serving two arms as feeds while keeping the third
compiled in would need a throwaway transitional query, and the call-outcome
payload plumbing must exist from the first commit to keep reasons identical.
Offered the choices of (a) a feeds-as-code rung followed by an admin-editing
rung, (b) a backend rung followed by a web rung, or (c) one L rung with two
parallel lanes, the user chose **(c)**: 011d is delivered whole, with a
backend lane owning the migration, vocabulary, feed path, commands, routes and
equivalence gate, and a web lane owning the admin surface, filter chips and
Today markers, coordinated through short-lived worktrees. This is a one-time,
explicit exception to the user's standing S–M rung rule, not a change to it.

The product rules the specification adopts as veto-able safe defaults
(admin-only editing, disabling allowed with typed confirmation for the
unanswered-inquiry rule, viewer-relative `assigned_to: me` locked on the two
person-state feeds, the anchor clause locked, preview for a chosen member,
canonical fallback for an invalid stored rule, and the v1 at-least-one-inquiry
feed constraint) are recorded in `docs/specs/SLICE_011d.md` §1. This decision records only the sizing
and delivery shape.

Follow-up, 2026-09-07: after independent review (READY WITH CORRECTIONS, all
applied), the user approved the complete `SLICE_011d.md`, including its
declared contracts and the seven §1 safe defaults, and chose to **hold
implementation for a later session**. This authorizes the planning documents'
commit only; implementation, commit of code, merge, push and deployment need
their own gates.

Blocks: nothing. Feeds the Slice 011d implementation gate.

### D-050 — Operating envelope, verification budget and performance gating (2026-09-07)

Accepted by the user on 2026-09-07 after Slices 011b-sort, 011c and the first
eight hours of 011d showed review, adversarial-test and performance effort
spent on scenarios outside any practical timeframe: twenty simultaneous
worst-case Today loads, eight review rounds on multi-tab authentication races,
collation and tie tests for a sort column, and absolute latency caps measured
on a developer laptop that also ran Postgres in Docker, Vite, browsers and
several coding agents. The user's rule: time is acceptable when necessary,
never for scenarios that will not exist in practical timeframes.

**Principle.** Correctness, tenant isolation and privacy hold everywhere.
Seamlessness and measured performance are owed only inside the declared
operating envelope. Outside it the product must fail closed, must not corrupt
or leak data, and must return an honest error. Nothing more is required.

**v1 operating envelope (twelve months from this decision).**

| Dimension | Envelope |
|---|---|
| People per Organization | 25,000 |
| Members per Organization | 50 |
| Concurrent Today loads per Organization | 5 |
| Browser sessions per agent | one active tab; other tabs must never show another actor's data but need not recover seamlessly |

**Verification budget.**

- Reviewer and tester findings carry the tags in
  `docs/prompts/06-verify-and-review.md`, now including `BEYOND_ENVELOPE`,
  whose default disposition is LATER regardless of slice size. A `TRUST`
  finding beyond the envelope is still applied, but the required fix is
  fail-closed behaviour, not seamless recovery.
- At most **two** review-then-fix rounds per slice. Findings still open after
  round two are recorded as LATER in the verification record; a third round
  requires the user's explicit approval.

**Performance gating.** Laptop measurements cannot predict production on
dedicated EPYC hosts with several API replicas, and API replicas do not
relieve the single Postgres. Therefore a slice gates on exactly two things:

1. **Paired relative regression** — new code against the previous code in
   the same build, machine, fixture and clock, request p95 within
   max(25 ms, 10%), payloads equal apart from declared envelope fields.
2. **Plan shape** — one `EXPLAIN (ANALYZE, BUFFERS)` of each new or changed
   hot statement on the worst realistic book, showing index use and no
   super-linear growth with People.

Absolute p95, concurrency above 5, pool-wait headroom and planner-toggle
comparisons are **reported for trend-watching, never gated**. Real capacity
testing happens once, on the production-shaped hardware with a
production-shaped dataset, using `./scripts/perf`; that run becomes the
capacity baseline and replaces laptop capacity inference. Until then no slice
spends more than one benchmark run on performance unless the paired
regression fails.

**Applied immediately to Slice 011d step 5:** gate on the paired Legacy
comparison and the person-state EXPLAIN; report the 1/10/20 matrix and pool
wait without caps; record the `enable_mergejoin` pair and drop the toggle
question from the gate. Spec §8's absolute caps are superseded by this
decision; the spec text is amended by pointer, not rewritten.

**Unchanged:** tenant-isolation and authorization tests, migration rules
(AGENTS §8), the rule that no check is claimed passed unless run, and the
once-only final-tree gates.

Blocks: nothing. Supersedes the absolute performance caps in 011c §8 and
011d §8 and the open-ended review loop in the coordinator skill's Phase 8.

### D-051 — Tag rename and delete: admins, plus the creator while the tag is unused (2026-09-07)

Accepted (user, answering the Slice 011e ladder item "rename/delete admin —
confirm at spec"). Refines D-043's tag dependency into a permission rule.

1. **Any active member creates tags, applies them to People and removes them
   from People.** Creation is inline and create-or-get by case-insensitive
   name, so two members typing the same tag get one definition.
2. **A tag is renamed or deleted by an Organization admin, or by its creator
   while no Person carries it.** Once any Person carries the tag, changing
   or removing the definition is an admin action, because it affects every
   Person and every saved list or Today rule that names the tag. The
   creator path lets a member fix a typo before anyone depends on it. The
   check is made inside the command under the tag row's lock, so a tag
   applied concurrently by another member is already "in use".
3. **Deleting a tag is a hard delete** (safe default accepted with the
   decision): its Person links go with it, and saved lists, list Today
   sources and system feeds naming the id report `invalid_tag` through the
   existing stale-reference paths until edited. Delete is not blocked while
   referenced: D-046 hides personal lists from admins, so a block could be
   neither shown nor repaired by the admin and would leak that a private
   list exists.

Platform admins have no tenant access and cannot manage tags. Tags are
relational CRUD (AGENTS §4.6); no history fact is written. No other 011e
contract is approved by this decision; the specification receives its own
approval.

Blocks: nothing. Feeds the Slice 011e specification and implementation brief.

### D-052 — Trigger-maintained derived columns are a read-model mechanism (2026-09-08)

Accepted (user, at Slice 012 specification). Slice 012 stores four derived
dates on `person` (`last_inquiry_at`, `last_contact_at`, `last_inbound_at`,
`last_outbound_at`), each equal at every commit boundary to the maximum of
the corresponding history rows, so filters and Today stop scanning history.

1. **Database triggers keep them exact.** `AFTER INSERT` triggers on
   `inquiry`, `contact_attempted` and `correspondence_captured` update the
   Person row in the same transaction, guarded so a maximum never moves
   backwards (backdated captures and corrections are no-ops). Chosen over
   application updates in each typed command because every writer is
   covered without remembering to (four commands and the future Slice 010
   import share one insert site), 23 raw-SQL test fixtures stay correct
   unchanged, and an old binary still running during a deploy keeps the
   columns exact.
2. **The history insert remains the only business mutation** (AGENTS §4.8,
   D-021). A trigger maintaining a derived read-model column (AGENTS §4.7)
   is in the same category as the schema's append-only `reject_mutation`
   triggers and composite foreign keys: database-enforced consistency, not a
   second mutation path. Future derived columns may use the same mechanism
   under the same rule; business facts may not be written by triggers.
3. **Byte-identical results are a gate, not a hope.** Every statement that
   switches from computing a maximum to reading a column proves identical
   results against its frozen pre-switch text before the switch lands
   (Slice 012 §4).

Blocks: nothing. Feeds the Slice 012 specification and brief. Recorded
lever, not taken: column-level `UPDATE` grants plus a `SECURITY DEFINER`
trigger function would make the invariant database-enforced against
application bugs.

### D-053 — Notes ship as plaintext erasable CRUD; O-012 amended (2026-09-08)

Accepted (user, at Slice 015 planning; chosen over resolving O-012 first).
O-012 as recorded on 2026-08-23 blocked free-text notes until the
per-Person key hierarchy and the raw-payload migration existed. Notes are
CRM core (thesis §11) and the recorded path back into the parked FUB
migration, and `inquiry.message` already ships as plaintext free-text
customer content in the erasable CRUD set (SLICE_002 §2, D-015 §6).

1. **The `note` table joins the erasable CRUD set** beside `person`,
   `contact_method` and `inquiry`: plaintext, cascaded with the Person
   (D-015 §5 erasure), never on a history fact row (D-015 §3). AGENTS §4.6
   already lists notes under ordinary relational CRUD.
2. **Constraints that keep the later O-012 migration a column move, not a
   redesign:** the body is read at exactly two sites (the Person detail's
   history projection and the mutation receipt); it never travels on the
   realtime channel (D-023), the Operator ledger (D-029), spans or logs
   (AGENTS §9); delete is a tombstone that empties the body; the body is
   capped at 10,000 characters; no derived column, filter clause or Today
   rule reads it.
3. **Accepted cost, stated:** when O-012 lands, the timeline switches from
   an inline body to an on-demand fetch (O-012 §4), a Web change plus one
   endpoint; the Operator's person view switches from inline untrusted
   text to a request-time decrypt (O-012 §3). Nothing else moves.
4. **Safe defaults adopted with this decision** (veto-able, recorded in
   the Slice 015 specification): any active member writes a note; the
   author or an Organization admin edits or deletes it (the D-051 shape,
   without D-051's "while unused" clause, which has no analogue); a
   deactivated author's notes stay visible and attributed (D-027 §2), so
   O-004 stays open; the Operator's `get_person` view carries the latest
   five note bodies as untrusted text, the exposure class inquiry messages
   already have.

Blocks: nothing. Feeds the Slice 015 specification. O-012 §"Blocks"
amended above; O-013's erasure runbook gains `note`.

### D-054 — Due tasks reach Today through a fixed built-in axis plus a task panel (2026-09-09)

Accepted (user, at Slice 016 planning, after asking what the industry
does). Tasks are the last unbuilt CRM-core model (thesis §11) and "overdue
task" is a thesis Today reason. Every real-estate CRM surveyed (Follow Up
Boss, Lofty, kvCORE, Sierra, BoomTown, Wise Agent; from training data, not
re-verified live) shows tasks as their own Overdue / Today / Upcoming list
with one-click complete and snooze, and none has a Person-ranked queue;
that queue is this product's differentiator, so both views ship.

1. **A fixed built-in task axis inside Today's query** (Slice 016b): the
   viewer's open tasks due within 24 hours or overdue, one row per
   Person, reasons `task_due` and `task_overdue`, overdue in the `high`
   tier, evaluated on its own savepoint under the call feed's budget and
   failure discipline. **This is a recorded, temporary exception to D-043
   and 011d** ("every built-in reason is an admin-tweakable feed"): admins
   cannot tweak or disable the axis yet. The exception closes when the
   task filter-clause family lands (`has_open_task`, `task_due_within`,
   `task_overdue`, plus a `next_task_due_at` column under D-052 if
   measured necessary) and the axis becomes feed four. Recorded LATER
   with that trigger.
2. **A task panel on the Today page**, the industry surface: the viewer's
   open tasks grouped Overdue and Due today, one-click complete and
   snooze-to-tomorrow, backed by one member-level read route. Reminders,
   action plans and a full `/tasks` page stay LATER; reminders are the
   first to pull forward when mobile arrives.
3. **Safe defaults adopted with this decision** (veto-able, recorded in
   the Slice 016 specification): a task always belongs to a Person; the
   assignee, the creator or an Organization admin edits, completes,
   reopens, snoozes or deletes (the D-053 shape); `due_at` is an instant
   and a date-only pick is sent by the client as local end of day (no
   Organization timezone exists); tombstone delete; no task body yet; a
   small closed `kind` enum (`call`, `email`, `text`, `follow_up`,
   `other`); the Operator reads tasks now, and its `create_task` /
   `complete_task` tools are the next S rung with their own decision
   (risk class and mechanism; D-034).

Blocks: nothing. Feeds the Slice 016 specification. Amends by pointer
011c §5 ("built-in items continue to have a real InquiryRef": a task item
on a zero-inquiry Person carries `latest_inquiry: null`, the list-only
precedent), 003 §3 ("People with no Inquiry are never on Today"; "high
iff new_inquiry"), and 011d §1 rule 7 and §5 (the feed-only inquiry
constraint is not lifted, the axis is not a feed; "`TodayReason` gains no
variant" is superseded). D-022's deferral of done/snooze/dismiss on Today
items stands: complete and snooze act on the task row, never on the Today
item.

### D-055 — Development telephony host on EC2; egress present but dormant (2026-09-09)

Recorded (user, by direct change in another session; reviewed by the
coordinator). The host behind `livekit1.tarams.org` is now an EC2
`c6i.xlarge` in us-west-1 running the same Slice 006 stack (Caddy, Redis,
LiveKit server with TURN, LiveKit SIP) plus LiveKit Egress; the earlier OVH
box is no longer referenced by DNS. The telephony test suite passes against
the new host (rooms and the webhook round trip with the real key pair).

1. **Scope: the development telephony host.** The production hosting
   direction in the architecture baseline (OVH bare metal) is not changed
   by this entry; it is decided at the deployment slice, where EC2 is now a
   candidate with a working template.
2. **Egress is dormant capability, not a feature.** No application code
   can start a recording (verified: no egress call in the workspace), and
   O-002 (recording consent) and O-012 (recordings as per-Person-keyed,
   shreddable blobs) remain open and blocking. The S3 target
   (`EGRESS_S3_BUCKET`, instance-role credentials, no keys in files) is
   infrastructure readiness only. Before any recording ships: O-002 and
   O-012 resolved, the bucket with public access blocked, encryption at
   rest and a lifecycle rule, and a cleanup for failed uploads left on the
   host's `egress_tmp` volume.
3. **Host posture, accepted for a single-purpose host:** egress runs with
   host networking and `SYS_ADMIN` (required by LiveKit since egress
   1.7.6); its rendered config is world-readable inside a 700 directory
   because the image runs unprivileged.

Blocks: nothing. Closes the 2026-09-08 "LiveKit down" environment note.
Still pending from the old host: the Telnyx SIP password rotation.


### D-056 — Inbound mail size cap at Cloudflare's 25 MiB ceiling; the relay streams (2026-09-09)

Accepted by the user on 2026-09-09 (the coordinator's recommendation; the
alternatives presented were 10 MiB as a constants-only change and deferral
to the object-storage slice). Resolves O-015 question 1; questions 2
(storage location) and 3 (retention) stay open. Delivered by Slice 017.

1. **The cap is Cloudflare's own inbound limit.** Email Routing rejects
   messages over 25 MiB before any worker runs, so the relay's threshold
   moves from ~1.4 MiB to 25 MiB and no message Cloudflare accepts is
   bounced by us for size. The relay's reject branch stays as defence in
   depth. The endpoint's per-route body limit becomes 34 MiB, derived from
   the raw ceiling (`4 * ceil(25 MiB / 3)` plus the JSON envelope): a value
   change on the frozen SLICE_007b §5 row "Body over 2 MiB → 413". The
   envelope, the handler order and every response shape are unchanged.
2. **The relay streams.** Buffering 25 MiB plus its base64 plus the JSON
   copy would need most of a Worker's 128 MB isolate, so the relay encodes
   chunk by chunk into a streamed request body (chunked transfer encoding;
   a fixed-length body was set aside because it couples the relay to
   `rawSize` accuracy and has no seat in the `node --test` harness). The
   `{"recipient","raw"}` contract is untouched. A raw `message/rfc822`
   pass-through body — no base64, near-zero CPU — was considered and
   recorded as the fallback, not adopted: it is a contract change.
3. **Precondition: the Workers plan.** The Free plan allows 10 ms of CPU
   per invocation and Cloudflare warns Email Workers exceed it; encoding
   megabytes is CPU-bound. The account's plan is not recorded in the
   repository and is verified before the walkthrough. If it is Free, the
   upgrade is the user's decision and the pass-through fallback needs its
   own approval.
   *Verified 2026-09-09 at the walkthrough: the account was on Workers
   Free. The user chose the Workers Paid upgrade (no code change); the
   pass-through fallback stays recorded, not built.*
4. **Storage cost accepted** at design-partner scale (O-015's analysis):
   raw MIME stays whole and encrypted in Postgres `BYTEA`; `content_hmac`
   dedup is unchanged. WAL amplification and the storage-flood exposure of
   a public intake address scale with the cap, not in kind; a
   per-Organization inbound byte budget that fails closed with an honest
   bounce is the D-050 control, recorded LATER with the
   production-deployment trigger.

Blocks: nothing. Feeds SLICE_017. Amends by pointer SLICE_007b §5,
SLICE_007g §3 and SLICE_009 §1 (the stated 2 MiB limitation closes).

### D-057 — Operator `complete_task` executes with a receipt and Undo; `create_task` proposes (2026-09-10)

Accepted by the user on 2026-09-10 (the coordinator's recommendation; the
alternatives presented were propose-then-confirm for both tools, and
execute-with-Undo for both). Resolves the risk-class decision D-054 §3
deferred to this rung; the D-034 mechanism question is a planning default,
not a product decision (below).

1. **`complete_task` is the first AGENTS §5.4 "low-risk and reversible"
   action.** On the agent's request the Operator completes an open task
   at once, the UI shows a receipt built from server data, and an Undo
   reopens it (the existing reopen command). Rationale: completing a task
   is already one click on the Today panel and is reversible, so an Undo
   is the same trust level as the panel. The application, not the model,
   classifies the action and enforces the receipt.
2. **`create_task` stays "consequential": propose, then confirm.** The
   tool only creates a proposal (the SLICE_006b shape); the agent confirms
   from a card built from server data before the task exists. Rationale:
   the title and the due instant are content the model may have
   mis-heard, and a task lands on a Person's record.
3. **Authorization is the D-053 shape unchanged:** the Operator acts as
   the signed-in member; `complete_task` succeeds only where that member
   could complete from the panel (assignee, creator or Organization
   admin); `create_task` assigns to the acting member unless told
   otherwise and every referenced Person and task is Organization-scoped.
4. **Mechanism, a planning default (D-034 stands):** no `crm-operator ->
   crm-app` edge; both tools are `ToolBackend` seam methods implemented
   by crm-api's adapter over the existing task commands, and
   `operator_proposal.tool` widens to admit `create_task`. The planner
   confirms this shape; if it does not hold, the mechanism returns as a
   decision.

Blocks: nothing. Feeds the Slice 018 specification. Amends by pointer
D-054 §3 (the rung's decision is taken). O-013's erasure runbook gains
`operator_task_proposal` (the proposed title, retained until Person
erasure; SLICE_018 §4).

### D-058 — Custom fields v1: four typed kinds, member-set values, archive-only definitions, filtering deferred (2026-09-10)

Accepted by the user on 2026-09-10 (the coordinator's recommendation; the
alternative presented was including custom-field filter clauses in the
same slice). Delivered by Slice 019a; the filter rung is 019b, separately
approved.

1. **Filtering is deferred.** Slice 019a ships definitions, per-Person
   values, the Person page, the Manage page and the Operator read. Custom
   fields join the filter vocabulary in a later rung (019b) with its own
   approval, because that rung amends SLICE_011a §4b ("one clause per
   kind" becomes one per field with a slot cap) and touches all fourteen
   bound statements. The dynamic-SQL fork the 011 ladder reserved is not
   taken: 019b is designed as fixed static-SQL slots.
2. **Safe defaults accepted with the decision** (veto-able, recorded in
   the specification): the type set is Follow Up Boss's four — text,
   number, date, single choice (a yes/no is a two-option choice; boolean
   and recurring dates are additive later types); any active member sets
   or clears a value on a Person, the rule every Person mutation uses
   today (AGENTS §4.4; assignment never restricts visibility); definitions
   and options are archived, never hard-deleted, because a value is
   customer data across up to 25,000 People (a deliberate divergence from
   D-051's hard delete for tags, whose links carry no content); a value
   change is ordinary relational CRUD, already settled by AGENTS §4.6, in
   the D-053 content posture (plaintext, erasable, cascaded with the
   Person, never on the realtime channel, ledger, spans or logs; reaching
   the Operator as untrusted text).
3. **Import readiness, schema only:** definitions carry `source` and
   `external_key` (FUB's `name`) and values carry `origin` and
   `correlation_id`, so the parked Slice 010 ladder's 010f rung can map
   FUB custom fields without a schema change. No import code in 019.

Blocks: nothing. Feeds the Slice 019 specification. Amends by pointer
SLICE_011_LADDER "explicitly not in this ladder" (custom fields now have a
scheduled rung) and SLICE_010_LADDER 010f (the destination exists after
019a). O-013's erasure runbook gains `person_custom_field_value`.


**D-058 follow-up — Slice 019b specification approved (2026-09-10).**
The user approved `docs/specs/SLICE_019b.md` after independent Astra review
returned READY: “approved. go ahead and implement it.” Approval includes the
five-slot custom-field filter vocabulary, archive/absence semantics, declared
contract changes and verification plan; Terra high implements the reviewed
brief. Commit, merge, push and deployment are outside this authorization.

### D-059 — First FUB migration targets a new, empty Organization (2026-09-10)

Accepted by the user while requesting the refreshed FUB migration planning
summary. Asked whether the first migration should target a new empty CRM
Organization, an existing populated Organization, or both, the user selected
“New Organization first (Recommended).”

The first migration targets a new, empty Organization. Merging a FUB book into
an independently populated Organization is outside that initial scope.
Ordinary Organization setup and member mapping, precise emptiness checks,
resumption of the same migration and duplicate source records must be defined
in the relevant slice specifications.

This decision resumes Slice 010 planning; it does not approve implementation,
API-only source scope, credential storage, matching/overwrite/rollback policy,
fidelity reductions, data erasure or a real customer's cutover. Those choices
remain open in the [migration summary](../plans/SLICE_010_MIGRATION_SUMMARY.md).

### D-060 — FUB assessment uses API-first access and encrypted saved credentials (2026-09-10)

Accepted by the user: “Yes, approved” in response to approval of API-first
assessment with encrypted saved credentials for 010a, after the specification
and implementation brief were presented with their new shared contracts.

010a implements the bounded read-only source assessment in
[SLICE_010a](../specs/SLICE_010a.md): admin-managed Organization-bound connection,
encrypted persistent credentials, six source checks, encrypted probe evidence,
durable report and retry/cancel recovery. The additive HTTP/persistence/Web
contracts are owned by that slice. This is not a full inventory or an import;
unknown and unchecked coverage remain explicit.

Implementation and verification are authorized. Source contract qualification
and authorized live validation remain necessary. Approval does not establish
permission from FUB, authorize reading a real customer book, decide later
mapping/cutover/erasure policies, or authorize deployment/merge/push. D-059's
new-Organization-first direction remains in force.

**D-060 follow-up — source integration authorized (2026-09-11).**
After being told that 010a is synthetically verified, uncommitted and not
deployed, with live FUB validation deferred, the user requested: “do any cleanup
and commit, merge, push”. This authorizes documentation cleanup, committing
010a, merging into main, publishing main (including the three local 019b
commits), and removing the merged 010a worktree/branch. It does not authorize
a runtime deployment, real-source access or later migration rungs.

**D-060 follow-up — shared-development deployment authorized (2026-09-11).**
The user requested “Deploy 010a and start 010b planning. I will do the validation
against an authorized FUB later in a few days.” This authorizes the existing
shared-development API/Web refresh and additive 010a migration. Live source
validation remains deferred; no production-cluster deployment or live account
access is implied. Completion is recorded in
[SLICE_010a_RELEASE.md](../tasks/SLICE_010a_RELEASE.md).

### D-061 — 010b captures core records first and explicitly tracks remaining data (2026-09-11)

Accepted by the user during 010b planning. Presented with a bounded first
snapshot versus requiring all accessible history/data in 010b, the user chose
“Core records first; explicitly track remaining data.” The core families are
People, users, stages, custom fields, notes and tasks. Other families remain
visible as not yet captured, with later snapshot work explicitly tracked.

This sequences extraction work; it does not reduce D-012's eventual migration
fidelity, classify missing content as preserved, or approve a core-only cutover.
Embedded data returned with core records is preserved, with its actual coverage
described separately from full collection retrieval. New-Organization-first
(D-059) still applies to import. Mapping, deduplication, retention, recovery and
cutover policies are not accepted merely by this scope choice.

Planning is authorized; the 010b specification, shared-contract changes and
implementation still require review and approval. Live authorized FUB validation
remains user-deferred. See [the draft](../specs/SLICE_010b.md).

### D-062 — Email bulk content belongs outside PostgreSQL (2026-09-11)

Accepted by the user during storage-capacity planning for a 50-agent
Organization: “emails gotta go to object storage or even a storage servers
with large disks”, citing the cost of bare-metal NVMe capacity.

Email bodies and raw messages, including their embedded attachments, are to
reside in bulk storage outside PostgreSQL. PostgreSQL retains authorized
metadata, relationships, integrity/deduplication identifiers and storage
references. Preserve complete raw content under D-012 and the whole-message
approach in O-015; this is a placement decision, not permission to strip content.

Object storage or dedicated storage servers remain candidates. No vendor,
storage software, hardware layout, capacity, replication or retention policy
is selected by this decision. Body-search design remains future work; this
does not authorize copying full email bodies into PostgreSQL search/read models
or changing D-042's current metadata/subject visibility rules.

The existing encrypted `raw_payload`/`correspondence_raw` BYTEA implementation
remains in place until a separately specified relocation step defines durable
writes, scoped reads, recovery, backup/restore consistency and existing-data
migration. O-012/O-013 prerequisites remain open. No runtime/schema change or
infrastructure purchase is authorized here. Email capture stays outside 010b's
six core families; this decision guides later email capture and migration work,
without relocating 010b's other source evidence implicitly.

### D-063 — Revised 010b specification and implementation approved (2026-09-11)

The user approved the independently reviewed specification and storage policy:
“approved...lets move on to implementation”, after the six review corrections
and remaining allowance/delegation choice were presented.

This accepts [SLICE_010b.md](../specs/SLICE_010b.md) and its
[execution brief](../tasks/SLICE_010b_IMPL.md), including additive snapshot,
preview, budget and overlap HTTP/persistence contracts; representation-aware
capture; distinct source/retained-read authority; durable byte reservations;
frozen preview inputs and bounded overlap groups. Synthetic development starts
with 2 GiB/run and 4 GiB retained/Organization allowances. Deployment operators
set ceilings; current Organization admins can confirm monotonic increases within
those ceilings, followed by a separate authorized resume action. These are not
production customer quotas, retention periods or physical PostgreSQL disk limits.

Implementation, necessary specification detail within the owned contracts and
synthetic verification are authorized. Preserve D-050, the new-Organization-first
future import, and the existing 010a retry contract. No business import, source
writes, actual customer-data processing or runtime deployment is included.
The user's live FUB validation deferral remains in force; synthetic evidence
does not close it or the D-015/O-012/O-013 customer-data prerequisites.

**D-063 follow-up — integration, deployment and cleanup authorized (2026-09-11).**
After implementation and synthetic verification completed, the user requested
“commit, merge, deploy and cleanup”. This authorizes committing the verified
010b changes, merging/publishing main, deploying the existing shared-development
API/Web and additive migration, and removing the merged worktree/branch and
temporary synthetic QA resources. Preserve release recovery artifacts and
customer data. Live authorized FUB validation remains deferred; no production
cluster, import/cutover or real-source operation is implied.
Completion and recovery evidence are recorded in
[SLICE_010b_RELEASE.md](../tasks/SLICE_010b_RELEASE.md).

### D-064 — 010c preserves separate People, approved stages and review-only use (2026-09-11)

Accepted by the user while requesting the 010c plan:

1. **“Keep separate People; flag overlaps.”** Distinct FUB People remain distinct
   CRM People even when email/phone values overlap. Shared household details do
   not authorize merging; overlap evidence remains visible.
2. **“Approve matching stage creation.”** Admins may explicitly approve creating
   matching CRM stages for unfamiliar source stages to preserve the pipeline.
   A suggestion or source label alone does not authorize stage creation.
3. **“Review-only until activation.”** Imported records remain available for
   admin review while ordinary agent use and outbound actions wait for a later
   activation capability. This is an explicit migration-readiness exception to
   normal operational Organization-wide member access under D-004/005, not
   Team/AssignedUser Person visibility. It does not establish communication
   consent or resolve private-source audience rules for eventual activation.

D-059's new, empty destination remains in force. The
[010c specification](../specs/SLICE_010c.md) proposes the concrete workspace gate,
mapping/provenance/recovery contracts and verification. Only these three choices
are accepted here; the complete specification and implementation still require
review and approval. Cancellation/repair/delta and later activation policy are
not inferred from the choices above. Live authorized FUB validation remains
user-deferred, and D-015/O-012/O-013 customer-data prerequisites remain open.

### D-065 — Reviewed 010c specification and implementation approved (2026-09-11)

After independent review returned READY and the completed specification/brief
were presented, the user said **“approved for implementation.”** This accepts
[SLICE_010c.md](../specs/SLICE_010c.md) and its
[execution brief](../tasks/SLICE_010c_IMPL.md), including their declared shared
contracts and the five reviewed corrections. D-064's three policy choices remain
in force. Implementation, concrete contract detail within this scope, isolated
synthetic verification and the required review/check gates are authorized.

The accepted scope includes retained-evidence People/contact/stage/assignment
import, frozen plans, provenance and reconciliation, per-item byte admission,
native/index eligibility, workspace read/write guards and Operator admission,
the admin review hold and compatible-runtime recovery. Cancellation retains
committed imports; activation and confirmed-mapping repair/deltas remain later
work. Existing Organizations default operational. No new source request,
customer-data processing, live FUB validation, commit/merge/push or deployment
is authorized by this implementation approval. The user's validation deferral
and D-015/O-012/O-013 prerequisites remain unchanged.

**D-065 follow-up — integration, publication and cleanup authorized (2026-09-11).**
After implementation and synthetic verification completed, the user requested
“commit, merge, push and cleanup”. This authorizes committing the verified 010c
changes, merging and publishing main, and removing the merged branch/worktree
and disposable 010c synthetic QA resources. Preserve committed verification
evidence, customer data and existing release recovery artifacts. Runtime
deployment is not included in this follow-up; shared development remains on
010b. Live authorized FUB validation remains user-deferred, and activation and
customer-data readiness retain their existing gates. Integration results belong
to [SLICE_010c_VERIFICATION.md](../tasks/SLICE_010c_VERIFICATION.md).

**D-065 follow-up — shared-development deployment and next-import planning authorized (2026-09-11).**
The user requested “lets deploy 010c and then start planning the next import
slice”. This authorizes the existing shared-development API/Web refresh,
additive 010c migration, compatibility inventory/preflight and release checks,
followed by a reviewed draft for the next core import. The proposed next bounded
scope is tags and custom-field definitions/options/values; notes/tasks remain
explicit following work. Planning does not approve that future specification,
shared contracts or implementation. Live authorized FUB validation remains
user-deferred; no customer-data import, activation or production-cluster work
is implied. Release evidence belongs to
[SLICE_010c_RELEASE.md](../tasks/SLICE_010c_RELEASE.md).

### D-066 — Reviewed 010f1 specification and implementation approved (2026-09-11)

After the independently reviewed specification and its remaining approval gate
were presented, the user said **“ok proceed with 010f1.”** This accepts the
reviewed [specification](../specs/SLICE_010f1.md),
[execution brief](../tasks/SLICE_010f1_IMPL.md) and their declared shared contracts.
The accepted defaults include completed-parent-only and one-child lifetime,
independent held-item subset execution with explicit acknowledgement, matching
catalog creation or approved mapping, unchanged native limits, no replacement
of differing local values and the conservative type/recurrence/collision rules.

Implementation, concrete contract detail within this scope, the additive child
migration and isolated synthetic verification/review gates are authorized. Keep
the original People import and workspace binding immutable and preserve the
admin review hold. Notes/tasks, repair/deltas and activation remain later work.
Live FUB validation remains user-deferred; actual customer-data processing,
commit/merge/push and a further runtime deployment are not included in this
implementation request. Existing D-015/O-012/O-013 prerequisites remain open.

**D-066 follow-up — integration, deployment and cleanup authorized (2026-09-11).**
After implementation and synthetic verification completed, the user requested
“commit, merge, push, cleanup and deploy 010f1”. This authorizes committing the
verified work, merging/publishing main, removing the merged worktree/branch and
disposable QA artifacts, and refreshing the existing shared-development API/Web
with the additive metadata migration and fresh compatibility inventory/preflight.
Preserve customer data, review bindings, verification evidence and release
recovery artifacts. Live FUB validation remains deferred; no real-source import,
activation or production-cluster deployment is implied. Results belong to
[SLICE_010f1_RELEASE.md](../tasks/SLICE_010f1_RELEASE.md).

### D-067 — Notes/tasks import planning: readable notes and explicit date-only timezone (2026-09-11)

After deploying 010f1, the user requested the next notes/tasks import plan:
**“yes, plan it.”** Planning and independent review of
[010f2](../specs/SLICE_010f2.md) and its [brief](../tasks/SLICE_010f2_IMPL.md)
are authorized. During planning the user accepted:

1. **“Use end of day in a confirmed timezone.”** A source task with a due date
   but no exact time may become due at the end of that day in a source timezone
   explicitly confirmed by the admin. No browser/server-zone or UTC default is
   implied. Freeze the zone and conversion in the preview/confirmed plan.
2. **“Readable text plus preserved original.”** FUB HTML notes may be imported
   as readable plain text while retaining the exact original for review and
   explicitly flagging unsupported content. This does not add native rich text,
   permit executing HTML or authorize silent truncation/content loss.

The concrete conversion profile, subject/reply policies, user/kind mappings,
identity/lifecycle/recovery, bounded read contracts and complete implementation
remain reviewable proposals in the draft. These two choices do not approve the
complete shared-contract changes or implementation. Live FUB validation remains
user-deferred; no source/customer processing, activation, commit/merge/push,
runtime change or production deployment is authorized by this planning request.

### D-068 — Reviewed 010f2 specification and implementation approved (2026-09-11)

After the complete notes/tasks plan and independent READY review were presented,
the user said **“Ok go ahead and implement 010f2.”** This accepts the reviewed
[specification](../specs/SLICE_010f2.md), [execution brief](../tasks/SLICE_010f2_IMPL.md),
their proposed policies and declared shared contracts. Implementation and the
required isolated synthetic verification are authorized.

Accepted scope includes the completed-parent activity child and its lifetime,
both-family source exhaustion, acknowledged subsets with at least one eligible
native unit at fresh confirmation, explicit user/type/time policies, readable
note conversion with original-source preservation, source-only replies/settings,
native limits, account-qualified identity and no overwrite/resurrection. The
bounded administrator review API, legacy-reader confirmation barrier, D-053
body-read amendment and activity-capable recovery contract are owned by 010f2.

Ordinary agent activation, live FUB validation/customer-data processing, and
commit/merge/push/deployment remain outside this implementation request. The
existing 010f1 shared-development release remains in service during implementation.

**D-068 follow-up — integration, cleanup and deployment authorized (2026-09-11).**
After implementation and synthetic verification completed, the user requested
“commit, merge, cleanup and push. also deploy.” This authorizes committing the
verified 010f2 work, merging/publishing main, removing the merged worktree/branch
and disposable QA resources, and refreshing the existing shared-development
API/Web with the additive activity migration and fresh observed compatibility
inventory/preflight. Preserve retained data, review bindings, verification
evidence and recovery artifacts. Live FUB/customer-data processing, activation
and production-cluster deployment remain separate, deferred scope. Results belong
to [SLICE_010f2_RELEASE.md](../tasks/SLICE_010f2_RELEASE.md).

### D-069 — Historical migration planning split (2026-09-12)

The user requested **“Ok lets plan it out”** for 010d and then selected
**“Split capture and timeline (Recommended)”**: 010d1 captures historical
events/calls/texts and reports coverage; 010d2 imports verified facts into a
bounded timeline. Planning and independent review of the
[010d ladder](../specs/SLICE_010d.md), detailed
[010d1 specification](../specs/SLICE_010d1.md) and
[execution brief](../tasks/SLICE_010d1_IMPL.md) are authorized.

This accepts sequencing, not the full proposed source, storage, lifecycle,
HTTP, release or timeline contracts. Those remain subject to completed review
and approval before implementation. No reduced migration fidelity, native
history/Today policy, email/media access or customer-data readiness is inferred.
Live FUB validation remains user-deferred. No source/customer processing,
activation, commit/merge/push, runtime change or deployment is authorized here.

### D-070 — Reviewed 010d1 specification and implementation approved (2026-09-12)

After the complete historical capture plan, independent READY review and
implementation approval boundary were presented, the user said **“Ok go for it.”**
This accepts the reviewed [010d1 specification](../specs/SLICE_010d1.md),
[execution brief](../tasks/SLICE_010d1_IMPL.md), proposed policies and declared
shared contracts. Implementation and required isolated synthetic verification
and bounded reviews are authorized.

Accepted scope includes the separate same-account history capture/profile,
immutable completed People parent, multiple independently confirmed attempts,
provisional fail-closed events/calls/text pagination with terminal distinct-ID
reconciliation, encrypted raw retention and metadata-only bounded review,
owned run/control reservations in the shared Org ledger, symmetric source
fencing, actor-bound receipts and independent capture release capability.

010d2 historical interpretation/timeline import and Today policy remain later
work. Live FUB validation remains user-deferred. This approval does not include
source/customer processing, email/media access, activation, commit/merge/push,
shared-development refresh or production deployment. Existing runtime and prior
completion-audit evidence must remain intact.

**D-070 follow-up — integration, deployment and cleanup authorized (2026-09-12).**
After the completed implementation, synthetic verification and two READY review
rounds were reported, the user requested **“commit, merge, push, cleanup and
deploy.”** This authorizes committing the verified 010d1 source and its audit/
planning/verification evidence, merging and publishing main, removing the merged
implementation worktree/branch, and refreshing the existing shared-development
API/Web with the additive history migration and freshly observed compatibility
inventory/preflight, including `fub-history-capture-v1` readiness.
Preserve existing data, review bindings, verification evidence, backup and recovery
artifacts. Live FUB/customer processing, native timeline import, activation and
production-cluster deployment remain outside this release. Actual results belong
to [SLICE_010d1_RELEASE.md](../tasks/SLICE_010d1_RELEASE.md).


### D-071 — 010d2 planning and metadata-first scope (2026-09-12)

After 010d1 release, the assistant recommended a focused specification for
separate imported historical facts, accurate source labels and paginated Person
review. The user replied **“Yes go for it.”** This authorizes drafting and
independent review of [SLICE_010d2.md](../specs/SLICE_010d2.md) and its
[execution brief](../tasks/SLICE_010d2_IMPL.md).

During planning, the user chose **“Metadata first (Recommended)”** when asked
whether the first timeline should expose event/call/text metadata only or also
readable captured message/note content. Metadata-only timeline/detail exposure
is accepted for 010d2. Existing 010f2 notes/tasks retain their approved readers;
this choice does not remove retained raw evidence or approve new body access.

The remaining proposed fact/time/identity/import/lifecycle, reader/fencing and
release contracts require completed review and implementation approval. Planning
does not authorize source/customer processing, implementation, commit/merge/push,
deployment, native-history promotion or activation. D-012/D-015 fidelity and
erasure, D-052 native maxima, D-064 review-only use and live FUB deferral remain
unchanged. The current release stays 010d1.

### D-072 — Reviewed 010d2 contracts and implementation approved (2026-09-12)

After the metadata-first specification, execution brief, READY independent
review and API/database contract approval boundary were presented, the user said
**“go ahead and do it.”** This accepts [SLICE_010d2.md](../specs/SLICE_010d2.md),
its [execution brief](../tasks/SLICE_010d2_IMPL.md), the reviewed policy and declared
shared contracts, including both planning corrections. Implementation, concrete
contract detail within this scope, isolated synthetic verification and bounded
implementation reviews are authorized.

Accepted scope includes three separate imported external fact types, metadata-only
exposure, retained-capture interpretation, stable identities and held variants,
one immutable parent history plan with resumable attempts, bounded paged review,
consistent page revisions, old-reader fencing and independent timeline readiness.
Native business facts, Today behavior and the admin review hold remain unchanged.

This approval does not authorize live FUB/customer processing, body/media access,
activation, commit/merge/push or shared-development/production deployment. Those
remain separate scopes; existing runtime and release evidence stay intact.

**Release follow-up — 2026-09-12:** After isolated implementation verification,
the user explicitly requested **“commit, merge, push, cleanup and deploy.”**
This authorizes Git integration, publication, owned worktree/branch cleanup and
deployment of 010d2 to the existing Mac-hosted shared-development API/Web using
the compatibility runbook, additive migration, preserved recovery artifacts and
release verification. It does not authorize live FUB/customer processing,
body/media access, workspace activation or a production-cluster deployment.

### D-073 — Offline native mobile foundation and seven-day access (2026-09-12)

The user established that field agents must work through unreliable cellular
connections and complete outages without discarding saved information, proposed
local SQLite, and endorsed the resulting offline-first direction. After the
[first mobile plan](../plans/MOBILE_OFFLINE_FIRST.md) recommended native tooling,
an owned sync contract, a backend foundation and parallel iOS/Android/migration
worktrees, the user replied **“Ok...sounds good”**. This accepts that planning and
preparation sequence, using Today-related People, assigned People and explicitly
saved records as the initial offline selection. Selection governs downloaded
availability, not Organization-wide Person visibility.

The user then explicitly chose **“7 days: supports extended outages with a
bounded access window (Recommended)”** for how long downloaded CRM data remains
usable after the last successful online authorization. On expiry, require online
reauthorization to reopen CRM data and preserve unsynced work in protected
storage. This is an access window, not a deletion deadline, receipt-retention
period, permission to process revoked work or promise of immediate remote wipe.

Prepare the [Mobile 001 specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md),
[execution briefs](../tasks/MOBILE_001_IMPL.md) and
[toolchain setup](../tasks/MOBILE_NATIVE_TOOLCHAIN_SETUP.md). Swift/SwiftUI and
Kotlin/Jetpack Compose remain D-001; SQLite is on-device persistence while the
existing Rust commands and PostgreSQL remain authoritative for accepted shared
state. Committed local saves must survive ordinary app restart; this cannot
recover never-synced data from a destroyed device or removed app storage.

The new HTTP/persistence contracts, detailed conflict/local-data lifecycle and
client compatibility policies remain proposals pending their concrete review
and approval. This planning approval does not silently amend Slices 015/016,
authorize real customer data, change the migration review hold or authorize a
release. Existing D-015/O-012/O-013 and production readiness gates still apply.

### D-074 — Reviewed Mobile 001 implementation and parallel work approved (2026-09-12)

After the reviewed Mobile 001 contracts, execution briefs and completed native
tooling setup were presented for implementation approval, the user replied
**“This is approved”** and confirmed that iOS, Android and migration should run
in parallel. The user also requested planning the next migration task before
starting those implementation lanes.

This accepts the reviewed [Mobile 001 specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md)
and [briefs](../tasks/MOBILE_001_IMPL.md), including shared HTTP/persistence
contracts, transaction-compatible commands, atomic operation receipts, task and
Person revisions, bounded reconciliation, encrypted local storage, conflict and
sign-out/revocation behavior. The two corrections in the
[planning review](../tasks/MOBILE_001_REVIEW.md) are included. Implementation and
isolated synthetic backend/iOS/Android verification are authorized. Exact schema,
locking, DTO fixtures and compatible native library versions are implementation
details to freeze within this scope, not a repeat approval request.

Prepare a bounded migration brief and coordinated ownership/check plan before
launch. Keep at most three short-lived implementation worktrees. The shared
mobile backend is a prerequisite for native API integration; migration need not
wait on that backend when file/schema ownership is disjoint. A newly proposed
migration contract is not accepted merely by approving Mobile 001.

This does not authorize live FUB/customer-data processing, workspace activation,
independent app distribution or a new deployment. Existing customer readiness
gates remain. Record implementation and verification evidence separately from
this approval and the already verified developer tooling.

### D-075 — Reviewed 010e1 and coordinated implementation approved (2026-09-12)

After the complete 010e1 specification, execution brief, READY review and parallel
launch sequence were presented, the user replied **“010e1 is approved...”**.
This accepts [SLICE_010e1.md](../specs/SLICE_010e1.md), its
[brief](../tasks/SLICE_010e1_IMPL.md) and corrected publication/pagination contract.
Implementation and isolated synthetic verification are authorized. D-074 already
approves Mobile 001 backend and both native apps; neither scope needs repeat
implementation approval.

Execute the [coordinated launch](../plans/MOBILE_MIGRATION_PARALLEL_LAUNCH.md):
mobile backend and 010e1 concurrently, then iOS and Android after backend
integration, with at most three implementation worktrees. Local checkpoints and
integration are used to supply a consistent approved base to dependent lanes.
Publishing Git branches, refreshing shared development, app distribution and
production deployment remain subsequent release scope. Live FUB/customer-data
processing, delta application/repair, deletion inference and workspace activation
remain outside this implementation.


**Release follow-up — 2026-09-12:** After the completed milestone and the proposal
 to merge/push and deploy its backend/Web to shared development, the user replied
**“yes release completed milestone first.”** This authorizes publishing the
integrated Mobile 001 (including native source) and 010e1 commits, merging to main,
owned branch cleanup, and deploying the compatible backend/Web and additive schema
to the existing Mac-hosted shared-development environment. Reuse attributed
implementation evidence and execute the release-specific preservation, artifact,
compatibility, HTTP/browser and recovery checks. Native app distribution, physical
phone/cellular testing, live FUB/customer processing, delta application/repair,
activation and production-cluster deployment remain separate. Actual results
belong to [the milestone release record](../tasks/MOBILE_001_010e1_RELEASE.md).
