# Mobile005 /010f3 — combined final verification

**IN PROGRESS.** Coordinator source `a52a1e620c4c63a1d23724f5de8f6aa15280fe4c`,
root `/Users/karrad/projects/crm`. Evidence below is relative to
`/private/tmp/crm-mobile005-010f3/integration`. Native/browser/fidelity and D-050
proof remain in their owning records; independent broad reviews returned changes required; bounded corrections are active.

## Sequential repository gates

Each command uses the private environment, Cargo/Web/SQLx output directories and
serialized database lock through:

```sh
python3 /private/tmp/crm-mobile005-010f3/run-db-check.py --lane integration --log LOG -- ../scripts/GATE
```

| Gate | Result | Evidence |
|---|---|---|
| `check` | PASS,267.78s:992 Rust,5 doctests,1,264 Web,51 preflight,11 worker tests; formatting/lint/type/build/fences pass | `final-check2.log` |
| `sqlx-prepare` | PASS,94.39s; query cache unchanged | `final-sqlx-prepare1.log` |
| `check-db` | Pending | No pass claimed |

The first `check` failed Clippy on a needless borrow in a migration test helper
(`final-check1.log`,108.54s). The one-line correction `839155a` was integrated at
`a52a1e6`; the complete corrected gate passes. The failed attempt is retained.
SQLx uses a separate copy-on-write cache after verified inactive incremental-cache
cleanup, recorded in `sqlx-cache-isolation.json`; no shared release output changed.

These passes belong to the pre-correction source. The migration handover and
native validation corrections require replacement final gates; the full DB gate
was deliberately held before running on source already known to require changes.

## Reused acceptance and protected resources

The actual installed/native journeys, final retained browser reconciliation,
source/native preservation, functional concurrency/failure cases and single paired
Today/changed-query proof are attributed in the linked
[implementation status](MOBILE_005_010f3_IMPLEMENTATION_STATUS.md).
No unchanged benchmark was repeated. `mobile005-performance-reuse-13640c4.json`
records unchanged hot source paths; later migration/readiness changes do not alter
those Person/Today statements.

`protected-resources-before-final-gates.json` verifies all74 protected artifacts,
including root `.env` and shared Web output, and all four shared listener PIDs.
`native-installed-inventory-corrected.json` confirms current iOS005 QA/upgrade
stores and six retained Android identities' protected stores. Its first probe was
inconclusive because simctl rejects the shutdown simulator and Android uses
`no_backup`, not `files`; the corrected read-only inventory documents that limit.
Actual store/key/receipt preservation is proved by the native verification records,
not inferred from this presence check. Final protected-resource comparison remains.

## Replacement gate attempts and runner correction

At `bc4af33`, `final-check3.log` fails after992 Rust tests/five doctests because
`pnpm` is absent from the runner PATH (237.72s,exit127). The already-cached pinned
pnpm11.22.0 is exposed through a private shim; `pnpm-gate-toolchain.json` records
its source/version/hash. No global installation or repository tool change occurs.

`final-check4.log` then passes the complete check (29.24s), and
`final-sqlx-prepare2.log` passes (97.9s; offline cache unchanged).
`final-check-db1.log` fails at the historical activity upgrade fixture:28 tests
pass, one fails,1047 are not run. The only before/after difference is the newly
added `person.details_revision = 1`; all existing fields are identical.
The fixture now explicitly verifies this new default alongside the other derived
revisions before comparing every old field, while retaining complete upgraded-row
equality after startup/API reads. The reviewer accepted this two-line test-only
correction. `historical-upgrade-details-revision1.log` passes1 test,40.01s total /
4.13s test. No production code changes.

Both failed runs remain retained. Replacement final sequential gates after this
focused correction remain required; no incomplete DB run is a pass.
