# Slice 010f2 — Bounded implementation reviews

D-068 authorizes implementation. D-050 and the execution brief allow two bounded
implementation review/fix rounds. Planning review is separate and does not count
as implementation evidence. Implementation is still in progress.

## Round 1 — backend, closed with fixes required

Independent reviewer: `snapshot_impl/metadata_extract`, with a bounded read-only
source subreview. Root independently verified the reviewer's authored test-only
paired performance harness; the reviewer excluded that harness from its own
independent production-code review. Model assignments were inherited unchanged.

The complete frozen checkpoint contains 778 file hashes in
[backend-r1-source-sha256.json](../design/qa/slice-010f2-2026-09-11/checks/backend-r1-source-sha256.json).
Manifest SHA-256:
`3f8bb002445a8e1019614ba3f4e874838d2bbcc156beaf4c63465e6997abbbe7`.
The reviewer verified every hash unchanged at closure. Review was read-only and
code-derived; no additional runtime reproduction was executed by the reviewer.

**Verdict: NOT READY.** Five findings require corrections before the final
integrated round. No additional full backend review is planned.

| ID | Priority / failure | Correction status |
|---|---|---|
| R1-01 | P1 — child source account/capture sequence/exact preview omitted from frozen-boundary validation; lowering capture sequence could silently omit source | Added comparisons with the eligible parent's exact values. Reader, confirmation and queued-worker corruption regression passed (1 test, 3.22s); positive execution and unchanged parent/source controls passed. |
| R1-02 | P1 — request HMAC checked for note details but not collections; an open-task request fingerprint could qualify a completed-task capture | Authenticated collection requests reconstructed from prior accepted checkpoints. Fingerprint-only open/completed substitution fails closed; users next-token/local-offset regression passed. |
| R1-03 | P2 — retained records from rejected snapshot attempts permanently block an otherwise recovered completed snapshot | Qualified valid observations and invalid occurrences retained across real invalid-ID and no-progress pause/corrected-response/retry cases. Both recovered parent/activity outcomes passed. |
| R1-04 | P2 — list-only attachments/unknown fields omitted from exclusion acknowledgements | Exact closed counters aggregate distinct qualified observations; list-only exclusions count, same-representation duplicates collapse, missing detail body remains held. Regression passed. |
| R1-05 | P2 — result issue filter uses frozen manifest issues and misses execution-time holds | Added an immutable tenant-scoped result-issue projection, indexed and measured in the same result settlement transaction. Post-confirmation assignee deactivation regression passed (1 test, 3.37s), including actual-result/planned-issue filters, deduplicated codes, foreign-Org FK and exact settlement. |

The reviewer found no additional concrete defect in bounded native review
queries, the ordinary-reader guard, normal ledger/receipt paths, or the declared
conversion and time profiles. This is a bounded code-review conclusion, not
proof that final gates, runtime plans or browser acceptance have passed.

The four source regressions passed on their first run (19.82s), completing all
six targeted R1 tests. Corrections are accepted for integration. Logs: [boundary](../design/qa/slice-010f2-2026-09-11/checks/r1-control.log),
[result filter](../design/qa/slice-010f2-2026-09-11/checks/r1-result-filter.log),
[source](../design/qa/slice-010f2-2026-09-11/checks/r1-source-db.log).

## Round 2 — integrated application, closed READY for final verification

Independent read-only reviewer: `snapshot_impl/metadata_extract/review_source`,
who authored no product code. The frozen manifest contains1,377 files:
[integrated-r2-source-sha256.json](../design/qa/slice-010f2-2026-09-11/checks/integrated-r2-source-sha256.json),
SHA-256 `c5455cf2d3c68d4a13678f10e90bd67934db4d468f38327cb4fa7cb17fcc888e`.
Start and end verification matched every hash, with zero mismatches.

**Verdict: READY for final verification.** No actionable P1/P2 findings.
Coverage includes the accepted R1 corrections, command/worker/native permit and
atomic accounting seams, current-admin and tenant gates, retained/native paging,
cursors, release capability/replay gates, and all new Web product files, including
mutation intent and identity/revision fencing. This was static source review;
the reviewer executed no tests or runtime checks. Root's separate Web spot check
also found no additional concrete defect; it does not substitute for runtime QA.

Both bounded implementation review rounds are now closed. Final sequential gates,
actual query plans, the one paired operational Person-read comparison,
production-Web synthetic walkthrough and native reconciliation remain mandatory.


## Verification correction after the closed rounds

The first complete DB gate caught a test expectation introduced with R1's new
result-issue projection: the successful overlapping-worker test expected two rows
in every table, including the issue table. Two successful tasks require zero
issues. Root corrected only that assertion; its focused rerun passed (1 test,
3.54s). Production behavior and contracts are unchanged. This is a required-check
correction, not a third implementation review round. The second full three-script sequence passed, including908 DB tests; the failed
gate is retained.

The actual-SQL measurement then exposed avoidable full-plan work for missing
negative detail fingerprints and rare/empty issue filters. Root added matching
indexes and made filtered queries start from the immutable issue projection.
The actual filter regression passed; the collector rerun passed all92 probes,
with empty issue probes reduced to one index block and missing negative lookups
to two blocks. Native response/traversal bounds remained unchanged. A fresh
three-script sequence verifies this measured correction. No shared API contract
changed, no third implementation review was opened, and no checksum of an
applied database migration was altered. The only new migration is unreleased;
the disposable browser-QA database was recreated before its fresh application.
