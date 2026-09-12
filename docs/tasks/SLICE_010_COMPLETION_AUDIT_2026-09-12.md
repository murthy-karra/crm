# Migration completion assurance audit — 2026-09-12

User request: “do a deeper dive and ensure that finished claims are valid.”

Audited source: main `028d6133e1b7c3f81642275e030f63e98b2cca49`, containing
implementation `9f457bc8db8707fa4ce361aac485fd6bf928bf34` and deployed merge
`fc5a8757bcb2591a43976544e282722b3616039f`. The scope is the preceding answer’s
010f2 completion/deployment claims and the migration sequence, with older-slice
evidence spot checks. This is not a new live FUB qualification or a whole-product
certification. AGENTS, D-050/D-068, the accepted specification and execution brief
govern the audit. The explicit user request authorizes this additional bounded
assurance review; no product redesign or shared-contract change is made.

**Verdict: the 010f2 implementation/integration/shared-development release claims are supported.**
The claim must remain scoped to synthetic verification and shared development.
A pre-existing call-history ordering defect reproduced twice, despite a later
complete 908-test pass; the repository is not proven consistently green. The
previously missing A6 examples now pass additional isolated audit tests.

## Findings and qualifications

### Existing call-history chronology defect reproduced

The unchanged call-correction test failed in the full DB gate and its isolated
follow-up at `db_calls_corrections.rs:371`. It expected two correction events
after `call_completed` and found only one. Earlier assertions in the same test
passed: all three attempts exist, the correction chain/head is correct,
`recorded_at` strictly increases, and superseded flags are correct.

This is the older chronology issue recorded in 011d verification and project
residuals. The current reproduction does **not** support the older “microsecond
ties” explanation. Hangup supplies application-host `Utc::now()`, while corrections
use PostgreSQL `clock_timestamp()`. Person history displays correction
`recorded_at` as its event time and sorts it beside the host-stamped call end.
Five later bracketed clock samples support this mechanism: the last four put the
host about 0.39–0.72 ms ahead of the owned PostgreSQL clock. They are not
measurements from the failing tests; SQLx removed those old test databases by the
end of the passing rerun, so exact historical skew is not established.
Clock skew can therefore invert this ordering; it is not evidence of a lost
attempt or an incorrect effective call outcome. The Web folds attempts into one
call row and obtains the effective outcome from superseded state.

Sources: [failing assertion](../../backend/crates/crm-api/tests/db_calls_corrections.rs),
[hangup](../../backend/crates/crm-app/src/domain/commands/hangup_call.rs),
[settlement](../../backend/crates/crm-app/src/domain/telephony/settle.rs),
[correction](../../backend/crates/crm-app/src/domain/commands/correct_call_outcome.rs),
[history](../../backend/crates/crm-app/src/domain/person/queries.rs),
[prior evidence](SLICE_011d_VERIFICATION.md). The accepted ordering is in
[006c §5](../specs/SLICE_006c.md). Treat consistent causal chronology as a bounded
follow-up; do not weaken the test or declare the observed failure harmless.

### A6 required more direct evidence

The checked-in activity collision test covers task equality, title edits,
tombstones, a legacy key and a missing target. The spec explicitly also names
snooze/reopen/completion and note protection. Static inspection found the relevant
field comparisons, but that alone does not prove those cases.

The audit added and ran two temporary test functions in the isolated checkout, after
finishing all unchanged-source gates: five note cases (equal, edited body,
deleted, changed author, changed time) and ten task records (equal open/completed,
a new insert control, snooze, reopen, local completion, changed creator/assignee/
time/completer). They assert explicit plan/result classifications, every original
native column unchanged, exact identity/count results, no duplication/resurrection
and zero new source calls. The source supplement and executed output are retained
as audit evidence; they are not silently described as original release coverage.
**Both tests passed (15.34 seconds test execution, 44.19 seconds including build).**
This closes the specific behavior-evidence gap for this audit. The supplement is
retained for promotion to permanent regression coverage; no product/test source
was changed in main.

### Overnight availability was not warning-free

