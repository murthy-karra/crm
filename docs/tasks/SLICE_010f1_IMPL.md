# Slice 010f1 — Execution brief

**APPROVED FOR IMPLEMENTATION — D-066, 2026-09-11.**
Implementation and synthetic verification are complete;
[executed evidence](SLICE_010f1_VERIFICATION.md). The D-066 follow-up authorized
integration and shared-development deployment: implementation `f37ddd1`, merged/
pushed and [deployed as `e36ce360`](SLICE_010f1_RELEASE.md). The worktree and branch
were removed after verification; the ownership notes below are historical.
The user approved the reviewed specification, brief and owned shared contracts.
Former primary implementation worktree: `/Users/karrad/projects/crm-010f1`, branch
`codex/slice-010f1-metadata-import`, baseline main `f01c2e3`.
`snapshot_impl` is the primary backend writer and sole database migration owner;
`root` owns Web integration, shared documentation and final verification.
Read-only support has no file or database write authority unless explicitly assigned.

Execution adjustment: after the concrete wire contract was frozen, root began
independent Web authoring in parallel with backend work to avoid an idle lane.
Backend checkpoint review/fixes still precede integrated Web/API verification;
the final integrated review remains round two. No behavior or ownership expands.
Root also owns the new `db_metadata_import_http.rs` support file; the primary
retains its registration and coordinated Cargo/DB execution.

Historical planning brief (approval gate satisfied by D-066):
2026-09-11, inspected main `f01c2e3` after deployed 010c. This brief accompanies
the [draft specification](../specs/SLICE_010f1.md) and
[code-contract findings](../research/SLICE_010f1_CODE_CONTRACTS.md).
D-065's next-import follow-up authorizes drafting/review only. Do not start code,
migrations, test databases, workers, commits or rollout from this document.

## 1. Outcome, inputs and boundaries

After full approval, build one retained-source metadata child for a completed
010c parent in its existing review workspace. Admins explicitly choose matching
tag/field/option creation or mappings, review counted exclusions, confirm once,
and inspect retry-safe per-item results/provenance. Set absent custom-field values
and add approved tag links; identical native data is already present, differing
values are held. Ordinary agent use/Operator/outbound remain blocked.

Authoritative inputs: AGENTS; DECISION_LOG D-050/051/058/063–065; architecture
baseline; complete 010b/010c specs and concrete contracts; the current tag/custom
field specs ([011e](../specs/SLICE_011e.md), [019](../specs/SLICE_019.md)) and
implementation; this reviewed 010f1 spec and code record. Read
prompt-library 05-implement/06-verify-and-review before execution. Preserve the
actual model/role assignment; MODEL_ROUTING profiles are advisory, not permission
to change models. Use one primary `design`-level implementation lane for this
authorization/fidelity boundary; escalate only a concrete unresolved concern.

Preserve original workspace/parent plans/results/identities and exact source
boundary. Customfields exhaustion is required for the whole child; no tags-only
fallback. Preserve 200 tags/Org, **20 tags/Person**, 50 live fields and 50 live
options/field. No first-N selection or quota increase. No source/provider calls,
delta/repair import, parent mutation, new People/contacts/Inquiries/stages,
notes/tasks, activation, per-Person visibility, richer field kinds or infrastructure.
Synthetic validation only; real FUB/customer readiness remains separately gated.

## 2. Ownership and approval gate

This draft owns proposed child persistence/permit/HTTP/UI/accounting contracts
within spec §§2–8. It does not yet authorize changing any shared contract. Obtain
independent plan review through 04-review-plan, reconcile findings, then request
full user approval of the spec, brief and proposed defaults in spec §10.

After approval use one short-lived `codex/slice-010f1-metadata-import` worktree
from then-current main, one primary application writer and **one named database
migration owner**, normally the same primary. The coordinator owns shared docs,
status/review/evidence and final integration. Preserve AGENTS' three-worktree cap
and never overlap DB-backed runs. The brief launches no agents or workloads.

If support is useful, assign only a named new pure extractor/test file or new Web
component/test after freezing its seam. Primary owns all module registrations,
shared files and integration; no overlapping writers or second migration owner.
Independent review reads the frozen tree. Backend review/fix precedes Web; final
integrated review is round two. D-050 allows at most two implementation review/fix
rounds; further review requires the decision's explicit approval rather than an
open-ended loop. A blocker remains reported, never silently marked complete.

## 3. Execution sequence and acceptance

| Step | Owned outcome / dependency | Checkpoint proof |
|---|---|---|
| 1. Freeze concrete contract | After approval, define child schema/DTOs/errors/crypto/cursors, source rules, catalog dependencies and exact byte inventory | Spec A1–A4/A9; explicit null/collision/recurrence rules, parent and source-account constraints, fan-out arithmetic |
| 2. Child engine | Sole additive migration; guarded typed metadata commands, qualified immutable plans, private DB/application permit, leases/identity/results/receipts/accounting | A1–A9; no parent rewrite, current authority and native limits, atomic work, no-overwrite/tombstones, exhausted cancel |
| 3. Backend checkpoint | Add scoped routes/field reads, startup/worker integration and narrow existing-preflight compatibility; finish focused source/DB/HTTP tests | First bounded review/fix; actual complete wire contract for Web, current admin and foreign-resource proof, unchanged ordinary guards |
| 4. Web | Add child plan/mapping/progress/inspector UI and Person metadata provenance; reuse shared workspace/session lifecycle | A6/A8/A11; dirty choice apply/discard, explicit confirmation/subset, exact uncertain replay, progress/role-change/reload recovery |
| 5. Final verification | Second integrated review/fix, final gates, actual query-plan collection and synthetic production-build walkthrough | A10–A12; source/log hashes, results/limitations, current contract pointers and compatible recovery instructions |

