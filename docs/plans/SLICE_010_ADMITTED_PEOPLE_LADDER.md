# Migration sequence for admitted People

**Updated 2026-09-15 — D-079 sequence, D-082 metadata implementation, D-084 activity implementation; D-086 history implementation.**
The user selected smaller sequential migration slices after the 010e3 release.
Each step's contracts and implementation require its own reviewed scope; the
table distinguishes accepted work from later unassigned scope. This is the
current follow-up to the [010e3 boundary](../specs/SLICE_010e3.md#1-outcome-and-deliberate-boundary).
Keep the administrator review hold, full fidelity reporting and source-data gates.

| Step | Outcome | Contract work / exit evidence |
|---|---|---|
| **010e4: later core refresh — delivered** | Update names/contacts/stage/assignee for successful admissions from a later retained report | [Released evidence](../tasks/MOBILE_004_010e4_RELEASE.md); admission-derived baseline, local-change holds, cohort watermark and recoverable confirmation |
| **010f3: metadata — deployed** | Tags/custom-field coverage for admitted People | [Accepted specification](../specs/SLICE_010f3.md), [final evidence](../tasks/MOBILE_005_010f3_FINAL_VERIFICATION.md); included in the [verified current release](../tasks/MOBILE_006_010f4_RELEASE.md) |
| **010f4: activity — deployed** | Notes/open/completed tasks for admitted People | [Accepted specification](../specs/SLICE_010f4.md): qualified newer notes/detail/tasks capture, exact admission identities, explicit roles/kinds/timezone, shared activity identity and exact remainder; paired with [Mobile006](MOBILE_006_010f4_PARALLEL_LAUNCH.md) |
| **010d3: history — implemented and verified** | [Accepted historical event/call/text metadata](../specs/SLICE_010d3.md) for admitted People | Exact captured account/cohort/sequence, identity dedupe and bounded metadata-only timeline; no native contact credit. [Implementation evidence](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md) owns current gates. |
| **010e5: stage/agent repair — deployed** | Repair blocked updates to existing original/admitted People | [Verification](../tasks/SLICE_010e5_VERIFICATION.md); [Release](../tasks/SLICE_010e5_RELEASE.md), D-088 accepted |
| **Later repair/delta/cutover scope** | Remaining coverage, deliberate mapping repair, per-family updates and activation | Separate accepted policies, final coverage/reconciliation, customer-data gates and recovery/cutoff procedure |

**Current boundary — D-090/D-091:** 010e6 never-imported mapping-hold recovery
is verified and merged locally as `2a4c207`; shared development remains 010e5.
The user deferred deployment and selected one combined metadata/activity/history
refresh package. [010g1](SLICE_010g1_COMBINED_FAMILY_REFRESH.md) owns its proposed
scope and contracts. It preserves family-specific execution while combining
review and delivery; its update/removal/correction policies are accepted under D-092 and implementation
is in progress.

Write the next family specification just in time after the preceding evidence.
010f3's assignment/contracts are accepted under D-082. 010f4 is the accepted
activity assignment under D-084, with [final acceptance](../tasks/MOBILE_006_010f4_FINAL_VERIFICATION.md)
complete; D-086 accepts the [010d3 specification and brief](MOBILE_007_010d3_PLAN.md), paired
with Mobile007 Find and save People. Both planning reviews are READY and implementation
is implemented and verified. A family extension must not imply ongoing synchronization or
refresh of that family's already imported records. Report those separate gaps.

## Questions each later family spec must settle

- **Which source boundary?** 010f1/010f2 use the original snapshot and 010d2 pins
  one original-parent capture. Newly admitted People may not exist there. Choose
  independently qualified retained evidence; do not join records across captures
  merely because IDs match. Incomplete/inaccessible data remains visible.
- **Which target cohort?** Use successful immutable admission identities, including
  explicit treatment of committed results from cancelled runs. Do not fabricate
  original results, reopen terminal original children or derive ownership from
  present-day values/name/contact matches.
- **Which lifetime and global identity?** Define family plan/attempt/remainder,
  deduplication across original/admitted imports, conflicting variants, deletion
  tombstones and no resurrection. New representation requires explicit authority.
- **Which mappings and local edits?** Freeze source-user/timezone/kind/catalog
  choices with preview/confirmation. Existing People mapping approval is not
  blanket approval for every family catalog or privileged source audience.
- **Which execution and reader fences?** Per-family private permits, source/Org
  byte admission, bounded review, legacy-reader rejection and compatible recovery
  must cover the new cohort before its first write.

No live FUB validation is resumed by this plan. Customer erasure/readiness,
email bodies/media, phone-number transfer, source writes and activation remain
separate. Native calling remains after the existing mobile progression. Source
milestones are evidenced in [project state](PROJECT_STATE.md); old ladder release
banners are historical and do not authorize a new execution lane.
