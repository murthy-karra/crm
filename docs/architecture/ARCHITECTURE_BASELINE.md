# Architecture Baseline

Reviewed 2026-09-11 against accepted decisions through D-063 and current source.
This is a derived system map, not another decision log. The
[decision log](../decisions/DECISION_LOG.md) and [AGENTS.md](../../AGENTS.md)
win on conflict. Observed runtime state and release evidence belong in
[PROJECT_STATE.md](../plans/PROJECT_STATE.md).

## Shape

One modular Rust application: Axum, Tokio and SQLx. PostgreSQL is authoritative.
Vue 3/TypeScript Web, the planned native SwiftUI and Jetpack Compose clients,
and the AI Operator use the same typed application commands and queries
(D-001, D-002, D-008). Native applications are not yet implemented; narrow-screen
Web layouts are still the Web application.

| Component | Responsibility and source | Current development shape | Production direction |
|---|---|---|---|
| API/admin | [crm-api](../../backend/crates/crm-api): HTTP/session boundary, runtime composition, migration/admin binaries | Local Mac process; migrations/admin commands invoked explicitly | Rust workloads on OVH/Talos/Kubernetes; packaging remains future work |
| Application domains | [crm-app](../../backend/crates/crm-app): authorization, commands, persistence, integrations | Linked into API/admin binaries; workers start from API composition | Keep modular; separate workloads only at a real boundary |
| AI Operator | [crm-operator](../../backend/crates/crm-operator): provider-neutral inference and typed tools | In-process, reaches application through ToolBackend | Same trust boundary; no privileged data path (D-028, D-034) |
| Web | [web](../../web): conventional UI and Operator | Vite dev server or built bundle through dev-web-prod | Vue Web; Vite preview is a development release mechanism |
| PostgreSQL | Canonical records, history, read models, sessions and integration state | Loopback-only Docker; application/migrator role split | CloudNativePG; topology/recovery evidence belongs to production work |
| Centrifugo | Realtime invalidation delivery | Loopback-only Docker | Centrifugo OSS; PostgreSQL stays authoritative |
| Inbound mail | [email-worker](../../infra/email-worker): Email Routing to authenticated API relay | Deployed external Worker; size handling under D-056 | D-039 relay boundary; no second mutation path |
| Calling | [telephony](../../infra/telephony): LiveKit/SIP/Telnyx | Separate EC2 development host (D-055); Egress dormant | Self-hosted LiveKit/Telnyx; production host choice at deployment under D-055 |

Current domains include People, inquiries/history, stages and assignment, tags,
notes, tasks, typed custom fields, saved filters/Today rules, administration,
intake/correspondence, calling and FUB assessment. The Operator has both read
tools and scoped mutation tools; it is no longer read-only.

## Trust boundaries

- ZITADEL authenticates in production; local development uses username/password
  behind the same identity/session abstraction. The application owns membership,
  role and resource authorization (D-003, D-016).
- Organization scopes every tenant query/mutation, including cross-Organization
  denial tests. Person visibility is Organization-wide behind
  PersonVisibilityScope; assignment controls responsibility (D-004/005).
- Global identities/memberships and platform administration are distinct from
  tenant CRM data. Platform admins are not tenant-data superusers; support and
  impersonation access remain open designs (D-021, D-026/027).
- The Operator receives trusted server context and approved tools. The application
  enforces command-specific risk, confirmation and receipt/Undo rules (D-008/009,
  D-057). Customer content never supplies authorization or privileged instructions.

## Persistence

Use hybrid persistence (D-007, D-015), not a generic event store:

- Typed append-only facts preserve inquiry receipt, routing, assignment, stage
  changes and call outcomes. Corrections add facts; history holds identifiers
  rather than customer content.
- Ordinary relational CRUD holds People/contact information, tags, notes, tasks
  and custom fields. Notes are erasable plaintext CRUD under D-053; not all free
  text already has per-Person encryption.
- Purpose-built read models serve Today/Person/timeline reads. Trigger-maintained
  activity columns are an accepted mechanism (D-052).
- Raw intake/correspondence payloads and FUB evidence/credentials currently use
  encrypted PostgreSQL storage. Key hierarchy, erasure and retention still need
  the work tracked by O-012/O-013/O-015.
- D-062 accepts future placement of email bodies/raw messages and embedded
  attachments outside PostgreSQL; metadata/references remain in PostgreSQL.
  Relocation, provider, durability, body search and lifecycle are not implemented
  or selected by that decision. It does not relocate 010b's other raw evidence.

