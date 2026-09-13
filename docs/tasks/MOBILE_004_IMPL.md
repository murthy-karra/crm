# Mobile 004 — Implementation briefs

**APPROVED FOR IMPLEMENTATION — D-080.** Implement the accepted
[spec](../specs/MOBILE_004_OFFLINE_STAGE_CHANGES.md) and declared shared changes.
[Coordination](../plans/MOBILE_004_010e4_PARALLEL_LAUNCH.md) assigns shared ownership;
[planning review](MOBILE_004_010e4_PLANNING_REVIEW.md) is source-based, not execution proof.

## A — Shared backend and frozen contract

Proposed branch `codex/mobile-004-backend`; one Terra high writer. Own
`domain/commands/change_person_stage.rs`, mobile domain/generation implementation,
new additive stage/catalog/receipt migration, focused command/mobile tests and
`mobile/contracts/mobile004/`. Supply shared router/module/SQLx patches to the
coordinator. No migration feature, Web, iOS or Android edits.

1. Inventory stage writers and catalog/Person triggers, receipt parser/visibility,
   generation cleanup/seal, current stage HTTP errors and native strict dispatch.
   Freeze `MOBILE_004_CONTRACT.md` before native coding: exact wire fixtures,
   positive revisions, command/no-op/conflict ordering, catalogue generation
   flag/cursor/seal and response bounds, DB constraint shapes, all-writer trigger
   and lock order, capability rollout and old-client behavior.
2. Add protected stage/catalog revisions; prove A→B→A and unrelated edits behave
   as specified. Factor the existing typed stage command into a transaction core
   without altering ordinary Web/Operator request bodies or unconditional behavior.
3. Add new operation and atomic person_stage receipt; exact replay remains prior
   acceptance, including after a later stage change. No task/contact fallthrough.
   Add bounded current-stage conflict read, opt-in coherent catalog generation
   and cleanup. Preserve old generation semantics when the flag is absent.
4. Verify real-DB failure paths, foreign Org/target, review hold, receipt races,
   no-op, publication, current-read access and new/old migration writer triggers.
   Include catalog coherence and lock-order evidence, not only mocked pages.
5. Integrate the backend plus frozen contract; close its worktree before both
   native lanes start. Actual source/fixtures/checks go in backend verification.

## B — iOS

Proposed branch `codex/mobile-004-ios`; one Terra high writer owns `ios/`, its
focused tests and `MOBILE_004_IOS_VERIFICATION.md`. Requires A's integrated API.

- Add explicit stage proposal/operation/receipt/current-read variants and local
  revision/catalog qualification markers. Upgrade populated Mobile 003 encrypted
  stores without changing old envelope bytes/IDs, receipts, keys or drafts.
- Download stage pages only under opted-in generation; promote complete catalog
  and Person generation together after seal. Handle same-revision format upgrade,
  capability absence, changed catalog and identity/editor epochs.
- Build stage picker, baseline/current/proposal conflict review, pending overlay
  and follow-up draft state. Keep ordinary server stage/Today freshness honest.
- Run actual Simulator offline Save→terminate→relaunch→real API acceptance,
  lost-response/replay, competing stage edits, revised proposal, mixed queues,
  full/locked store, lease lock/reauthorization and installed-store upgrade.

## C — Android

Proposed branch `codex/mobile-004-android`; one Terra high writer owns `android/`,
focused tests and `MOBILE_004_ANDROID_VERIFICATION.md`. Requires A's integrated API.
Same behavioral contract as B; implement explicit Kotlin/Compose/SQLCipher typed
branches, atomic draft/outbox CAS and catalog generation promotion. No destructive
Room fallback, catch-all task receipt or reserialization of old pending uploads.
Use actual emulator instrumentation, force-stop/relaunch and real API conflict/
replay/upgrade checks. A runner reinstall does not prove installed-store upgrade.

## Checks and handoff

Spec §6 is the acceptance matrix. Run focused native storage/model/UI and real
API checks; backend formatting/Clippy/SQLx and regression tests appropriate to A.
Coordinator runs combined `scripts/check`, `scripts/sqlx-prepare`,
`scripts/check-db` sequentially on final integrated source, using current script
instructions and isolated targets. No overlapping DB-backed gates. Reuse
attributed unchanged checks and repeat affected checks after concrete fixes.
Include the specified D-050 paired read/hot plans, not an open-ended benchmark.

Reserve independent QA app identities and a populated Mobile 003 fixture store;
preserve the user's installed demo and API3101. Allocate actual API/database/port
and build paths at launch. Evidence records source/API/device versions, exact
commands, failed attempts and accepted/no-op/conflicted counts. Use at most two
review/fix rounds; blockers remain blockers. Physical phones, cellular, broad
redesign, calling, distribution and releases remain deferred/separate.
