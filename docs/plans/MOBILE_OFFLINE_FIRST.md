# Native mobile: first offline field-work slice

**IMPLEMENTATION APPROVED — 2026-09-12, D-074.** The first concrete
[specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md) and
[execution briefs](../tasks/MOBILE_001_IMPL.md) are approved for isolated synthetic
implementation. D-073 records the earlier planning and seven-day access choices.
This plan is not an implementation or a claim of native verification. Existing
accepted decisions remain authoritative.

## Outcome

An agent on iOS or Android can open previously downloaded People and tasks,
write a note, create a task and record task completion without connectivity.
Successfully committed local work survives ordinary app termination/relaunch
and device restart. When connectivity and valid authorization return, the app
resumes synchronization, applies each accepted action once and exposes conflicts
or rejected work instead of silently losing it.

The user established unreliable cellular service, periods without connectivity
and preserving agent-entered information as first-version requirements. SQLite
is the agreed local-store direction. Use Swift/SwiftUI and Kotlin/Jetpack Compose
under D-001, with the existing Rust application and PostgreSQL under D-002/D-008.
Local storage does not replace the shared server's authority over accepted CRM
state, permissions, deterministic Today ranking or migration review holds.

## First workflow and approved scope

1. Sign in while connected to an operational synthetic Organization. Download
   an explicitly bounded set of People, tasks and note data; show download
   completion and last successful synchronization. Data not downloaded must be
   visibly unavailable offline, not represented as an empty authoritative result.
2. Read Today/People from local storage with clear freshness. An offline task
   completion may show as pending locally; it must not pretend the team-wide
   Today result or server permission checks have already changed.
3. Autosave note/task drafts locally. On submission, commit the proposed local
   change and its pending upload together in one SQLite transaction. Show
   "Saved on this device" only after successful commit. Disk-full, locked/key
   unavailable and other storage failures must not produce a success indicator.
4. Preserve drafts and pending operations through process death, restart and
   schema upgrades. Ordinary cache eviction/re-download must not discard them.
   Define the autosave acknowledgment boundary; text not yet committed cannot
   be included in the durability claim.
5. Submit each pending action through the normal authorized Rust command layer.
   Keep one stable operation identity through retries; persist the server result
   atomically with its business change. A lost response must not cause another
   note/task or reapply an old completion after another user reopens a task.
6. Mark "Synced" only after the server accepts the action and that result is
   committed locally. Surface "Needs attention" for a conflict, permanent
   rejection or required sign-in. Resolve local-work retention/access according
   to the approved policy; do not silently drop failed work or resurrect erased
   records in pursuit of upload success.

Proposed initial writes are new notes, new tasks and task completion on already
downloaded People. Editing existing notes/tasks, new People, reassignment and
stage changes need separate conflict semantics before being added. A new task
completed before its creation uploads needs explicit dependency ordering and
identity mapping, not two unrelated retries.

Calls, sending messages, server AI execution, recordings/media, migration admin
and full historical replication are later capabilities. A locally retained
message draft is not a sent message. Native notification/call handling remains
part of the product roadmap; it is not included in this first durability proof.

## Decisions and concrete contract work

D-073 accepts Today-related People, assigned People and explicitly saved records
as the initial offline set. Selection affects offline availability, not
Organization-wide Person visibility. The user selected a seven-day offline
access window following successful online authorization; expiry locks access
while protecting pending work. Exact fields, history bounds, task dependencies,
local-data lifecycle and storage limits are approved in Mobile 001 under D-074.

Before implementation, one backend owner must turn this table into an owned
specification with exact payloads, errors, compatibility and migration behavior.
D-074 approves these changes within Mobile 001; implementation is still pending.

