# Mobile 002 — Implementation briefs

**APPROVED FOR IMPLEMENTATION — D-076, 2026-09-12.** The user accepted the
[offline-edits specification](../specs/MOBILE_002_OFFLINE_EDITS.md).
[Planning review](MOBILE_002_REVIEW.md) and the
[coordinated launch](../plans/MOBILE_002_010e2_PARALLEL_LAUNCH.md) own readiness
and the three-worktree sequence. Mobile design cleanup and physical-phone
testing are deferred; continued native feature development is not.

## Outcome and shared constraints

Both apps edit downloaded note text and task title/kind/due date offline, preserve
saved proposals and surface server-version conflicts without automatic overwrite.
Keep the existing lease, encryption/key policies, scoped context, immutable
outbox, atomic server receipts and sealed-cache rules. No reassignment, deletion,
reopen, general dependency chain, new mobile service or visual redesign.

Coordinator alone owns shared specs/decisions/status, root scripts, Cargo
workspace/dependency files, `.sqlx`, module/router registration, AppState, shared
workspace guards, and release inventory/preflight. Writers supply exact patches
for those files; coordinator integrates them sequentially. Native writers own
their entire platform only. The mobile backend lane alone owns Mobile 002 SQL
migrations; the migration lane alone owns 010e2 migrations. Reserve distinct
versions and never edit an applied migration.

## A — Shared backend foundation

**Branch:** `codex/mobile-002-backend`. One primary writer; may run concurrently
with the separately approved 010e2 lane. Own affected mobile/note/task modules,
mobile HTTP route implementations, note revision migration, focused DB/API tests,
and new shared wire fixtures under `mobile/contracts/mobile002/`. Coordinate
registration/SQLx/guard seams rather than editing them in parallel.

1. Freeze the exact additive capabilities, payload/receipt/read DTOs and
   normalization, Note revision writer/trigger inventory, current-record read
   bounds, typed transaction cores, unchanged canonical v1 bytes/key domains,
   and error/lock/publication behavior in `MOBILE_002_CONTRACT.md`. This is owned
   implementation detail after approval, not another policy approval gate.
2. Add independent note revisions; preserve existing Note/Web/Operator response
   shapes. Verify original note import, tombstone and Person revision behavior.
   Do not use `updated_at` or Person revision as the note concurrency token.
3. Factor EditNote and UpdateTask transaction cores. Mobile requires a baseline;
   legacy wrappers keep their semantics. Task edits preserve the current assignee,
   including null/inactive, without a second mutation path or default assignment.
4. Add atomic edit operations, content-free versioned receipts and bounded current
   record reads. Prior accepted operations replay before fresh version checks;
   every new execution rechecks live authority and expected revision before no-op.
5. Run focused real DB/API and old-client compatibility proof; include old
   add-note null receipts, original serialized payload digest/replay, capable/
   incapable bootstrap, stale/equal edits, null assignee, deletion and two actors.
   Integrate the verified foundation and close this worktree before both native
   worktrees open. Preserve the exact fixture/API build for platform integration.

**Exit:** frozen usable contract and passing synthetic backend evidence. No
native success claim from fixtures or backend tests alone.

## B — iOS editing and conflict workflow

**Branch:** `codex/mobile-002-ios`. Own `ios/`, native tests and
`MOBILE_002_IOS_VERIFICATION.md`; backend/root/shared files remain coordinator-owned.
Prerequisite: integrated A plus frozen fixtures; use actual SwiftUI on Simulator.

- Add explicit operation/draft kind handling rather than add-note-versus-task
  fallbacks. Preserve original envelope bytes and add-note null revision handling.
- Transactionally upgrade encrypted SQLite; mark revision-less old note bundles
  for edit qualification/refetch while preserving readable cache and every draft/
  queue. Store immutable baseline/proposed fields and CAS revisions separately.
