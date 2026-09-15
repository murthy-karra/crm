# Mobile005 / 010f3 — implementation status

**COMPLETE and independently READY — D-082,2026-09-14.** Both accepted plans are
implemented and verified within their isolated synthetic scope. Final tested code:
`ab4a362a224183dd1d51acd51196b71315826fd6`; subsequent completion commits change
only documentation. Local integration branch: `codex/mobile005-010f3-integration`.
Publication and deployment remain separate.

## Delivered behavior

**Mobile005:** iOS and Android support offline Person name/contact editing,
protected drafts and queued operations, exact replay, and explicit conflict
resolution against complete current profile data. The shared typed backend command
enforces authorization, tenant/workspace boundaries, idempotency and details
revisions across writers. Malformed/incomplete profile pages and impossible
receipts preserve saved work; stale cache qualification can recover at the same
broad Person revision.

**010f3:** admins can import tags and typed custom fields for later admitted
People from qualified retained source evidence. Bounded Web preparation, mapping,
confirmation/replay, cancellation, exact continuation, reconciliation and Person
provenance are implemented. Catalog ownership is bound to Organization, root and
source lineage; erased or mismatched People retain visible tag coverage while
Person writes remain held. Additive migrations preserve prior evidence and
accounting, including historical replan reservations. Readiness fails closed on
incomplete ownership schemas and reuses the established cohort index.

## Verification and reviews

[Combined final verification](MOBILE_005_010f3_FINAL_VERIFICATION.md) owns exact
commands, tested revisions, failed attempts, successful corrections and evidence.
All three sequential repository gates pass: `check`, `sqlx-prepare` and `check-db`.
The final DB suite passes1,076/1,076 tests; offline SQLx cache is unchanged.

| Area | Final evidence |
|---|---|
| iOS | [47 storage/model tests, real native journeys and installed upgrade](MOBILE_005_IOS_VERIFICATION.md) |
| Android | [42 storage/repository/Compose tests,2 JVM tests, build/lint and sealed native/upgrade proof](MOBILE_005_ANDROID_VERIFICATION.md) |
| Migration | [Functional, concurrency, fidelity, upgrade/accounting and query-plan proof](SLICE_010f3_VERIFICATION.md) |
| Readiness | [184 incomplete-schema variants and51 preflight tests](SLICE_010f3_READINESS_VERIFICATION.md) |
| Web | [Full browser workflow and final retained010 upgrade/preservation](SLICE_010f3_WEB_VERIFICATION.md) |
| Performance | [Single paired Today comparison and scoped plan evidence](MOBILE_005_010f3_PERFORMANCE.md) |
| Independent review | [Mobile005 round2 READY](MOBILE_005_IMPLEMENTATION_REVIEW_2.md); [010f3 round2 READY](SLICE_010f3_IMPLEMENTATION_REVIEW_2.md) |

All review findings are resolved within the two allowed rounds. The final reviewers
accepted evidence reuse for unchanged production/native/performance paths. No
remaining implementation or required verification work is deferred.

## Preserved resources and handoff

All74 protected artifacts and shared listeners on3000/5173/3101/3102 are unchanged.
Root `.env`, source evidence, old native stores/keys and shared services are retained.
The browser fixture preserves all15 protected rowsets, both original native cells
and exact import/snapshot/Organization-storage row hashes after009/010 upgrades.

Private evidence remains under `/private/tmp/crm-mobile005-010f3`. Owned native
API3103, metadata API3104/Web5174 and retained QA databases remain available;
immutable binary/runtime/source identities are in their verification records.
Migration, Android and iOS worktrees are complete and retained for the release
handoff. They have no active writers. Only inactive private compiler caches were
removed as needed; test artifacts and stored work remain.

The shared-development release remains Mobile004/010e4. No push, deployment,
native distribution, live FUB/customer processing, workspace activation or physical
phone/cellular work occurred. Calling remains deferred until the later calling
track. Future release work must follow the existing compatibility/readiness and
preservation requirements with fresh actual-workload evidence.
