# Slice 010e2 — D-050 evidence

Source: `a0eb294` combined with the verified native clients at `07a0fbf`.
The plan collector is `db_people_refresh_plans.rs`, opt-in `perf-harness`.

`plan-shapes.json` contains one representative collection at 25,000 People,
50 members and 299,994 contact comparison rows. A tiny real import/report/refresh
settled before inert cardinality clones were added. Those clones have copied,
unqualified ciphertext and are never used for decrypt/fidelity/accounting
claims. Actual production SQL literals, SHA-256 hashes, binds and complete
EXPLAIN (ANALYZE, BUFFERS) output are retained. No planner settings were changed.

Eight of nine initial statements used indexes without spills. The remaining
report-group anti-join scanned all 25,000 groups and imported results before
its limit. The fix reads 50 raw descriptors first and checks original membership
by an indexed point lookup, advancing the checkpoint over skipped descriptors.
`plan-shapes-fix.json` records only these two changed statements on the **same
retained 25k fixture**, with indexed access and no spills (0.157ms/0.077ms).
No successful statements or full benchmark matrix were repeated. Earlier
fixture-construction failures preceded all measurements and remain in local logs.

`paired-today.json` is the successful same-build, fixed-clock 40-pair comparison
using the existing frozen `9eaeb0a` reader and current code. DTOs match. The current
Today module is byte-identical to the verified Mobile002 foundation `2fd9a9d`;
010e2 adds a mutation guard and does not change ordinary read bodies. Current p95
48.633834ms is within the 25ms permitted increase over baseline 35.023667ms.
This pair uses the isolated 100-Person native fixture; the separate 25k collection
provides statement-shape evidence. Neither predicts production capacity.

The older 010f2 Person-detail harness rejected its stale shared-helper hash
**before** building its fixture or taking measurements. Its frozen manifest was
not weakened or rewritten. The compatible Today pair above is the completed
unchanged-reader check.