Step 1 must specify the treatment of machine-key/case collisions, source choices
without IDs, invalid tag elements, >20 links, option dependencies and the exact
absence/equality/no-overwrite comparison. Freeze deterministic native lock order
and stable child identity keys; derived tag/option keys are not vendor IDs.
Use bounded scoped HMAC identities with encrypted exact-label collision checks,
not permanent plaintext tag/choice tombstones.
Prove native NUL/index-size eligibility before ready, and bounded full-evidence
access for both Person metadata and supporting mappings. Approval of this brief
does not authorize inventing a new lossy rule when a fixture is inconvenient.

## 4. Implementation file boundary

- `backend/crates/crm-app/src/domain/migration/`: new metadata extractor,
  plan/commands/worker/query modules; reuse lossless source/crypto/display patterns.
  Existing imports/snapshot accounting changes only where needed for separate
  child ownership. No expanding old People-source family CHECKs or parent DTOs.
- App workspace and tag/custom-field persistence seams: narrow typed child permit,
  native validation/locking and absent-or-equal writes. Ordinary commands/guards,
  source attribution, facts and value erasability retain their current behavior.
- Exactly one new versioned migration under `backend/crates/crm-api/migrations/`;
  child composite tenant/source identity/index/grant/guard definitions and generated
  `backend/.sqlx/` metadata. Never edit applied 010b/010c migrations.
- API migration route module, registration/error/state/worker startup and existing
  release readiness/preflight only for required child artifact capability. Keep
  `crm-workspace-v1`; no generic release framework or operational mode bypass.
- Web additive metadata API/types/query keys/components; MigrationView and Person
  detail/provenance integration with existing workspace refresh and authorization.
  Reuse UI_STYLE and relevant styling skill. No native/Operator execution surface.
- Focused tests: proposed `db_metadata_import_source.rs`,
  `db_metadata_import_gate.rs`, `db_metadata_import_http.rs` and opt-in
  `db_metadata_import_plans.rs`, plus pure/Web tests and minimal shared synthetic
  fixture changes. Primary owns test registrations. Reuse raw-qualified capture
  fixtures, not the non-executable query-plan book.
- New concrete contract/verification evidence plus coordinator-owned amendments
  to migration summary/current-state/compatibility pointers. Implementation must
  list actual files and generated metadata at each frozen checkpoint.

Do not refactor unrelated CRM readers or create new dependencies/services. If a
shared reader change becomes necessary, document its current/proposed behavior
and add the D-050 paired check; do not silently widen this brief.

## 5. Required checks and prerequisites

During implementation, run focused pure Rust and Web tests, formatting/lint and
scoped DB/HTTP suites matching spec A1–A9/A11. Include actual direct-domain/DB guard
calls, current-role changes, cross-Org IDs/cursors, parent/capture/key corruption,
full exact number/text segments, quotas/collisions, archived/tombstoned targets,
existing equal/different values and zero post-capture reader calls. Atomicity tests
control before/after commit, expired lease and cancel/confirm races. Account every
retained variable column and receipt/control reservation using small limits;
lower deployment ceilings without illegally decreasing stored approved budgets.

After the two bounded reviews, use the then-current repository scripts sequentially
on the frozen tree, with the documented isolated local service prerequisites:

```sh
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

`sqlx-prepare` and `check-db` require the authorized disposable PostgreSQL/service
setup and exclusive DB slot; `check` is service-free. Use the repo-pinned Node/pnpm
and Rust dependencies; never print .env values. No shared `crm_dev` reset, seed or
customer mutation. Preserve logs and actual counts; do not call an unrun check passed.

Register the new plan collector under existing `perf-harness` test support and run
it explicitly after normal gates so successful EXPLAIN stdout is retained:

```sh
cd backend
SQLX_OFFLINE=true cargo test -p crm-api --test all --features perf-harness --locked db_metadata_import_plans:: -- --ignored --nocapture --test-threads=1
```

Use actual new/changed hot SQL at 25k People/50 members, realistic metadata density,
dense shared tags and rare/empty filters. Establish bounded fan-out, native row
sizes and retained ledger arithmetic separately from physical disk estimates.
Do not automatically require a full CRM-reader paired benchmark: if existing
reader SQL/shared behavior is unchanged, its previous evidence remains applicable.
If changed, one paired baseline/final p95 check within max(25 ms,10%) applies to
the affected requests, with relevant five-concurrent Today load per D-050. Record
fixture/cardinality limitations; no planner toggles or laptop absolute latency gate.

Run a synthetic real-API production-build browser walkthrough after gates:
completed-parent plan, explicit catalogs/options/subset confirmation, separate
People receiving expected links/values, held conflicts, exact long evidence,
uncertain replay, budget pause/retry, cancellation retaining partial work, disconnect,
member hold/admin demotion and desktop/390px layout/named dialogs. Assert unchanged
parent binding/results and zero source requests. Private harnesses must clean only
their own sessions/resources; runtime/deployment requires separate authorization.

## 6. Draft completion and remaining approval

This planning handoff consists only of the three linked new documents. Coordinator
arranges independent review and records acceptance. Spec §10's defaults remain
proposed, including completed-parent-only/one-child lifetime, partial independent
metadata subset, unchanged quotas, no overwrite and conservative type/collision
holds. No extra research or FUB access is a prerequisite to review the draft.
Stop here until the user approves the reviewed specification/brief/contracts.
