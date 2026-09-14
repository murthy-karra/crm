# Slice010f3 — independent implementation review round1

**CHANGES REQUIRED.** Sol high reviewed the complete backend/Web implementation
at immutable `a52a1e6` against published base `e36c9b3`, the accepted specification
and literal contract. Source/evidence inspection was read-only. This is the first
of at most two implementation review/fix rounds under D-050.

| ID | Priority/tag | Finding at pinned source | Disposition |
|---|---|---|---|
| 1 | P1 CONTRACT | Prepare commits catalog handover before admission/report/workspace/account qualification; a rejected authorized request can activate readiness and charge original owners | Corrected in `3755623`; focused regression passes |
| 2 | P1 TENANT/CONTRACT | New child/claim/observation/alias/remainder references lack the frozen composite Organization/owner provenance FKs | Corrected in `02f8cdb`; negative persistence and readiness checks pass |
| 3 | P1 FIDELITY | Tags unique to erased or identity-mismatched settled cohort People disappear from catalog coverage | Corrected in `02f8cdb`; erased and identity-mismatch regressions pass |

Pinned locations: `admitted_metadata.rs:489`; initial metadata migration:53;
preparation migration:19, views migration:16 and remainders migration:3;
`admitted_metadata/preparation.rs:330,523`. The systemic ownership finding is one
finding covering the whole relation graph, not a separate review round per FK.

The reviewer found no further actionable defect in source qualification, typed
values, immutable replans, claim execution, permits, bytes, cancellation/remainder,
bounded readers, Web replay or Person provenance. Final008 browser/runtime and exact
15-rowset/two-cell preservation evidence was independently checked. Earlier final
repository passes belong to `a52a1e6`; substantive corrections require replacement
gates before a READY review verdict.

## First correction evidence

`3755623` extracts existing exact preparation qualification and runs it under the
first workspace/Organization transaction before registry activation, then repeats
it under root creation. Successful qualified handover still survives later preview
failure, as specified. Existing populated handover rollback/accounting/replay and
legacy-session fencing tests pass in `integration/review1-prequalification-db1.log`.
That run's new regression initially failed because its adversarial fixture tried
to mutate immutable report-account inputs; the existing23514 guard correctly
rejected that fixture mutation. This is retained as a failed attempt, not a pass.

Removing that unreachable mutation leaves a realistic matrix of five invalid or
foreign admission/report tuples and an operational-workspace request. All fail
without readiness, claims, roots, receipts or owner-ledger changes. Original
identity/plan/result evidence and source-call counts are unchanged. A later valid
request activates readiness and exact replay charges once. The focused regression
passes in `integration/review1-prequalification-db2.log`:65.20s total,11.28s test.
Paths are relative to `/private/tmp/crm-mobile005-010f3`.

All three first-round findings are corrected. The final migration verification
record owns 24 functional tests, the historical cancellation-owner repair proof,
nine new/changed query plans and scoped lint results. Shared readiness passes
183 incomplete-schema variants. Independent implementation round 2 is assessing
code `afe4725`; final repository gates and retained-browser upgrade proof remain
pending. No READY verdict is claimed here.