D-015 requires an erasure runbook before real design-partner data enters the
system. Restoring keys, content and database references consistently requires
evidence, not merely an archive file; see [readiness](../plans/PRODUCTION_READINESS.md).

## Realtime and notifications

Centrifugo delivers IDs-only invalidations on server-authorized Organization
channels (D-023). Publishing is best-effort after commit; clients refetch on
reconnect, focus and intervals. An invalidation is not durable business work
or proof of delivery.

D-023 defers outbox/history/per-user-channel changes until stronger event
semantics or multi-pod deployment requires review. That is a review trigger,
not an instruction to install a broker. APNs/FCM and platform-native call
handling remain part of future native-client work.

## Integration and background work

[API composition](../../backend/crates/crm-api/src/lib.rs) starts intake
extraction, telephony sweeping and FUB assessment workers when their configured
dependencies are present. Each domain owns its behavior; no general worker
platform is implemented. FUB assessment already has durable claims, fenced
settlement and bounded retries. Its
[reader pacing](../../backend/crates/crm-app/src/domain/migration/reader.rs)
is process-local: several API copies do not create one shared rate limiter.
Credential validation must also be considered when coordinating source calls.

010a assesses bounded source access; it is not a complete download or import.
[010b](../specs/SLICE_010b.md) is implemented and synthetically verified under D-063:
resumable capture/preview of People,
users, stages, custom fields, notes and tasks. Other families remain explicit
future work (D-060/061). Its unmerged implementation adds shared assessment/snapshot
source permits within one process, durable storage reservations, explicit budget
increases and DB-only preview recovery. [Verification](../tasks/SLICE_010b_VERIFICATION.md)
records passing full gates; runtime deployment and live FUB validation remain
separate. Fidelity, idempotency and reconciliation stay mandatory.

## Network and telephony

Development public HTTP uses a dashboard-managed Cloudflare Tunnel to local
Web/API/realtime routes (D-024/025). The tracked development ingress example
is documentation, not the live routing authority. Production D-018 specifies
2+ in-cluster cloudflared replicas, OpenBao-sourced credentials, Cilium Gateway
API and HTTPRoute resources; describing it does not mean it is deployed.

LiveKit media bypasses cloudflared. D-055's development host does not enable
recording: consent, keys and lifecycle remain open. Internal database/storage
connectivity belongs to the private production network; the HTTP tunnel is
not a database replication or storage fabric.

## Observability and security

OpenTelemetry to OpenObserve and OpenBao as production secret authority are
accepted directions. Development currently uses console tracing and gitignored
.env; D-016 defers CI and OpenObserve deployment. The configuration-name inventory
is [.env.example](../../.env.example), with empty credential values.

Keep safe trace/request/correlation and Organization/resource context; never log
secrets or unnecessary customer content. Production objectives, alerts, named
owners, recovery targets and cost dashboards are not established by this map.
They are proposed in the readiness plan.

## Contracts and capacity

Shared HTTP, realtime, Operator and persistence contracts live in owning slice
specs and code; there is no separate contracts directory. Changes follow
AGENTS §11 and explicit supersession pointers. Native-client compatibility and
online schema evolution are proposed in [FOUNDATIONS.md](../plans/FOUNDATIONS.md).

D-050's envelope remains 25,000 People and 50 members per Organization, five
concurrent Today loads and one active browser tab per agent. Aggregate
100,000-agent capacity is unproven. Its relative checks, production-shaped
capacity baseline and two-review-round budget remain unchanged.

## Working from this map

1. [Development guide](../../README.md#development): actual prerequisites,
   configuration and scripts. dev-bootstrap is destructive; never use it as
   a routine release command against shared data.
2. [Current state](../plans/PROJECT_STATE.md): source/release evidence, active
   work, live residuals and next action.
3. [Foundations](../plans/FOUNDATIONS.md) and
   [readiness](../plans/PRODUCTION_READINESS.md): proposed work, triggers,
   ownership roles and required proof; these are not accepted ADRs.
4. [Release](../prompts/07-deploy.md), [monitoring](../prompts/08-monitor.md)
   and [handoff](../prompts/09-handoff-and-improve.md) guidance.

Update this map when a slice changes a deployable, trust boundary, storage
location or operational entry point. Keep policy in decisions/specs and
historical progress in evidence records.
