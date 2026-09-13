# Mobile 001 and migration — Coordinated implementation launch

**2026-09-12: Mobile 001 approved under D-074; 010e1 approved under D-075.** This records
ownership and dependencies established before implementation, as requested. Both
backend worktrees are now active; see the [coordination record](../tasks/MOBILE_MIGRATION_IMPLEMENTATION_STATUS.md). Tool installation and both virtual-device
boots are verified in the [setup record](../tasks/MOBILE_NATIVE_TOOLCHAIN_SETUP.md).

## Deliverables and approval

| Work | Deliverable | Authority |
|---|---|---|
| Mobile backend | Durable accepted-operation receipts, safe retries/conflicts and bounded download reconciliation through existing Rust commands | [Mobile 001](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md), D-074 approved |
| iOS | SwiftUI Today/People/Person and offline note/task workflow backed by protected SQLite | [Mobile brief B](../tasks/MOBILE_001_IMPL.md#b--ios-field-workflow), D-074 approved |
| Android | Kotlin/Compose equivalent with the same durable-save and synchronization behavior | [Mobile brief C](../tasks/MOBILE_001_IMPL.md#c--android-field-workflow), D-074 approved |
| Migration 010e1 | Review what changed between the original core capture and a newer one: People, users, stages, custom fields, notes and tasks | [Approved spec](../specs/SLICE_010e1.md) and [brief](../tasks/SLICE_010e1_IMPL.md); D-075 approved |

010e1 supplies evidence for later delta application and repair. It does not
update imported CRM records or treat a missing source record as a deletion.
Standalone tags need a qualified catalog source; email/media, remaining families,
live qualification, repair and cutover/activation remain separate work. Completing
this comparison is not completing migration or making a customer ready to cut over.

The [010e1 bounded review](../tasks/SLICE_010e1_REVIEW.md) is ready after the
report-publication correction and targeted ownership/pagination recheck.

## Schedule and dependencies

1. Both reviewed briefs are approved under D-074/D-075. Create a consistent
   local planning checkpoint for the implementation branches.
   Freeze mobile wire fixtures, shared-file inventory and revision ownership
   before native protocol code. Do not ask for Mobile 001 approval again.
2. Start **mobile backend and migration concurrently** in two worktrees with
   separate primary writers. Migration does not depend on mobile sync. Native
   fixture/design review can occur here without a separate implementation lane.
3. Verify and integrate the mobile foundation, then close its implementation
   worktree. Start **iOS and Android concurrently**, leaving migration's worktree
   running if its bounded task is still in progress. This is the intended three-
   lane phase: iOS + Android + migration. Native API integration needs the actual
   backend; do not claim mobile completion against fixtures alone.
4. Merge each verified result when ready. If migration finishes first, close its
   lane; do not silently start another migration task to occupy the slot. Shared
   backend fixes during native work go to one designated writer with affected
   shared-file edits serialized. No fourth implementation worktree is opened.

This is a dependency order, not an estimate or a guarantee of equal finish times.
Migration is not held behind all mobile backend work, and mobile does not wait
for migration, live FUB qualification or cutover.

## Branches, writers and file ownership

Branches below are allocated at launch. Use fresh branches from the coordinated
integration base; do not copy uncommitted shared planning files inconsistently.

| Branch | Primary writer / owned files | Excluded shared files |
|---|---|---|
| `codex/mobile-001-backend` | Mobile backend writer: approved mobile modules/routes, note/task transaction cores, native origin, revision triggers, assigned DB/API tests | Migration domain/worker changes and migration Web UI |
| `codex/migration-010e1` | Migration writer: additive `migration/core_change_*`, report schema/tests, dedicated API route/client and migration UI component; exact list in its brief | Mobile routes, canonical note/task writes, mobile revisions, `ios/`, `android/` |
| `codex/mobile-001-ios` | iOS writer: `ios/`, platform tests and platform evidence | Backend, Android and root/shared documents |
| `codex/mobile-001-android` | Android writer: `android/`, platform tests and platform evidence | Backend, iOS and root/shared documents |

The first backend branch closes before both native branches open: these are four
successive branches, at most three simultaneous implementation worktrees.

The coordinator is the sole writer for decision/state/shared specs, root scripts,
workspace manifests/locks and shared registration files (`crm-api/src/lib.rs`,
`routes/mod.rs`, app configuration/state/domain registry, shared migration admin/
preflight wiring). Lane owners supply concrete patches; the coordinator applies
these sequentially to the appropriate branch and communicates the resulting
commit/base to other lanes. A lane may not silently edit a coordinator-owned file.
Shared wiring needed to build a lane is integrated early, not deferred until final
QA. `migration/mod.rs` is also coordinator-owned so module declarations and worker
wiring have one writer throughout parallel work.

Only the mobile backend writer owns Mobile 001 SQL migrations. Only the migration
writer owns 010e1 SQL migrations. Reserve unique increasing migration versions at
launch and merge the foundational schema first; never rewrite an applied file.
010e1 may create report-owned tables, not a competing revision system or canonical
CRM mutations. Integration rechecks the combined schema and affected SQLx cache.

## Verification without service collisions

- Allocate separate PostgreSQL databases, API ports and build outputs per lane.
  Preserve the existing shared-development database and API/Web service. Never
  use `dev-bootstrap` for test setup: it removes Docker volumes.
- Serialize `scripts/check-db` against the same PostgreSQL server because its
  throwaway database name is fixed. Serialize shared SQLx cache generation,
  service resets, migration application on a shared DB, merges and release steps.
- Use a separate simulator/emulator and app data directory for each native lane.
  Against synthetic API data, verify local save, terminate/relaunch, lost-response
  replay, conflict handling, interrupted download and seven-day access locking.
- Migration verifies immutable comparison inputs, uncertainty, tenant isolation,
  replay/recovery, storage bounds and zero native business writes. A report with
  disclosed gaps can be complete as a computation without proving source fidelity.
- Follow D-050: one bounded review and at most two implementation review/fix rounds
  per slice; required focused tests plus one relevant performance/plan run. Do not
  repeatedly rerun broad suites after they pass without a concrete reason.

Completion requires actual native builds and simulator/emulator workflows for
both platforms, plus API/DB evidence. Physical-device/cellular checks remain
explicitly pending unless run. Record each slice's source revision, commands,
results and limits. Git publication, shared-development deployment and app
signing/distribution are subsequent concrete release work, not implicit in
creating a worktree.
