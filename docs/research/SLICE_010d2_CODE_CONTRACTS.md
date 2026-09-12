# Slice 010d2 — Focused code findings

Read-only discovery on 2026-09-12 at
`27f3fe4654ace5840412e1364ea36974a1266ad8`. Two independent bounded inspections
checked capture interpretation and existing readers; root checked the envelope,
source research and native history ordering. No runtime/DB change, live source
request, test execution or new vendor qualification was performed. Line numbers
below refer to that baseline; links target files so later edits do not masquerade
as immutable line evidence.

| Verified seam | Consequence for the proposed specification |
|---|---|
| [History parser](../../backend/crates/crm-app/src/domain/migration/history_capture_source.rs), lines 295, 360, 415–427: uncertain nested/group references, separately qualified dates, merged source-user ID set and body-presence-only flags. | `linked` is not fan-out permission; the metadata projection cannot restore caller/creator/editor roles or text sent/status. Interpret authenticated original capture+ordinal under a new version. Missing updated does not invalidate known created. |
| [History crypto](../../backend/crates/crm-app/src/domain/migration/crypto.rs), lines 375–382: identity/semantic HMAC scope includes run UUID. | Introduce stable import-owned identity/fingerprint purposes; never compare run-scoped digests across attempts. |
| [History store](../../backend/crates/crm-app/src/domain/migration/history_capture_store.rs), line 327 and [capture schema](../../backend/crates/crm-api/migrations/20260921000001_fub_history_capture.sql), lines 47–86: immutable page/ordinal evidence and primary Person/erasure linkage. | Freeze complete parent/capture boundary, validate canonical record binding and maintain multi-Person erasure inventory. Diagnostics are not successful record input. |
| [Existing source qualification](SLICE_010d_FUB_SOURCE_CONTRACT.md), sections 2–4. | FUB events do not establish native Inquiry/consent; call logs do not establish local call lifecycle/outcome; text metadata does not establish delivery or content accessibility. Source-created chronology must be named honestly. This remains live-unqualified. |
| [Fact envelope](../../backend/crates/crm-app/src/domain/envelope.rs), lines 73–103 and 157–189. | `Origin::Migration` exists. The local typed import observation can use the standard envelope while its historical source time remains a separate nullable, qualified field. |
| [Legacy Person route](../../backend/crates/crm-api/src/routes/people.rs), lines 268–272 and [Inquiry query](../../backend/crates/crm-app/src/domain/inquiry/queries.rs), line 87. | Both inquiry summary and full history are unbounded. Inquiry ordering has no ID tie-breaker. Use a new paged representation; do not truncate old complete arrays. |
| [Person history](../../backend/crates/crm-app/src/domain/person/queries.rs), lines 1316/1380/1451 and 1587–1596. | Core combines eight entire families then sorts in Rust. Bound each family in SQL before merging. Existing total order is `(display occurred_at, recorded_at, kind_rank, id)`; correction contact attempts display recorded time (lines 1325–1330). |
| [Activity review](../../backend/crates/crm-app/src/domain/migration/activity_review.rs), lines 88, 162–175 and 317. | It enforces admin/workspace/Person binding but reads all inquiries/core history before a 512-KiB check. Existing note/task pages are already bounded and should remain separate. |
| [Current 010f2 contract](../tasks/SLICE_010f2_CONTRACT.md), core-history paragraph. | It lists assignment/stage/inquiry only, while actual core additionally includes person_imported, contact attempts, calls and correspondence. New contract must explicitly correct that inventory rather than silently drop actual families. |
| [Web complete DTO](../../web/src/api/types.ts), line 588 and [Person detail](../../web/src/views/PersonDetailView.vue), lines 1501–1543. | Existing arrays are complete; the operational client folds all call-derived attempts into completed calls and derives outcome actions. A new paged read-only fact timeline must not reuse that fold or treat external calls as local calls. |
| [Activity migration guard](../../backend/crates/crm-api/migrations/20260920000001_fub_activity_import.sql), line 188, [activity core](../../backend/crates/crm-app/src/domain/migration/activity_review.rs), lines 158–163 and [outer workspace middleware](../../backend/crates/crm-api/src/auth/workspace_http.rs), line 12. | The 010f2 core bypasses the existing complete-reader helper; both paths encounter the common `crm_workspace_read` guard. Add the durable history binding and capability there before writes, under the shared/exclusive barrier, plus upgraded typed complete-reader fences. Cover outer errors/no-store. Current guard's history-capture no-store fix is included in baseline. |
| [Workspace compatibility](../../backend/crates/crm-app/src/auth/workspace.rs), read helpers at lines 87–111 and startup at 345–389, and [preflight](../../scripts/migration-release-preflight), line 267. | Add timeline-import/reader capability on each actual connection with transaction-local lifetime; middleware and handler connections differ. Old startup cannot detect a new anchor table, so trusted inventory must retire unsupported artifacts and select compatible recovery. A fresh report cannot upgrade old artifact code. |
| [Operator backend](../../backend/crates/crm-api/src/operator/backend.rs), line 397. | It truncates after loading, but the operational hold already excludes imported review workspaces. Preserve that hold; operational-reader modernization belongs to activation. Native individual-call detail is a scoped point read, not a paging problem. |

Recommendations are proposed in [SLICE_010d2.md](../specs/SLICE_010d2.md), not
newly accepted policies. The user separately selected **metadata first** while
planning. D-015 history/erasure rules, D-052 native maxima and D-064 review-only
use are existing authority; none is relaxed by these findings.
