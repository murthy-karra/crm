# Slice 010c — integrated Round 2 checkpoint

This freezes the approved backend plus integrated Web for the second and final bounded implementation review. The original BACKEND_CHECKPOINT.md and BACKEND_R1_CORRECTIONS.md remain immutable historical evidence. No live FUB/provider, shared development database, main runtime, Git mutation or deployment was performed.

## Integrated behavior

The current `/me` Organization mode/revision is the client authority. Workspace verification synchronously aborts private requests, advances a separate workspace epoch and clears private query/mutation caches while coalescing a fresh `/me` read. Ordinary route state, realtime, Operator and calls are fenced; terminal call cleanup stays available. Missing workspace authority fails closed. Same-identity import confirmation recovery remains mounted and hidden during verification, preserving only the reviewed request until the same administrator identity is verified; role/session/Organization changes discard it.

Members receive the waiting/logout experience. Administrators can inspect People, contacts and `person_imported` history, source provenance, exact scoped fields and mappings while ordinary mutations/outbound actions remain unavailable. Migration mounts the new import flow independently of source credentials and retains the existing assessment/snapshot controls. No activation, Inquiry creation or automatic source fetch was added.

The two opt-in performance/plan test modules are registered behind `perf-harness`, including the exact new mapping-field point query. Backend production behavior is unchanged from the READY R1 correction checkpoint. The two helper SHA256 encodings were adapted to the installed sha2 API; final helper edits after compilation were formatting only.

## Focused checks

All Web commands used Node 24.16.0/pnpm 11.22.0. Tests below overlap; do not add their counts as independent totals.

- Shared Web: `pnpm exec vitest run src/workspaceLifecycle.test.ts src/api/workspaceClient.test.ts src/api/client.test.ts src/api/queries.test.ts src/router.test.ts src/components/AppShell.test.ts src/components/PersonPreview.test.ts src/views/PersonDetailView.test.ts src/views/PeopleView.test.ts src/views/MigrationView.test.ts src/views/WorkspaceReviewView.test.ts src/telephony/useCall.test.ts src/realtime/useRealtime.test.ts src/todayPrefetch.integration.test.ts src/sessionLifecycle.recovery.test.ts src/sessionLifecycle.concurrent.test.ts` — **429 passed, 16 files, 5.37s**, no unhandled errors; `/private/tmp/crm-010c-web-shared-4.log`.
- After the final fail-closed-before-`/me` correction, the workspace/client/query/router/shell/session authority subset — **123 passed, 8 files, 2.84s**; `/private/tmp/crm-010c-web-fence-final.log`.
- Six new import API/component test files, including root-owned field/provenance cases — **34 passed, 6 files, 1.05s**; `/private/tmp/crm-010c-web-support-vitest.log`.
- Current project `pnpm typecheck` — **PASS**; `/private/tmp/crm-010c-web-typecheck-final.log`. Scoped primary and support ESLint — **PASS**, no warnings; `/private/tmp/crm-010c-web-primary-eslint-final.log` and `/private/tmp/crm-010c-web-support-eslint.log`.
- `SQLX_OFFLINE=true cargo check -p crm-api --test all --features perf-harness --locked` — **PASS, 33.38s**; `/private/tmp/crm-010c-web-harness-compile-2.log`. It emitted 74 dead-code warnings (36 duplicates) from the existing shared Today performance driver included by both opt-in collectors. These warnings were retained, not suppressed; the ordinary final Clippy gate does not enable perf-harness.
- Current Cargo formatting and Git whitespace checks — **PASS**; formatting log `/private/tmp/crm-010c-r2-fmt.log`.

Initial focused runs found a broad existing Person test mock returning a Person body at the new provenance endpoint, a duplicate Vue key attribute, and platform-only session-recovery fixtures attempting Organization Today access. Those were corrected; successful replacement logs above contain no unhandled errors. The initially registered opt-in collectors also exposed unsupported SHA256 LowerHex formatting and standalone Rust2024 formatting; both were corrected without changing production SQL or its frozen baseline bytes.

## Pending required work

Round 2 review and bounded corrections, then final sequential SQLx prepare, check and check-db, the actual D-050 comprehensive query plans and one paired reader regression, plus private synthetic browser QA remain required. Neither the plan collector nor paired load was executed at this checkpoint. Authorized live-FUB qualification remains the user's later validation; this checkpoint makes no such claim.

## Frozen source and logs

`R2_SOURCE_SHA256.json` contains **207 source hashes** and eight current verification-log hashes. Its SHA256 is `f35f3d5e6487debb107776898c59b4b1d801260d8a25f8cc9478d3be8b37773f`. The source group counts are 143 backend, 59 Web and 3 script files plus `.env.example` and the concrete contract. The optional performance-fixture formatting changed only Rust wrappers; frozen SQL and its baseline manifest bytes are unchanged. All sources are frozen for Round 2 review.
