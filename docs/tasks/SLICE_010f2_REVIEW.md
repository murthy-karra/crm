# Slice 010f2 — Independent plan review

2026-09-11; source baseline main `cd3b010`, following the verified
[010f1 shared-development release](SLICE_010f1_RELEASE.md). Scope: complete
[specification](../specs/SLICE_010f2.md), [execution brief](SLICE_010f2_IMPL.md),
[code/source evidence](../research/SLICE_010f2_CODE_CONTRACTS.md), authoritative
decisions and relevant native/source/coordinator/read-guard code under
[04-review-plan](../prompts/04-review-plan.md).

Author/coordinator: `root`. Independent complete-plan reviewer:
`activity_plan_review`, with a read-only storage/concurrency subreview. Source
research: `next_import_review`; coordinator/read-boundary research:
`release_audit`. No model override was made; billed usage is unavailable here.

## Verdict and disposition

**READY for user review. Full specification, shared contracts and implementation
remain unapproved.** D-067 accepts only the readable-note/preserved-original and
confirmed-zone date-only choices, plus planning and independent review.

The reviewer read the complete frozen specification, brief and evidence, and
checked exact raw source qualification, note list/detail separation, source-only
and held data, native schemas/validators, authorship, timestamps, identity/erasure,
atomic writes, lease/receipt recovery, shared reservation settlement, workspace
read barriers, pagination, release compatibility and implementation ownership.
One actionable finding was returned as READY-WITH-FIXES and is now resolved:

| ID | Severity / concrete failure | Correction and disposition |
|---|---|---|
| AF2-001 | P2 — an all-held plan could be confirmed without any eligible note/task, permanently consuming the one activity child before mappings were chosen | Spec §5/§6 and the confirm contract require at least one eligible INSERT or verified `already_present` unit. All-held fresh confirmation returns 409 and remains replannable. Zero inserts after cancellation or post-confirmation revalidation remain supported. A7 and the brief require these tests. Targeted independent re-review confirmed resolved and returned READY. |

The reviewer also requested wording clarity, now applied: explicit Retry adopts
the currently authorized admin under the existing 010f1 policy; a worker cannot
silently adopt another executor. No other blocking findings remained. The
correction received targeted re-review only; no additional full review loop or
implementation review was performed.

Final reviewed document hashes (SHA-256), independently verified after correction:

| Document | Hash |
|---|---|
| Specification | `324a53395f5b633a926c19f7e88c3dd95042dfd13e84372da5f6ae14f4eb1289` |
| Execution brief | `abfa8faddba734cdfed86054dd2caa82ad640d075864f2215fd0aad8c22543d6` |
| Code/source evidence | `4e4552a5940f66b40e70d5f296ce65a201855af28c31416c9e59c23ee2556720` |

## Verification scope and approval handoff

This is a documentation/design review against local code and public source
documentation. Saved notes-list, note-detail and tasks schema hashes match the
existing 010b manifest. No credentials/customer data, application builds/tests,
database operations, runtime mutations or live FUB API calls were used. This
does not qualify an actual FUB account or prove the proposed implementation.

Coordinator handoff checks passed: all eight changed/untracked files are Markdown
under `docs/`; 135 local links and two heading anchors resolve; all three reviewed
hashes match; `git diff --check` passes. No application test gate was run for this
documentation-only task.

Full approval must accept or amend spec §10: one completed-parent activity child
and its lifetime; both-family exhaustion and acknowledged subset execution;
subject conversion and source-only replies/reactions/settings; explicit user/type
mapping and timestamp holds; native limits and no overwrite/resurrection; the
account-qualified source key, bounded review API/legacy-reader barrier and
activity-capable recovery requirement. These policies and shared contracts
remain proposals under AGENTS §11/§16, beyond D-067's two accepted choices.

Planning remains uncommitted on main. No implementation starts from READY alone.
010f1 remains the deployed release. Live FUB validation, customer-data readiness,
later repair/deltas and activation remain separate work.

## Subsequent approval

The user then said “Ok go ahead and implement 010f2.” D-068 accepts the complete
reviewed specification, execution brief, policies and declared shared contracts.
Implementation and isolated synthetic verification are authorized. The hashes
above identify the reviewed draft before approval annotations; live-source work,
Git integration and deployment remain separate scope.
