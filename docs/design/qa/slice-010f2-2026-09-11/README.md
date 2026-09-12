# Slice 010f2 synthetic verification

010f2 is implemented under D-068 in the uncommitted
`codex/slice-010f2-activity-import` worktree, based on main `cd3b010`.
The two bounded implementation reviews, query measurements, one actual paired
Person-detail comparison, production-Web walkthrough and exact native/business/
storage reconciliation have completed. All final gates and resource cleanup
passed, as recorded in the [verification record](../../../tasks/SLICE_010f2_VERIFICATION.md).
No commit, release, live FUB/customer processing or activation is claimed.

- [Implementation review](../../../tasks/SLICE_010f2_IMPLEMENTATION_REVIEW.md):
  R1 corrections and independent R2 READY verdict with frozen source manifests.
- [Measurements](measurements/README.md): 92 actual-SQL probes, complete bounded
  traversal and the single paired operational Person-detail comparison.
- [Runtime walkthrough](runtime/README.md): all 18 actual browser phases,
  desktop/390px screenshots, failed harness attempts and exact reconciliation.
- `checks/`: complete gate logs, source manifests and credential-scanned evidence
  inventories. `runtime/reproduction/` preserves the private synthetic drivers
  and audit rules; they require the documented disposable setup and fresh secrets.

Complete and storage-recovery cases each imported 54 notes and 7 tasks, holding
9 records. Cancellation retained 27 notes and 7 tasks. Full native values,
identities/results, parent/sibling hashes and 25 unrelated business tables per
Organization reconcile. All 12 durable activity stores match their measured
retained totals, and all reservations settle to zero.

Owned services used PostgreSQL 55432, Centrifugo 18082, API 3017 and production
Web 5187. Screenshots at 390px show responsive Web; SwiftUI and Compose are outside
this slice. All inputs were synthetic. Activity processing added zero source
calls and publications, including after source disconnect. These fixtures are
not live-source qualification, production-fleet certification or disk-capacity
estimates. Physical relation allocation and recorded native cost are distinct
from logical retained-data allowances.

A transient read during confirmation encountered the intended workspace barrier.
Transaction-state notices from pinned SQLx cancellation paths are retained with
their attribution limits; passing final audits does not prove every notice benign.
The verification record and production-readiness follow-up preserve that caveat.
