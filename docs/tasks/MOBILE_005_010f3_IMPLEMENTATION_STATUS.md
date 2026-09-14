# Mobile005 / 010f3 — Implementation status

**IN PROGRESS — D-082, 2026-09-14.** Both accepted plans are being implemented
with isolated synthetic verification. No implementation review has declared READY.
Publication, deployment, live-source migration and native distribution remain separate.

## Ownership and protected resources

- Coordinator: root `codex/mobile005-010f3-integration`; shared backend guards,
  readiness, SQLx, Web, integrated iOS corrections and final verification.
- Migration writer: `/Users/karrad/projects/crm-010f3`, `codex/migration-010f3`;
  exclusive migration Rust/SQL ownership, `20261002` migration prefix.
- Android finisher: `/Users/karrad/projects/crm-mobile005-android`,
  `codex/mobile-005-android`; exclusive Android ownership. Concrete verification
  blockers escalated to Astra under `docs/prompts/MODEL_ROUTING.md`.
- iOS implementation worktree remains clean; coordinator owns subsequent fixes.
  Mobile backend worktree was integrated and closed. At most three implementation
  worktrees; private build/evidence root `/private/tmp/crm-mobile005-010f3`.
- Preserve API PIDs61780/71121/33892 on3000/3101/3102 and Web PID59404 on5173,
  shared PostgreSQL/Centrifugo, protected build artifacts, `.env` and old native stores.
  Isolated native API3103, metadata API3104 and metadata Web5174 are assigned.

## Current implementation

Mobile backend and native profile editing are integrated. Independent review
round 1's 13 findings and round 2's six findings are corrected. The targeted
second-round assessment found no remaining actionable defect; READY still depends
on replacement final repository gates.
[MOBILE_005_IMPLEMENTATION_REVIEW_2.md](MOBILE_005_IMPLEMENTATION_REVIEW_2.md)
owns the final correction assessment and source attribution.

iOS passes 47 storage/model tests. Android checkpoint `80b6c48` passes 42 storage,
repository and Compose tests, plus two JVM tests and lint/build checks. Both
platforms retain passing actual native journeys and installed upgrade evidence;
reviewer-confirmed reuse preserves schema/key/envelope/UI-flow attribution.
[MOBILE_005_IOS_VERIFICATION.md](MOBILE_005_IOS_VERIFICATION.md) and
[MOBILE_005_ANDROID_VERIFICATION.md](MOBILE_005_ANDROID_VERIFICATION.md) own the
sealed evidence and earlier failure dispositions.

Migration backend fidelity and exact continuation are integrated through `1e9ed69`,
with the test-only Clippy correction `839155a`. All21 final functional checks and
the35,000-cell oversized native-baseline preservation test pass. All17 changed
hot statements have retained plan evidence on25,000 People/50 members, with exact
normalized logical bytes reported separately from physical relation sizes.
Runtime/preflight readiness passes51 tests and61 rollback-only incomplete-schema
variants. Populated original-import handover rollback/accounting/replay, legacy
session fencing and all204 outer HTTP responses pass. See
[SLICE_010f3_READINESS_VERIFICATION.md](SLICE_010f3_READINESS_VERIFICATION.md) and
[SLICE_010f3_VERIFICATION.md](SLICE_010f3_VERIFICATION.md).

Web implementation follows the frozen contract and passes focused components/views,
lint, typecheck and isolated production builds. The complete phased synthetic
browser journey passes preview/mappings, uncertain confirmation replay, partial
cancellation, exact continuation, reconciliation/reload and Person provenance at
desktop/390px. All15 protected table sets and both native cells remain unchanged;
56 People settle once and completed catalog work is reused. See
[SLICE_010f3_WEB_VERIFICATION.md](SLICE_010f3_WEB_VERIFICATION.md).

## Remaining completion gates

Native/browser/fidelity/performance acceptance is complete. The retained browser
reconciliation and preservation check also passes on final production API source
`e0bb7e3`. Final `scripts/check` passes on `a52a1e6`:992 ordinary Rust,5 doctests,
1,264 Web,51 preflight and11 worker tests, plus lint/type/build checks.
[Combined verification](MOBILE_005_010f3_FINAL_VERIFICATION.md) owns final gate
commands, evidence and any failures. SQLx preparation and the full DB gate run
sequentially with private outputs and serialized DB access.

Independent broad reviews returned **CHANGES REQUIRED**: Mobile005 round2 has six
response-validation findings;010f3 round1 has three qualification/ownership/coverage
findings. The handover-order correction `3755623` passes its new regression.
[Mobile round2](MOBILE_005_IMPLEMENTATION_REVIEW_2.md) and
[migration round1](SLICE_010f3_IMPLEMENTATION_REVIEW_1.md) own the finding records.
Native fixes are complete and independently assessed; the migration writer and
coordinator are completing ownership/coverage corrections and readiness checks.
Replacement final gates follow the corrected integrated source. Mobile005 is in
its final second review/fix cycle;010f3 has one review remaining. No missing trust
check or required evidence is a pass. Shared resources and stored work remain
preserved; final preservation verification follows the corrections.
