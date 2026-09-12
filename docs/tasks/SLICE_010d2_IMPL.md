# Slice 010d2 — Execution brief

**IMPLEMENTATION AND SHARED-DEVELOPMENT RELEASE COMPLETE — 2026-09-12.**
See [verification](SLICE_010d2_VERIFICATION.md) and the separately authorized
[release](SLICE_010d2_RELEASE.md) under D-072's explicit follow-up.
The user approved the reviewed
[specification](../specs/SLICE_010d2.md), metadata-first policy and declared import,
timeline and compatibility contracts after the [READY planning review](SLICE_010d2_REVIEW.md).
The original implementation used isolated synthetic data. Its release follow-up
authorized publication/deployment; live source/customer processing remains outside scope.

## Outcome and authority

Deliver a separately confirmed import of qualified FUB event/call/text record
facts, with metadata-only, paginated Person review and visible coverage/holds.
Preserve native truth/Today and the existing migration-review workspace gate.
Start from then-current main; planning inspected `27f3fe4654ace5840412e1364ea36974a1266ad8`.

Read AGENTS, D-012/015/050/052/059/062/064/065/068/070/071, architecture baseline,
010d ladder/specification, 010c/f2 contracts, 010d1 capture contract and the
[focused discovery](../research/SLICE_010d2_CODE_CONTRACTS.md). Original 010d1
capture/parser/crypto purposes and completed parent boundaries stay immutable.
This slice owns new interpretation, imported facts, paged review and the declared
old-reader fences; it does not implement native fact promotion, body reading,
activation or an erasure/key-management service.

## Delivery order and ownership

Use one short-lived `codex/slice-010d2-history-timeline` worktree with one primary
backend/schema writer. Root coordinates specification/status and once-only final
gates. Optional Web support owns only its declared client files after concrete
DTO freeze. Never overlap writers or DB suites; only the backend lane creates the
single additive migration. Keep the current assigned model/role configuration.

Implementation checkout: `/Users/karrad/projects/crm-worktrees/slice-010d2`.
The primary backend/schema lane owns new history-import modules, commands/routes,
the concrete contract and the sole additive migration. A bounded reader lane owns
only new history-review domain/route modules and its tests, supplying schema
requirements to the primary writer. Web owns `web/` after DTO freeze. Root owns
shared Rust wiring/auth/legacy fences, release-preflight tooling, status and final
gates. No concurrent shared-file writers or database suites.

| Step | Deliverable | Exit checkpoint |
|---|---|---|
| 1. Freeze contracts | Concrete `SLICE_010d2_CONTRACT.md`: typed facts, interpretation/date/actor fields, import anchor and retry receipts, lifecycle, exact bounded DTOs/cursors/errors, byte inventory, revision writers and release capability. Correct the documented 010f2 core inventory explicitly. | No unresolved trust/contract choice. Metadata-first exposure and native side-effect exclusions are testable. |
| 2. Reader boundary first | New v2 core, paged native Inquiry/history adapters, consistent-snapshot revision/counts/rows, common DB capability guard on every actual reader connection, upgraded complete-reader fences and closed HTTP errors. New routes work on existing review bindings before history import. | T1/T6/T7: fence precedes full query, both complete paths rejected after an anchor, no capability leakage through pooled reuse, unsupported-artifact retirement specified, existing success DTOs unchanged beforehand, all eight native core families accounted for. |
| 3. Retained interpreter/preview | Verify capture+ordinal, fixed parent/profile and full canonical records; immutable per-occurrence manifest, stable import identities, explicit unknown dates/actors, holds and byte bounds. | T2/T3/T5: no source calls or native writes, lossless evidence linkage and complete preview reconciliation. |
| 4. Confirmed application | Immutable parent anchor, preview/confirm receipts, leased bounded worker, three typed imported facts and encrypted metadata projection, exact budgets and checkpoints, cancel/same-plan restart. | T1/T3/T4/T5/T7: atomic no-duplicate facts/results, no resurrection or unauthorized changed plan; compatible takeover and cancellation tested. |
| 5. Web review | Migration preview/confirm/progress/holds and Person v2 History with known/unknown dates, labels, metadata provenance, page controls and refresh. Preserve separate bounded notes/tasks. | T8: populated actual API flow, admin/member boundaries, no operational call folding or body exposure. |
| 6. Review and final verification | At most two bounded implementation review/fix rounds, focused failure tests, one paired performance/SQL-plan run, final source gates and concise evidence. | All T1–T8 mapped to actual evidence; no unverified claim marked complete. |

