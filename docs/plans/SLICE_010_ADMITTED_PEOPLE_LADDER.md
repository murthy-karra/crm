# Migration sequence for admitted People

**Updated 2026-09-13 — D-079 sequence, D-081 next-pair planning.** The user selected smaller sequential migration
slices after the 010e3 release. Only sequencing/planning is accepted; subsequent
contracts and implementations require their own reviewed scope. This is the
current follow-up to the [010e3 boundary](../specs/SLICE_010e3.md#1-outcome-and-deliberate-boundary).
Keep the administrator review hold, full fidelity reporting and source-data gates.

| Step | Outcome | Contract work / exit evidence |
|---|---|---|
| **010e4: later core refresh — delivered** | Update names/contacts/stage/assignee for successful admissions from a later retained report | [Released evidence](../tasks/MOBILE_004_010e4_RELEASE.md); admission-derived baseline, local-change holds, cohort watermark and recoverable confirmation |
| **010f3: metadata — draft** | Tags/custom-field coverage for admitted People | [Draft specification](../specs/SLICE_010f3.md): admission-cohort child, qualified capture, explicit mappings, shared catalog claims and exact remainder; paired with [Mobile005](MOBILE_005_010f3_PARALLEL_LAUNCH.md) |
| **Following activity slice** | Notes/open/completed tasks for admitted People | Define qualified newer notes/detail/tasks capture, admission target identities, source-user/kind/timezone mappings and one-time family ownership; preserve native/local changes, O-012/O-013 handling and bounded review |
| **Following history slice** | Historical event/call/text metadata for admitted People | Extend 010d1 linkage and 010d2 anchoring deliberately; exact captured account/cohort/sequence, identity dedupe and bounded metadata-only timeline; no native contact credit |
| **Later repair/delta/cutover scope** | Remaining coverage, deliberate mapping repair, per-family updates and activation | Separate accepted policies, final coverage/reconciliation, customer-data gates and recovery/cutoff procedure |

Write the next family specification just in time after the preceding evidence.
010f3 is the proposed numeric assignment for the metadata step; subsequent steps
remain outcome boundaries without assigned IDs or approved contracts. A family extension must not imply ongoing synchronization or
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
