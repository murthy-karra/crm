# Mobile 007 — Implementation briefs

**IMPLEMENTATION ACCEPTED — D-086, 2026-09-15.** The user accepted both plans
and declared contracts for implementation and isolated synthetic verification.
Both independent planning reviews are READY. Compatible review
corrections are owned work; materially different policy and publication/deployment
remain separate. [Current implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md)
owns progress and evidence; planning-era wording below does not limit D-086.

[Specification](../specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md)
and [coordination/checks](../plans/MOBILE_007_010d3_PLAN.md) govern scope. No writer
or worktree is launched by this document.

## A. Shared backend

Proposed branch `codex/mobile-007-backend`, one primary writer. Own mobile domain
search adapter, narrow Person discovery query, mobile route/fixtures/tests and
`mobile/contracts/mobile007/`. Coordinator owns common router/guard/script/cache
integration. No migration import schema, ownership or Web migration edits.

1. Freeze `MOBILE_007_CONTRACT.md` and wire fixtures for request validation,
   capability, rows/nulls/order/has_more, auth/context/no-store and errors. Inspect
   current PersonSummary bounds and primary contact ordering. Extract only shared
   match/normalization logic needed; do not refactor unrelated search or full reads.
2. Add typed bounded operational search through existing server-owned context and
   visibility. No client identity, search URL parameters, inquiry aggregates,
   business mutation or access-window renewal. Keep no-PII tracing.
3. Prove M7-02/03 and query shape at realistic book size; verify existing
   Operator/Web matching and mobile protocol behavior remain compatible.
4. Integrate fixtures and verified backend before native clients begin. No schema
   migration is expected; an index, if justified by measured query shape, belongs
   only to this lane after coordinator version allocation.

## B. iOS

Proposed `codex/mobile-007-ios`, exclusive `ios/` and assigned evidence. Read
`Protocol.swift`, `API.swift`, `FieldModel.swift`, `LocalStore.swift` and People
screens. Use the frozen API capability and preserve existing metadata generations.

Implement distinct local/Organization search, explicit submission, transient
result state and cancellation/late-response fences. Replace UUID entry as the
normal discovery flow with selected-result pinning. Persist requested state before
acknowledgement, reuse one sync coordinator and derive availability only from a
complete local sealed bundle. Add explicit request cancellation without deleting
protected work. Preserve known-ID compatibility as a secondary fallback if needed,
not a requirement for ordinary discovery.

Prove M7-01/04–08 on owned simulator/API, including interrupted request+download,
pin during generation, invalid pin recovery, same-name disambiguation, failed local
commit, access/account epochs, all pre-existing drafts/receipts/overlays and an
in-place populated Mobile006 upgrade without uninstall/key replacement.

## C. Android

Proposed `codex/mobile-007-android`, exclusive `android/` and assigned evidence.
Read `Protocol.kt`, `FieldApi.kt`, `FieldRepository.kt`, `FieldStore.kt`,
`FieldDatabase.kt`, `MainActivity.kt` and the shared fixtures. Mirror B in Compose
and existing encrypted Room/SQLCipher storage; retain platform-native lifecycle
handling and the same truth boundary for search hits versus downloaded bundles.

Same acceptance matrix on owned emulator/API. Pin removal must not cascade away
pending drafts/outbox or suppress Today/assigned selection. Run installed-store
upgrade separately from connected tests that clear/reinstall their test app.

## Reviewed storage/scheduling checkpoint

Both native owners implement spec §4a explicitly: additive encrypted pin intents,
monotonic pin-set/staging/admitted revisions, coalesced follow-up after in-flight
add/cancel and restart, retained manifest/last-sealed selection reasons, generic
all-pins-failure review and populated-Mobile006 upgrade. No independent mutable
search-result cache or second pin authority. Freeze exact schema/DAO/transaction
checks before UI work. The accepted search error split preserves400 malformed
JSON/shape/unknown fields and422 invalid well-formed terms.

## Handoff and verification

The coordinator freezes expanded commands, exact SDK/device/variant/API and build
output ownership using current native READMEs at launch. Each lane records runner,
source/contract revision, command exit status, acceptance IDs and failures in its
own verification file. Follow paired final gates; no physical-device, live customer
or distribution evidence is implied. Required independent review is separate
from this author's source inspection; use at most two review/fix rounds per slice.
