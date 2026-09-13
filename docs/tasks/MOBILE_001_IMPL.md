# Mobile 001 — Execution briefs

**APPROVED FOR IMPLEMENTATION — D-074, 2026-09-12.** The reviewed
[specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md), contracts and isolated
synthetic implementation are authorized. This file assigns implementation
boundaries; it does not claim worktrees, apps or endpoints already exist.

The [planning review](MOBILE_001_REVIEW.md) is READY FOR IMPLEMENTATION APPROVAL;
its two corrections are incorporated in the specification and this brief. D-074
records the subsequent user approval; do not ask for Mobile 001 approval again.

## First deliverable and sequence

Deliver the synthetic offline note/task workflow on both native platforms. Start
with the backend contract/foundation alongside the D-075-approved migration
slice, then run iOS, Android and migration in three short-lived worktrees. See the
[coordinated launch plan](../plans/MOBILE_MIGRATION_PARALLEL_LAUNCH.md). Do not create permanent
platform branches or a fourth independent backend writer. Each branch uses
`codex/` and starts from the integrated foundation needed by its bounded task.

The coordinator owns shared specs, decision log, project state, root build/release
scripts and integration. Tools are prepared once under the
[native setup record](MOBILE_NATIVE_TOOLCHAIN_SETUP.md). Native projects use their
own device/runtime and output directories; no lane runs shared service resets.

## A — Backend foundation (concurrent with the migration lane)

**Owner:** one backend primary writer. **Scope:** `backend/` modules/migrations
explicitly enumerated in the frozen contract; assigned API fixtures and tests.
Only this lane creates database migrations for Mobile 001. Coordinate root
scripts/docs through the coordinator. No Web feature work or migration changes
except compatibility verification of affected shared writers.

1. Freeze exact route/DTO/error/schema/locking contracts from the approved spec.
   Record context binding, canonicalization, digest-key rotation, receipt replay,
   expired or invisible operation behavior, and the complete revision-trigger
   inventory. Fixtures include success, replay, conflict, generation pages,
   seal/expiry, lease expiry, invalid identity and workspace denial.
2. Factor transaction-compatible note/task and Today cores, preserving existing
   public wrappers. Implement native origin, atomic operation receipts, task
   revisions and new authenticated mobile adapter. Declare every changed origin
   constraint/old reader compatibility consequence before migration.
3. Implement bounded selection, complete component reads, mobile Person versions,
   reconciliation metadata and sealing. Receipt/content lifecycle and generation
   expiry must not discard pending local work or recreate erased resources.
4. Verify with actual isolated PostgreSQL and application-router calls: lost
   response, stale completion, create dependency, authorization changes, cross-
   Organization resources, partial/stale download, acknowledgment/promotion races
   and generation admission/cleanup limits independent of installation IDs.
5. Run focused integration checks, one bounded review/fix pass, then required
   final Rust/SQLx/DB and affected Web/Operator checks. Run one D-050 paired
   regression and query-plan pass for changed hot paths; repeat only affected
   checks if concrete fixes require it. Serialize shared DB gates.

**Exit:** approved/frozen contract, passing backend proof, reusable synthetic
fixtures and integrated foundation. No native success or deployment claim.

## B — iOS field workflow

**Owner:** one iOS writer. **Files:** `ios/`, its tests and assigned native docs.
Backend, root scripts and shared documents are coordinator-owned.
**Prerequisite:** full compatible Xcode, iPhone simulator and integrated A.

Build the SwiftUI app shell, existing development sign-in, authorized offline
lease, protected actor/Organization SQLite store, draft/outbox lifecycle, server
projections/staging and shared protocol client. Implement Today/People/Person,
note/task composition, pending completion and visible sync recovery. Keep business
authorization server-owned; share fixtures with Android, not a second command
implementation. Pin actual SQLite/encryption wrapper and test-library versions
with their reproducible installation/build instructions.

**Required checks:** native build; file-backed persistence/schema upgrade;
storage-failure no-false-save; terminate/relaunch with pending work; controlled
lost-response retry against real API; conflict/dependency handling; account/lease
lock; interrupted generation/promotion; representative UI flow on a booted
simulator. Record physical-device restart/cellular checks as pending unless run.

**Exit:** a usable native app against the synthetic backend with attributed
evidence; no substitution of Web screenshots or a CLI database example.

## C — Android field workflow

**Owner:** one Android writer. **Files:** `android/`, its tests and assigned docs.
**Prerequisite:** compatible Studio/JBR, pinned Gradle wrapper, SDK and ARM64
phone AVD, plus integrated A. A global Gradle install is unnecessary.

Implement the same field workflow and failure semantics in Kotlin/Jetpack
Compose with a protected SQLite store and the approved native contract. Select
a compatible Room/SQLite/encryption integration with verified native binaries;
do not silently use an unencrypted database because a wrapper compiles. Keep
credentials out of backup/logs and outbox identities stable across process death.

**Required checks:** debug build; local and instrumented persistence/migration
tests; process termination/relaunch; lost-response retry against real API;
conflict/dependency, identity/lease lock, storage failure, interrupted download
and atomic promotion; representative UI flow on a booted emulator. Separately
record physical-device restart/cellular checks when available.

**Exit:** Android native proof equivalent to B, with its own actual evidence.

## Parallel migration lane and integration

The approved [010e1 comparison task](SLICE_010e1_IMPL.md) is prepared before launch
and starts alongside A under D-075. After A integrates
and its worktree closes, B and C occupy two worktrees; 010e1 continues in the third
if still in progress. Its brief owns its separate backend/Web/DB files. Do not infer live FUB validation or customer-data permission from this
parallelism. A required native contract fix is assigned to the coordinator or
rotates a lane; it cannot be authored concurrently against the same shared files.

Each lane gets isolated test data/ports/build outputs. `scripts/check-db` uses a
fixed temporary database, so serialize it on a shared PostgreSQL server. Main
coordinates compatible merges and any later approved release. Platform app
distribution, shared-development deployment, customer activation and production
deployment each need their concrete release scope; no worktree is a deployment.

## Evidence budget

One bounded planning review and at most two implementation review/fix rounds per
slice under D-050. Keep a concise evidence map: source revision, commands actually
run, fixture/device/runtime, result and unresolved limits. Verify changed files'
checks after fixes rather than repeatedly rerunning every suite. Native data
durability, server idempotency, conflicts and tenant isolation remain mandatory.
