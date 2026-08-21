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

Shared HTTP, realtime, Operator-tool, and persistence contracts live under
`contracts/` once created and never change silently (`AGENTS.md` §11).
