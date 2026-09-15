# Slice 010e5 — Execution brief

**IMPLEMENTATION ACCEPTED — D-088.** D-087 selects existing People first; D-088
accepts execution with one primary writer and one reviewer. Follow [the specification](../specs/SLICE_010e5.md)
and [plain-language plan](../plans/SLICE_010e5_MAPPING_REPAIR.md).

## Required context

Read AGENTS.md, full D-015/D-050 and applicable D-059/D-064/D-065/D-075/D-076,
D-078/D-079/D-080/D-087 plus dependencies/amendments via the decision index.
Read 010c provenance/permits, 010e2/010e3/010e4 and the architecture baseline.
Preserve D-015/O-012/O-013 customer gates and the D-064 review hold.

## Ownership and execution

One bounded migration backend/database writer owns additive SQL, both refresh
engines/stores, shared mapping helpers and API/compatibility changes. Complete
backend contract integration before Web implementation. Do not create parallel
writers unless separately assigned with exclusive ownership; no mobile scope.

1. Complete planning review within D-050's two-round limit. Record material policy
   differences before implementation acceptance. Freeze
   `SLICE_010e5_CONTRACT.md`: exact additive SQL/FKs, DTOs/errors/bounds, mapping
   key identity, lock order, source-boundary checks, permits, storage charging,
   capability inventory and normal-refresh compatibility changes.
2. Implement repair routes and typed commands under both existing refresh bases.
   Derive candidates from immutable held results or sealed held previews plus
   successful Person ownership. Cover all-held previews with atomic, explicit
   retirement of the unconfirmed root and replayable repair-draft creation.
   Freeze target choices and exact source evidence; never mutate old approvals.
3. Extend both existing workers and baseline/provenance readers. Maintain typed
   original/repair bindings, approval-only outcomes, normal-refresh follow-through,
   atomic settlement, crash fencing, cancellation and byte accounting.
4. Extend real database guard/release admission paths, not only application checks.
   Prove incompatible normal refresh workers cannot touch repaired workspaces.
5. Add admin Web selection, grouped choices, full core preview, explicit
   confirmation and durable recovery/results links. Use existing components and
   query patterns, bounded pages and access-loss cache clearing.
6. Run focused tests while implementing, then all required final-tree checks once.
   Keep failed output/evidence honest and rerun only for corrections/stale evidence.

## Required checks

Use the spec §9 matrix for both original and admitted paths. Include Rust format,
lint/unit/integration/tenant/DB contract tests, SQLx offline/migration checks,
Web typecheck/lint/tests/build and real API/Web synthetic walkthroughs.
Record exact runner/source and isolated `CARGO_TARGET_DIR`/Web output directory.
Do not overwrite artifacts used by shared API/Web or run overlapping DB suites.

D-050 gates: realistic new/changed hot-statement plans and paired relative
Person/Today regressions as affected; one benchmark run unless the paired gate
fails. At most two review/fix rounds; trust/correctness blockers are never a pass.

Include pre/post native business counts, facts, old mapping/result immutability,
byte-ledger balances and startup worker count. No live FUB validation is required
or authorized by this synthetic slice. Record residual production handoff and
SQLx cancellation concerns without claiming this slice has resolved them globally.

## Done

Accepted scope implemented, both owner paths verified, compatibility fenced,
admin journey demonstrated, tests/evidence recorded and remaining migration gaps
visible. Publication, shared-development deployment and production rollout have
their own release instructions. Do not infer them from this planning brief.
