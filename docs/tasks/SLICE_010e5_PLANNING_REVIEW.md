# 010e5 — Independent planning review

**READY — 2026-09-15, round 1.** Reviewer: `/root/mapping_repair_review`,
explicitly authorized by the user. Read-only inspection; no tests or runtime work.
Reviewed the spec/plan/brief, applicable decisions and both existing refresh
command/worker/store paths, permit schemas and compatibility code.

The selected scope is coherent: existing original/admitted People, per-Person/key
successful mapping approvals, full core preview, both existing workers and all-held
preview replacement. No policy/scope blocker was found.

Required concrete implementation corrections:

1. **TRUST:** Original worker execution currently accepts any original mapping
   matching the destination (worker lines 110–166); use the exact source-key typed
   binding. Its already-current path skips checks before baseline advancement
   (lines 1102–1109 and 1230–1255); validate native/baseline/head/target for no-op
   and approval-only settlement as well as business mutations.
2. **CONTRACT:** Both retry commands must reject `awaiting_mapping_choices`.
   Only explicit plan sealing may start preparation with repair choices.
3. **BOUNDARY:** Existing same-snapshot noncancelled-root uniqueness must allow
   explicitly owned repair roots while preserving ordinary-root uniqueness and
   one active writer. Exact source-order checks still apply.
4. **CONTRACT:** Successful-resolution exclusion applies to the anchored held
   item. A later fresh hold for the same Person/key may supersede an invalid binding.
5. **TRUST/BOUNDARY:** Enumerate DB reader, root/claim, baseline/result and private
   business-write fences, plus both owner-specific byte measurement paths. An API
   check alone or a charge recorded only for new business rows is insufficient.

These are compatible concrete requirements for the accepted outcome. Readiness
does not establish implementation verification or release approval. Primary writer
must carry each into the contract/tests and request the authorized implementation
review when a concrete implementation exists.
