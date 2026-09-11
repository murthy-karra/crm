# First-customer and production readiness

Updated: 2026-09-11. **Planning checklist; no production release authorized.**
This records existing gates and proposes the evidence needed to close the
remaining work. It does not approve implementation, customer access, new
contracts, retention policy or service commitments. Accepted decisions in the
[decision log](../decisions/DECISION_LOG.md) remain authoritative.

## Current evidence

| Area | Verified state / remaining gap |
|---|---|
| Shared development | 010b deployed from `89471f0b7bccc3e91c5f6c94aacecf7e7bd2216d`; API/Web/auth/tunnel smoke checks passed. This is the Mac-hosted runtime, not a production-cluster deployment. |
| Source integration | Synthetic checks passed. Live FUB validation is user-deferred; registered system identification is unset; no live connection or assessment was created by the release. |
| Backup | A private custom-format PostgreSQL dump was created and its catalog read. **No restore exercise was performed.** Its temporary local location is not a durable backup policy. |
| Migration | 010a assesses access only. 010b core snapshot/preview is implemented, synthetically verified and deployed in shared development under D-063. Business import, remaining capture families and cutover are later work. |
| Privacy | D-015 requires an erasure runbook before first real design-partner data. O-012/O-013 remain open; disconnecting FUB removes a credential, not captured evidence or backups. |
| Production services | ZITADEL integration, OpenBao integration, CI and OpenObserve deployment remain deferred under D-014/D-016. Development uses local authentication, `.env`, local checks and console logs. |
| Capacity | D-050 defines 25,000 People, 50 members and five concurrent Today loads per Organization. A production-shaped capacity baseline is still required; planning estimates for larger populations are not measured capacity. |

Evidence: [010b release](../tasks/SLICE_010b_RELEASE.md),
[010a verification](../tasks/SLICE_010a_VERIFICATION.md),
[010b verification](../tasks/SLICE_010b_VERIFICATION.md),
[current project state](PROJECT_STATE.md). Historical release evidence does not
prove that a service is healthy at a later date.

## Separate readiness gates

| Gate | Trigger and required scope | Current status |
|---|---|---|
| V — Live source validation | User resumes the deferred 010a validation against an authorized account with agreed data scope. Use synthetic source records while C remains open. Confirm access route, registered identification, exact GET probe profile and evidence handling. | **PENDING — user-deferred.** Record results in 010a verification; do not silently read a customer book. |
| C — First real customer data | Before any real design-partner payload or source evidence enters any environment, including assessment/snapshot storage. D-015's runbook prerequisite applies before entry, not only before CRM record creation. | **OPEN — existing privacy gates.** Synthetic source records remain the test input while these gates are open. |
| M — Business import / cutover | Before writing imported People or other business records, and separately before making this CRM operationally authoritative for a customer. | **NOT READY — later rungs.** D-059 requires a new, empty destination; D-061 does not approve core-only cutover. |
| P — Production deployment | Before deploying the application to the production cluster or making production service commitments. Requires an approved production slice and a concrete release plan. | **NOT READY — specification and implementation pending.** A production Web build on the development machine does not close this gate. |

V can close using synthetic data while C remains open. Source read-only access
can still persist personal data, so V does not bypass C when a real customer
book is involved. M requires C and its own import/reconciliation/recovery
acceptance; production customer service additionally requires P.

## Ownership and status rules

The user remains the decision authority for open product, policy and major
architecture choices. Each implementation/release brief assigns its execution
owner under the existing workflow. **No standing platform/SRE, privacy or
incident-response owner is currently assigned by this document.** Roles below
are proposed responsibilities; each needs a named person and backup before
its operational gate closes. An agent task assignment is not ongoing on-call
coverage.

Use `OPEN`, `IN PROGRESS`, `VERIFIED` or `NOT APPLICABLE — reason` for each item.
To mark `VERIFIED`, link a dated evidence record with revision, environment,
commands, outcome, limitations and named reviewer. Existing decision gates
cannot be waived by changing a checkbox.

## First-customer checklist

All items below are **OPEN** unless current evidence above says otherwise.
Roles are **unassigned**. “Required” identifies an existing decision; other
items describe proposed work to scope in the relevant slice.

