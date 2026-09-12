# Project state

Last updated: 2026-09-12 (010d1 implemented and synthetically verified in uncommitted worktree; bounded completion audit and 010f2 release qualifications retained).
This file holds current operational status, active work and live residuals.
[PROJECT_HISTORY.md](PROJECT_HISTORY.md) preserves earlier progress, the slice
ledger and historical measurements; its old instructions are not current work.

## Current state

The [2026-09-12 completion audit](../tasks/SLICE_010_COMPLETION_AUDIT_2026-09-12.md)
independently matched GitHub/main, all 1,016 source hashes, the running release
artifacts and all 35 applied migration checksums. Shared business counts/workspace
states remain unchanged. Fresh service-free checks and a complete 908-test DB
rerun passed; two earlier runs reproduced the pre-existing call-history ordering
failure, and nextest LEAK annotations have passing isolated follow-ups. Two extra
A6 audit tests passed note/task local-state preservation cases absent from the
original focused coverage. An overnight database-warning episode strongly
correlates with host sleep/background wake and stopped on full wake; its exact
mechanism is unconfirmed. No product source/runtime change or live FUB operation
was made. Audit documentation/evidence remains uncommitted.

**010f2 is deployed and verified in shared development**, including assessment,
core snapshot/preview, People, tags/custom fields and retained notes/tasks.
The deployed source is `fc5a8757bcb2591a43976544e282722b3616039f`
(implementation `9f457bc8db8707fa4ce361aac485fd6bf928bf34`) at
[app.tarams.org](https://app.tarams.org/manage/migration).
Migration `20260920000001` is applied on `crm_dev`; API PID 25757 on port 3000
and Web PID 25780 on port 5173 were observed at 22:18:29 PDT on 2026-09-11.
Verify live identity again before a later operational action.

The [release record](../tasks/SLICE_010f2_RELEASE.md) and
[sanitized evidence](../design/qa/slice-010f2-2026-09-11-release/README.md)
record locked API/admin/migrator and production Web builds (36.89/1.42 seconds),
the additive migration, backup catalog, actual-DB compatibility preflight,
50 HTTP/auth/asset checks and eight completed public browser checks with ten
inspected desktop/390px screenshots. Activity refresh/reload returned 200;
member access was denied and temporary sessions were revoked. The browser's
final telemetry assertion was qualified offline after all functional checks:
four deliberately blocked Cloudflare Insights GETs were the only unexpected-origin
entries. The raw attempt is retained; the UI flow was not repeated.

All 45 business counts and three operational workspace revisions are unchanged;
all 56 migration tables (including 13 activity tables) and operation admissions
are empty. The 110,063,438-byte backup's 932-entry catalog was validated;
restoration was not exercised. Tunnel routing passed 200/200/101. A bounded
214.61-second UTC observation found stable listeners and zero API/Web WARN/ERROR
entries; three backend, 73 Web and 1,016 source hashes matched. Implementation
gates remain applicable because production source did not change during release.
The merged worktree/branch and disposable QA files were removed after preserving
249 original evidence files and the eight pending main planning files.

Import confirmation requires a fresh operator compatibility report at
`CRM_MIGRATION_RELEASE_REPORT`. Actual-DB launch and confirmation checks passed
with workspace, metadata and activity readiness. The recorded five-minute report
expires at **2026-09-12 05:18:49 UTC** (2026-09-11 22:18:49 PDT).
Renew from an actual workload inventory before a later confirmation. Expiry
leaves ordinary CRM access available and import confirmation unavailable.
No automatic renewal or recurring monitor was established.

No live FUB connection, assessment, snapshot or import was created. Registered
system configuration remains unset, so upstream reads fail closed. Live
validation remains user-deferred. Source authorization does not replace
customer-data prerequisites, including D-015's erasure runbook. No activation
or production-cluster deployment occurred. The bounded SQLx cancellation
follow-up remains open in production readiness.

## Current slice

- **010d1 implementation:** implemented and synthetically verified under D-070 in
  `/Users/karrad/projects/crm-worktrees/slice-010d1` on
  `codex/slice-010d1-history-capture`; source remains uncommitted and shared
  development remains 010f2. Both bounded reviews are READY. SQLx preparation,
  full checks (949 Rust/1,131 Web plus supporting suites), 934 DB tests, the
  75,000-observation collector and desktop/390px production-Web walkthrough
  passed. See [verification](../tasks/SLICE_010d1_VERIFICATION.md) for exact hashes,
  failure/recovery evidence, query-work limits and owned cleanup. D-069's
  [split](../specs/SLICE_010d.md) is preserved: capture stores encrypted source
  evidence and bounded admin coverage without native timeline/Today changes.
  Public text pagination remains provisional and live-unqualified. 010d2 native
  semantics/timeline, email/media, deltas, activation and customer-data readiness
  remain separate work. Git integration/publication and deployment were not
  performed or authorized by this implementation approval.
- **010f2 implementation:** implemented under D-068; both bounded reviews,
  measured SQL/paired-reader checks, all 18 production-Web phases and exact
  native/business/storage reconciliation have passed. All final gates and owned
  cleanup passed; see [verification](../tasks/SLICE_010f2_VERIFICATION.md).
  Retained notes/open/completed tasks use explicit mappings, preserved originals,
  confirmed timezone and bounded admin review under the unchanged hold. Source
  is committed, merged, pushed and deployed under the D-068 follow-up; the merged
  worktree/branch was removed. See [release evidence](../tasks/SLICE_010f2_RELEASE.md).
  SQLx cancellation notices remain tracked with attribution limits in production
  readiness. Live FUB/customer-data work and activation remain separate scopes.
- **010f1 implementation and release:** complete under D-066 and its follow-up. The
  [specification](../specs/SLICE_010f1.md),
  [brief](../tasks/SLICE_010f1_IMPL.md) and
  [code evidence](../research/SLICE_010f1_CODE_CONTRACTS.md) cover tags and
  custom-field definitions/options/values for the already imported People in
  the existing review workspace. Notes/tasks are now delivered by 010f2; full
  standalone-tag capture, deltas and activation remain following work. [Independent plan review](../tasks/SLICE_010f1_REVIEW.md)
  returned READY with its one finding resolved. The accepted scope preserves current limits,
  including 20 tags per Person, 200 tags per Organization and 50 live custom
  fields; incompatible/ambiguous data and differing destination values are held.
  Implementation used an isolated worktree with one backend/database owner,
  Web authoring after the frozen contract and two bounded review/fix rounds.
  [Verification record](../tasks/SLICE_010f1_VERIFICATION.md)
  records 900 Rust, 1,054 Web and 881 DB test passes, 90 query plans / 246 checks,
  51 real-API/production-Web browser checkpoints and 18 inspected screenshots.
  Both bounded implementation reviews are READY. Exact native values, unchanged
  review bindings, storage pause/resume and partial cancellation are verified.
  Implementation `f37ddd1` and merge `e36ce36` are published; the merged worktree/
  branch and disposable QA services/databases are removed. The
  [shared-development release](../tasks/SLICE_010f1_RELEASE.md) passed its separate
  HTTP, browser, compatibility and tunnel checks. No live FUB validation occurred.
- **010c implementation:** approved under D-065, committed as `fcd2480`,
  merged as `c3f6ca9` and pushed to main under its follow-up authorization.
  The [spec](../specs/SLICE_010c.md) and [brief](../tasks/SLICE_010c_IMPL.md) cover
  People/contact/stage/assignment import from retained 010b evidence, separate
  People with contact overlap flags, explicit mappings, atomic empty-Organization
  entry and a durable administrator review hold. Both bounded implementation
  reviews and their corrections are complete; executed query-plan and browser
  findings received targeted fix confirmation. The final sequential gates passed:
  877 Rust tests, five doctests, 1,002 Web tests, 845 DB tests, 14 release-preflight
  tests and 11 email-worker tests. All 59 query plans/258 assertions and the single
  paired reader benchmark passed. Production-Web browser verification passed all 50 recorded checkpoints,
  including six states at six viewport widths. [Evidence summary](../design/qa/slice-010c-2026-09-11/README.md) and
  [verification chronology](../tasks/SLICE_010c_VERIFICATION.md) retain the exact
  source, checks and limits. Git integration, publication and cleanup are complete;
  the subsequent [shared-development deployment](../tasks/SLICE_010c_RELEASE.md)
  is verified. No live FUB operation or activation occurred.
- **Foundations documentation:** user authorized the three deliverables on
  2026-09-11: refresh the system map/state, draft foundations, define readiness.
  [System map](../architecture/ARCHITECTURE_BASELINE.md),
  [foundations proposal](FOUNDATIONS.md) and
  [readiness checklist](PRODUCTION_READINESS.md) are the handoff artifacts.
  New policies are proposals, not accepted decisions or implemented capabilities.
- **010b:** [spec](../specs/SLICE_010b.md), [brief](../tasks/SLICE_010b_IMPL.md)
  and [source qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md) are
  approved under D-063 after independent review returned READY. Core-first
  sequencing remains D-061. The approved contract includes representation-aware
  capture, retained-read authorization, frozen preview inputs, bounded overlap
  groups, durable logical-byte reservations and revisioned budget increases.
  Backend and Web are complete, including explicit budget review, separate source
  and preview recovery, partial/previous reports and session fencing. Both bounded
  review/fix rounds are complete. Final sequential `sqlx-prepare`, `check` and
  `check-db` passed: 860 Rust tests, 5 doctests, 933 Web tests, 11 email-worker tests
  and 809 DB tests. The 25,000-People group case and real-API/production-build Chrome
  workflow, storage recovery and desktop/390px verification passed. See the
  [verification record](../tasks/SLICE_010b_VERIFICATION.md) and its sanitized evidence.
  The subsequent D-063 follow-up authorized integration, shared-development
  deployment and cleanup; all are complete. Live FUB validation stays deferred.

## Current branch

Main contains 010f2 implementation `9f457bc8db8707fa4ce361aac485fd6bf928bf34`
and merge `fc5a8757bcb2591a43976544e282722b3616039f`, published to origin/main.
Shared development runs that merge; all 1,016 verified source hashes match.
The subsequent release documentation records evidence without changing the
runtime implementation. Only main remains; the merged activity branch/worktree
and disposable QA directory are removed. The temporary integration stash was
dropped after preserving the eight pending planning files and checking their
merged versions (four identical, four expected implementation/status updates).

Shared `crm_dev` data and historical recovery material are preserved. All 32
recorded retired binary paths retain their bytes without executable permission.
Recovery requires the compatibility runbook and fresh activity-capable workload
evidence. Inspect Git and [the release record](../tasks/SLICE_010f2_RELEASE.md)
for current source/runtime identity.

## Last accepted decision

**D-070:** the user said “Ok go for it” after the complete 010d1 plan and READY
review were presented. The full spec/contracts, implementation and isolated
synthetic verification are approved. Native history/timeline, live FUB/customer
work, activation, Git publication and deployment remain separate scope.

**D-069:** the user authorizes 010d planning and selects capture/coverage in
010d1 followed by verified historical timeline import in 010d2. This accepts
the sequence only; complete proposed contracts and implementation still require
review/approval. Live FUB validation remains deferred.

**D-068:** the user approved the complete reviewed 010f2 specification, policies,
shared contracts and implementation with isolated synthetic verification. This
includes the bounded native review/body-read amendment and activity-capable
recovery contract. The subsequent follow-up authorized Git integration, cleanup
and shared-development deployment, now complete. Live FUB/customer processing
and activation remain separate scope.

**D-067:** next notes/tasks import planning and independent review are authorized.
The user accepts readable plain-text HTML notes with preserved originals and
date-only deadlines at the end of the day in an explicitly confirmed source
timezone. D-068 subsequently accepts the complete reviewed 010f2 specification and
implementation; its follow-up completes the shared-development release. Source
access remains separate scope.

**D-066:** the complete independently reviewed 010f1 specification, execution
brief, policies and declared shared contracts are approved for implementation
and isolated synthetic verification. Completed-parent-only, one metadata child,
explicit creation/mapping, held-item subset acknowledgement, unchanged limits
and no replacement of local values are accepted. The explicit follow-up
authorized commit, merge, push, cleanup and shared-development deployment; those
steps are complete. Live FUB/customer-data work, activation and production-cluster
deployment remain outside this authorization.

**D-065:** the complete reviewed 010c specification, shared contracts and synthetic
implementation/check work are approved. Its follow-up authorizes commit, merge,
push and cleanup; the subsequent follow-up authorizes shared-development
deployment and next-import planning. Deployment is verified; D-066 separately
approved 010f1. Further import families, activation, real-data processing and
live-source validation retain their own scope and readiness boundaries.

**D-064:** 010c preserves distinct source People despite shared contacts, allows
explicitly approved matching stages, and keeps imported records in an admin
review workspace until later activation. D-065 subsequently accepts the complete
010c contracts after independent READY review.

**D-063:** the reviewed 010b specification, contracts, implementation and synthetic
verification are approved. Initial synthetic-development allowances are 2 GiB/run
and 4 GiB retained/Organization; current admins may explicitly raise them within
operator ceilings, with a separate resume action. These are logical payload
allowances, not production quotas or physical disk limits. **D-062** places future
email bulk content outside PostgreSQL; implementation/provider remain open.
Foundations F-01/F-02/F-03 remain proposals except where separately accepted.

## Operational entry points

| Need | Repository entry point / limitation |
|---|---|
| Start or inspect development | [README](../../README.md#development); local processes differ from Docker services; dev-bootstrap wipes data |
| Understand boundaries | [Architecture map](../architecture/ARCHITECTURE_BASELINE.md); actual state here, policy in the decision log |
| Release or recover a release | [010f1 release/evidence](../tasks/SLICE_010f1_RELEASE.md), [compatibility runbook](../tasks/SLICE_010c_RELEASE_PREPARATION.md), [release procedure](../prompts/07-deploy.md); fresh metadata-capable workload inventory is required, and private temporary backups are not a production backup system |
| Prepare real data or production | [Readiness](PRODUCTION_READINESS.md); decisions, accountable roles and required proof remain visible |
| Find a previous merge/checkpoint | [History and slice ledger](PROJECT_HISTORY.md#slice-ledger); detailed evidence in per-slice records |

## Parked / queued tracks

- **Slice 010b (FUB migration): DEPLOYED AND VERIFIED.** Live validation
  remains deferred. The approved 010b implementation passed its synthetic/full gates.
  [Current summary](SLICE_010_MIGRATION_SUMMARY.md); historical survey retained
  in [the ladder](SLICE_010_LADDER.md). New, empty Organization first and
  010a's source/credential contracts are accepted. Full inventory, mapping,
  later imports and cutover need their own specifications and approvals. 010c and
  010f1 are implemented, integrated and deployed. The next notes/tasks scope is
  implemented in [010f2](../specs/SLICE_010f2.md), with synthetic verification recorded separately from release.
  Live FUB/customer-data work and activation stay deferred.
- **Remote gates (gate-speedup phase 2): DEFERRED** pending local
  phase-1 results (now in: local gates are ~3 min — pressure is low).
  Survey recorded so it is not re-litigated: first choice GitHub
  Actions + self-hosted runner on the user's 64-core machine (origin
  github.com/murthy-karra/crm, gh authenticated; hosted default
  runners too small; GitHub larger runners need a paid Team org;
  Depot/Blacksmith-class vendors are the cheap escape hatch with no
  workflow rewrite; Buildkite the only non-Actions product seriously
  weighed). Re-verify vendor pricing at spec time.

## Live residuals and follow-ups

Carried forward; everything else previously listed here was resolved
and now lives only in git history.

**For the 011b spec (recorded 2026-08-28, not blocking):**
- M7 positive span pin (`filter_kinds` values) was skipped in 011a.
- Resolved in the 2026-09-06 filter UX pass (`0117b87`): dedicated
  Back/Forward, empty-filter URL normalization, and invalid fractional-day
  regressions. Fractions are rejected, not truncated, under amended 011a §6.
- "20 clauses accepted" ceiling is unconstructible (10 kinds ×
  one-per-kind) — record as closed/wontfix; the two reachable
  ceilings are pinned.

**Deferred live walkthroughs (user's choice, test-pinned meanwhile):**
- 011a §8 functional walkthrough completed in the 2026-09-06 filter UX
  pass before D-045's visual refresh: combined Stage + Me + Source,
  reload, Clear all, navigation, and invalid-day dismissal verified live.
- 009 walkthrough steps 3–5 (reply-all→client_replied, retroactive
  forwards, rotation) deferred 2026-08-28; one stray held row was
  left in the capture queue deliberately, for the user to dismiss as
  the dismiss-path exercise.

**Watch items:**
- Three plain-language docs of UNKNOWN authorship appeared
  mid-011a-implementation (SLICE_011_LADDER "In plain language",
  SLICE_011a.md preamble, SLICE_011a_EXPLAINED.md). Kept by user
  decision; the implementation lane denied authorship twice. Watch
  for a recurrence in the next implementation cycle.
- Always diff a subagent's reported file list against real `git
  status` (standing rule since the Slice 002 undisclosed
  PII-dump-tool incident).

**Gate/tooling residuals (gate-speedup + consolidation, 2026-08-28):**
- 4th 501-INSERT test left unbatched (db_people.rs unresolved-queue).
- Stray sqlx ephemeral test databases from killed runs — drop at
  leisure.
- The doctest step (73s) now dominates ./scripts/check — future
  micro-lever: scope `--doc`.
- lld linker experiment failed cleanly on this Xcode/clang and was
  reverted (don't retry without a new toolchain reason).
- Never overlap two db-backed test runs in one checkout (self-
  inflicted collision during 008 verification).

**Telephony (006x, all pre-existing):**
- Rotate the Telnyx SIP password; update the trunk (user action,
  still pending).
- Busy/ring-out outcomes never proven live; `placing` sweep horizon
  vs slow mic prompts; orphaned "outcome needed" calls when a caller
  is deactivated (O-004 territory). LiveKit hostname:
  `livekit1.tarams.org`.
- Known ordering issue: `db_calls_corrections::a_second_correction_chains_onto_the_first_
  with_strictly_increasing_recorded_at` failed twice in the 2026-09-12 audit,
  including in isolation, before passing in the complete rerun. The actual
  failure was timeline placement around `call_completed`; strict timestamp,
  chain and complete-row assertions passed. Host/DB clock comparison is the
  supported mechanism, not proven timestamp ties. See the completion audit.

**Known accepted edges / small gaps:**
- 007h1: a forwarder's trailing signature is part of the inner body
  in plain text (spec §5); HTML gmail_quote separation is a later
  rung.
- Pre-existing D-027/O-004 gap: `is_organization_member` lacks a
  status filter on the manual explicit-assign path (flagged in 007c
  exclusions, deliberately not fixed there).
- `set_local_password` has no test coverage; no test executes the
  `crm-admin` binary (recorded at the dev-seed rework).
- Early-slice (000/001) deferred review minors — dev-only or latent
  library-internal edge cases (cookie-parsing pins, `[::1]` bind,
  `x-request-id` trust, empty `DATABASE_URL`, migrate/seed error
  `Debug` propagation) — full text in this file's git history at
  2026-08-28; revisit only if the affected surface changes.

**Environment (standing):**
- **LiveKit is back (2026-09-09) on a new host:** `livekit1.tarams.org`
  is an EC2 `c6i.xlarge` in us-west-1 with the Slice 006 stack plus a
  dormant LiveKit Egress (D-055); `./scripts/check-telephony` passes
  against it with `CRM_TEST_LIVEKIT_API_URL=https://livekit1.tarams.org`
  (the variable is empty in `.env`, so the script refuses unless it is
  exported). The Telnyx SIP password rotation is still pending.
- Dev tunnel routing lives ONLY in the Cloudflare dashboard (D-025);
  `config.yml`'s ingress section is documentation. A fresh clone or
  recreated tunnel needs the three dashboard routes set by hand per
  README.
- The orphaned-dev-api hazard remains structural: "restart services"
  restarts only Docker; crm-api keeps running the old binary. Compare
  process start time vs binary mtime; kill by exact PID only. Bit us
  again 2026-08-29 (011a filters). Run ./scripts/db-migrate after
  checking out a branch with a new migration.
- The current shared-development release is 010f2 from `fc5a875`, deployed
  2026-09-11; see [the release record](../tasks/SLICE_010f2_RELEASE.md). Source
  validation remains deferred; registered FUB system configuration is unset.

## Backlog (deferred product tracks — full notes in the decision log)

- **O-014 email epic**: remaining products — send (O-006),
  transactional, migration reconstruction. Gmail restricted-scope
  CASA assessment is the schedule-driver — start paperwork early.
- **O-013 "Delete my data"**: Person erasure on O-012 crypto-shred;
  must be addressed before the first external customer holds real
  consumer data.

- **O-015 blob storage / retention:** the size-cap question was resolved by
  D-056 and implemented/deployed in Slice 017 (25 MiB relay threshold,
  34 MiB endpoint envelope limit); its live sends remain user-deferred.
  D-062 accepts email bulk content outside PostgreSQL, using object storage
  or dedicated storage servers; provider/layout and implementation are still
  open. Whole-message relocation and retention remain later work, coordinated
  with recordings and O-013. Raw MIME still lives encrypted in Postgres BYTEA.
- **O-008 AI next-step suggestions**: after every communication and
  daily; reminder only; no work before the communication slices.
- O-006 (outbound messaging consent) blocks the SMS slice; O-002
  (recording consent) blocks recording features.

## Latest verification

- 2026-09-11 010f2 planning: complete specification, implementation brief and
  source/code evidence received independent review. The all-held confirmation
  finding is corrected with a minimum eligible-unit check and explicit tests;
  [the review record](../tasks/SLICE_010f2_REVIEW.md) retains the disposition and
  reviewed hashes. Planning uses public documentation and local code only; no
  application tests/builds, DB operations, runtime changes or live FUB calls.

- 2026-09-11 010f1 release: implementation `f37ddd1`, merge `e36ce36`, published
  and all 975 source hashes verified. Locked backend/Web builds and additive
  migration `20260919000001` passed; the 109,990,155-byte backup catalog was
  validated without a restore. Actual-DB launch/confirmation preflight passed
  with metadata capability and a five-minute report expiry. All 43 HTTP/auth/
  asset checks, eight public browser workflows, eight inspected desktop/390px
  screenshots and tunnel 200/200/101 passed. All 45 business counts are unchanged,
  43 migration tables remain empty and three Organizations stay operational at
  revision 1. The 72-second runtime observation found zero WARN/ERROR entries;
  binary/Web/source hashes match, and sessions were revoked. Main-only cleanup
  preserved the 45 pending main docs and all recovery material. See
  [release evidence](../tasks/SLICE_010f1_RELEASE.md). No live source operation,
  customer import or activation occurred; later confirmation needs fresh evidence.

- 2026-09-11 010f1 implementation: both bounded reviews are READY. Final gates
  passed 900 Rust, 1,054 Web and 881 DB tests; 90 plans / 59 distinct SQL hashes /
  246 checks and 51 synthetic real-API/production-Web browser checkpoints passed.
  Eighteen screenshots were inspected. Exact native reconciliation, role/tenant
  fencing, retry/cancel and unchanged parent/review bindings are recorded in the
  [verification record](../tasks/SLICE_010f1_VERIFICATION.md). These synthetic
  workflows are distinct from the public empty-state release check above.

- 2026-09-11 010f1 planning: the complete specification, execution brief and code
  evidence received independent READY review after the declared-choice correction.
  [Review record](../tasks/SLICE_010f1_REVIEW.md) preserves the finding, disposition,
  reviewed hashes and then-remaining human decisions. At that planning checkpoint,
  no 010f1 implementation or application/DB/browser test had been performed;
  the later implementation and release results are recorded above.

- 2026-09-11 010c release: locked API/admin/migrator and staged Web builds passed;
  database backup catalog read, migration applied, actual-DB compatibility
  launch/confirmation preflights passed. Retired 23 old executable paths and
  observed the new runtime/workers/admin launch inventory. All 38 HTTP/auth/asset
  checks, seven public browser workflows and tunnel 200/200/101 passed. All 45
  business counts unchanged; 30 migration tables remain empty; zero runtime
  WARN/ERROR entries in bounded observation. Six screenshots visually inspected.
  [Release evidence](../tasks/SLICE_010c_RELEASE.md) distinguishes operational
  release checks from prior isolated synthetic review-hold proof. No restore,
  live source operation or activation.

- 2026-09-11 010c implementation: both bounded reviews and targeted corrections
  are complete. Final gates passed: 877 Rust, 5 doctests, 1,002 Web, 845 DB,
  14 preflight and 11 email-worker tests. All 59 plans/258 assertions and the
  single paired reader benchmark passed. Production Web/real synthetic API
  passed import/replay/provenance, role and late-response fencing, isolation,
  storage resume/cancellation and six-state/six-width browser checks. Source and
  runtime manifests, 32 screenshots and logs are in the
  [evidence summary](../design/qa/slice-010c-2026-09-11/README.md).
  Follow-up Git integration and cleanup passed: implementation `fcd2480`, merge
  `c3f6ca9`, pushed to origin/main; identical implementation/merge trees and all
  236 source entries verified. The merged worktree/branch and disposable QA
  resources are removed. The later release is verified separately above; live
  FUB validation remains deferred.

- 2026-09-11 010c planning: independent full review returned READY-WITH-FIXES;
  five corrections received targeted READY confirmation. Documentation paths,
  Markdown anchors, whitespace and documentation-only scope passed: 111 local
  paths and three anchors across eight changed Markdown files. No application/DB/browser/performance test, source call,
  commit/push, restore or runtime operation was performed. See the
  [plan review](../tasks/SLICE_010c_REVIEW.md).

- 2026-09-11 010b release: API/migrator and staged Web builds passed; database
  backup catalog read; additive migration applied. All 29 HTTP/auth/asset checks,
  tunnel 200/200/101 and public admin/member/reload/desktop/390px browser checks
  passed. All 45 business counts unchanged; 12 snapshot tables present and empty;
  zero API/Web WARN/ERROR entries during bounded observation. All 37 source hashes
  match the verified implementation and merge tree. Branch/worktree and disposable
  QA database cleaned up. No full gate rerun or live source operation. See
  [release evidence](../tasks/SLICE_010b_RELEASE.md).

- 2026-09-11 010b implementation: both bounded review/fix rounds complete;
  sequential `sqlx-prepare`, `check` (860 Rust, 5 doctests, 933 Web, 11 worker tests)
  and `check-db` (809 DB tests) passed. Synthetic real-API/Chrome flows, storage
  exhaustion/recovery, tenant/session boundaries, 25k indexed pagination and six
  final desktop/390px screenshots passed. All 37 final source hashes match.
  Temporary QA servers were stopped; shared-development 010a was not redeployed.
  See [verification](../tasks/SLICE_010b_VERIFICATION.md). Live FUB validation remains deferred.

- 2026-09-11 revised 010b plan: independent review returned READY-WITH-FIXES;
  two findings were corrected and targeted confirmation returned READY. All
  47 local document paths/anchors, whitespace and documentation-only scope
  checks passed. See the review record for dispositions. During that planning
  phase, no 010b code, tests, browser walkthrough, live FUB call or deployment was performed.

- 2026-09-11 documentation publication: staged whitespace and scope checks
  passed for 26 documentation/evidence paths; six JSON files parsed and the
  bounded credential-pattern check found no matches. Commit `a498a2c` pushed
  to origin/main successfully; local/remote-tracking revisions matched and
  the working tree was clean before 010b revisions began.

- 2026-09-11 foundations documentation: the system map, README/index, current
  state/history, foundations proposal and readiness checklist were reviewed.
  Independent review found no actionable issue. Local Markdown paths/anchors
  and whitespace checks passed; historical sections were moved mechanically
  with preservation assertions, and live residuals/backlog were retained.
  No application tests, restore exercise, source call or runtime operation was
  performed. New policies remain proposals; the existing working-tree decision
  log and 010b draft were not changed by this milestone.

- 2026-09-11 010a deployment: API/migrate and Web builds passed; private database
  backup catalog checked; additive migration applied; 21 HTTP/auth/asset checks,
  tunnel/realtime checks and public desktop/mobile/member-denial browser smoke
  passed. Business counts unchanged, no FUB connection/assessment created,
  zero API/Web WARN/ERROR entries in the bounded observation. All 25 source
  hashes match prior verification. No full test or benchmark rerun.

- 2026-09-11 source integration: all 25 final code/config hashes and 17 backend
  checkpoint hashes match the verified 010a tree; implementation/merge tree
  comparison is empty. Documentation links and Git whitespace checks passed.
  The remote main ref matched merge `cd224fe` after the source push.
- 2026-09-10 010a: `sqlx-prepare`, `check` (839 Rust, 5 doctests, 889 Web,
  11 worker tests), `check-db` (796 DB tests), query plans and synthetic
  API/browser walkthroughs passed. See [verification](../tasks/SLICE_010a_VERIFICATION.md).
  No live FUB validation or repeat full gate was performed during integration.

## Next recommended action

1. Review the completed, uncommitted [010d1 implementation and evidence](../tasks/SLICE_010d1_VERIFICATION.md)
   for an explicitly authorized integration/release step. Then plan 010d2's
   verified historical fact semantics and bounded timeline reader separately;
   010d1 completion does not approve that contract. The SQLx cancellation,
   audited call-history ordering and inherited core worker handoff concerns
   remain bounded follow-ups with their existing evidence qualifications.
2. Resume authorized FUB qualification when the user is ready, against an agreed
   dataset and applicable readiness gates. Later data families, mapping repair,
   deltas and activation need their own approved specification.
3. Use readiness C gates to prepare for first real customer data; use V gates for
   the user's later authorized FUB validation. Identify dataset type and satisfy
   applicable prerequisites before connecting. No live call is scheduled here.
4. Review F-01/F-02/F-03 proposals as their triggers approach. First production
   planning owns recovery/service targets, deployment, identity/secrets and
   worker roles. Native support windows and tenant relocation come at their
   respective capabilities; do not expand D-050 now.
5. Existing user-deferred work remains: Slice 017 live sends/walkthrough, 009
   walkthrough steps 3–5, and Telnyx SIP password rotation. See live residuals
   and the owning verification records before acting.

## Approval currently required

- D-070 approves full 010d1 source, lifetime, storage, HTTP and capability
  contracts, implementation and isolated synthetic checks. No repeat approval
  is needed for those owned changes. Live source/customer work, publication,
  deployment and activation remain separate scopes.
- D-068 approves the complete 010f2 specification, shared contracts, implementation
  and isolated synthetic checks; its follow-up authorized the now-completed
  Git integration, cleanup and shared-development deployment. Live validation,
  customer-data processing and activation remain separate scopes.
- D-066's complete 010f1 implementation and synthetic verification, followed by
  explicitly authorized Git integration, cleanup and shared-development deployment,
  are finished; no repeat release approval is needed. Notes/tasks implementation
  and release are complete under D-068; activation and live source/customer-data
  operations remain separate scopes.
- D-065 approves 010c's complete workspace/session/import/history/persistence
  contracts and implementation. No repeat specification approval is needed for
  owned implementation detail. Its authorized Git integration, publication and
  cleanup and subsequently authorized deployment are complete. Next-import
  planning is authorized; its specification/implementation, activation and
  source operations retain their own approval boundaries.
- The foundations documentation assignment is authorized; its new architecture
  and policy proposals are not automatically accepted. Their owning specs must
  identify contract changes and acceptance under AGENTS §11/§16.
- 010a cleanup/integration and shared-development deployment are complete under
  D-060. Live FUB validation remains deferred by the user. 010b contracts, implementation and synthetic verification are approved under
  D-063. Its authorized commit, merge/publication, shared-development deployment
  and cleanup are complete. Customer-data processing, live source validation and
  later import/cutover remain outside scope. No repeated release authorization is needed.
- Slice 017 live sends remain the user's deferred action, not a new approval gate.
- Recovery targets, retention/erasure policy details and support-access policy
  remain open in the readiness plan and decision log. No values were invented.
- R1 (auto-hangup of a live call on identity change) remains a later product
  choice, not a blocker for this documentation milestone.
