# Mobile 004 / migration 010e4 — Coordinated implementation plan

**APPROVED FOR IMPLEMENTATION — D-080, 2026-09-13.** Both contracts and this
coordinated sequence are accepted. Actual resources/progress are recorded in
[implementation status](../tasks/MOBILE_004_010e4_IMPLEMENTATION_STATUS.md).
Current published source is `44dcf52`; runtime remains the recorded Mobile003/
010e3 release. Launch from a freshly verified base plus accepted planning docs.

## Outcomes and scope

| Track | Specification / brief | Exit |
|---|---|---|
| Shared mobile backend | [Mobile 004](../specs/MOBILE_004_OFFLINE_STAGE_CHANGES.md), [brief A](../tasks/MOBILE_004_IMPL.md#a--shared-backend-and-frozen-contract) | Stage-specific concurrency, shared command/receipt, coherent catalog and conflict read |
| iOS | [Brief B](../tasks/MOBILE_004_IMPL.md#b--ios) | Actual protected offline stage/change/conflict/upgrade behavior |
| Android | [Brief C](../tasks/MOBILE_004_IMPL.md#c--android) | Equivalent native behavior on the same API contract |
| Migration backend then Web | [010e4](../specs/SLICE_010e4.md), [brief](../tasks/SLICE_010e4_IMPL.md) | Core refresh for one successful admission cohort, exact preservation and recovery |

All implementation writers use the user's **Terra high** preference. Consequential
contract/review questions use the design profile, escalating only on a named hard
problem under [model routing](../prompts/MODEL_ROUTING.md). Required review is
independent of the writer; routine gate execution does not require another agent.
Native calling, redesign, physical phones, live FUB/customer work and activation
remain deferred/separate. Subsequent family work stays in the [ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md).

## Dependency schedule

| Stage | Worktree 1 | Worktree 2 | Worktree 3 |
|---|---|---|---|
| After scope/contract acceptance | Mobile backend | 010e4 backend then Web | Unused |
| After mobile API/fixtures integrate and backend tree closes | iOS | 010e4 continues if needed | Android |
| Final integration | Close completed lanes; one coordinator owns shared fixes and gates | No replacement scope merely to fill slots | At most three trees |

This is an order of dependencies, not a delivery-time estimate. Native work need
not wait on migration. The coordinator root is for shared integration, not a
fourth independent writer. Reassign ownership explicitly before shared repairs.

## File and schema ownership

| Proposed branch | One primary writer owns | Coordinator-only integration |
|---|---|---|
| `codex/mobile-004-backend` | Stage command core, mobile domain/generation/current reads, Mobile004 schema, API/DB fixtures/tests | Route/module registration, root scripts/locks and `.sqlx` |
| `codex/mobile-004-ios` | `ios/`, tests and assigned evidence | Backend/shared docs/contracts |
| `codex/mobile-004-android` | `android/`, tests and assigned evidence | Backend/shared docs/contracts |
| `codex/migration-010e4` | Admitted-refresh domain/store/worker, small shared migration helpers, new schema, Web API/panel/tests | Workspace mutation/reader guards, release inventory/preflight, router/worker registration, shared navigation and `.sqlx` |

Coordinator owns decision/spec/state edits and cross-lane contract arbitration.
Only the mobile backend writer creates Mobile004 migrations; only the migration
writer creates 010e4 migrations. Allocate unique versions and exact paths from a
fresh inventory after acceptance; mobile integrates first. No applied-file edits.
Freeze mobile wire/schema fixtures before native implementation, and migration
wire/schema before Web. No concurrent edits of shared files by multiple writers.

## Integration risks and required checks

- Stage revision must advance for every real stage-ID transition, including
  migration refreshes, and not for unrelated mobile edits. Fact/receipt commits
  must not double-increment or produce duplicate stage history.
- Catalogue revision triggers must preserve original import/admission/metadata
  stage creation and workspace guards. No broad Organization UPDATE or review
  bypass is granted merely to maintain derived revisions.
- New admitted-refresh permits remain separate from original import, 010e2
  refresh and admission INSERT permits. Ordinary mobile stage commands must
  still fail in every review workspace, even for an admin.
- Old mobile clients/receipts and non-opted-in generations remain compatible;
  old migration readers/workers fail closed at the new capability boundary.
- Shared locks (workspace, parent, catalog, Person, baseline, operation/worker)
  need a documented consistent order before integration; tests cover conflicting
  edits, catalog changes and lease cancellation without open-ended race matrices.

Run focused lane checks during implementation. Coordinator serializes final
`scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db` and affected checks
on the combined source. Attribute reusable evidence to runner and tree. Each
slice gets at most two review/fix rounds and its relevant D-050 plan/paired-read
checks; no broad audit after passing gates without a new failure/change.

## Resource isolation and exit

Preserve shared API3000/Web5173, demo API3101, installed stores and recovery files.
Allocate free QA ports/database names at launch. Mobile uses an isolated operational
workspace with disjoint platform Person fixtures; migration uses a separate review
workspace and production Web build. Use separate Cargo/Web outputs, Xcode derived
data and Gradle output paths; never overwrite artifacts used by shared services.

Native QA uses separate app identities and populated Mobile003 stores upgraded
in place. Record actual Simulator/emulator offline/restart/replay/conflict flows;
no distribution/physical-cellular claim. Migration records actual desktop/390px
preview/confirm/cancel/remainder/reload and exact source/native/ledger preservation.
Record actual resources/commits/results in implementation evidence when work starts.
After acceptance and verification, publication and deployment are a separately
scoped release; existing approvals for older milestones are not reused for it.
