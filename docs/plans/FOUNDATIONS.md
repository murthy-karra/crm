# Foundations and operational handoff

**Status: DRAFT PROPOSALS — documentation work authorized 2026-09-11.**
The user accepted the immediate work to refresh the system map, separate current
state from history, draft foundations proposals, and define first-customer and
production readiness. This authorizes these documents; it does not accept every
new policy below, change application contracts, or authorize infrastructure work.

Authority: [decision log](../decisions/DECISION_LOG.md),
[AGENTS.md](../../AGENTS.md), [system map](../architecture/ARCHITECTURE_BASELINE.md).
Current status lives only in [PROJECT_STATE.md](PROJECT_STATE.md). Recovery,
security and rollout evidence is tracked in [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md).

## Outcome and scope

A new engineer can locate the running revision, understand data ownership,
diagnose a failed import, release a change and restore an isolated environment
using the repository. This is an acceptance exercise to execute later, not a
claim that today's documentation already proves production readiness.

Preserve D-002's modular application and D-050's operating envelope. The earlier
100,000-agent discussion is planning context, not a benchmark target, service
commitment or requirement to build sharding, a message broker or multiple regions.
Keep existing development workflows under D-016 until their replacement is owned
by a specified production task. No code, schema, provider, retention policy or
configuration changes are part of this documentation milestone.

## F-01 — Organization portability (proposed)

**Existing contract:** D-004 isolates tenant queries/mutations. D-021 separates
platform administration from tenant access. These do not yet define a tenant
export, relocation or selective-restore contract.

**Proposed rule:** new tenant data designs identify ownership for relational rows,
stored content, jobs and derived data, and avoid coupling customer-facing IDs to
a database/server location. Preserve the ability to move one Organization with
its stable IDs. Do not introduce cross-Organization business dependencies without
an explicit decision. Shared identities and memberships are legitimate global
dependencies and must be inventoried, not copied as if exclusively tenant-owned.

**Why:** later isolation of a large customer or a group of customers should not
require changing every domain model or client identifier.

**Affected components and compatibility:** schema relationships, object references,
jobs, caches/search, authentication/memberships and future tenant routing. This
proposal changes none of those contracts now. Actual relocation must specify
quiescing or delta capture, job fencing, identity handling, keys, reconciliation,
routing cutover and recovery. A logical object identifier is not authorization;
downloads still require current tenant/resource checks. Do not persist expiring
signed download URLs as the canonical object identity.

**Trigger and evidence:** apply an ownership review when specifying new storage;
design relocation before the first Organization moves between clusters. Prove a
synthetic two-Organization transfer with unchanged public IDs, complete data/key
inventory, no transferred unrelated data, and no duplicate execution of jobs.
The required downtime and failure recovery are decisions in that future task.
The application/data owner writes the owning spec/ADR; named owner is unassigned.

## F-02 — Durable work and explicit runtime roles (proposed)

**Existing contract:** domain workers start from the API process. FUB assessment
uses durable claims, fenced settlement and bounded retries, while source pacing
is process-local. See [composition](../../backend/crates/crm-api/src/lib.rs),
[worker](../../backend/crates/crm-app/src/domain/migration/worker.rs) and
[reader](../../backend/crates/crm-app/src/domain/migration/reader.rs). Other workers
need their own inspection; FUB behavior is not proof that they share it.

**Proposed rule:** long-running work records tenant/initiator, authorization
requirements, progress, retry eligibility, cancellation and terminal result in
durable state. Define claim ownership and fence stale results. Retry the same
logical action safely; when a provider outcome is uncertain, reconcile it or
expose that uncertainty rather than promising exactly-once external effects.
Do not hold database transactions/connections while waiting on external I/O.

**Initial deployment recommendation:** keep one active FUB executor and explicitly
separate API and worker runtime roles before multiple API instances. All FUB
calls, including credential validation, must participate in the selected pacing
mechanism. A second inactive/failover executor is not concurrent permission to
call the provider. Scaling execution later still requires coordinated limits,
per-Organization fairness and bounded work alongside interactive requests.

**Affected contracts:** worker startup/configuration, job persistence, progress
APIs and credential-validation behavior. Routing synchronous checks through a
worker may require a contract change; its owning task must compare alternatives
and specify latency, errors and compatibility. No new mode flag, queue API or
worker service is defined or implemented here. A PostgreSQL-backed domain queue
remains an option; no general queue framework is selected.

**Trigger and evidence:** review the runtime/worker inventory before more than one
API instance; specify strong delivery before adding consequential background
actions. Prove restart at a checkpoint, duplicate claim/retry, stale lease,
credential/permission revocation and provider throttling with synthetic data.
An affected domain owner owns its spec and migration; deployment owns process
roles and dependency shutdown. Named owners remain unassigned.

