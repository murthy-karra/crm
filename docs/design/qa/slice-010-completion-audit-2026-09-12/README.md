# Migration completion audit evidence — 2026-09-12

See the [audit report](../../../tasks/SLICE_010_COMPLETION_AUDIT_2026-09-12.md)
for the verdict, findings, scope and interpretation. Audited product source is
main `028d613`, deployed merge `fc5a875`, implementation `9f457bc`.

- [Exact source identity](source-integrity.json) and
  [fresh baseline gate identity](baseline-fresh-gate-identity.json).
- [Live Git/runtime/HTTP/DB/backup audit](live-audit.json), with its
  [read-only helper](live_audit.py). The helper is specific to the observed
  release paths/processes; inspect current identity before reusing it.
- [Preserved recovery files and retired binaries](recovery-artifact-audit.json).
- [Overnight warnings](runtime-warning-qualification.json) and
  [later bracketed clock samples](clock-diagnostic.json). Power-state correlation
  is not a controlled reproduction; later clock samples do not establish exact
  clock skew during the failed tests.
- [Fresh service-free check](checks/check.log),
  [first DB setup failure](checks/check-db-attempt-1.log),
  [setup qualification](checks/check-db-attempt-1-qualification.json),
  [second DB attempt/chronology failure](checks/check-db-attempt-2.log),
  [isolated chronology failure](checks/call-history-followup.log), and
  [complete 908-test pass](checks/db-complete-no-fail-fast.log).
  Neighboring JSON files record exact commands, exits and durations.
- [Health annotation follow-up](checks/health-leak-followup.log) and
  [activity-source annotation follow-up](checks/source-leak-followup.log).
  Both passed in isolation without the earlier nextest LEAK annotation.
- [A6 source supplement](a6_regression_test.rs.txt),
  [exact test patch](a6-supplement.patch) and [two-test pass](checks/a6-supplement.log).
  The patch was applied only after unchanged-source gates. It adds five note
  scenarios and ten task records with explicit native-state/identity invariants.

To reproduce A6, use an isolated checkout of the audited HEAD, apply the patch,
provision the repository’s two database roles on disposable PostgreSQL 18.6,
and supply synthetic `CRM_DB_APP_PASSWORD`, `CRM_DB_MIGRATOR_PASSWORD`,
`DATABASE_URL` (migrator role), and applicable Centrifugo settings. Run the command
recorded in `checks/a6-supplement.json`. Never point this fixture at shared
`crm_dev`; native preexisting rows are deliberately seeded for the collision cases.

[Inventory](evidence-inventory.json) records copied bytes, original/published
SHA256 values and any trailing-empty-line normalization. Generated private
credentials were scanned for exact matches and excluded. No customer content,
environment file, backup bytes or key is published. The historical implementation
evidence remains in its original 010f2 directory; it was inspected, not relabeled
as a fresh browser or performance execution.

The [cleanup record](cleanup.json) records removal of owned audit resources and
preservation of shared runtime/artifact identity. This audit does not restart or
deploy services, refresh the expired import-compatibility report, contact FUB,
restore a backup, activate a workspace or establish recurring monitoring.