| ID / trigger | Work and evidence required | Proposed accountable role |
|---|---|---|
| C1 — Before C; required D-015 | Write the erasure runbook for CRUD rows, unresolved payloads, migration credentials/evidence, blobs, read models, search, logs and backup expiry. Inventory what exists and explicitly mark absent systems. Include failure/retry handling and evidence that reconstruction does not resurrect erased content. | Privacy/product owner with backend owner |
| C2 — Before external customer consumer data; O-012/O-013 | Resolve the key hierarchy and erasure/product policy, specify required migrations and implement their accepted scope. Verify deletion propagation and recovery against synthetic data; do not claim the current single development key provides per-Person crypto-shred. | Privacy/product owner with security/backend owner |
| C3 — Before C | Record authorized source/account/environment, allowed data and audience, retention/deletion handling, integration permissions and required customer agreements. Capture no credentials or customer content in the readiness record. | Customer onboarding owner with privacy owner |
| C4 — Before C; proposed restore proof | Restore a synthetic database backup into an isolated disposable environment using the procedure below. Retain sanitized results; do not restore over shared development or production to demonstrate recovery. | Database/platform owner |
| C5 — Before M; required D-012/D-059 | Approve and verify the relevant import contracts: transactional destination eligibility, source-to-destination identity, mappings, idempotency, rerun/recovery, reconciliation and explicit unsupported/inaccessible data. Approval of a snapshot is not approval to import it. | Migration/backend owner with product owner |
| C6 — Before cutover; required D-012 | Approve delta/reconciliation, final coverage, cutover steps and recovery/rollback policy for that customer. Record unresolved gaps and their accepted disposition; a completed core snapshot is not complete migration fidelity. | Migration owner with customer onboarding owner |

## First restore exercise

Prepare this as a bounded execution brief using synthetic data. It is a next
task, not a claim of completed testing or authorization to alter a live system.

1. Identify a source revision/schema, backup artifact and isolated target.
   Record artifact integrity and encryption/key prerequisites without values.
   Include representative tenant data, encrypted raw content and migration
   state; create a dedicated synthetic backup if the available archive's data
   classification is uncertain.
2. Start timing at the declared recovery trigger. Restore roles/schema/data
   with documented commands; restore required keys through the appropriate
   environment procedure. Keep external calls, workers and inbound traffic
   disabled until their recovery behavior is deliberately exercised.
3. Boot the matching application against the isolated target. Check schema,
   representative record counts, authorized decrypted reads, tenant isolation,
   and failed/paused job recovery without contacting FUB or sending messages.
4. Demonstrate how an erasure recorded after the selected backup is reapplied
   before serving recovered data. If the mechanism is not yet implemented,
   record that failure to meet C1/C2; do not label the full recovery path ready.
5. Record elapsed restoration and verification time, recovered data boundary,
   actual missing data and manual steps. Distinguish restoring the database
   from restoring usable service. Sanitize evidence, remove the disposable
   environment and handle artifacts under the approved retention procedure.

This validates only the exercised development backup path. Production CNPG
base-backup/WAL recovery and key recovery require a production-shaped exercise.
When D-062 bulk storage ships, include versioned object availability and
database-reference/key consistency; a database-only restore cannot prove that.

## Production-slice checklist

All items are **OPEN**; proposed accountable roles are **unassigned**. Scope
the initial supported service and D-050 envelope, not a speculative
100,000-agent topology. This checklist creates no new service dependency.

