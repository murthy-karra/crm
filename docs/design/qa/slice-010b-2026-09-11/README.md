# Slice 010b synthetic implementation evidence

Implementation branch: `codex/slice-010b-core-snapshot`, based on approved
planning commit `49581a0`. Full commands, failures/corrections, review rounds,
acceptance results and limitations are in the
[verification record](../../../tasks/SLICE_010b_VERIFICATION.md).

This is isolated synthetic verification, not a shared-development or production
release. The actual API uses the compile-fenced `snapshot_qa` example with an
injected fixture reader and the explicit disposable `crm_slice010b_qa` database.
The Web runs as a production `web/dist` build through Vite preview. No authenticated
FUB request, customer-data processing, import or cutover was performed.

- `source-sha256.json`: exact changed implementation/configuration/contract files;
  all 37 hashes independently match; private environment files are excluded.
- `build-sha256.json`: actual test-support API binary and 69 final Web build files.
- `gate-results.json`: final sequential SQLx preparation, service-free checks
  and database checks, including counts and hashes of private full logs.
- `normal-api-result.json`: six source families, qualified note-detail gap,
  semantic variants, one-item record/group/member pagination and safe counts.
- `business-counts.json`: all 45 non-migration/non-session business-table counts
  before capture, after the normal API capture/preview and after all walkthroughs;
  all three inventories are equal.
- `browser-flow-result.json`: confirmation, reload, source recovery, cancellation,
  older/partial reports, retained reads after disconnect, Organization/member
  isolation and cross-tab session clearing.
- `browser-budget-result.json`: small-limit exhaustion, deployment ceiling
  changes, stale approval rejection, explicit budget approval without automatic
  work, source resume and DB-only resume of the same preview.
- `query-plans.json`: 13 actual `EXPLAIN (ANALYZE, BUFFERS)` plans from the
  25,000-People fixture. The fixture includes one shared phone, 100 repeated
  observations and one record beyond the frozen boundary. It verifies indexing
  and bounded group pagination, not source-capture fidelity or production scale.

- `final-browser-result.json`: final-build render with no page errors or migration
  mutations; capture, preservation notice, escaped notes and budget confirmation.
- Six PNGs show the final production build at desktop and 390px widths. All were
  visually inspected. The narrow layout is Web QA; native SwiftUI/Kotlin work
  belongs to later slices.
- `artifact-sha256.json`: integrity inventory for the evidence bundle, excluding
  that manifest itself.
- `handoff-checks.json`: final source/link/JSON/whitespace checks, bounded
  credential-pattern scan and confirmation that temporary QA ports are closed.

All required full gates passed, followed by final screenshots/build hashes.
All screenshots use synthetic users/source records and empty credential inputs.
Private credentials, session cookies, logs and temporary runtime configuration
remain outside Git. Temporary QA API/Web servers are stopped. Live authorized
FUB validation remains user-deferred; no runtime release was performed.