| Current contract | Proposed work and reason | Affected components / compatibility |
|---|---|---|
| AddNote/CreateTask can duplicate after a lost response; client IDs were deferred in Slices 015/016. | Stable operation IDs, request-payload binding, durable acknowledgments/result lookup and deduplication scoped to trusted identity/Organization. Define receipt retention and expired-operation behavior for the supported offline period. | Rust commands, HTTP adapter, PostgreSQL receipts/constraints, both local upload queues; existing Web/Operator calls must retain their declared behavior unless explicitly amended. |
| Note edits use last-write-wins; task updates lack an expected revision. Complete/reopen target-state checks do not identify an earlier operation. | Specify baseline/version checks and conflict results for first-scope commands. Preserve conflicting proposals; no timestamp-based silent overwrite. Define task completion versus intervening reopen/edit and create-then-complete dependencies. | Shared commands/schema, mobile reconciliation UI; audit effects and existing callers need compatibility tests. Existing note/task editing is outside the first mobile write scope. |
| Web reconnect invalidates/refetches; realtime is not a durable change feed. Ordinary reads hide note/task tombstones. | Define bounded initial download and resumable changes/deletions, cursor expiry/recovery and permission-driven removal. Commit downloaded data and cursor together; protect pending local work during re-download. | New mobile-oriented reads, PostgreSQL change discovery, SQLite projections; use existing typed queries/commands, not direct table replication or a generic event-sourced CRM. |
| Current authentication is cookie-based; no independently released native-client support policy exists. | Native session/storage/revocation behavior behind the existing identity abstraction; specify supported clients, offline duration, credential expiry and safe upgrades. Cached permissions never authorize server writes. | Session/identity boundary and both apps; preserve current Web login and parked production ZITADEL integration until its owning scope. No token format or support-window duration is chosen here. |
| CommandContext uses WebSession; many command timestamps represent server execution. | Native origin plus distinct device-reported action time and server receipt/acceptance time, with explicit clock/trust and Today/audit semantics. | Shared envelope/persistence constraints, mobile DTOs and affected readers; adding native origin is a declared contract change, not a client-selected trusted actor. |
| Customer data currently has server-side retention/erasure prerequisites; no native local-data lifecycle is specified. | Approve on-device data protection, keys, backup treatment, offline access period, sign-out/account switching and revoked/deleted-data handling. Bind every pending operation to its original actor/Organization. | SQLite files/journals, credential/key storage, device backup, local caches/drafts/receipts and erasure inventory. Offline devices cannot receive an immediate remote revocation or wipe. |

The first detailed specification must declare an offline test envelope: number
of cached records, queued actions, outage duration, device/app versions, concurrent
editors, storage budget and recovery bounds. D-050's Web operating envelope is
not evidence for these new native/offline dimensions. Do not invent customer
quotas, retention periods or support commitments in implementation.

## Execution order and worktrees

The [coordinated launch plan](MOBILE_MIGRATION_PARALLEL_LAUNCH.md) owns branch,
file, schema and test-resource allocation. Implement the approved
[010e1 comparison brief](../tasks/SLICE_010e1_IMPL.md) before launch. Mobile 001
is approved under D-074; D-075 approves 010e1's reviewed contracts.

**Step 1 — Freeze shared contracts and prepare all briefs.** Native tooling is
verified. Freeze mobile wire fixtures and backend ownership before client code;
review the bounded migration report contract and isolate its shared wiring.

**Step 2 — Run mobile backend and migration concurrently.** Two primary writers
own disjoint modules and schemas. Native contributors may review fixtures and
prepare UI designs. Verify and integrate the shared mobile backend foundation.

**Step 3 — Close the backend worktree; run iOS, Android and migration in parallel.**
Both native apps now implement against the same integrated backend. Migration
continues independently if still in progress; do not delay mobile until migration
finishes. This keeps at most three active implementation worktrees.

| Lane | Primary ownership | Required outcome |
|---|---|---|
| iOS | `ios/`, its tests and assigned native docs | SwiftUI field workflow, protected SQLite drafts/outbox, accepted sync contract and native failure tests. |
| Android | `android/`, its tests and assigned native docs | Jetpack Compose equivalent, same behavioral contract and native failure tests. |
| Migration | Proposed 010e1 and its assigned backend/Web/schema files | Bounded report of changes between retained core captures, preserving the mobile contract and current migration hold. |

Each worktree needs one branch, writer, brief, file boundary and required checks
under AGENTS §12. Main coordinates shared documents and integration. There is no
fourth simultaneous mobile-backend lane: shared-contract fixes go through the
designated backend owner and may require rotating/pausing a lane. Only the lane
explicitly assigned database ownership may add migrations for its task.

