# Migration deltas completion — verified scope

**Status: implementation brief for existing engines only.** This follow-on does
not add a delta-cycle resource, source-deletion semantics, a polling worker,
or shared-contract changes. It proves repeated retained-evidence refresh behavior
already required by D-092. Live FUB/customer work and activation remain deferred.

## Verified current behavior

010g1 already provides the intended repeat-refresh substrate:

| Concern | Existing behavior to preserve |
| --- | --- |
| Newer source evidence | A new family refresh requires a complete, strictly newer retained capture from the same source account. A prior accepted scan, even when every unit was held or cancelled, fences same/overlapping captures with `source_not_newer`. |
| Applied baseline | Every applied/already-current family unit atomically writes immutable after-state and advances its family head with compare-and-swap. A subsequent refresh discovers that head as its baseline. |
| Local conflict | Execution rechecks the full native row/revision against the frozen baseline. A local change holds the unit and does not advance the baseline/head. |
| Idempotency/recovery | Request receipts, immutable manifest/result rows, fenced leases, atomic transactions, `Resume`, cancellation, and exact `Remainder` are present. Remainder copies only unfinished frozen units; it does not replay applied work. |
| Source absence | Complete activity/history owner walks create immutable `source_not_observed` held manifests for previously owned identities absent from the new capture. Metadata preserves missing/unknown source data and only removes a positively owned tag link from a qualified complete list. |
| Deletion/tombstone safety | Source absence, inaccessible records and source 404s are not deletions. Native deleted rows and erased history identities are rejected/held and cannot be recreated. |
| Existing repair boundary | 010e5 repairs original/admitted **People** stage/assignee mapping holds only. It is not a repair path for settled 010g1 metadata/activity/history holds. |

## Remaining limitation

A settled 010g1 hold is immutable. A later qualified capture can prepare a new
refresh and safely use the latest successful baseline, but it does not mutate or
reopen the prior held result. Exact remainder is recovery for unprocessed frozen
units, not a mechanism to replay an applied unit or override a local conflict.
This is the correct D-092 boundary and is recorded here rather than replaced with
new machinery.

## Owned implementation

Add `backend/crates/crm-api/tests/db_migration_repeat_cycles.rs`, with any
synthetic fixture helpers unique to that test. The coordinator owns registration
in `backend/crates/crm-api/tests/all.rs`; do not edit it. No migration, router,
or schema change is expected.

The first regression is **Activity-family coverage**. History correction chains
remain covered by the existing 010g1 history tests and are not duplicated here.
The test must use the real existing commands/workers and establish this sequence:

1. First refresh from a complete retained capture applies activity units and
   records their heads/baselines.
2. A strictly newer retained capture drives a second refresh. One source unit is
   changed and applies against the first refresh head. A separately locally edited
   native row is held; its prior head/baseline remain unchanged.
3. Lose the response after a committed confirm or remainder command and replay
   the same request ID/body. The durable response is returned and no second
   bundle/plan/result/head/history version is created.
4. Omit a previously owned activity identity from an otherwise complete
   later capture. It settles as `source_not_observed`; no native delete is made.
5. Cancel after an eligible unit has committed, create the exact existing
   remainder, and complete it. Assert the applied unit was not copied or replayed,
   while the eligible unprocessed unit converges once.
6. Assert every frozen manifest has one terminal result; count, head version,
   immutable result and byte/accounting totals reconcile across all cycles.

Run the suite only in the coordinator-allocated serial disposable database. Use
an explicit isolated `CARGO_TARGET_DIR` for compilation/checks.

## Registration snippet for the coordinator

Add this adjacent to the existing family-refresh DB modules in
`backend/crates/crm-api/tests/all.rs`:

```rust
mod db_migration_repeat_cycles;
```

The test must be included in the same serial DB gate as the existing
`db_family_refresh*` modules.
