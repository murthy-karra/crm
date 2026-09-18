# Migration backlog

Updated 2026-09-17. Migration follow-up is parked at the user's request.
This is a handoff list, not authorization to start another implementation,
contact a live source, activate a workspace or deploy a release. Existing
decisions and accepted specifications remain authoritative.

## Completed checkpoint

`codex/migration-completion` at `3db23e0` contains the coverage inventory,
reconciliation API/Web panel, repeated-refresh head fix, report deadlock fix,
and regression coverage. It is committed locally, not merged, pushed or deployed.
The [verification record](../reviews/migration-completion-2026-09-17.md) owns
test results, independent review and the seven query plans at 25,000 People /
50 members. Do not reopen that completed work as an implementation backlog.

The report summarizes historical initial lineage results and latest generic
metadata/activity/history refresh plans. It is not a cutover certificate.
Supported imported workspaces remain in administrator review.

## Release follow-up

**MIG-01 — Merge and release the verified branch.** Pending a later release task.
Compare the branch with current main, resolve any intervening changes, reuse
applicable evidence and rerun checks affected by changes. Merge/push and any
shared-development deployment are separate from this backlog-writing task.
Done when the requested integration/release has its actual revision, checks and
runtime evidence recorded. Use the [release process](../prompts/07-deploy.md).

## Before live customer migration

**MIG-02 — Qualify the real FUB source.** Explicitly deferred. Requires an
authorized test account/dataset and current release/data-readiness gates.
Verify production HTTPS adapter behavior, account identity, pagination,
permissions, restricted/inaccessible records, rate limits and retry behavior;
reconcile source totals and retained payloads. Qualify the standalone tag catalog
separately from embedded Person tags. Done when a versioned coverage matrix
distinguishes verified, inaccessible, unsupported and unknown source data.
Owners: D-060/D-061 and the [migration summary](SLICE_010_MIGRATION_SUMMARY.md).

**MIG-03 — Complete customer-data readiness.** Requires the outstanding
privacy/retention decisions and operational ownership. Define and prove erasure,
key lifecycle, retention/deletion handling, legal-hold interactions where
applicable, and restore procedures. Resolve or disposition the existing
multi-worker handoff and SQLx cancellation residuals using reproducible evidence.
Done when the [production-readiness gates](PRODUCTION_READINESS.md) have named
owners and passing evidence. O-012/O-013 own erasure; this backlog selects no policy.

**MIG-04 — Specify and implement activation/cutover.** Decision-dependent.
Define final source cutoff/delta capture, reconciliation acceptance, exceptions,
confirmation, recovery/rollback, and transition out of administrator review.
Decide whether an immutable cutover report is needed. Done when an accepted
specification and end-to-end evidence cover safe activation and recovery.
Preserve D-064/D-065 until that capability is accepted and implemented.

## Missing fidelity and destination work

**MIG-05 — Represent retained addresses and relationships.** Source evidence is
preserved only when returned inside captured Person records; native import
representation remains a gap. First settle source-to-destination semantics,
identity, local-edit conflicts and repeat behavior in family specifications.
Done when qualified fields are usable in the destination and unsupported
variants remain visible in reconciliation.

**MIG-06 — Capture and import remaining structured families.** Appointments,
Deals, external files, and automation/settings are not covered by current
migration capture. Scope each family separately after destination capability
and source qualification exist; do not conflate Person, Inquiry and Deal.
Done per family when preserved source, typed imports, retries/deduplication,
tenant isolation and honest reconciliation are verified. Use the
[family ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md) for specification questions.

**MIG-07 — Qualify and represent correspondence/media content.** Historical
events/calls/texts currently supply metadata, not correspondence bodies or media.
Email, recordings and attachments need their own source, access, storage and
retention/consent contracts. D-062/O-015 own bulk-content storage; recording
policy remains separate. Done when accepted content paths preserve fidelity
and access/erasure rules without manufacturing native contact credit.

**MIG-08 — Decide source deletion and ongoing-sync semantics.** Deferred policy
work. Missing records, inaccessible data and source 404s are not deletion proof.
Specify any future tombstones, removals, local-edit precedence and prevention of
resurrection before adding ongoing synchronization. Existing repeatable retained
refresh and exact remainders remain available; no replacement engine is implied.
Done when qualified deletion signals and failure/retry behavior are explicitly
approved and tested.

## Optional follow-ups within existing capabilities

**MIG-09 — Expand reconciliation navigation and People-core coverage.** The
current panel deliberately leaves later People core refresh and mapping-repair
outcomes in their detailed review panels. If a unified view is selected, define
separate units/cohorts and immutable evidence links without adding attempts or
historical holds to current outcomes. Done when cross-run totals reconcile and
supported detailed evidence can be reached from the summary.

**MIG-10 — Extend browser coverage of existing engines.** Reuse the isolated
Playwright system for standalone People refresh, admission, admitted dependent
families, mapping repair/recovery and source failure/pagination journeys. Existing
DB tests remain evidence; the sixteen-step migration browser journey does not
cover every branch. Also audit manual acceptance fixtures versus the ordinary
ignored-test runner so normal testing does not require interactive fixture
inputs or privileged scale setup. Done when selected journeys run reproducibly
with real guards, tenant checks and verified cleanup. See
[current E2E boundaries](../tasks/WEB_E2E_MIGRATION.md).

**MIG-11 — Revisit report performance only on evidence.** No current D-050
blocker. Latest-refresh aggregation measured approximately 1.3 seconds with a
roughly 12 MB temporary sort in the recorded synthetic envelope. Revisit if
representative report use warrants it; retain exact query plans and measure
relative changes. Production-shaped capacity testing remains separate. Do not
start another benchmark matrix merely because this item exists.

## Resuming work

Select one bounded item and read its owning decisions/specifications first.
Keep live-source and customer-data gates explicit. Update this file when an item
is assigned or closed; keep detailed execution history in the linked evidence,
not duplicated in project status. Until then, migration implementation is parked.
