# Bounded SQLx 0.8.6 cancellation dependency analysis

Date: 2026-09-12 UTC.

Scope: inspection of the locally pinned SQLx 0.8.6 transaction/pool implementation and the specified QA API warning log. This was a bounded dependency/runtime-warning analysis, not another application implementation review. No repository source inspection, dependency changes, database calls, browser operation, test execution, or reproduction was performed for this analysis. This private report is the only authored artifact.

## Conclusion

The pinned dependency has documented cancellation cleanup and a concrete source-derived cancellation path consistent with the observed transaction warnings. The available logs do not establish which path occurred, do not prove the warnings harmless, and do not establish application data failure. In particular, the BEGIN cancellation path below can leave server transaction state different from SQLx's transaction-depth bookkeeping; it should not be described as ordinary successful cleanup without additional evidence.

## Exact inspected dependency sources

Registry root: `/Users/karrad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`.

| File | SHA256 | Relevant one-based lines |
| --- | --- | --- |
| `/Users/karrad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlx-postgres-0.8.6/src/transaction.rs` | `3173deb653f48080aa3d4575e8e7e1b9cee4e6b73236efee8942879a9ed12520` | 18–40: BEGIN ordering; 45–69: COMMIT/ROLLBACK ordering; 73–79: queued rollback requires positive depth; 87–109: BEGIN rollback guard |
| `/Users/karrad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlx-postgres-0.8.6/src/connection/mod.rs` | `1b03f1869531596c75511d12c6abf796c09263df8697d3207357ddc981a26d6d` | 84–120: readiness draining and server transaction status; 123–139: queued queries and transaction-status predicate; 176–185: ping sends Sync and drains readiness |
| `/Users/karrad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlx-core-0.8.6/src/transaction.rs` | `b0c303f7e0c75abf3afb7a76758bfa1632b9a1268ffec3173435cb1fa4d04a7e` | 52–58: documented rollback on drop; 99–112: Transaction constructed after successful begin; 115–128: open flag cleared after COMMIT/ROLLBACK await; 260–274: drop queues rollback; 277–300: BEGIN/COMMIT/ROLLBACK SQL selection |
| `/Users/karrad/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlx-core-0.8.6/src/pool/connection.rs` | `e67d3755b3f9d57154658b1f898bb863d5fd553cdf089d86cd653ba5952bafa2` | 130–154: asynchronous return to pool; 198–209: PoolConnection drop; 272–327: return hooks, ping, failure closure and successful pool return; 307–313: explicit cancellation-recovery limitation comment |

These hashes were calculated from the local dependency files during the bounded inspection. They are not hashes of application source or of the final application binary.

## Established source behavior and plausible paths

### Documented transaction-drop and pool cleanup

`sqlx-core` documents that an open Transaction rolls back on drop. Its drop implementation queues rollback for the next asynchronous operation on the connection, including return to the pool. The pool return code explicitly comments that dropping an executor future during an await can leave an inconsistent connection state and describes its ping-based recovery as a temporary mitigation. Failed ping closes the connection; successful ping returns it to the pool.

This documents cancellation handling and its limitations. It does not document these particular warning messages as harmless.

### Plausible duplicate terminal command: "no transaction in progress"

PostgreSQL COMMIT/ROLLBACK execution is awaited before SQLx decrements `transaction_depth`; the generic Transaction clears its `open` flag only after that operation succeeds. If cancellation occurs after the server processed the terminal command but before SQLx finishes the await and bookkeeping, dropping the still-open Transaction can queue another ROLLBACK. An extra ROLLBACK after the server transaction ended is consistent with the observed warning.

This is a source-derived possible interleaving, not a reproduced or PID-correlated attribution of a QA warning. Other causes have not been excluded by this bounded analysis.

### Plausible BEGIN cancellation exposure: "already a transaction in progress"

The PostgreSQL begin implementation reads the current depth, queues BEGIN, and awaits readiness before incrementing the depth. At depth zero, cancellation during that await invokes the local rollback guard, but `start_rollback` does nothing because the depth is still zero. The generic Transaction object has not yet been constructed.

The pool-return ping sends Sync and drains outstanding responses. It does not require the backend transaction status to be idle or independently queue a rollback for an active backend transaction with zero SQLx depth. Therefore, a cancellation in this interval can leave or finish a server BEGIN while SQLx's depth remains zero, allowing a later BEGIN on that connection to encounter an already-active transaction.

This is a concrete dependency-code path and a reason not to assume benignity. It was not reproduced here. Whether a browser abort cancelled an application future at this precise point, whether that connection was reused, and whether any application transaction was affected remain unestablished.

## Observed log evidence

The inspected epoch log was `/private/tmp/crm-010f2-qa-694szdwe/api-final-activity.log`.

Initial interval:

- 04:20:34.393187 UTC: no transaction in progress.
- 04:20:34.463263 UTC: already a transaction in progress.
- 04:20:36.274921 UTC: no transaction in progress.
- 04:20:36.334982 UTC: already a transaction in progress.
- 04:20:39.495861 UTC: already a transaction in progress.
- 04:21:19.163302 UTC: no transaction in progress.
- 04:21:27.050045 UTC: no transaction in progress.

During inspection that epoch log also contained no-transaction warnings at 04:37:57.923249, 04:37:57.924166 and 04:38:01.156430 UTC. The warning entries contain no backend PID, query text, transaction identifier, or request identifier. Timing proximity alone cannot associate the warning pairs with the same connection or prove a cancellation cause.

Root's final aggregate `runtime-log-audit.json` reports **8 no-transaction warnings and 3 already-transaction warnings across all four API epochs**. That aggregate was supplied by root; this analyst did not independently inspect the other epochs or recompute the aggregate.

The 04:33:48.246508 HTTP 503 is separately attributed by root to the expected workspace-busy barrier. It is not evidence explaining the transaction notices.

## Root-reported runtime context and its limits

Root's read-only privileged observation during active workflow found six idle connections, one active connection (maximum observed transaction age 0.084722 seconds), and three idle-in-transaction connections (maximum observed age 0.101931 seconds). This found no long-running transaction at that instant. It does not establish historical connection state or warning attribution.

Root reports final native/business/storage audits passed: complete and budget workflows each produced 54 notes and 7 tasks; cancelled workflow retained 27 notes and 7 tasks. Exact body/actor/time/provenance checks, 25 unrelated table comparisons per Organization, and reconciliation of all 12 durable tables, measured/retained/native bytes and reservations passed. These are root-owned audit results supplied as context, not independently rerun by this analyst. They constrain observed business impact but do not prove historical pool transaction cleanliness.

## Evidence that would distinguish cleanup from meaningful failure

Without prescribing a fix or initiating further work, the discriminating evidence would be:

1. Correlation of a cancelled transaction phase and warning to the same backend PID and request.
2. Server transaction state and SQLx depth around connection release/reacquisition, showing rollback/closure or an active transaction returned for reuse.
3. Correlated subsequent behavior: transaction-local state, lingering locks, command failures, and native/result/receipt/ledger consistency.

A controlled cancellation reproduction could establish the dependency path; successful unrelated requests or a clean later snapshot alone cannot. No such experiment was conducted in this bounded analysis. No claim of proven harmlessness, proven data loss, or proven tenant-boundary failure is made.
