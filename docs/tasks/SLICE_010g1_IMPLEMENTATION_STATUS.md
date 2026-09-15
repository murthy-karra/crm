# 010g1 — Implementation status

**IN PROGRESS — D-092, 2026-09-15.** User accepted P1–P4 and shared contracts
through Lavish Send & End; do not reopen the ended session. Deployment deferred.

- Base `dd140b0`, branch `codex/010g1-family-refresh`; one primary writer.
- Independent planning review READY, round 1; single reviewer retained for later
  implementation review. No implementation approval remains pending.
- Concrete [contract](SLICE_010g1_CONTRACT.md) records columns, proof, ownership,
  lock order and accounting. Policy model and focused regressions are in progress.
- Isolated Rust target `/private/tmp/crm-010g1-target-20260915`; Web output will be
  `/private/tmp/crm-010g1-web-dist-20260915`. DB work stays synthetic and serial.
- Shared 010e5 runtime, native stores and verified local 010e6 remain preserved.
- User authorized a foundation checkpoint commit and continued implementation.
  Merge, push and deployment remain deferred.

Remaining: persistence guards/accounting; source/cohort/baseline discovery;
metadata/activity/history execution; commands/bounded readers; common Web workflow;
independent implementation review; database/browser/performance/final gates.

### Foundation checkpoint

- Policy comparisons and bounded evidence encryption: eight focused Rust tests
  pass (`/private/tmp/010g1-policy-tests.log`). Evidence rejects rebinding across
  Organization, bundle, plan, family, revision, row or purpose and enforces size
  limits before encryption/decryption. Comparison guards cover edit-and-revert,
  deletion, ownership and history corrections.
- Persistence draft adds immutable write proofs, tenant/owner references, native
  before/after digests, account-qualified activity identities, current-head CAS,
  and 60-second lease/epoch fences. Exact timestamp values are included in proofs.
- A foundation revision applied to `crm_010g1_schema_20260915`; subsequent guard
  edits still need a fresh schema run and runtime regression tests. This is not
  final schema or release evidence. Accounting/history extensions, capability
  inventory, application workers and Web integration remain unfinished.

The two synthetic database foundation regressions now pass
(`/private/tmp/010g1-foundation-db-tests.log`): permanent capability enforcement,
timestamp coverage in write digests, exact original-result cohort binding,
active-lease theft rejection, token-bound renewal and terminal cancellation.
An initial schema syntax error and stale compatibility hashes were corrected;
the prior failed test run is not counted as passing evidence. The four existing
shared schema inventories now match the observed workspace function definitions.
Full 010g1 inventory and accounting are still pending; a retained-size function
has been drafted but has not yet been wired to ledgers or verified.

Latest foundation checks: **12 policy/evidence/source/count tests passed**,
**2 database foundation tests passed**; a fresh schema application and a direct
retained-size regression (UTF-8 text, decoded ciphertext/nonce bytes, encoded
counts, absent rows) passed. Formatting and tracked diff whitespace checks pass.
The database-test linker reported the existing-size warning about a large
`__eh_frame` section; tests completed successfully. No full-package test, browser,
performance or independent implementation review has been completed.

The implementation remains a foundation, with no exposed refresh HTTP commands
or running refresh worker. In particular, the draft retained-size function is
not yet connected to reservations/ledgers, history correction storage/readers
are absent, and native-write execution has not been tested end to end. Do not
merge/deploy or report 010g1 complete from these targeted checks. The user has
authorized committing this explicitly incomplete foundation checkpoint.
