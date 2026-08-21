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

The event-sourced aggregate research in
`docs/research/event-sourced-crm-aggregates-and-events.md` informs the
design of the immutable-history areas (event envelope metadata, two clocks,
fix-forward corrections, crypto-shredding for erasure) but is not accepted
architecture; its adoption scope is open decision O-005.

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
the start (`AGENTS.md` §9). The secrets-manager product is open (O-001);
until resolved, configuration is process-injected.

## Contracts

Shared HTTP, realtime, Operator-tool, and persistence contracts live under
`contracts/` once created and never change silently (`AGENTS.md` §11).