D-023's best-effort realtime invalidations remain valid. Durable business work
must not rely on them. Multi-pod rollout triggers review of realtime recovery;
it does not automatically require an outbox for every notification.

## F-03 — Client, schema and release compatibility (proposed)

**Existing contract:** AGENTS §11 prevents silent changes. Some development
releases explicitly require API and Web to ship together. Native clients have
not shipped and no support-window policy exists.

**Proposed rule:** releases identify supported client versions and schema states.
Prefer additive changes; deploy compatible readers/writers, perform bounded
resumable backfills, then remove old representations only when their users have
been retired. Database migrations run under explicit ownership, not concurrently
from every API replica. Record lock/transaction behavior and failure recovery.

**Why and impact:** older browser bundles and installed mobile apps may outlive a
backend deployment. Restarting servers must not invalidate ongoing work or
silently change a command's meaning. Application rollback is not database undo;
record when forward recovery is safer than rollback.

**Affected contracts/specs:** versioned HTTP/realtime payloads, Operator tools,
database migrations, native release policies and deployment artifacts. These
remain unchanged today. The production spec must define artifact/configuration
provenance and rollback; the first native-client spec must define a support
window and a safe upgrade path. No arbitrary version count or duration is adopted.

**Trigger and evidence:** before production customer releases, then before the
first independently released native client. Demonstrate a supported older client
against the new API, staged schema/code compatibility and an interrupted backfill
resuming in an isolated environment. Product owns the support-window decision;
application and release owners implement the checks. Named owners are unassigned.

## Work sequence and proof

| Step | Outcome / trigger | Owned surface | Proof / remaining gate |
|---|---|---|---|
| Documentation milestone (this task) | Current entry point, preserved history, reviewable proposals/readiness | Coordinator: overview, README/index, project state/history, this plan; helper: readiness file only | Relative links/anchors and whitespace; verify history preserved and no runtime files changed |
| 010b implementation | Completed under D-063; shared-development release authorized and in progress | 010b spec/brief, source-profile evidence and implementation | Both bounded reviews and all synthetic/full gates passed; see the implementation verification record |
| First customer-data readiness | Before any real customer payload/evidence enters the system | Owning privacy/data/recovery specs and runbook under readiness C gates | Settle applicable open decisions; isolated restore/erasure proof; source authorization alone does not close readiness |
| Production-readiness slice | Before production deployment or multi-instance operation | Explicitly assigned deployment, identity/secrets, observability, runtime-role and migration surfaces | Readiness P evidence, agreed service/recovery targets; no production implementation in this milestone |
| Later capabilities | Before native release or tenant relocation, respectively | Client compatibility or relocation spec | Accept detailed contracts and demonstrate the corresponding F-01/F-03 exercise |

The coordinator's prior 010b draft review identified four issues, now addressed
in the approved specification and tracked in the [review record](../tasks/SLICE_010b_REVIEW.md):

1. Distinguish note list/detail representations from genuine source drift; do
   not treat normal enrichment as changed raw bytes for the same representation.
2. Separate authorization to read retained captures from eligibility for new
   upstream work after credential changes; preserve current Organization/admin
   checks and the accepted evidence visibility policy.
3. Specify what the logical byte budgets count and how an authorized operator
   can recover from exhaustion. The accepted synthetic-development 2 GiB/run and 4 GiB/Organization values
   are not accepted customer quotas or permission to delete evidence.
4. Define when item/family denial produces an explicit gap versus a paused run;
   reconcile that with credential and permission failure handling.

These coordinator findings have accepted corrections under D-063; the review record
tracks independent readiness and user acceptance of the 010b contracts. The
separate F-01/F-02/F-03 foundations remain proposals. Core-first sequencing
remains accepted under D-061;
email bulk placement under D-062 does not change its six-family scope.

## Handoff exercise and maintenance

Use an engineer who did not perform the setup, a clean checkout and an isolated
synthetic environment. Record the revision, environment, steps attempted and
actual outcomes. The engineer should be able to:

1. Bootstrap using README prerequisites and identify safe versus destructive
   commands before starting services or seeding data.
2. Locate the deployed artifact/configuration evidence and trace an example
   failed import using safe identifiers, then use the documented recovery path.
3. Release a tested change and follow its rollback/forward-recovery procedure.
4. Restore an isolated backup with keys/content and reconcile expected data.

This exercise is pending. Application tests do not replace release/recovery
evidence; documentation checks do not prove any of these operational actions.
Record gaps in the owning checklist instead of giving undocumented instructions.

Keep current state short and archive completed history. Update the system map
when boundaries or operating entry points change. Each accepted new foundation
must be recorded with its acceptance source in the decision log and amended
owning specs; do not treat this proposal file as an accepted ADR. Follow D-050
for review/testing effort and avoid adding gates to unrelated feature slices.
