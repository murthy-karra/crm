# Slice 010f3 — verification record

Implementation branch: `codex/migration-010f3`, base `fcc05b3`. This record
separates completed local evidence from the coordinator-owned integrated gates.

| Check | Status | Evidence / scope |
|---|---|---|
| `git diff --check` | passed | Local 010f3 owned files, before DB check. |
| `cargo check -p crm-api` | passed | Isolated target `/private/tmp/crm-mobile005-010f3/migration/target`; compiles the admitted domain plus registered API route. |
| isolated `db_metadata_import_gate` | passed | First run exposed the pre-handover identity-guard regression (2 passed / 4 failed); `5e06b2c` scopes that guard to durable readiness. Rerun through `run-db-check.py --lane migration` passed all 6 focused tests. Logs: `/private/tmp/crm-mobile005-010f3/migration/db-metadata-gate.log` and `db-metadata-gate-rerun.log`. Post-handover claim-proof and admitted-child tests remain required. |
| Web typecheck | blocked locally | `npm run typecheck` fails before checking source because `vue-tsc` is absent (`sh: vue-tsc: command not found`). No dependency installation was performed. |
| API/browser acceptance | pending | Requires integrated readiness/worker/router guard and synthetic terminal-admission capture; not claimed. |
| final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db` | coordinator-owned pending | Must run sequentially after Mobile005 and 010f3 integration. |

No live FUB/customer request, service deployment, shared runtime mutation or
release artifact overwrite was performed. Required remaining coverage includes
the ready-handover race, rejected old original identity/native commits, rollback
accounting, exact cancelled remainder, all private permit negatives, two-Org
isolation, and real desktop/390px workflow.
