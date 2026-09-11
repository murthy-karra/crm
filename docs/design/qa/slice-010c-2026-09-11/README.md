# Slice 010c implementation evidence

**Implemented and synthetically verified; uncommitted and not deployed.** Worktree
`codex/slice-010c-people-import`, baseline
`c6c5930ee4975706a87398e0a7e7766b05417cf5`. D-065 authorizes implementation and
synthetic checks. Shared development continues to run the 010b release.

## Implemented behavior

- Import People, their contact methods, stages and assignments from a completed
  retained 010b snapshot. Distinct source People remain separate; shared contact
  details are flagged. Mapping and matching-stage creation require explicit choices.
- Preserve encrypted source provenance and exact identifiers, with bounded
  inspection of large fields. Import creates no synthetic Inquiry or communication
  history. Notes, tasks, tags, custom fields and other remaining data stay explicit
  future work.
- Enter a durable administrator review workspace before the first business write.
  Members receive a waiting screen; ordinary mutations, Today, Operator, realtime
  and outbound actions remain blocked. Completion and cancellation retain the hold.
- Resume durable preparation and per-Person work without duplicate People or facts.
  Storage shortages pause before writes; an allowance increase requires a separate
  resume action. Cancellation retains committed rows and releases reservations.
- Provide release compatibility preflight and a recovery runbook. After a review
  binding exists, an old 010b executable cannot safely access that database.

The [approved specification](../../../specs/SLICE_010c.md),
[concrete contract](../../../specs/SLICE_010c_CONTRACT.md) and
[verification chronology](../../../tasks/SLICE_010c_VERIFICATION.md) describe the
scope and detailed evidence. [Implementation review](IMPLEMENTATION_REVIEW.md)
records both bounded review rounds, resolved findings and targeted confirmations.

## Verification

All required checks passed. Browser-discovered dialog accessibility, HTTP error
precedence and workspace permission-refresh defects were fixed and verified.

| Check | Final result |
|---|---|
| `./scripts/sqlx-prepare` | Passed; fresh migration and regenerated offline metadata |
| `./scripts/check` | Passed: 877 Rust tests, 5 doctests, 1,002 Web tests, 14 preflight tests, 11 email-worker tests; formatting, lint, typecheck and production build |
| `./scripts/check-db` | Passed: all 845 DB tests, including tenant isolation, retries, authority, races and storage recovery |
| Query plans | Passed: 59 executed plans, 258 assertions |
| Paired reader regression | Passed once: all five comparisons and response hashes |
| Production Web / real synthetic API | Passed: 50 recorded checkpoints, including six states at six widths |

Successful logs are in [checks](checks/). The final browser records are
[happy path](checks/browser-happy.json) and [complete run](checks/browser-recovery.json);
the latter includes the former's checkpoints. A separate
[delayed-response check](checks/browser-authority.json) proves that a demotion
refreshes authority before old Person bytes are released and cannot restore them.

The final indexed-query collector passed 59 executed plans and 258 assertions on
25,000 People, 50 Organization members and 100,000 contacts/source keys. The
single paired HTTP reader benchmark passed all five comparisons, including
five concurrent Today loads; complete response hashes match the frozen baseline.
See [query plans](checks/query-plans.json),
[protocol](checks/performance/protocol.json) and
[comparisons](checks/performance/comparisons.json).

These measurements verify the changed readers within the approved fixture.
They do not establish capacity at 10,000 or 100,000 agents, measure total workspace
guard overhead, or translate logical-byte allowances into PostgreSQL disk quotas.
Empty or sparse disposition filters can inspect the remaining indexed import
tail; the bounded mixed-density page measurements are not a universal row bound.

## Source and runtime identity

[VALIDATION_SOURCE_SHA256.json](VALIDATION_SOURCE_SHA256.json) records changed
implementation/contract files and explicit deletions: 236 paths, including 18
obsolete SQLx cache deletions. Canonical map SHA256:
`45f1c940403f29e56479dd17714ca033b0a9e99a7d1b4575adb086909ee787ad`.
Historical review manifests remain immutable.
An uncommitted executable is not labelled as the baseline Git build.

Browser QA uses a production Vite build and the actual local API/domain layer
through the `import_qa` example. The example's fake FUB reader and release-readiness
seam are compile-time test support; no production route selects a fake reader.
Disposable synthetic Organizations and a dedicated QA database are used.
[BROWSER_ARTIFACTS_SHA256.json](BROWSER_ARTIFACTS_SHA256.json) identifies the
example binary and 72 production Web files used for the successful walkthrough.
Final integrity verification matched all 236 source paths and 73 runtime
artifacts. The owned browser/API/Web processes exited, ports 3012/5182 are closed,
the protected QA environment is restored, and original `main` is clean.
The dedicated synthetic QA database is retained for local inspection; it contains
no customer data. [Integrity results](checks/final-integrity.json) and
[document-link checks](checks/document-links.json) record the final handoff checks.

## Browser results and visuals

The main scenario imported four distinct People/eight contacts and held one Trash
Person. A source value larger than 2 MiB matched exactly through both plan and
Person provenance inspection. No import phase read FUB or fetched source URLs.
The storage scenario paused before work exceeded the current ceiling, required
an explicit resume after an allowance increase, and then cancelled after three
People/four contacts. Those rows, review mode and zero reservations survived restart.

All six states passed document/body overflow checks at 390, 640, 768, 1024, 1280
and 1536 pixels. Source text is inert and bounded; wide tables scroll within their
containers. Full-page and focused viewport evidence is in [screenshots](screenshots/).

| State | Desktop | 390px Web |
|---|---|---|
| Ready plan | [Full page](screenshots/ready-import-1536-full.png) | [Full page](screenshots/ready-import-390-full.png) |
| Completed results | [Results](screenshots/completed-import-results-1536.png) | [Results](screenshots/completed-import-results-390.png) |
| Member waiting | [Waiting screen](screenshots/member-waiting-1536.png) | [Waiting screen](screenshots/member-waiting-390.png) |
| Person source field | [Field viewer](screenshots/person-review-field-1536.png) | [Field viewer](screenshots/person-review-field-390.png) |
| Preparation paused | [Full page](screenshots/paused-preparation-1536-full.png) | [Full page](screenshots/paused-preparation-390-full.png) |
| Partial cancellation | [Results](screenshots/cancelled-partial-import-results-1536.png) | [Results](screenshots/cancelled-partial-import-results-390.png) |

These are responsive Web screens. Native SwiftUI and Kotlin/Compose clients remain
separate work.

## Deferred work

Live authorized FUB validation remains user-deferred. No customer data, live
source request, deployment, Git publication or activation is covered by this
evidence. A later authorized release must establish actual artifact/workload
inventory and retire every incompatible process; synthetic preflight tests do
not certify a deployed fleet. See the
[release and recovery runbook](../../../tasks/SLICE_010c_RELEASE_PREPARATION.md).
