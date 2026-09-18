# Migration coverage, repeated refresh, and reconciliation

Status: scoped implementation verified, 2026-09-17. The
[verification record](../reviews/migration-completion-2026-09-17.md) owns final
tests, review, query plans and residuals. Local branch only; not merged or deployed.

## Assigned scope

The user authorized migration follow-up work 2–4 in separate parallel worktrees
after merging the Person projection changes. Base: `d3b9f39`. Live FUB
qualification remains deferred. Existing decisions D-059–D-092 and the released
010g1 contract remain authoritative. This task owns the additive read contract
below; it does not change source ingestion, mutation semantics or activation.

Inspection found that repeated family refresh, accepted baseline heads, resume,
idempotent receipts and exact remainders already exist. Reuse those mechanisms.
Missing destination domains cannot be declared imported merely because raw
payloads are retained. Source absence remains unknown, never a deletion.

## Bounded deliverables and ownership

- Coverage lane (`codex/migration-coverage`): static typed coverage inventory,
  inventory unit tests, and corrected migration summary. Distinguish embedded
  tags from an unqualified standalone catalog; metadata from correspondence
  bodies; preserved source from usable native records.
- Delta lane (`codex/migration-deltas`): multi-cycle regression tests over
  existing typed commands/workers, including accepted heads, local conflicts,
  retries, absent source and supported repair/remainder behavior. Narrow fixes
  require coordination; no competing refresh engine or new deletion semantics.
- Reconciliation lane (`codex/migration-reconciliation`): dedicated application
  reader, route, Web client and panel, and focused tests. Reuse existing durable
  evidence without adding report tables or another worker.
- Integration (`codex/migration-completion`): shared module/router/test wiring,
  MigrationView insertion, browser journey assertions, final checks and review.
  Only this lane may create a migration, if evidence demonstrates a need; none
  is currently planned.

## Additive shared contract

`GET /api/migrations/fub/imports/{original_import_id}/reconciliation` returns a
bounded, read-only summary for an authenticated Organization administrator in
the migration review workspace. It uses one consistent database snapshot and
validates the original import under the trusted Organization. Foreign resources
must not be disclosed. Responses are `Cache-Control: no-store`.

The summary identifies its observation time and source/import evidence, shows
the original/admitted/recovered cohorts and family outcomes, and includes the
coverage inventory with explicit limitations. Counts/revisions use decimal
strings. Do not add incompatible units or multiple attempts together as unique
People totals; distinguish original results, accepted heads and later refresh
outcomes. A cancelled family's successor must not erase other families' results.
Source enumeration completeness and destination results are separate facts.
Unknown is not zero. No raw customer content, credentials or retained payloads
appear in this DTO. The response is capped at the existing 512 KiB convention.

This is a live observation, not an immutable cutover certificate. No cutoff
policy, activation readiness state, source fetch, mutation or new retention
policy is introduced. Existing detailed readers remain the route to evidence.
The Web panel must discard stale responses on Organization/session changes and
show loading, denied, empty and error states honestly.

## Repeat-cycle defect found during verification

The new same-Organization regression reached a previously untested second
refresh execution. PostgreSQL rejected advancing the existing generic family
head with `stale initial family head`: `INSERT ... ON CONFLICT DO UPDATE` invokes
the `BEFORE INSERT` guard before choosing its conflict action, while a subsequent
refresh correctly carries a non-null expected head.

Integration owns the narrow Rust fix in `family_refresh/execution.rs`: an
existing expected head uses an explicit conditional update of its result and
version; a first head uses insertion with conflict-do-nothing. Both require
exactly one affected row in the same native-write/result transaction. Existing
tenant, lease, provenance and compare-and-swap guards remain authoritative; the
original storage payer remains unchanged. No schema or mutation contract change
is needed. The regression and affected family execution tests must pass before
this fix is considered accepted.

## Verification

Each lane uses isolated build output. Database checks run serially. Integration
runs formatting, lint, production compilation, unit/Web tests, focused database
and authorization/tenant tests, and the migration Playwright journey through
the real application and synthetic source adapter. Inspect the new report
query plan at the D-050 envelope. Obtain independent substantive review, repair
blocking findings, and record actual evidence and remaining gaps before calling
the scoped work complete. No live qualification or production readiness claim
may be inferred from synthetic acceptance.