The release’s 214.61-second zero-warning observation remains valid: it ended at
05:18:29 UTC. A subsequent episode produced 4,925 warnings between 06:44:02 and
13:09:43 UTC: 295 explicit pool timeouts and 4,630 generic migration database
errors, with no ERROR lines or transaction-state notices in the inspected log.

The host entered sleep at 06:42:13, cycled through 39 background wakes, and fully
woke at 13:09:42 UTC. All warnings align with that window; the last arrived about
1.7 seconds after full wake. PostgreSQL has no recorded restart/OOM, and its scoped
log has no WARNING/ERROR/FATAL entries. This strongly supports sleep-associated
database availability loss, but does not prove the precise Docker/network/pool
mechanism. Current health/readiness and read-only DB reconciliation pass.
Do not expand the bounded release observation into a claim of uninterrupted uptime.

## Claims checked against evidence

| Claim | Independent check and result |
|---|---|
| Committed, merged and pushed | Live `git ls-remote` matches local main at `028d613`; the implementation and deployed merge are ancestors. Main was clean at audit entry. |
| Correct source was verified | Rehashed all 1,016 frozen source/configuration files: zero mismatches. No manifest file differs from implementation or deployed merge. Manifest digest matches the recorded value. |
| Deployed to shared development | Actual listeners remain API PID 25757 and Web PID 25780 with expected executable/cwd. All three backend binaries and 73 Web files match release hashes. Public index, entry bundle and Migration bundle also match. Local/public health is 200, local DB readiness 200, and anonymous activity access 401. |
| Additive schema applied without tracked business changes | Read-only repeatable-read snapshot: all 35 applied migration checksums/success values match Git, latest `20260920000001`; 45 business-table counts and three workspace states match release, all 56 migration tables and admissions remain empty. Counts do not prove every business row’s contents. |
| Recovery/cleanup artifacts retained | Backup still has exactly 110,063,438 bytes, matching SHA256 and a readable 932-entry catalog. All 249 private original evidence files match bytes/hash; all 32 retired binaries match hashes and remain non-executable. Prior implementation worktree and QA directory are absent. No restore is implied. |
| Historical gates passed | Raw final logs confirm 935 Rust, 1,108 Web, five doctests, 20 release-preflight, 11 email-worker and 908 DB tests; SQLx completion and terminal success markers are present. Source identity ties those logs to the deployed product. |
| Measurement and browser claims are supported | Rechecked 238 artifact inventory entries and 19 release cross-artifact hashes, all 92 query probes, source SQL hashes, row/spill bounds, 80 measured paired responses plus ten warmups, and all three exact baseline functions against `cd3b010`. Recomputed p95: 14.145333 ms baseline, 15.906 ms current. No new performance run. |
| Exact synthetic import reconciliation exists | Raw browser evidence has 18 successful phases and three retained failed attempts; 84 screenshots exist. Recounted 54 notes/7 tasks complete, 27/7 cancelled, 54/7 recovered. All 12 logical store sums/reservations and 75 unrelated business-table baseline comparisons agree. Source/publication counters are unchanged. |

Three independent bounded lanes covered backend behavior/acceptance, Web behavior/
acceptance, and evidence integrity. The coordinator independently checked live
runtime, Git/remote, database, recovery files and fresh tests. No behavioral
010f2 defect was found in those source reviews; that is a bounded conclusion,
not proof that every possible path is correct.

Prior-slice spot checks substantiate raw totals for 010b (860 Rust/933 Web/809 DB),
010c (877/1,002/845) and 010f1 (900/1,054/881). The 010a check examined published
result JSON/hashes rather than the full original raw logs. Fresh workspace tests
include earlier migration regression coverage, but this is not a full independent
reconstruction of every historical slice’s acceptance run.

## Fresh verification

All fresh checks use an owned short-lived checkout from the exact audited HEAD,
APFS-cloned independent caches, pinned toolchains, PostgreSQL 18.6 on loopback
55439 and Centrifugo 6.9.2 on 18089, with fresh private synthetic credentials.
The coordinator owns the sole DB verification lane. No shared seed/reset,
business mutation, runtime restart, FUB request or external communication occurs.

