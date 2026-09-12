# Slice 010d2 — Focused planning review

2026-09-12. Baseline: `27f3fe4654ace5840412e1364ea36974a1266ad8`.
Scope: [specification](../specs/SLICE_010d2.md),
[execution brief](SLICE_010d2_IMPL.md) and
[code findings](../research/SLICE_010d2_CODE_CONTRACTS.md).
D-071 authorizes planning/review and the user's metadata-first selection.

**Status: READY for implementation approval.** One independent planning review
returned NEEDS REVISION with two findings. The author made one correction pass;
the reviewer confirmed both resolved and returned READY with no remaining
concrete gap in that check. This is not implementation or runtime verification.

| Finding | Evidence and correction |
|---|---|
| HD2-P01 · P1 · CONTRACT — executable old-artifact fence | The deployed 010f2 core bypasses the activity complete-read helper; old startup cannot discover a new anchor table. Spec §§2/6/8 now require capability enforcement in the common DB read guard, transaction-local on each actual reader connection, preserving authorization precedence and pool isolation. Only upgraded APIs promise the exact new 409. Trusted inventory drains/retires unsupported artifacts and selects compatible recovery; a fresh report does not upgrade old code. Brief step 2 and T7 require actual old-artifact and connection-isolation evidence. |
| HD2-P02 · P2 · BOUNDARY — consistent page revision | Atomic revision writers alone cannot prevent a commit between separate family queries. Spec §6 requires one consistent DB snapshot or equivalent locking for revision, counts and every candidate query. T6 and the brief require a concurrent commit between family reads, plus metadata/suppression refresh coverage. |

The same correction pass clarified the no-body boundary, current responsible
Confirm/Resume executor, kind-qualified detail keys and rendered-metadata revision
writers. No additional blocking finding was reported in interpretation, date
qualification, stable identity/held variants, native-effect exclusions,
cancellation or erasure scope. Existing customer-erasure and live-source limits
remain explicit; they were not reopened or claimed complete.

Review method: read-only source/decision inspection using `rg`, `sed`, `nl` and
`git rev-parse`. Two earlier bounded discovery inspections supplied code findings.
No application tests, DB actions, source requests, services or deployments ran.
Planning checks: all local Markdown file targets in the seven changed documents
exist, and `git diff --check` passed. Runtime acceptance T1–T8
belongs to the later implementation, with concrete DTO/SQL/cursor freeze first.
