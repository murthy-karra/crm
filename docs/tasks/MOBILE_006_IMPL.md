# Mobile 006 — Execution briefs

**IMPLEMENTATION AUTHORIZED — D-084, 2026-09-14.** The user requested “implement them”.
The earlier draft/planning labels below preserve the proposal history; D-084
supersedes their approval boundary. Complete independent planning review before
code work, then implement and verify without requesting repeated approval of
these contracts. Materially different policy remains outside this authorization.

**DRAFT — planning only, D-083.** [Specification](../specs/MOBILE_006_OFFLINE_METADATA.md)
and [coordination](../plans/MOBILE_006_010f4_PARALLEL_LAUNCH.md). Independent planning
review and acceptance of the proposed contracts precede implementation. Keep Terra
high for substantive writers; no worker is launched by this brief.

## A — Shared backend

Proposed branch `codex/mobile-006-backend`; one primary writer. Own mobile domain/
HTTP metadata reads and operation dispatch, typed metadata composer, narrow
transaction-compatible tag/value command helpers, catalog/value revision schema,
generation snapshots and cleanup, focused tests and `mobile/contracts/mobile006/`
fixtures. No activity import schema or Web migration panels. Coordinator owns
shared guard/grant integration, root registration, Web event compatibility fixtures,
`.sqlx` and serialized patches to original/admitted metadata writers.

1. Inspect Mobile005's final integrated implementation, not old README schema
   numbers. Freeze `MOBILE_006_CONTRACT.md` and fixture files: exact actions/value
   serialization, request opt-in/representation identity, receipts/errors, current
   and generation routes, catalog pages, completion validation and publication.
2. Write the complete affected-writer/lock inventory before triggers. Include
   every tag link/catalog command, custom-field value/definition/option command,
   original/admitted metadata workers, tag deletion and Person erasure cascades.
   Demonstrate barrier placement before the adapter's common Person lock and
   before any conflicting catalog writer lock. Stage/details commands remain
   outside metadata conflicts. Resolve any inverse graph through an explicitly
   owned narrow compatibility change before native handoff.
3. Add disjoint additive migrations for metadata/catalog revisions, snapshot
   representation and receipt kinds. Allocate actual versions at implementation
   launch. Preserve existing data, guards, old receipt serialization and old
   generation sealing; enforce no-op/ABA/overflow rules. Keep catalog snapshots
   bounded despite archived definitions. Freeze their counted temporary-byte
   bound, eviction/cleanup behavior and oversized-row error fixture.
4. Extract transaction-compatible helpers from existing typed tag/value commands;
   keep old wrappers and validation/error precedence. Compose a fully validated
   atomic patch with stable receipt identity, same-Org targets and current authority.
   Validate final tag capacity and apply removals before additions, so replacing
   a tag at the cap does not fail an intermediate helper quota check.
   Reuse existing event variants only after changed commit.
5. Implement complete versioned metadata/catalog download and current conflict
   reads. Prove same-revision representation upgrade and catalog-only invalidation.
6. Verify M6-02 through M6-07 and backend parts of M6-01/04/09. Integrate the
   verified shared contract and close this worktree before both native lanes start.

## B — iOS

Proposed `codex/mobile-006-ios`; one writer owns `ios/` and its assigned evidence.
Read `Protocol.swift`, `API.swift`, `LocalStore.swift`, `FieldModel.swift` and the
frozen contract. Add native tag picker and four typed value editors, explicit clear,
archived/deleted states, encrypted metadata drafts, immutable atomic submission,
conflict comparison and accepted overlays. Locale presentation must not change
wire decimal/date meaning. Complete catalogs/metadata qualify editing; missing
sections never become authoritative empty values.

Verify M6-01–05/07/08 using real simulator + owned synthetic API. Upgrade an
installed populated Mobile005 store without uninstall/key replacement, preserving
old drafts/queues/receipts and extending protected backup/erasure inventory. Include
disk/key/lock failure, CAS draft races, seven-day expiry, late account/editor reads,
lost response, unrelated edit and catalog archival/delete during offline work.
Record actual source/API/schema/device/scheme/commands and inventories in
`MOBILE_006_IOS_VERIFICATION.md` when executed.

## C — Android

Proposed `codex/mobile-006-android`; one writer owns `android/` and assigned evidence.
Read `Protocol.kt`, `FieldApi.kt`, `FieldDatabase.kt`, `FieldStore.kt`,
`FieldRepository.kt`, `MainActivity.kt` and the frozen fixtures. Implement the same
behavior in Compose/Room/SQLCipher, retaining stable IDs, exact decimals, protected
outbox and current identity/lease boundaries. Use the server's rules, not a second
mobile-only validation policy.

Acceptance and upgrade matrix matches B, on a real emulator/owned synthetic API.
Use an owned QA variant; connected tests must not uninstall the user's retained
demo/store. Prove upgrade separately from any harness that clears app data.
Record `MOBILE_006_ANDROID_VERIFICATION.md` when executed.

## Checks and handoff

The paired plan owns exact command templates, prerequisites, output isolation and
final gates. Every M6 acceptance ID is required. Native owners report API base and
contract fixture revision before coding. Backend changes after freeze require
coordinated fixture refresh and relevant native reruns. No physical-phone,
distribution, calling or live customer evidence is claimed by simulator tests.

Keep focused tests and required integration gates; at most two independent review/
fix rounds under D-050. Record failures and remaining limitations honestly. Planning
does not accept the new aggregate conflict/catalog/atomic-edit policy.
