# Mobile 002 and 010e2 — Coordinated next milestone

**APPROVED FOR IMPLEMENTATION — D-076, 2026-09-12.** The user
requested continued iOS/Android development alongside migration and then asked
to prepare/review the mobile specifications. Physical-phone testing and the broad
mobile design cleanup are deferred. Neither deferral parks native feature work.

## Deliverables and authority

| Deliverable | Owning proposal | Status |
|---|---|---|
| Mobile shared edit/version/receipt support | [Mobile 002 spec](../specs/MOBILE_002_OFFLINE_EDITS.md), [brief A](../tasks/MOBILE_002_IMPL.md#a--shared-backend-foundation) | Approved under D-076; Terra high implementation |
| iOS note/task offline editing and conflict recovery | [Brief B](../tasks/MOBILE_002_IMPL.md#b--ios-editing-and-conflict-workflow) | Approved under D-076; Terra high, Simulator verification |
| Android equivalent | [Brief C](../tasks/MOBILE_002_IMPL.md#c--android-editing-and-conflict-workflow) | Approved under D-076; Terra high, emulator verification |
| Existing imported-People refresh | [010e2 spec](../specs/SLICE_010e2.md), [brief](../tasks/SLICE_010e2_IMPL.md) | Approved under D-076; Terra high implementation |

Mobile 001 / 010e1 is released and complete. D-076 explicitly accepts both new
edit/refresh contracts and their implementation. No repeat approval is pending. No phone, live FUB qualification,
new customer dataset or activation is a prerequisite for the synthetic work.

## Three-worktree schedule

| Phase | Worktree 1 | Worktree 2 | Worktree 3 |
|---|---|---|---|
| Contract and foundation | Mobile 002 backend | 010e2 migration, backend then Web under one writer | Free |
| Native implementation | iOS Mobile 002 | 010e2 migration continues if unfinished | Android Mobile 002 |
| Integration | Close each finished lane after verified integration | Shared regressions and affected fixes are serialized | No replacement feature invented to fill a slot |

1. After the applicable approvals, record one consistent local planning base
   and assign the mobile backend and migration writers. Freeze exact Mobile 002
   fixtures and shared seams; migration can proceed independently in its files.
2. Verify/integrate the mobile backend foundation and close its worktree. Then
   launch iOS and Android against that real backend while migration continues.
   Early read-only native fixture review is possible during foundation work;
   fixture review is not a running native implementation or completed integration.
3. Each native platform implements in its own worktree. Migration's one writer
   owns both its backend and Web work, so all three can run without a fourth
   worktree. A separate 010e2 Web branch from the initial standalone proposal is
   replaced by this ownership assignment, with its behavior/checks unchanged.
4. If backend corrections are needed during native work, the coordinator assigns
   one writer and serializes shared edits/integration. Rotate a worktree if required;
   never open a fourth or let two writers patch the same command/schema file.

This is a dependency schedule, not a duration estimate or a promise that all
three lanes finish together. Native work needs the shared editing API, but never
waits for migration completion. Do not restart completed Mobile 001 work.

## Ownership and compatibility seams

| Branch | Primary ownership | Boundary |
|---|---|---|
| `codex/mobile-002-backend` | Mobile/note/task commands, note revision migration, mobile routes and fixtures/tests | No migration executor/Web or platform edits |
| `codex/mobile-002-ios` | `ios/` plus assigned platform evidence | No backend/Android/shared-root edits |
| `codex/mobile-002-android` | `android/` plus assigned platform evidence | No backend/iOS/shared-root edits |
| `codex/migration-010e2` | People refresh domain/routes/schema, Web API/component/tests and migration evidence | No note/task mutations, mobile wire/receipt changes or native source |

There are four successive branches and at most three simultaneous worktrees.
Coordinator owns shared docs, registration, guards/preflight, root scripts,
dependency manifests/locks and `.sqlx`. Lane owners propose specific shared patches
for sequential integration. Migration alone creates 010e2 migrations; mobile
backend alone creates Mobile 002 migrations. Allocate unique increasing versions
before coding, including the intended merge order. No applied file is rewritten.

The important shared seam is Person revision/guard compatibility: Mobile 002
adds note revision and factors note/task writes; 010e2 changes only existing
People/contact/stage/assignment. Preserve permitted import insert triggers and
ordinary review-mode denial. One final combined fixture verifies both schemas,
old import/report/mobile operations and new scoped mutations.

## Isolated verification and completion

- Shared development and the current native demo stay intact. All verification
  Cargo/Vite/native build outputs are isolated from running-service artifacts.
- Use a new operational synthetic mobile API/database with per-platform record
  reservations; migration uses a separate review-mode dataset/API/Web port.
  Assign ports at launch from the actual listener inventory. Do not change phone
  configuration, native signing identities or the user's current simulator data.
- Serialize SQLx preparation and fixed-database `scripts/check-db` runs on the
  host. One coordinator owns shared integration, regression gates and any later
  authorized release; per-lane tests must not collide with another fixture.
- Both native platforms must build and run their actual offline editing/conflict
  UI against the real synthetic API. Migration must run actual preview/confirm/
  recovery in desktop and 390px Web. Simulator/emulator results remain distinct
  from user-deferred physical/cellular tests.
- Use the specs' focused checks, one relevant D-050 plan/paired regression pass
  and at most two bounded review/fix rounds. No repeat broad audit after gates
  pass without concrete changes/failures.

The [Mobile 002 review](../tasks/MOBILE_002_REVIEW.md) covers the new specification
and this coordinated ownership. The original [010e2 review](../tasks/SLICE_010e2_REVIEW.md)
continues to cover its unchanged write policy; its brief's scheduling amendment
is reviewed separately here. New implementation and release evidence are future
deliverables, not claims made by this plan.

## Launch allocation — 2026-09-12

Coordinator integration branch: `codex/mobile002-010e2-integration`. The reviewed
planning documents and D-076 are checkpointed locally before lanes branch.
Implementation uses Terra high. Mobile owns migration version `20260925000001`;
migration 010e2 owns `20260926000001`. Mobile foundation integrates first.

Initial worktrees are `/Users/karrad/projects/crm-worktrees/mobile-002-backend`
and `/Users/karrad/projects/crm-worktrees/migration-010e2`. Reserve mobile QA
API port 3102/database `crm_mobile002_qa` and migration QA API3103/Web5174/database
`crm_010e2_qa`, subject to a fresh listener check before launch. Root owns shared
patch integration and serial DB gate admission. Neither lane runs root builds or
changes API3000/Web5173/demo3101. Future native QA uses isolated app identities
and fixture stores.
