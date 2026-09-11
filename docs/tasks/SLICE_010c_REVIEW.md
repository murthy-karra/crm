# Slice 010c — Independent plan review

2026-09-11; source baseline main `c6c5930` (deployed 010b `89471f0`).
Scope: complete [specification](../specs/SLICE_010c.md),
[execution brief](SLICE_010c_IMPL.md),
[current-code record](../research/SLICE_010c_CODE_CONTRACTS.md), D-064 and relevant
implementation/test precedents under `04-review-plan.md`. Read-only reviewer:
`snapshot_harness`; source and workspace/domain consultations were separate.
Actual inherited model/effort and billed usage are not exposed in the returned
review evidence; no model switch or cost claim is made.

## Verdict and corrections

The complete independent review returned **READY-WITH-FIXES**. All five findings
were corrected; one targeted confirmation returned **READY**, with no introduced
blocker from the plan-replacement clarification. No additional full review ran.
Readiness is not
full specification approval or implementation authorization. Only D-064's three
user choices are accepted.

| ID | Severity / concrete failure | Correction and required proof |
|---|---|---|
| C010-R1 | P1 — raw qualification applied only to People; truncated/conflicting stage/user evidence could drive writes | Spec §3 applies accepted-capture/ID/HMAC/ordinal/variant rules to all supporting evidence; unqualified mappings and dependent People stay held. Add supporting-record negative fixtures. |
| C010-R2 | P2 — normalizable strings can violate native text or full-value index limits after confirmation | Spec §4 requires NUL/native checks and a disclosed import-specific 2,048-byte indexed-value bound before readiness; preserve/hold unsupported inputs. Test poorly compressible near-4 KiB inputs and boundary inserts; ordinary normalization remains unchanged. |
| C010-R3 | P2 — fixed 2 MiB execution admission cannot fit a larger valid provenance record | Spec §6 freezes a proved per-item retained-byte bound, reserves before atomic commit and settles exact bytes within the 64 MiB work-unit ceiling. Intrinsic oversize is held before confirmation; >2 MiB within allowance must succeed. |
| C010-R4 | P1 — old server/worker/CLI rollback ignores review mode and reopens use | Spec §7 requires retiring pre-gate runtimes before confirmation, no mixed operation, and persistent-binding-aware deploy/rollback preflight that rejects old/unknown artifacts. Recover with compatible code while preserving mode/data; eventual release evidence must demonstrate the boundary. |
| C010-R5 | P2 — inventory disqualified all permanent Operator audit, contrary to pending-work-only spec | Spec §5 and inventory now allow terminal IDs-only audit, reject active admissions/actionable/claimed work and independently exclude actual business content. Verify a completed harmless turn permits entry and active work does not. This eligibility clarification is proposed with the full spec, not silently added to D-064. |

R1/R3 incorporate coordinator self-checks; R2 also received an independent factual
source/destination check. The full reviewer checked all five against the code.
No finding reopens separate People, explicit stage approval or review-only use.
The author also clarified pre-confirmation plan replacement: paused/failed
revisions may be explicitly superseded, while cancellation stops the whole run.

## Verification and limits

- Documentation paths/anchors, whitespace and documentation-only scope: **PASS**
  across all eight changed Markdown files; final counts are recorded in the
  current project state. Initial checks preceded creation of this review file;
  all its links are included in final validation.
- Application, DB, browser, performance and release checks: **NOT RUN** during
  planning. Prior 010b results are not 010c evidence. No code/schema/runtime change.
- Live authorized FUB validation: **USER-DEFERRED**. Public documentation and
  repository code were inspected; no source-account operation or customer record.
- No deployment, commit/push, import, restore exercise or recurring monitor was
  performed. Planning leaves these docs uncommitted for review.

## Approval handoff

Present the complete spec/brief, including the proposed frozen-plan recovery,
eligibility limits, admin review boundary and compatibility constraint. Full
approval is needed before implementation under AGENTS §11's shared-contract rule.
Later activation, real-data prerequisites and live-source qualification stay
separate. The authorized 010b release remains unchanged.

## Subsequent approval

The user then said “approved for implementation.” D-065 accepts the full
reviewed specification, declared shared contracts and execution brief. The
planning findings/results above remain historical evidence; implementation and
synthetic verification are now authorized. Release and live-source operations
remain separate.