| Fresh check | Result |
|---|---|
| `./scripts/check` | Exit 0, 199.58 seconds: 935 Rust, 1,108 Web, five doctests, 20 preflight and 11 email-worker tests; formatting/lint/build passed. One health-test LEAK annotation, isolated follow-up passed without it. |
| `./scripts/check-db`, attempt 1 | Setup failure: omitted audit environment variables; retained and corrected as described below. |
| `./scripts/check-db`, attempt 2 | Actual-schema `cargo sqlx prepare --check --workspace` passed. Test stage stopped at 91 passed / one failed / 816 unrun after the pre-existing chronology assertion failed. Script exit 100. |
| Isolated chronology follow-up | Same assertion failed again, exit 100. No source/test assertion was weakened. |
| `cargo nextest run --workspace --locked --run-ignored only --no-fail-fast` | Exit 0, **908/908 passed**, 415.74 seconds overall / 415.336 seconds test execution. One slow test and one activity-source LEAK annotation; its isolated follow-up passed without the annotation. |
| Two additional `activity_a6_audit_` tests | **2/2 passed**, including five note scenarios and ten task records, exact existing-row preservation and positive native controls. |
| Final unchanged-source check before the supplement | All 1,016 manifest hashes match, and the audit checkout was clean. The supplement was added only afterward. |

The full script is not relabeled as passing: its attempts failed. The eventual
schema/cache-plus-full-DB evidence is reported with the exact command that ran.
No new benchmark, live FUB flow or full browser replay was performed.

The first DB attempt omitted `CRM_DB_APP_PASSWORD` and
`CRM_DB_MIGRATOR_PASSWORD` from the audit environment. Ten tests failed during
fixture setup before application behavior ran; the role passwords already
existed in the owned service. The audit environment was corrected, with no
product change. Both attempts remain evidence. The service-free run passed with
one nextest LEAK annotation on `health::ready_returns_503_without_database_url`;
its isolated follow-up passed without that annotation. The later complete DB run
had one activity-source LEAK annotation; its isolated follow-up also passed.
These annotations are retained rather than described as an unqualified clean run.

## Boundaries that remain unfinished

- Actual FUB/customer-data qualification remains deferred. Empty live migration
  tables confirm this release/audit did not perform an import.
- The installed compatibility report expired at **2026-09-12 05:18:49 UTC**.
  Ordinary CRM access works; later confirmation needs a fresh actual-workload
  report. The audit does not renew it or establish recurring monitoring.
- Backup catalog/hash verification is not a restore exercise. Privacy/erasure,
  production identity/secrets/deployment and capacity gates remain separate.
- The prior SQLx transaction-cancellation notices still need controlled
  attribution before production cutover. The overnight pool-availability episode
  is distinct from those earlier notices.
- Browser scope stays qualified: cursor proof combines a real limit-25 API 409
  and separate default-50 UI refresh; headroom recovery did not exhaust the
  original allowance; retry outcome is proved by receipt/trace reconciliation,
  not its transition screenshot. The release browser’s final telemetry assertion
  was qualified offline, without a rerun.
- Remaining history families, complete standalone-tag capture, repair/deltas and
  activation still need their own specifications. 010d is the proposed next
  planning rung, not an accepted implementation or a claim of full migration.

The appropriate completion wording is: **010f2’s agreed synthetic implementation
and shared-development release are complete; real-source, cutover and production
readiness are not.** Preserve the chronology defect and runtime qualifications
when describing the current repository and deployment.


## Evidence and documentation cleanup

[Audit evidence](../design/qa/slice-010-completion-audit-2026-09-12/README.md)
retains successful and failed logs, exact source identity, live checks, backup/
retired-artifact checks, clock/power-state qualifications and the reusable A6
supplement. Log publication removes only trailing blank EOF lines and records
original/published SHA256 values; private environment files and credentials are
excluded. Existing 010f2 evidence is referenced, not relabeled as new execution.

Stale current-state references to a deployed 010f1 runtime and continued 010b
planning are corrected. The old implementation/review checkpoints gain explicit
historical labels, preserving their original evidence and chronology. No product
code, schema, dependency, authorization policy, runtime configuration or customer
data changes. No commit, push, deployment or recurring monitor is performed.

Owned audit services, volume, checkout and temporary branch are removed after
preserving evidence; shared services and recovery material remain. See the audit
cleanup record for verified final resource/source state.
