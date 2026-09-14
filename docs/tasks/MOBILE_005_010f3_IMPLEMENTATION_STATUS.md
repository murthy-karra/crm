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

Mobile backend and native profile editing are integrated. Independent Mobile005
review round1 returned **CHANGES REQUIRED: 13 findings**, recorded in
[MOBILE_005_IMPLEMENTATION_REVIEW_1.md](MOBILE_005_IMPLEMENTATION_REVIEW_1.md).
Coordinator backend/iOS corrections include stable profile reads, derived revision
protection, deterministic added-contact order, per-kind primary fallback and bounded
current-profile traversal. Five focused backend regressions pass. iOS passes43
storage/model tests and both actual native journeys; installed upgrade evidence is
retained in [MOBILE_005_IOS_VERIFICATION.md](MOBILE_005_IOS_VERIFICATION.md).

Android correction checkpoint `debfccb` addresses its round1 findings. Captured
storage runs pass11 and8 tests. Complete repository/native acceptance and a
verifiable installed-upgrade record remain open with the Android finisher. Earlier
failed/empty logs do not count as acceptance; no protected store may be reset to
obtain a pass. See [MOBILE_005_ANDROID_VERIFICATION.md](MOBILE_005_ANDROID_VERIFICATION.md).

Migration atomic execution and bounded staged preparation through `e8a0165` are
integrated: all four field types, tags, local-value protection, claims and per-unit
settlement. Staged preparation has11 functional/recorded-plan checks passing.
Immutable replanning through `380bcfb` is also integrated. Frozen transport/readers
and recovery/remainder completion remain with the migration writer. Runtime and preflight readiness now require the
full staged schema, enabled guards and work indexes;51 preflight tests and the
22-variant rollback-only DB readiness regression pass. See
[SLICE_010f3_READINESS_VERIFICATION.md](SLICE_010f3_READINESS_VERIFICATION.md) and
[SLICE_010f3_VERIFICATION.md](SLICE_010f3_VERIFICATION.md).

Web implementation follows the frozen contract and passes focused components/views,
lint, typecheck and isolated production build. The synthetic browser journey passes
login, original-import/admission selection and390px layout checks. Full preparation,
confirmation, cancellation, remainder and provenance acceptance awaits the complete
migration transport; see [SLICE_010f3_WEB_VERIFICATION.md](SLICE_010f3_WEB_VERIFICATION.md).

## Remaining completion gates

1. Finish Android acceptance and migration lifecycle/transport; integrate tested commits.
2. Complete the real synthetic metadata browser journey.
3. Run sequential final `scripts/check`, `scripts/sqlx-prepare`, `scripts/check-db`
   with private outputs and serialized DB access.
4. Finish migration changed-query evidence. The single D-050 Today pair and all11
   Mobile hot plans pass; see [MOBILE_005_010f3_PERFORMANCE.md](MOBILE_005_010f3_PERFORMANCE.md).
5. Complete independent implementation reviews, then verify protected resources.

Mobile005 has consumed one of at most two implementation review/fix rounds;
010f3 has consumed none. Round2 starts only after the Mobile005 corrections and
required evidence are complete. Blocking gaps are never reported as a pass.
