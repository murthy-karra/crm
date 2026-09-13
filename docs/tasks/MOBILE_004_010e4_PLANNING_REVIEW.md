# Mobile 004 / 010e4 — Coordinator planning review

**2026-09-13, source inspection against `44dcf52` plus this planning change.**
User approved planning and selected sequential migration slices. This is a
coordinator's source-to-spec review, not independent review, implementation
verification or approval of the proposed contracts. No application tests, API
requests, database writes, native installs or source calls were performed.

## Findings incorporated into the draft

| Evidence | Planning consequence |
|---|---|
| [Stage command](../../backend/crates/crm-app/src/domain/commands/change_person_stage.rs) locks Person, validates Org stage, commits internally and no-ops on same ID | Factor one typed transaction core; add mobile-only expected revision and atomic receipt while keeping ordinary wrapper behavior |
| [Mobile schema](../../backend/crates/crm-api/migrations/20260923000001_mobile_offline_foundation.sql) has a broad Person revision covering joined activity | Add stage-only revision; do not make note/task edits conflicts or miss A→B→A |
| [Generations](../../backend/crates/crm-app/src/domain/mobile/generations.rs) accept no catalog flag and validate only existing components/Today | Add opt-in revision-bound catalog paging/seal; preserve old generation behavior and same-revision cache qualification |
| [Mobile constants/bootstrap](../../backend/crates/crm-app/src/domain/mobile/mod.rs) use 512-KiB pages and 1,800-second generations | Reuse actual bounds; corrected an initial draft's mistaken 100-KiB attribution before completion |
| [Operation dispatcher](../../backend/crates/crm-app/src/domain/mobile/operations.rs) and [receipt constraints](../../backend/crates/crm-api/migrations/20260927000001_mobile_contact_receipt.sql) enumerate note/task/contact variants | Add explicit person_stage parse/visibility/revision/publication/native branches; no catch-all task behavior |
| [010e2 worker](../../backend/crates/crm-app/src/domain/migration/people_refresh_worker.rs) enumerates original results and reads original-result mapping evidence | New admitted-refresh scope cannot be implemented just by including `new` report groups |
| [010e2 schema](../../backend/crates/crm-api/migrations/20260926000001_fub_people_refresh.sql) binds original results/baselines | Separate feature-owned admitted origin/baseline state; old result FKs and API remain intact |
| [Admission execution](../../backend/crates/crm-app/src/domain/migration/people_admission_worker.rs) commits encrypted projection/contact UUIDs and a global identity/result | Seed B from those exact immutable stores, not live Person state or fake original results |
| [Admission schema](../../backend/crates/crm-api/migrations/20260928000001_fub_people_admission.sql) supports terminal cancellation after successful units | Include settled results of cancelled terminal cohorts; separately specify exact-boundary remainder so cancellation does not strand refresh work |
| 010f1/010f2 bind original snapshot/child lifetime; 010d1/010d2 bind original parent/capture | Keep subsequent family source/lifetime changes in the explicit sequential ladder |

## Bounded review conclusions

- Mobile draft covers authority, stage/no-op/ABA conflicts, immutable retries,
  offline clocks, cache/catalog coherence, upgrade, Today freshness and all-writer
  stage coverage. Stage-specific concurrency and new catalog/read/receipt shapes
  remain proposed product/shared contracts requiring acceptance.
- Migration draft covers successful cohort origin, exact B/N/C comparison,
  source ordering, same-boundary remainder, immutable provenance, private permits,
  byte ownership, bounded review and capability fencing. It does not broaden
  original-People refresh or claim later families are complete.
- Integration ownership resolves shared guards/router/SQLx and stage-trigger
  collisions sequentially; planned implementation stays within three worktrees
  with one primary writer per owned file set and isolated resources.
- No authoritative conflict was found requiring a change to accepted policy.
  New behaviors are explicitly proposals rather than retroactive amendments.

## Review and approval boundary

Ready to present for user scope/contract review. Independent planning review has
not been performed in this task; do not label this record an independent READY.
Any required independent review is a remaining pre-implementation gate and should
use these two specs, the code evidence and the coordinated plan as its bounded
input. Concrete DTO/SQL/lock fixtures are implementation contract checkpoints,
not permission to invent different conflict, identity, privacy or lifecycle rules.

Approval should explicitly cover Mobile004 and 010e4's declared changes plus
isolated synthetic implementation; later family specs and all release/customer/
physical-device scope remain separate.

Documentation verification passed: `git diff --check`; 238 local paths/anchors
across all 11 changed/new Markdown files; all 94 decision IDs indexed exactly
once; pre-existing decision-log text preserved verbatim; documentation-only scope
and draft/approval-boundary assertions. No application-test pass is inferred.