Worktrees do not isolate running services. Assign test databases, ports and build
outputs. `scripts/check-db` currently uses a fixed throwaway database name, so
serialize it against the same PostgreSQL server. Shared service resets, merges
and deployments have one owner. Native tests must not mutate shared development
or a migration lane's dataset. Keep branches short-lived and merge validated
increments; deployment remains a separate concrete release step.

## First proof and definition of done

Use a small, populated, isolated synthetic operational workspace plus a second
Organization and a second actor. Before broad feature expansion, prove:

- Offline committed notes/tasks survive process termination, relaunch and device
  restart; schema upgrade retains drafts, pending identities and upload order.
- Server commit followed by a lost response/retry produces one business result;
  a conflicting payload cannot reuse the same identity. Authentication is checked
  before replaying a receipt. A completed-then-reopened task is not recompleted
  by a retry of the earlier accepted operation.
- Interrupted upload/download resumes; cursors cannot advance past uncommitted
  local data. Expired cursors and re-download preserve unsynced work. Deletions
  and concurrent edits do not silently overwrite local proposals or resurrect
  server records.
- Disk-full/write failure never reports a save. Session expiration, revocation,
  account/Organization switch and migration-review denial preserve the approved
  security and local-work lifecycle; cross-Organization access is denied.
- UI distinguishes downloaded/unknown/stale data, saved locally, server accepted
  and needs-attention states. Background suspension cannot be mistaken for sync
  completion. Sync runs when foreground/connectivity permits and resumes after
  interruption; do not promise immediate background execution.

Run focused server integration and native persistence/UI tests, one bounded
independent review and the appropriate final repository/native checks. Repeat
only affected checks after fixes under D-050. Record actual simulator/emulator
and physical-device evidence separately; no native completion claim based only
on narrow-screen browser tests. Physical restart and poor-cellular checks remain
pending until actual devices are available.

Durability has a precise boundary: committed device data can survive ordinary
software interruption, but a destroyed/lost device, removed app data or lost
encryption key cannot be recovered if its work never reached another durable
copy. Protect pending work through normal upgrades/cleanup; agree backup and
device lifecycle policy rather than claiming SQLite alone guarantees zero loss.

## Workstation observation — 2026-09-12

See the [current setup record](../tasks/MOBILE_NATIVE_TOOLCHAIN_SETUP.md) for
subsequent installation progress; the paragraph below preserves the initial
read-only observation.

Read-only checks found Swift 6.3.3 on arm64 macOS, with Command Line Tools selected.
`xcodebuild -version` requires full Xcode and `simctl` is unavailable. No Xcode
was found in standard application locations. Java reports no runtime; Gradle,
adb, emulator, sdkmanager and avdmanager are absent from PATH, and no standard
Android SDK/AVD installation or configured SDK path was found. Custom locations,
devices, signing and actual native builds are unverified. No install, build or
native test was performed. Native toolchain setup is therefore the first local
execution prerequisite; backend planning can proceed now.

## Evidence and authority

- [Repository instructions](../../AGENTS.md), especially §§3–4, 11–16;
  [decision log](../decisions/DECISION_LOG.md): D-001/002/004/008/015/050 and
  D-064/065's separate migration workspace gate.
- [Foundations F-03](FOUNDATIONS.md#f-03--client-schema-and-release-compatibility-proposed)
  is a proposal; no numeric native support window is accepted.
- [Notes contract](../specs/SLICE_015.md): §2 edit/delete semantics and §6 retry
  limits; [tasks contract](../specs/SLICE_016.md): mutation/retry semantics.
- [Realtime contract](../specs/SLICE_003.md),
  [command envelope](../../backend/crates/crm-app/src/domain/envelope.rs) and
  [customer readiness](PRODUCTION_READINESS.md).
- [Android offline-first guidance](https://developer.android.com/topic/architecture/data-layer/offline-first),
  [SQLite atomic commit](https://sqlite.org/atomiccommit.html),
  [SQLite durability settings](https://sqlite.org/pragma.html#pragma_synchronous),
  [Apple background execution](https://developer.apple.com/videos/play/wwdc2025/227/).
  These inform the design; they do not select this product's conflict or privacy
  policies, approve customer data or constitute implementation evidence.
