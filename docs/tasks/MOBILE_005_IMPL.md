# Mobile 005 — Execution briefs

**APPROVED FOR IMPLEMENTATION — D-082.** [Specification](../specs/MOBILE_005_OFFLINE_PERSON_DETAILS.md)
and [coordination](../plans/MOBILE_005_010f3_PARALLEL_LAUNCH.md) are accepted after
independent planning review returned READY. Implementation review remains required.
Use Terra high for substantive implementation under existing routing preferences.

## A — Shared backend and frozen contract

Proposed branch `codex/mobile-005-backend`, one primary writer. Own new typed
Person-details command, small shared contact validation changes, mobile domain/
HTTP reads and operation adapter, `realtime/events.rs` details variant, owned additive revision/receipt schema, focused
DB/API tests and `mobile/contracts/mobile005/` fixtures. The coordinator integrates
registration, workspace/SQL grants, shared writer patches, Web realtime type/
invalidation changes and `.sqlx` sequentially. The migration lane does not edit
Web realtime files; no new Web profile editor is included.

1. Record exact DTO/error fixtures, receipt extension, capability advertisement,
   old-client compatibility, field bounds and new-kind digest bytes in
   `mobile/contracts/mobile005/`. Freeze a writer/trigger/lock inventory including
   intake, `capture::link_unmatched(add_contact_method)`, original/admitted imports
   and both core-refresh workers. Place the new kind's intake admission before the
   adapter's current common Person lock; acquiring it only after dispatch is invalid.
   Existing
   generation contacts are paged in the summary section; extend that representation
   instead of adding an unnecessary fourth full-generation component.
2. Add revision coverage and shared `UpdatePersonDetails`, explicit adds/edits/
   removals, identity validation and transaction-compatible receipt commit. Retain
   current normalizers and per-Person contact uniqueness; intake ordering stays
   authoritative. Schema alterations must compose with review permits and cascades.
3. Add bounded current-profile traversal, completion/revision validation, new
   capabilities and summary/order fields. Preserve old generation behavior and
   old receipt serialization. Document server-assigned contact ID mapping exactly.
   Declare/freeze `details_changed` in the Slice003 v1 envelope; wire new-kind
   publication after commit, no-op/replay suppression and publish-failure recovery.
   Supply the coordinator Web type/Person/People/Today/list-count invalidation
   fixtures, including old broad fallback compatibility.
4. Verify authorization, replay/no-op/conflict, rollback, contact/ABA writers,
   concurrent intake, bounded queries and DTO compatibility before native handoff.
   Integrate this foundation and close its worktree before opening both native lanes.

## B — iOS

Proposed `codex/mobile-005-ios`; one primary writer owns `ios/` and assigned
verification evidence only. Read `Protocol.swift`, `LocalStore.swift`,
`FieldModel.swift` and `API.swift`; use the frozen backend fixtures.

Add encrypted profile drafts, atomic immutable submission, exact contact operations,
profile editor/conflict/current/follow-up flow, capability and representation
qualification, receipt ID mapping and accepted overlays. Preserve mixed old queues
and all existing lease/account boundaries. Upgrade a populated Mobile004 store
(currently schema 7; verify at launch), without reinstalling or replacing keys.

Run existing platform checks and actual simulator offline → restart → sync →
conflict/review/replacement. Include disk/key failure, old capabilities, full
contact pagination, primary fallback, lost response and late context responses.
Record source/API/app/toolchain versions and evidence in `MOBILE_005_IOS_VERIFICATION.md`.

## C — Android

Proposed `codex/mobile-005-android`; one primary writer owns `android/` and assigned
evidence only. Read `Protocol.kt`, `FieldDatabase.kt`, `FieldStore.kt`,
`FieldRepository.kt`, `FieldApi.kt` and `MainActivity.kt`.

Implement the same frozen contract and product flow in Compose with protected
Room/SQLite persistence. Keep CAS draft saves, typed receipt validation, stable
operation identities and actor/Org/editor fencing. Upgrade populated Mobile004
schema 5 in place (verify at launch), retaining mixed pending work and keys.
Run platform checks and real emulator/API acceptance equivalent to B; record in
`MOBILE_005_ANDROID_VERIFICATION.md`. UI parity means identical safety/behavior,
not a shared UI toolkit or speculative native redesign.

## Checks and handoff

Every criterion in spec §5 is required. Backend uses fmt, Clippy, focused unit/
DB/HTTP/contract tests and one relevant D-050 paired read/changed hot-plan pass.
Native lanes use actual current repository test/build commands and populated-store
upgrade tests. Coordinator serializes `scripts/check`, `scripts/sqlx-prepare` and
`scripts/check-db` on combined source; do not overlap DB-backed gates.

Use independent synthetic operational fixtures for mobile plus a second Org/actor
and a review-workspace denial fixture. Preserve shared API/Web, native demo and
installed QA stores. Isolate Cargo/Web/Xcode/Gradle outputs and QA app identities.
Record actual commands, failures, runner/source revisions, preserved rowsets,
accepted/no-op/conflicted counts and any limitation. Two review/fix rounds maximum;
no missing trust check is a pass. Physical phones/distribution/calling remain later.
