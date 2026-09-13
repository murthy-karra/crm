# Mobile 001 — Backend verification and native handoff

Backend foundation checkpoint under D-074. This record distinguishes focused
proof from the full regression gate, native acceptance and deployment. The
[contract](MOBILE_001_CONTRACT.md) and [actual JSON fixtures](../../mobile/contracts/README.md)
are the shared native integration surface.

## Implementation

Additive migrations `20260923000001` and `20260923000002` provide task/Person
revisions, context registration, content-free keyed operation receipts, trusted
admission serialization and bounded reconciliation manifests. The existing
AddNote/CreateTask/CompleteTask commands now expose transaction cores; Web and
Operator wrappers retain their DTOs and post-commit behavior. Mobile writes and
receipts commit atomically, and native completions enforce a task baseline.

The native adapter revalidates the existing session, current membership,
operational workspace and context binding. It implements seven-day authorization,
HMAC-bound retries/cursors, bounded pages and snapshot sealing. HTTP responses,
including upstream denials, have no-store. Query/lock/whole-request deadlines and
bounded request-independent cleanup are present. Readiness is separate from any
customer-data or distribution release.

## Focused evidence

- `SQLX_OFFLINE=true cargo check -p crm-api --features test-support`: passed.
- `cargo test -p crm-api --test all db_tasks:: -- --ignored --test-threads=4`:
  34 passed. Existing completion/reopen/delete semantics and HTTP shapes preserved.
- `cargo test -p crm-api --test all db_notes:: -- --ignored --test-threads=4`:
  19 passed, including content limits, permission races and publication behavior.
- Initial complete focused mobile suite: seven passed in 12.16 seconds. It
  exercised atomic duplicate/lost-response retries, payload mismatch, stale
  completion/reopen, create dependencies, database-failure rollback, lease
  renewal, current authority, tenant/account boundaries, deletion markers,
  pages/seal, admission/context rotation/cleanup failure, concurrent admission
  and the two-download bound.
- Representative fixture: 100 People, 1,000 notes, 1,000 tasks and 100 queued
  operations, each retried. Note/task pages returned all 1,000 rows in ten pages;
  accepted receipts outranked an older seal through Person revisions. The old
  fixed-clock Today DTO matched the seal's Today response exactly.
- The 25,000-Person selection accepted the full envelope and explicitly refused
  25,001. A PostgreSQL EXPLAIN sample is retained in
  [selection_25000_plan.json](../../mobile/contracts/selection_25000_plan.json).
  It is a synthetic development observation, not a production capacity claim.

The first full database gate exposed an integration regression: valid 010f2
activity inserts triggered a derived Person revision update that the ordinary
review-hold guard rejected. Migration `20260923000002` confines the derived update
to schema-owned trigger functions with qualified tables, a fixed safe search_path
and no PUBLIC execution. Original business BEFORE guards remain authoritative;
there is no new client permit or request-callable bypass. The three originally
failing lifecycle tests passed after correction. The focused lifecycle run
passed nine tests; its trace-capture positive-control test also passed when run
alone in its own process (11.51 seconds), as the full Nextest gate runs it. The
assertion was preserved.

The final focused mobile run passed all eight tests in 52.95 seconds under
concurrent synthetic regression load. Added proof includes a 20-second upstream
session-table lock deadline with no-store and recovery, the 5-second statement
rollback, direct review-hold revision-write denial, revoked trigger execution,
and content-free removed-pin counts. The focused suites above plus this run are
the local dependency checkpoint evidence.

**Passed at checkpoint:** coordinator full `./scripts/check` (125 seconds),
including formatting, clippy, production compilation, unit/doc tests and Web/Operator
checks. **Pending:** full database regression and the D-050 same-build timing pair.
SQLx prepare-check passed against both additive migrations;
this is not a full-gate completion claim. The coordinator continues those gates
while native app implementation begins against the frozen contract. Native
acceptance and deployment remain unclaimed.

## Retained synthetic fixture

Run from the mobile worktree after exporting its private `.env`:

```sh
SQLX_OFFLINE=true cargo run --manifest-path backend/Cargo.toml -p crm-api \
  --features test-support --example mobile_fixture
```

The helper only accepts a database whose name begins `crm_mobile_`, applies
additive migrations and preserves an existing fixture. It never runs a reset.
The retained development database is `crm_mobile_001`; native API address is
`http://127.0.0.1:3101` on the host and `http://10.0.2.2:3101` in the Android
emulator. The configured native client must restrict credentials/cookies to its
trusted API origin.

Synthetic login: `agent@mobile.test`, password `Mobile-demo-only-123!`.
Second same-Organization actor: `second@mobile.test`, same synthetic password.
These credentials are deliberately public test data, never production defaults.

Retained identifiers:

- Organization: `4f35d754-6852-4418-a789-5b2d7091638f`
- Actor: `5431b023-0da7-4344-8e76-6e01c258d536`
- First Person: `2afb6552-eff3-4f86-9f29-bf7a5393758c`

Seed and app startup are separate. The coordinator owns API startup and the
native worktrees; this helper does not launch or deploy anything.
