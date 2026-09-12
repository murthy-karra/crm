# Slice 010f1 synthetic verification

**Implemented and verified, 2026-09-11; uncommitted and undeployed.** Branch
`codex/slice-010f1-metadata-import` is based on main `f01c2e3`. D-066 authorizes
the retained-source tag/custom-field child. The deployed 010c runtime is unchanged.

All A1–A12 criteria pass within the synthetic scope. See the
[acceptance map](ACCEPTANCE_MAP.md), [complete chronology](../../../tasks/SLICE_010f1_VERIFICATION.md),
[review dispositions](IMPLEMENTATION_REVIEW.md) and the
[concrete contract](../../../specs/SLICE_010f1_CONTRACT.md).
The [machine-readable verification summary](checks/verification-summary.json)
records final counts, commands and the verified source identity.

## Executed checks

| Check | Final result / evidence |
|---|---|
| Sequential `./scripts/sqlx-prepare`, `./scripts/check`, `./scripts/check-db` | Passed; no SQLx cache diff. 900 Rust tests, five doctests, 1,054 Web tests, 881 DB tests, 19 preflight and 11 email-worker tests. Formatting, Clippy, dependency fences, production compilation, lint/typecheck and production build passed. |
| New metadata DB cases | All 36 passed within the 881-case gate, including source fidelity, current authority/foreign tenants, immutable receipt replay, expiry, quotas, tombstones, real concurrent cancellation and full transactional rollback. |
| Actual hot queries | 90 plans / 59 SQL hashes / 246 checks passed at 25,000 People and 50 members. [Measured plans and storage](PERFORMANCE.md). |
| Real API / production Web | Five final phases, 51 checkpoints, zero page errors, zero post-capture reader calls; all 18 screenshots inspected. [Completion](checks/browser-happy.json), [partial cancellation](checks/browser-cancel.json), [queue](checks/browser-budget-queue.json), [storage pause](checks/browser-budget-paused.json), [explicit resume](checks/browser-budget-resume.json). |
| Exact native reconciliation | Both completed fixtures have eight tag links and eight native values on five separate People. Exact tags, text, numbers, dates and literal choices match; overflow and unsupported cells stay held. [Native API evidence](checks/native-api-reconciliation.json). |
| Startup / compatibility | Guarded synthetic API health/readiness passed. [Actual-DB preflight](checks/synthetic-preflight.json) requires metadata capability while preserving ordinary 010c readiness. No report was installed or used for release. |
| Cleanup | All setup/browser/audit sessions revoked; isolated API/Web stopped; dedicated containers and anonymous PostgreSQL volume removed. [No QA listeners remain](checks/cleanup.json). |

The final scripts ran in order, taking 16.16 seconds for SQLx compilation,
54 seconds for the repository gate and 388 seconds for the DB gate. Database
execution was 364.314 seconds; one test was slow, none failed. Counts are actual
executed tests, not a sum of development reruns. Commands and checked log copies
are in [the log inventory](checks/log-inventory.json).

## Browser evidence

These are responsive **Web** checks at 1440×1050 and 390×844, not native mobile
implementation. Some screenshots capture the named evidence region at its full
height. Source content is escaped and available in bounded UTF-8 segments.

| State | Desktop | Narrow Web |
|---|---|---|
| Held plan and approved mappings | [Initial held plan](desktop-held-plan.png), [mappings](desktop-mappings.png) | [Plan](mobile-plan.png) |
| Named choices / subset confirmation | [Existing field dialog](desktop-existing-field.png), [confirmation](desktop-confirmation.png) | — |
| Exact large source evidence | [Loaded segment](desktop-large-field.png) | [Loaded segment](mobile-large-field.png) |
| Original tag occurrences | — | [128-digit source ID](mobile-128-digit-tag-occurrence.png), [long tag](mobile-long-tag-occurrence.png) |
| Completed import / Person provenance | [Completed](desktop-completed.png), [provenance](desktop-person-provenance.png) | [Provenance](mobile-person-provenance.png), [128-digit ID](mobile-128-digit-provenance.png) |
| Revoked / ordinary member access | — | [Administrator demotion](mobile-admin-demotion.png), [member hold](mobile-member-hold.png) |
| Terminal partial cancellation | [One of five People settled](desktop-partial-cancellation.png) | — |
| Current ceiling / explicit recovery | [Paused before Person writes](desktop-budget-paused.png), [resumed completion](desktop-budget-recovered.png) | — |

The completion workflow disconnects its source before confirming, deliberately
loses the successful confirmation response, and retries identical request bytes.
Cancellation preserves the settled Person result and releases both reservations.
Lowering deployment ceilings pauses before native Person writes; restoring them
does not resume automatically. Parent plans/counts and original workspace review
revisions remain unchanged throughout. Ordinary members and demoted admins cannot
read child evidence or use the operational workspace.

Initial failed attempts are retained: the plan collector corrected UUID fixture
bias and PostgreSQL JSON count parsing without changing production SQL or bounds.
Browser tooling corrected two DOM locators, capture waits and a read racing the
confirmation response. [Chronology](../../../tasks/SLICE_010f1_VERIFICATION.md)
distinguishes those attempts from final passing evidence. No production code
changed after the integrated review checkpoint.

## Source and artifact identity

- [Final verified source hashes](checks/verified-source-sha256.json): 975
  non-documentation repository files, including the new tests and collector.
- [Integrated review source](checks/integrated-r2-source-sha256.json) and
  [backend review source](checks/backend-review-corrections-source-sha256.json).
- [Production Web artifact hashes](checks/web-build-sha256.json): 72 files.
- [Browser artifact hashes](checks/browser-artifact-sha256.json) and
  [20 checked log copies](checks/log-inventory.json). Private environment-value
  and database-URL checks required no redaction in these selected logs.

These identify an uncommitted worktree, not a release commit. The API example is
compile-fenced test support and is not a production deployment artifact.

## Isolation and evidence boundaries

All new service checks use dedicated loopback PostgreSQL port 55431 and
Centrifugo port 18081. The synthetic API is restricted to port 3016 and database
`crm_010f1_qa`; production Web preview uses port 5186. No shared `crm_dev` reset,
customer credentials, real FUB request or workspace activation is permitted.
Private environment files, session credentials, original logs and setup controls
stay outside the repository. Published evidence contains synthetic data only.

The test-only API example validates its exact database/role/port scope before
mutation. It exercises ordinary typed commands, migrations, workers and HTTP
routes with a synthetic source reader. Its bounded worker scheduler enables
partial cancellation and pause/retry evidence without production permit bypasses.

The opt-in query collector uses actual application SQL and a 25,000-Person,
50-member synthetic fixture. Bulk inert metadata rows test query shape and
storage size, not executable fidelity or accounting correctness. Real encrypted
capture and command tests separately prove those properties. Logical retention
ledger bytes, native row sizes, indexes, relation sizes and replication/WAL costs
must remain distinct in the report. The fixture is not a maximum-capacity claim.

The disposable databases and services are now removed. Notes/tasks import,
full standalone-tag capture, repair/delta imports, activation and live
FUB/customer readiness remain separate work. Current native limits and the
administrator review hold are preserved. Commit, merge, push and deployment
remain later authorized steps.

Publication cleanup normalized trailing whitespace in six checked log copies.
Their original private hashes remain recorded alongside the published hashes;
no test result or source byte changed.
