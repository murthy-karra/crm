# Mobile 003 / 010e3 — Implementation status

**Native, browser and regular database acceptance passed; final query-plan check in progress —
2026-09-13.** D-078 accepts both contracts and Mobile 003's occurrence-time policy.
Terra high implemented the parallel lanes; the coordinator integrated the shared
contracts and verified actual native/browser outcomes. The remaining check is
the final 25k-Person admission query-plan fixture.

The local integration branch is `codex/mobile003-010e3-integration`, based on
`619c1b3` plus approved planning documents. Main merge, push, shared deployment,
mobile distribution, live FUB processing and customer activation are separate.

## Completed implementation

- **Mobile backend:** atomic manual-contact fact and mobile receipt, immutable
  operation replay, server-enforced authorization and occurrence-time limits,
  and a fresh sealed Today reconciliation after acceptance. Existing Web and
  Operator commands retain their normal server-time behavior.
- **iOS and Android:** contact forms save into the protected SQLite outbox before
  network delivery; pending work survives process termination, lost responses
  and upgrade. The UI distinguishes locally saved, server-accepted and reconciled
  work. Both native lanes have actual simulator/emulator evidence against the
  isolated API, including installed previous-version database upgrades.
- **Migration 010e3:** exact retained-evidence preview, inherited mappings,
  bounded contact/source-field review, explicit confirmation, resumable per-item
  execution, global identity/provenance, immutable admission history and
  cancellation/remainder planning. New People remain core-only and under the
  existing administrator review hold.

## Verification

| Area | Actual evidence |
|---|---|
| Mobile backend | Three focused real SQLx/router cases cover time normalization, exact replay/concurrency, old-contact/newer-Inquiry chronology, Today parity, rollback and current authorization. API3102 also accepted and replayed real HTTP operations. [Record](MOBILE_003_BACKEND_VERIFICATION.md). |
| iOS | 32 storage/model tests; real API lost-response replay; offline contact → terminate/relaunch → receipt → fresh Today; installed Mobile002 schema5 → Mobile003 schema6 retained active generation, accepted note, queued task and draft bytes. Upgrade comparison passed with zero skips. Default app build-for-testing also passed. [Record](MOBILE_003_IOS_VERIFICATION.md). |
| Android | Six storage instrumentation tests; real API offline task/contact → force-stop/relaunch → exact outbox bytes → receipts and fresh Today; installed Mobile002 schema3 → Mobile003 schema4 preserved queued and accepted work. [Record](MOBILE_003_ANDROID_VERIFICATION.md). |
| Migration backend | Retained-source execution, cancellation/remainder, budget/lease/authority fencing, immutable identity/plan, scoped DB permits, executable readiness/partial-schema rejection, exact Unicode/contact/cursor traversal and atomic rollback. Four additional adversarial PostgreSQL tests passed. [Record](SLICE_010e3_VERIFICATION.md), [negative cases](SLICE_010e3_ADVERSARIAL_VERIFICATION.md). |
| Actual browser | Production Web build and production router/commands/worker with explicit synthetic harness readiness; desktop 1280/390px preview → confirm → partial cancellation → new remainder → both Person profiles/history/provenance → reload/logout. Exact 63-KiB Unicode source and 56-contact traversal passed. [Record](SLICE_010e3_VERIFICATION.md#actual-browser-acceptance). |
| Preservation | Final browser fixture: two new People, two identities/results/provenance/admission facts, 57 contacts, zero Inquiries, zero remaining reservations. Original People, contacts, parent, mappings and results had identical canonical row-byte fingerprints before and after. |
| Performance | Paired Today passed (baseline p95 122.84 ms, current 131.89 ms). Quiet paired ordinary Person read passed (baseline p95 21.40 ms, current 22.50 ms, allowed 46.40 ms) with equal responses on 25k People/50 members. First loaded-machine Person run failed and remains recorded. These are laptop regression checks. |
| Repository/Web | Combined `scripts/check` passed: 981 nonignored Rust tests, 5 doctests, 44 preflight tests, 90 Web files/1212 tests, 11 email-worker tests, format/lint/type/build checks. After browser fixes, the full Web run passed 90 files/1215 tests; affected typecheck/lint/build passed. |
| Database | All 1,009 regular cases have passing evidence: 136 initial passes plus 873 remaining/corrected cases. The initial run stopped on a test comparing separate app and Docker clocks; the repaired test observes lock contention and samples the app clock. Exact test-selection and log-hash manifests preserve the distinct passes. After the final paging/index change, all 15 affected admission cases passed again, as did fresh-schema SQLx `prepare --check --workspace` and workspace Clippy with warnings denied. |

## Integration and environment

The mobile backend and both native worktrees have been integrated and closed.
The Web lane was integrated and its worktree closed after actual browser testing.
The final migration worktree remains until database/query-plan closeout. No
shared API3000/Web5173/demoAPI3101 process or root build artifact was replaced.

Private synthetic runtime/database/build evidence is retained under
`/private/tmp/crm-mobile003-qa/` and `/private/tmp/crm-010e3-qa/`. The final browser
API3103/Web5174 processes were stopped and the temporary browser closed after
acceptance. Both browser fixture databases remain available for inspection;
ordinary installed native stores and shared development databases are intact.

Physical phones and real cellular testing remain deferred by the user's choice.
Broad mobile design work, production capacity, customer readiness, publication,
live source migration, activation and deployment are not claimed by these tests.
