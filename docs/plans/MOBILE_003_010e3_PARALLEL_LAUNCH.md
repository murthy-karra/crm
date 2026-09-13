# Mobile 003 and 010e3 — Coordinated implementation plan

**APPROVED FOR IMPLEMENTATION — D-078, 2026-09-13.** The user accepted both
specifications and Mobile 003's occurrence-time policy after D-077 planning.
Use Terra high and the three-worktree sequence below. Mobile 002 / 010e2 remains
the released runtime; new implementation uses isolated synthetic resources.

**Implementation closeout — 2026-09-13:** The sequence below is complete. The
[implementation record](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md)
contains native/browser acceptance, database, query-plan and paired regression
evidence. Publication and deployment remain later release actions.

## Deliverables

| Work | Specification / brief | Outcome |
|---|---|---|
| Shared mobile backend | [Mobile 003](../specs/MOBILE_003_OFFLINE_CONTACT_LOGGING.md), [brief A](../tasks/MOBILE_003_IMPL.md#a--shared-backend-foundation) | Atomic manual contact fact/receipt, compatible time and retry contracts |
| iOS | [Brief B](../tasks/MOBILE_003_IMPL.md#b--ios-contact-workflow) | Protected offline contact form, recovery and truthful Today status |
| Android | [Brief C](../tasks/MOBILE_003_IMPL.md#c--android-contact-workflow) | Equivalent Kotlin/Compose workflow and durable upgrade |
| Migration backend and Web | [010e3](../specs/SLICE_010e3.md), [brief](../tasks/SLICE_010e3_IMPL.md) | Previewed core-only new-Person admission with global identity and visible gaps |

The [planning review](../tasks/MOBILE_003_010e3_PLANNING_REVIEW.md) distinguishes
current code evidence, proposed changes and outstanding decisions. Physical-phone
testing and broad mobile design remain deferred; they do not block this synthetic
feature work. Implementation acceptance does not imply a new release.

## Schedule and primary writers

| Phase | Worktree 1 | Worktree 2 | Worktree 3 |
|---|---|---|---|
| Contract and backend | Mobile 003 backend | 010e3 backend, then Web under one writer | Free |
| Native implementation | iOS Mobile 003 | 010e3 continues if unfinished | Android Mobile 003 |
| Integration | Integrate/close completed lanes | Serialize shared fixes and final gates | No replacement scope to fill a slot |

1. After acceptance, checkpoint the approved docs and allocate exact paths,
   writers, migration versions and isolated resources from fresh inventories.
   Start the two backend lanes on that same base. Freeze each lane's wire/schema
   contract before its dependent UI integration.
2. Verify/integrate the shared mobile foundation and close its worktree. Launch
   both native lanes against that backend. Native implementation need not wait
   for migration to finish, and migration never waits on the contact API.
3. Keep one migration writer for backend and Web. Any ownership transfer is
   explicit before edits. If native verification needs a backend correction,
   serialize it through the coordinator/designated owner; rotate a lane if an
   extra worktree is needed. Never have a fourth implementation worktree.
4. Integrate each bounded lane when verified. Run final shared gates once on the
   combined source and check concrete cross-lane risks. No repeated broad audit
   after passing gates without a new change or failure.

This is a dependency schedule, not a duration estimate. The four successive
branches below produce at most three simultaneous implementation worktrees.

## File and schema ownership

| Proposed branch | Primary writer owns | Excludes |
|---|---|---|
| `codex/mobile-003-backend` | Mobile domain/HTTP, contact command and its fact insertion helper, receipt schema, mobile fixtures/tests | Migration modules, Web and native code |
| `codex/mobile-003-ios` | `ios/`, platform tests and assigned evidence | Android/backend/root/shared docs |
| `codex/mobile-003-android` | `android/`, platform tests and assigned evidence | iOS/backend/root/shared docs |
| `codex/migration-010e3` | Admission domain/routes/schema, identity extension, new Web API/components/tests, migration evidence | Contact command/mobile API, native code and coordinator integration files |

Coordinator owns shared decisions/spec/status, module/router/worker registration,
workspace guard/release inventory/preflight, history registration, navigation/
Person profile integration, root scripts/manifests/locks and `.sqlx`. Lane owners
provide exact patches; integration is sequential. In particular, migration's new
fact registration in `domain/facts.rs` must not collide with mobile's contact
insertion helper: freeze/merge the mobile edit before applying the migration
registration patch. The coordinator's root checkout is for integration, not a
fourth independent implementation lane.

Only the mobile backend writer adds Mobile 003 migrations; only the migration
writer adds 010e3 migrations. Assign unique versions with mobile integrating
first; never rewrite an applied file. `.sqlx` preparation and migrations on
fixed-name test databases have a single coordinator-controlled admission slot.

Combined checks must prove the admission INSERT permit cannot authorize ordinary
contact logging in a review workspace; old import tokens remain INSERT-only and
010e2 UPDATE/DELETE stays item-scoped. New admission identities cannot be consumed
as original results by child readers. Contact facts must refresh Today without
requiring a changed downloaded Person revision. The two slices do not share a
new mutation path or weaken the Organization boundary.

## Isolated resources and acceptance

- Preserve shared API3000/Web5173 and private native-demo API3101, database and
  installed stores. Planning changes no runtime. Future API/database/port names
  are allocated at launch, not assumed from the previous milestone's retired QA.
- Mobile uses one isolated operational fixture API/database with per-platform
  Person reservations. Migration uses a separate review-mode fixture API/database
  and production Web build/port. No customer credentials or source calls.
- Assign isolated Cargo targets, Web outputs, Xcode derived data and Gradle
  outputs. Native QA uses isolated app identities and a preserved populated
  Mobile 002 store upgraded in place; never erase the user's demo to pass a test.
- Native evidence must exercise actual offline/relaunch/real-API behavior on
  both platforms. Migration must exercise actual desktop/390px preview/confirm/
  reload/recovery with exact result reconciliation. Mock or compiler results do
  not establish these workflows.
- Each slice gets the focused checks and relevant D-050 plan/paired regression
  in its spec. Shared repository/live SQLx/DB gates are serialized once on final
  integrated source. Use at most two bounded review/fix rounds and attribute
  reused evidence; do not retest unaffected areas to fill time.

Future implementation status records actual commits, assigned resources and
checks. The integration branch is `codex/mobile003-010e3-integration`. Git
publication, deployment and native distribution are later
concrete release actions.

## Launch allocation — 2026-09-13

D-078 accepts both contracts and the occurrence-time policy. Coordinator records
actual allocation/progress in the
[implementation status](../tasks/MOBILE_003_010e3_IMPLEMENTATION_STATUS.md).
Mobile owns migration `20260927000001`; 010e3 owns `20260928000001`. Reserve
API3102/database `crm_mobile_003` for mobile and API3103/Web5174/database
`crm_010e3_qa` for migration, following the fresh listener inventory. Preserve
API3000/Web5173/demo3101. All four implementation lanes use Terra high.