Reader/fencing work lands before any historical fact write is enabled. These are
steps in one reviewable slice; do not add a new infrastructure service, generic
event store, ORM, source integration or dependency without a concrete need.

## Meaningful verification

- Source fixtures remain constructed, with event types, invalid/duplicate IDs,
  full canonical variants, ambiguous groups, source-role distinctions and separate
  created/updated/sent timestamps. Do not substitute 010d1's lossy projection.
- Build a real completed People parent and terminal history capture in isolated
  PostgreSQL. Compare exact original raw/parent/native business rowsets before
  and after. Only the declared new read revision/derived metadata may differ.
- Use the full application router for outer-workspace/no-store and authorization
  failures. Include current member, revoked admin, foreign Org/Person/child,
  current checks before receipt replay and cross-scope cursors.
- Race first confirmation against both old complete readers. Verify all-held
  rejection, pre-existing/hypothetical zero-row anchor safety, partial cancellation
  and same-plan continuation. Exercise actual old artifacts and distinct/reused
  pooled connections; verify deployment retirement and compatible-only recovery
  selection rather than claiming old startup can detect the new anchor table.
- Exercise key/integrity failure, oversized interpretation, exact budget settlement,
  owned reservations and full-budget cancellation. Demonstrate missing/erased Person
  suppression and stable tombstone no-resurrection using synthetic fixtures; do
  not claim the whole customer erasure runbook or per-Person keys are complete.
- Verify all pages terminate without duplicates/omissions, with equal timestamps,
  backdated commits causing refresh, sparse/empty family filters and unknown dates.
  Force a writer commit between family queries and verify one consistent snapshot
  of counts, revision and rows. Cover changes to rendered metadata and suppression.
  Native correction entries preserve their existing display time. Page tests must
  not reconstruct operational completed-call/outcome folding from partial input.
- Use one realistic historical book informed by the 75,000-observation 010d1
  baseline. Collect actual plan loops/rows/buffers for every changed hot query.
  Pair unchanged ordinary-reader results/performance under D-050; no laptop
  absolute-latency or production-capacity gate. Verify revision-trigger overhead
  on the existing writers affected by the schema change.
- Run the populated Web walkthrough once after client/API fixes; wait for requests
  started by the action/new document. Reuse the corrected release harness patterns
  for request generations, original-cookie cleanup and narrow logout qualification.
  Repeated harness failures are retained with concise diagnoses, not disguised passes.
- Run SQLx prepare, service-free repository checks and DB checks sequentially once
  on the final implementation tree. Only affected checks repeat after a new change
  or concrete failure. Existing tests remain attributed to their actual source.

Record commands/results, source revision, applied/deferred review findings,
reconciliation, query-plan limits and owned cleanup. No large evidence archive or
repeat audit is required for this task. The implementation verification
record must disclose remaining raw-content/erasure/live-source/activation limits.

## Release handoff

Implementation approval does not imply deployment. A later authorized release
uses the existing shared-development runbook: preserved data/recovery artifacts,
actual built artifact inventory and independent `fub-history-timeline-v1` readiness,
additive schema, matching Web, concise HTTP/public browser checks and owned cleanup.
Never use capture capability as a substitute, remove a confirmed anchor for
rollback, or renew an operator report by editing its timestamp.
