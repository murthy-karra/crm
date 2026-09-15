# Mobile006 / 010f4 — integrated verification

**IN PROGRESS.** Full database suite, paired
Person/Today performance and Android UI submission evidence remain open. Both
independent implementation reviews are in their final allowed second round.
No final READY verdict is claimed yet.

## Scope and evidence ownership

D-084 authorizes local implementation and isolated synthetic verification.
Integration branch: `codex/mobile006-010f4-integration`. The final backend
remainder repair is `22c6c06`; native Android repairs follow on the same branch.
Private evidence: `/private/tmp/crm-mobile006-010f4-thyhauvv`.

- [Migration acceptance and browser reconciliation](SLICE_010f4_VERIFICATION.md).
- [iOS store, live recovery and installed upgrade](MOBILE_006_IOS_VERIFICATION.md).
- [Android store, live recovery and installed upgrade](MOBILE_006_ANDROID_VERIFICATION.md).
- [Performance inventory and evidence](MOBILE_006_PERFORMANCE.md).
- [Independent findings and final verdicts](MOBILE_006_010f4_IMPLEMENTATION_REVIEW.md).

## Mobile006 acceptance mapping

| ID | Implemented behavior | Evidence |
|---|---|---|
| M6-01 | Atomic mixed tag/value edits, durable local outbox, exact replay/receipt | `db_mobile::mobile006_metadata_atomic_receipt_current_and_catalog_generation`; both native offline/restart/live records |
| M6-02 | Metadata ABA and competing edits conflict; unrelated notes do not; changed catalog requires qualified current review | `db_person_metadata::metadata_typed_conflict_detects_aba_but_allows_unrelated_note`; native two-actor catalog/conflict proofs |
| M6-03 | Four field types, exact decimal/date rules, final tag cap, archived clear, foreign/invalid options and sibling rollback | `db_person_metadata` typed atomic/capacity and archived/foreign-option cases; native form/storage suites |
| M6-04 | Complete opted-in metadata/catalog pages and sealed generations; incomplete or stale metadata cannot be reused for editing | `db_mobile` catalog-generation, snapshot-after-barrier and 100-value-bound cases; iOS33 storage cases and Android10 storage cases |
| M6-05 | Current actor/tenant/lease/workspace authority applies to commands, current reads and receipts | `metadata_typed_rechecks_actor_and_tenant_before_noop`, `mobile006_metadata_receipt_and_current_authority_gates`, platform lock/expiry/key-loss cases |
| M6-06 | Real typed/import writers and deletion cascades advance derived revisions; no-op/overflow/erasure remain safe | Seven retained compatibility tests, `db_person_metadata` cascade/overflow/erasure and blocked tag-delete cases |
| M6-07 | Existing Web/Operator commands remain compatible; events are content-free, with no publication on rollback/replay | `mobile006_metadata_events_and_noop_replay_are_content_free`, existing command/Operator suites and Web invalidation fixtures |
| M6-08 | Populated installed Mobile005 upgrades retain keys, drafts, outbox bytes and receipts | Actual iOS schema8→9 and Android schema6→8 installed-store inventories in platform records; no reinstall of those proof packages |
| M6-09 | Bounded/indexed realistic reads and unaffected Person/Today paired regression | Mobile18-query plan run passed; migration26-query plans passed; single paired run pending |

Backend focused evidence predates the final shared suite and remains traceable in
[implementation status](MOBILE_006_010f4_IMPLEMENTATION_STATUS.md). The final full
DB gate must actually pass before this matrix is accepted as complete.

## Shared final gates

- `scripts/check` at `22c6c06`: **PASS**, 149.671s,
  `logs/integration/final-local-check-6.log`. Includes 51 preflight tests, Rust
  formatting/Clippy/production compilation and dependency fences, 993 ordinary
  Rust tests, five doctests, Web lint/typecheck, 1,298 Web tests, isolated Web build
  and 11 email-worker tests. 1,103 DB tests were skipped here and remain a separate
  required gate. Subsequent Android changes use their separate platform checks.
- Standalone `scripts/sqlx-prepare`: **PASS**, 134.92s,
  `logs/integration/final-sqlx-prepare-1.log`; no cache changes. The full DB gate
  checks the cache again against a freshly migrated throwaway database.
- Current focused 010f4 DB matrix: **PASS 14/14**, 138.094s,
  `logs/integration/activity-bounded-remainder-db-r2.log`. Includes tenant/admin,
  source fidelity, rollback, retry, exact remainder, partial-copy recovery,
  no-remainder/excluded coverage, per-row settled checkpoints and old-binary fences.
- Full `scripts/check-db`: **running corrected second attempt**. The first run
  failed the historical populated010f1 upgrade comparison in 145.867s
  (`logs/integration/final-check-db-1.log`). Its exact row diff contained only new
  `person.metadata_revision=1`; `71f5b87` explicitly verifies that initial value
  alongside the other derived revision columns before comparing every older
  field. This is a test expectation correction, with no production change.
- Final realistic migration plan run: **PASS 26 probes, zero failures**, 208.328s,
  `logs/integration/activity-hotplans-db-r3.log` and
  `integration/activity-hotplans-4.json`, frozen backend `22c6c06`. The previous
  22-probe failure and bounded-availability/settled-tail repair remain retained.
- One paired Person/Today run: **pending; not run yet**.

## Preserved resources and limits

Protected shared API3000/Web5173 processes and artifact hashes are unchanged;
`inventory/protected-artifacts-final.json` records the comparison. All verification
build outputs are separate. Owned native API3106/DB and browser API3107/Web5177/DB,
installed native stores, source captures and failed attempts remain retained.

This verification contains no publication/deployment, native distribution,
physical-phone/cellular, live FUB/customer, activation or calling evidence. The
prior Mobile005/010f3 release remains separately recorded IN PROGRESS and is not
repeated by this implementation.