| ID / trigger | Work and evidence required | Proposed accountable role |
|---|---|---|
| P1 — Before P | Specify and reproduce OVH/Talos/Kubernetes/Cilium/CNPG placement, storage and failure behavior. Implement D-018 ingress with 2+ cloudflared replicas through Cilium Gateway. Verify authentication and realtime recovery during a planned instance failure; LiveKit media bypasses tunnels. | Platform owner |
| P2 — Before P | Integrate ZITADEL through the existing identity/session boundary. Verify Organization authorization, revocation and named administrative access; document recovery access and decide the production admin ingress policy. | Identity/security owner |
| P3 — Before P | Integrate OpenBao under D-014. Inventory secrets and encryption keys, document access/rotation/revocation and independently recover the needed key material. Verify that missing configuration fails safely and no secret appears in artifacts or logs. | Security/platform owner |
| P4 — Before P | Establish CI from existing required checks and reproducible versioned artifacts. Document ordered deployment, migrations, compatibility/backfills, running revision, health/smoke checks and rollback or forward recovery. Exercise the procedure on staging; never treat full-database restore as routine application rollback. | Release/platform owner with backend owner |
| P5 — Before multiple API instances | Inventory in-process workers, source calls, caches and connection budgets. Resolve 010a's process-local pacing/cooldown for every FUB path, including credential validation. Verify restart, stale-worker fencing, rate-limit coordination and connection bounds in the chosen topology. | Backend owner |
| P6 — Before P | Connect application tracing to OpenTelemetry export and OpenObserve. Verify intake success, Today latency, import age, database/queue pressure and dependency failures. Define controlled Organization usage/cost attribution for AI, provider calls and stored bytes; bound label cardinality and redact content. Route an actionable test alert to the assigned responder. | Observability/platform owner |
| P7 — Before P | Specify durable database/WAL/key backups, access and retention; restore in isolation and test planned failover. Demonstrate erased-data handling and storage consistency. Compare measured recovery with accepted RPO/RTO; unresolved or missed objectives remain a release gap. | Database/platform owner |
| P8 — Before customer service commitments | Assign incident lead, responder/backup and escalation/customer communication ownership. Rehearse one failed import and one service outage using sanitized diagnostics. Supported recovery actions must use the existing authorization/command discipline; customer-content access requires its own policy. | Operations owner with product/support owner |
| P9 — Before production capacity claims; required D-050 | Run the production-shaped dataset/hardware baseline with `./scripts/perf`. Record tested topology, load, connection/worker bounds, storage headroom and failure behavior. Define expansion triggers from measurements; do not infer capacity from laptop or revenue estimates. | Backend/database owner |
| P10 — Before each enabled integration's production use | Close its applicable consent, provider access and privacy gates. Track credential rotations and vendor dependencies. Dormant recording infrastructure is not approval to record; O-002/O-012 and the recording slice remain necessary. | Integration owner with privacy/product owner |

## Decisions that remain open

These need explicit resolution in the relevant specification/decision before
implementation or commitments. This checklist intentionally chooses no values.

| Decision | Required answer / evidence | Due |
|---|---|---|
| Recovery point objective (RPO) — **OPEN** | Acceptable committed-data loss by service/data family; identify whether any source can be replayed and what cannot. Compare with actual backup/WAL/object/key recovery boundaries. | Before backup design is accepted and before customer promises |
| Recovery time objective (RTO) — **OPEN** | Acceptable time to usable service for defined incidents, including detection, restore, validation and reconnect. A database copy time alone is insufficient. | Before recovery design is accepted and before customer promises |
| Retention and deletion — **OPEN**, O-012/O-013/O-015 | Data/backup/key retention, expiry, legal holds, deletion authority and propagation after restore; distinguish proposal text in open decisions from accepted policy. | C / relevant storage slice |
| Availability and support commitments — **OPEN** | Success, latency and freshness objectives for critical journeys; supported service hours, response expectations, escalation and measurement boundaries. Assign named owners before offering coverage. | Before customer promises |
| Production configuration — **OPEN where not settled** | Size/topology, backup destination and bulk-storage backend, key recovery and admin access details within D-001/D-014/D-018/D-062. No hardware purchase or vendor choice follows from this checklist. | Production/storage slice |

## Next bounded work

1. Prepare the synthetic restore execution brief and inventory the erasure
   coverage required by C1/C2; surface policy decisions without inventing them.
2. Review the production slice scope and assign the proposed roles before
   scheduling implementation. Keep infrastructure integration in that slice.
3. Resume live FUB validation only when the user resumes it. Continue 010b
   planning independently; its approved core-first scope does not close V/C/M/P.

Maintain this checklist when evidence lands. For every closed item, link the
actual result; for each remaining gap, retain its trigger and accountable role.
