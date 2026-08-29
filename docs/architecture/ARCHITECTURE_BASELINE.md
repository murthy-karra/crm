# Architecture Baseline

This document summarizes the accepted architecture starting point. It is
derived from the accepted decisions in `docs/decisions/DECISION_LOG.md` and
the engineering policy in `AGENTS.md`. Where this summary and those documents
disagree, they win.

## Shape

One modular Rust application (Axum, Tokio, SQLx) with a small number of
deployable workloads. PostgreSQL is the authoritative store. Vue 3 web,
native SwiftUI and Jetpack Compose clients, and the AI Operator are all
clients of the same typed application command/query layer. New services are
added only for a real scaling, failure, security, deployment, or technology
boundary (D-002).

## Trust boundaries

- ZITADEL authenticates; the application authorizes (D-003).
  (Production posture; development uses local username/password behind
  the same session/identity abstraction, D-016 §3.)
- Organization is the tenant boundary; every tenant-owned query and mutation
  enforces it, and tests must attempt cross-Organization access (D-004).
- Person visibility is Organization-wide behind a server-side
  `PersonVisibilityScope` abstraction (D-005).
- The Operator receives server-owned trusted context, calls approved typed
  tools only, and never receives SQL access, table writes, or
  model-generated authorization (D-008, D-009).
- Untrusted content (emails, messages, notes, imports, transcripts, web
  content) is never interpreted as privileged instructions.

## Persistence

Hybrid (D-007):

- Immutable durable history for: Inquiry receipt, source attribution,
  assignment/reassignment, routing decisions, stage changes, recording
  consent, completed calls, phone-number ownership/port state, future Deal
  lifecycle.
- Relational CRUD for: names, contact methods, tags, notes, tasks, custom
  fields, notification preferences, routing configuration, integration
  connections, UI configuration.
- Purpose-built read models for user-facing screens (Person summary, Today,
  timeline, inbox, call history); PostgreSQL remains authoritative.

D-015 settles how the event-sourced research applies: history tables carry
a standard envelope (actor, on-whose-behalf, origin, occurred_at/
recorded_at, correlation IDs), are database-enforced append-only with
fix-forward corrections, and are PII-free — they hold IDs whose only
correlation to personal data is the CRUD Person table. Inherently personal
content (raw lead payloads, migration exports, recordings) lives in
encrypted deletable blobs referenced by pointer + hash. Erasure deletes the
correlation row and blobs and writes a redaction event; history keeps
orphaned IDs. The research document's transaction/compliance aggregates are
deferred. The first history-bearing slice uses four typed fact tables
(InquiryReceived, RoutingDecision, AssignmentChanged, StageChanged), not a
generic event store.

## Realtime and notifications

Centrifugo OSS delivers realtime events; it is delivery, not truth (D-011).
Reconnecting clients recover authoritative state from the application.
Backgrounded mobile clients receive important notifications via
application-owned APNs/FCM integration. Incoming calls use platform-native
handling (PushKit/CallKit; Android Telecom).

## Telephony

Self-hosted LiveKit with Telnyx SIP. Media never routes through cloudflared
tunnels. Recordings are encrypted objects with per-recording data-encryption
keys and crypto-shred capability, subject to legal hold. Consent policy is
open (O-002).

## Observability and security

OpenTelemetry instrumentation to OpenObserve. Propagate trace/request/
correlation IDs and safe actor/Organization/domain IDs. Never log secrets,
tokens, or unnecessary customer content. Build toward SOC 2 readiness from
the start (`AGENTS.md` §9). Secrets are a local gitignored `.env` file in
development (D-013) and OpenBao in production (D-014); OpenBao integration
belongs to a future production-deployment slice.

## Contracts

Shared HTTP, realtime, Operator-tool, and persistence contracts live in
the per-slice specifications (`docs/specs/`) and the code itself — a
separate `contracts/` directory was planned but never created and is not
used. Contracts never change silently (`AGENTS.md` §11); superseded spec
sections carry explicit amendment pointers.

## Amendments since baseline (recorded 2026-08-29)

The prose above is frozen at D-015 (2026-08-20). Materially
architecture-shaped decisions accepted since, one line each — the
decision log is authoritative:

- D-016: all development is local (MacBook + Docker + Cloudflare tunnel);
  dev auth is local username/password behind the ZITADEL-shaped
  abstraction.
- D-017/D-018: web stack (Tailwind, PrimeVue unstyled, TanStack
  Table/Query) and production ingress (cloudflared → Cilium Gateway API).
- D-019: Person stages are an Organization-scoped seeded table, not an
  enum.
- D-023: realtime model — one org channel, server-side subscriptions,
  ids-only invalidation events, recovery by refetch.
- D-028/D-034: the AI Operator is an in-process crate (`crm-operator`)
  compiled into `crm-api`, reaching data only through the `ToolBackend`
  seam; crate-boundary fences are enforced in `./scripts/check`.
- D-039: a second deployable exists — the `infra/email-worker` Cloudflare
  Email Worker relaying inbound mail to the API.
- D-042: correspondence capture v1 (CC/BCC + address book, encrypted
  bodies pending O-012 key hierarchy).
- D-043: smart lists are first-class and the filter model is the Today
  configuration language (Slice 011 ladder).