- Implement note/task Edit, pending overlay, one submitted action per target,
  durable follow-up draft and explicit conflict/current-version/revised-edit
  flows. Same-target completion must obey sequencing; legacy create/completion
  dependencies remain intact. Full note text and exact due instants are reviewable.
- Keep current-record comparison data separate from complete bundle promotion.
  Fence response application by context and editor request epoch so late data
  cannot reset a newer draft or show another account's content.
- Prove native offline edit/relaunch, storage/CAS failure, follow-up preservation,
  successful lost-response replay, real second-actor conflict and review/resubmit,
  missing resource/permission lock, and schema upgrade from installed Mobile 001
  data. Do not wipe/reinstall the demonstrated app's store to make an upgrade pass.

**Exit:** actual native build/launch, focused storage/model and UI evidence against
the real isolated backend, recorded with source/runtime/fixtures and limitations.
Physical-device/cellular testing and signing/distribution are not claimed.

## C — Android editing and conflict workflow

**Branch:** `codex/mobile-002-android`. Own `android/`, tests and
`MOBILE_002_ANDROID_VERIFICATION.md`; prerequisites and backend boundaries match B.
Use actual Kotlin/Compose and the installed emulator, retaining production key/
storage policies and existing narrowly scoped debug setup.

- Add explicit edit kinds to draft entities, DAO/store/repository, pending
  summaries, receipt parsing and render paths. Note-edit receipts have a revision;
  old add-note receipts still require null. Do not weaken all receipt validation
  merely to admit the new kind.
- Add non-destructive Room/SQLCipher migration with immutable baseline/proposal,
  target/predecessor and CAS state. Preserve stored original JSON bytes, UUIDs,
  binding/context and legacy create/completion ordering; old cache stays readable.
- Implement matching Edit/conflict/follow-up flows and native date/time controls,
  preserving the stored instant when untouched. Current-record fetches have
  context/editor fences and never mark a partial component complete.
- Run focused instrumented encrypted-store/upgrade/failure tests plus real-API
  offline edit/force-stop/relaunch, two-actor conflict, explicit revised action,
  lost response and same-resource sequencing. Preserve the target app's store
  through upgrade evidence; test runners that uninstall it are not upgrade proof.

**Exit:** actual Android build/launch and equivalent attributed synthetic behavior
proof. No physical device, cellular, minimum-OS support or distribution claim.

## Integration and evidence budget

Use distinct platform Person/task reservations in one coordinator-owned isolated
operational fixture API; coordinate the deliberate two-device conflict cases.
010e2 uses its own migration-review fixture database/API. Preserve the shared
API3000/Web5173 and the user's existing native-demo API3101/stores. Assign free
ports and isolated `CARGO_TARGET_DIR`, Vite output, Xcode derived-data and Gradle
output paths. No destructive bootstrap or customer/FUB credentials.

The native lanes own a narrowly scoped Debug test-origin override and isolated
QA app/store configuration needed by this brief. The current iOS Simulator
HTTP allowance and Android emulator origin target the demo on port 3101; permit
the coordinator-selected isolated loopback API for synthetic QA while retaining
existing demo defaults and production HTTPS/origin/key policies. This is not a
user-entered endpoint chooser, broad cleartext allowance or distribution setup.
Upgrade proof uses an isolated preserved Mobile 001 fixture store, never an
erase/reinstall of the demonstrated app or its store.

Run final integrated `scripts/check`, serialized live SQLx preparation when SQL
changes, `scripts/check-db`, platform builds and the focused native acceptance
matrix in spec §8. D-050 allows one relevant hot-plan/paired regression run and
at most two bounded review/fix rounds. Attribute reusable Mobile 001 evidence;
do not claim it exercises new edits. A fixed shared DB gate never runs concurrently
with another lane's global DB gate. Repeat only affected checks after fixes.

Record exact source/check commands, device/runtime, evidence actually inspected,
failed attempts, preserved-work counts and outstanding limitations. Local
integration supplies dependent lanes; publishing, shared-development deployment
and app distribution remain later concrete release actions.
