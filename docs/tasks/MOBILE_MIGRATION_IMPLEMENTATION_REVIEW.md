# Mobile 001 / 010e1 — Bounded implementation review

In progress, 2026-09-12. Approval is D-074/D-075. This record distinguishes
implementation review from the earlier approved planning reviews and from actual
test execution. The coordinator independently reviews the delegated domain code;
coordinator-authored registration/configuration changes are covered by checks,
not claimed as independently reviewed by that same coordinator.

## Mobile backend — round 1 and targeted recheck

Reviewed typed-command transaction factoring, receipts and replay authorization,
context and generation admission, revision inventory, component pagination,
selection/seal consistency and router errors. Corrections requested:

1. Bound statement execution and total HTTP duration, not only lock waits.
2. Replace quadratic pin membership checks at the 25,000-Person bound.
3. Normalize path/query errors to the frozen JSON error envelope.
4. Convert nullable attribution permission expressions to explicit false.

Targeted source recheck found the 5-second statement and 20-second request
budgets, sorted-pin lookup, JSON rejection handling and COALESCE booleans present.
The implementer reports passing failure/rollback, null-attribution, physical
deletion and 25,000-Person tests. Full gate evidence and performance attribution
are recorded separately before foundation integration. Native persistence,
acknowledgment/promotion and actual app evidence remain platform work.

## Migration 010e1 — round 1 findings

Reviewed the five new retained comparison modules, report route and additive SQL,
including raw qualification, grouping, tuple receipts, authorization, budgets,
lease ownership, output sealing and bounded cursors. Requested targeted fixes:

1. Runtime-role permission for pause-reserve adjustment; preserve cancellation
   when ordinary storage admission is exhausted.
2. An old worker must not pause a different worker's expired lease.
3. Validate lease expiry immediately before terminal publication and bound
   statement/unit execution.
4. Bound identity ciphertext bytes before fetching/decrypting many observations.
5. Always label evidence references representative, per the approved contract.
6. Include the implemented variant aggregation table in the frozen contract.

The implementer accepted these corrections; item 1 was also observed as an actual
PostgreSQL permission failure. Targeted recheck and passing recovery evidence are
pending. The inherited core snapshot handoff receives its separate required
independent-process reproduction; no outcome is assumed.

## Shared Web integration gate

Coordinator ran lint, type checking, all Web tests and production build in the
migration worktree. One initial lint warning in the coordinator's component
attribute ordering was corrected. The completed run passed 86 files / 1,181
tests, type checking, lint and build. Vite retained its existing large-chunk
advisory. This does not substitute for the required actual API browser walkthrough.

No third broad implementation review is planned. Any second pass is restricted
to these findings and concrete test failures under D-050.
