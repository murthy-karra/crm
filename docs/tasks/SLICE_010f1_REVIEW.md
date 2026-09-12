# Slice 010f1 — Independent plan review

2026-09-11; source baseline main `f01c2e3`, following the verified
[010c shared-development release](SLICE_010c_RELEASE.md).
Scope: complete [specification](../specs/SLICE_010f1.md),
[execution brief](SLICE_010f1_IMPL.md),
[code-contract evidence](../research/SLICE_010f1_CODE_CONTRACTS.md), repository
decisions and relevant implementation/test precedents under
[04-review-plan](../prompts/04-review-plan.md). Independent read-only reviewer:
`next_import_review`; author: `snapshot_impl`; coordinator: `root`.
No model override was made; actual billed usage is not available in this record.

## Verdict and disposition

**READY for user review; specification, shared contracts and implementation
remain unapproved.** The reviewer read all three frozen documents completely
after author handoff and found no unresolved actionable findings. The review
checked source qualification, native limits, immutable parent binding/results,
child permissions, no-overwrite behavior, identity/erasability, transaction and
ledger settlement, cancellation, compatibility and implementation ownership.

| ID | Severity / concrete failure | Correction and disposition |
|---|---|---|
| 010F1-R1 | P2 — an undefined “null-like sentinel” heuristic could hold a valid declared `None`, `N/A` or `null` choice | Spec §4 now treats every declared literal as an ordinary choice through its approved mapping. Actual JSON null remains separate. The code evidence records that no source contract permits the heuristic. Reviewer confirmed resolved in the frozen draft. |

The coordinator independently identified the same ambiguity. Before final review,
the author also corrected tag fan-out to respect both 200 tags/Organization and
20 tags/Person, and specified bounded application-owned HMAC identities with
encrypted exact-label collision checks for source tags/choices lacking vendor IDs.
The reviewer included these changes in the final complete-document pass. No
additional full review loop or implementation review was performed.

Frozen reviewed document hashes (SHA-256):

| Document | Hash |
|---|---|
| Specification | `0a569679f50c671d018b37bac9f87b6ba811723a24ccf93185a5a9b507dce2b1` |
| Execution brief | `c8f0fd56119372cc766db4712edd9edaa97d1edd403de846692d1fa3ce34091f` |
| Code-contract evidence | `45b94e6d21be73ea21d18a5b9623565672c35f594dd2e850a13bb037fae64a24` |

## Verification scope and approval handoff

This is a read-only design review against retained source documentation and local
code/test precedents. No application builds, tests, database operations or live
FUB requests were performed for 010f1 planning. The separate 010c release evidence
does not prove the proposed metadata implementation. Coordinator handoff checks
passed across 45 documentation files: 16 Markdown files, 302 local links,
eight Markdown anchors, 19 JSON files, whitespace and all three reviewed hashes.
Planning remains uncommitted.

Full approval must accept or amend spec §10's proposed completed-parent-only and
one-child lifetime, independent held-item subset, explicit catalog mappings and
creation, unchanged native limits, no replacement of local values and conservative
type/recurrence/collision rules. The shared-contract approval boundary is
AGENTS §11. These are reviewable proposals, not newly accepted decisions.
Notes/tasks, repair/delta imports, activation, source-account validation and
customer-data readiness remain separate work. No implementation starts from this
READY verdict alone.

## Subsequent approval

The user then said “ok proceed with 010f1.” D-066 accepts the full reviewed
specification, execution brief, proposed policies and declared shared contracts.
Implementation and isolated synthetic verification are now authorized. The hashes
above identify the reviewed draft before this approval annotation; release and
live-source work remain separate.
