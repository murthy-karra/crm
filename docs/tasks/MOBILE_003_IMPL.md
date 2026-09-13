# Mobile 003 — Implementation briefs

**APPROVED FOR IMPLEMENTATION — D-078, 2026-09-13.** The
[specification](../specs/MOBILE_003_OFFLINE_CONTACT_LOGGING.md) and occurrence-time
policy are accepted. The
[planning review](MOBILE_003_010e3_PLANNING_REVIEW.md) and
[parallel plan](../plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md) define readiness and
ownership. Implement the approved scope with isolated synthetic verification.

## Outcome and common constraints

Both native apps save a manual contact log on a downloaded Person, retain it in
protected SQLite and synchronize one accepted fact per operation despite retries.
Freeze the accepted occurrence-time policy in the wire fixtures.
Keep existing note/task operations, seven-day access, account fencing and saved-work
retention. Contact logging does not send communications or replace call settlement.

The coordinator owns shared decisions/specs/status, module/router registration,
workspace guards, root manifests/locks/scripts, `.sqlx` and release inventory.
Lane writers supply exact shared patches for sequential integration. Reserve
distinct additive migration versions at launch; only the mobile backend writer
creates Mobile 003 migrations. No applied migration changes.

## A — Shared backend foundation

**Proposed branch:** `codex/mobile-003-backend`. One Terra high primary writer,
following the user's implementation preference. Own mobile domain/HTTP code,
`domain/commands/log_contact_attempt.rs`, affected contact-fact insertion code,
the receipt schema extension, focused API/DB tests and
`mobile/contracts/mobile003/` fixtures. No migration executor or native edits.

1. Inspect all note/task-only receipt branches, current fact clocks, contact
   command locks and Today generation behavior. Freeze `MOBILE_003_CONTRACT.md`
   with exact capability, payload/canonical bytes, receipt validation, errors,
   accepted-time precision/range, locking and transaction boundaries. After
   policy approval, compatible DTO/schema encodings are owned implementation
   details; they are not another approval gate.
2. Factor the existing typed command into a transaction-compatible core. The
   ordinary wrapper keeps its server clock and existing repeated-submit behavior.
   Mobile uses that core inside its receipt transaction with live authorization.
   Preserve call settlement/correction and existing note/task canonical bytes.
3. Add the explicit contact operation and fact-backed receipt visibility path.
   Handle null committed revision, target tombstones, changed-content conflicts,
   rollback and accepted replay before fresh time validation. Publish only after
   a new commit. Do not fall through to task/note revision or event logic.
4. Prove accepted contact effects on Today even when Person mobile_revision is
   unchanged. The client needs a fresh sealed Today generation, not a schema
   revision bump or a new replicated history component.
5. Run focused real-DB/API authorization, receipt concurrency/lost response,
   occurrence-time and old-client regression cases in spec §6. Integrate the
   verified contract/backend and close this worktree before both native lanes open.

**Exit:** usable integrated API/fixtures with attributed passing backend evidence;
native behavior is not established by backend fixtures alone.

## B — iOS contact workflow

**Proposed branch:** `codex/mobile-003-ios`. Own `ios/`, platform tests and
`MOBILE_003_IOS_VERIFICATION.md`. One Terra high writer; prerequisite is A's
integrated backend and frozen contract. Use SwiftUI and the actual Simulator.

- Add dedicated contact draft, outbox and receipt variants with a non-destructive
  encrypted SQLite upgrade from populated Mobile 002. Preserve prior operation
  bytes/IDs, drafts, receipts, key material and cache generations.
- Build the manual channel/outcome/time form and saved-work states. Explicitly
  distinguish CRM calls already logged. A successful Save means a committed
  draft+operation transaction; storage failure leaves the editor recoverable.
- One draft identity/CAS revision prevents double Save; two deliberate contact
  drafts on one Person remain independent. Do not use the note/task edit slot
  to collapse them. Never rewrite an uncertain operation when correcting time.
- Keep pending contact badges separate from server Today membership. Acceptance
  requests bounded reconciliation even with unchanged Person revision; refresh
  failure retains synced work and stale Today without another contact submission.
- Run actual offline Save → termination/relaunch → real-API sync, lost response,
  duplicate taps, two distinct logs, time correction, mixed old/new queues,
  full/locked store, access expiry/reauthorization and installed-store upgrade.

**Exit:** native build/run, focused storage/model checks and inspected actual UI
evidence with preserved-work counts, source/API/device identity and limitations.

## C — Android contact workflow

**Proposed branch:** `codex/mobile-003-android`. Own `android/`, tests and
`MOBILE_003_ANDROID_VERIFICATION.md`. One Terra high writer; same prerequisites
and behavioral contract as B. Use Kotlin/Compose on the actual emulator.

- Add explicit contact entities/DAO/repository/receipt/render dispatch. Upgrade
  Room/SQLCipher without destructive fallback or reserialization of old uploads.
  Preserve existing note/task sequencing and strict per-kind receipt validation.
- Implement the equivalent form, local CAS/save transaction, independent contact
  draft identities, recoverable time rejection and accepted/pending status.
  Preserve resolved instants across timezone changes and editor recreation.
- Persist accepted receipts atomically; request Today reconciliation independently
  of Person revision and fence late responses by current context/editor epoch.
- Run instrumented encrypted-store/failure/upgrade checks and actual offline
  Save → force-stop/relaunch → real-API sync with the same retry, time, mixed-queue
  and access matrix as B. A test-runner reinstall is not upgrade evidence.

**Exit:** equivalent attributed Android behavior proof, not just compilation or
mock networking. Neither platform claims physical cellular or distribution proof.

## Integration and evidence budget

Use coordinator-assigned isolated API/database resources with disjoint platform
Person reservations. Preserve shared API3000/Web5173, the native-demo API3101 and
the user's installed stores. QA uses separate app identities, retained Mobile 002
fixture stores and narrowly scoped Debug loopback configuration; production
origins, HTTPS, signing and key policies stay intact. Keep build outputs isolated.

The coordinator serializes live SQLx preparation and fixed-database
`scripts/check-db`. Run final integrated `scripts/check`, DB gates, platform builds,
spec §6 acceptance and one relevant D-050 hot-plan/paired Today regression. Reuse
attributed existing evidence only for unchanged behavior; repeat affected checks
after concrete fixes. At most two bounded review/fix rounds.

Record exact commands, failures, inspected runtime evidence and unresolved risks.
Broad mobile redesign and physical-phone testing remain user-deferred. Git
publication, deployment and app distribution are later release work.
