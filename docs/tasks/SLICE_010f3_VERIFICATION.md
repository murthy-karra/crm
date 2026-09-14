# Slice 010f3 — verification record

Implementation branch: `codex/migration-010f3`, base `fcc05b3`. This record
separates completed local evidence from the coordinator-owned integrated gates.

| Check | Status | Evidence / scope |
|---|---|---|
| `git diff --check` | passed | Local 010f3 owned files, before DB check. |
| `cargo check -p crm-app` | pending completion | Isolated target `/private/tmp/crm-mobile005-010f3/migration/target`; the new domain module is compiled directly. |
| isolated `db_metadata_import_gate` | running | Invoked through `run-db-check.py --lane migration`; log `/private/tmp/crm-mobile005-010f3/migration/db-metadata-gate.log`. This applies the full migration chain to the isolated synthetic DB and detects SQL syntax/guard regressions. |
| Web lint/type/unit | pending | New admitted API/panel await shared route/navigation registration and API fixtures. |
| API/browser acceptance | pending | Requires integrated readiness/worker/router guard and synthetic terminal-admission capture; not claimed. |
| final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db` | coordinator-owned pending | Must run sequentially after Mobile005 and 010f3 integration. |

No live FUB/customer request, service deployment, shared runtime mutation or
release artifact overwrite was performed. Required remaining coverage includes
the ready-handover race, rejected old original identity/native commits, rollback
accounting, exact cancelled remainder, all private permit negatives, two-Org
isolation, and real desktop/390px workflow.
