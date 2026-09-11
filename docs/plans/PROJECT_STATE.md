# Project state

Last updated: 2026-09-11 (010b implementation authorized under D-063).
This file holds current operational status, active work and live residuals.
[PROJECT_HISTORY.md](PROJECT_HISTORY.md) preserves earlier progress, the slice
ledger and historical measurements; its old instructions are not current work.

## Current state

**010a is deployed in shared development; 010b implementation is authorized and starting.**
The latest release record identifies source `0735015` (implementation `e4e0658`,
merge `cd224fe`) at [app.tarams.org](https://app.tarams.org/manage/migration).
It records migration `20260916000001` on `crm_dev`, API PID 9429 and Web PID 9455.
These are release-time observations, not a fresh runtime inspection in this
documentation task. Verify live process identity before any operational action.

The [release record](../tasks/SLICE_010a_RELEASE.md) and
[sanitized evidence](../design/qa/slice-010a-2026-09-11-release/README.md) cover
builds, backup catalog, migration and HTTP/browser checks. Backup restoration
was not exercised. Source hashes matched the prior synthetic verification;
full gates were not repeated for release.

No live FUB connection/assessment was created. Registered system configuration
remains unset in the last release evidence, so upstream reads fail closed.
Live authorized validation remains user-deferred. Source authorization does not
replace customer-data prerequisites, including D-015's erasure runbook.
No production-cluster deployment or recurring monitor is established by this work.

## Current slice

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
  Backend implementation precedes the Web flow and final synthetic verification.
  Live FUB validation and runtime deployment remain outside this assignment.

## Current branch

The approved planning baseline follows documentation commit `a498a2c` on main.
Implementation uses branch `codex/slice-010b-core-snapshot` in
`/Users/karrad/projects/crm-worktrees/010b`, with one primary writer for backend
and its sole additive migration, followed by Web. The coordinator owns current
state and verification evidence. Inspect Git for current implementation changes.
Application deployment remains source `0735015`; no 010b runtime deployment is
part of this assignment.

## Last accepted decision

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
| Release or recover a release | [010a evidence](../tasks/SLICE_010a_RELEASE.md), [release procedure](../prompts/07-deploy.md); private temporary backups are not a production backup system |
| Prepare real data or production | [Readiness](PRODUCTION_READINESS.md); decisions, accountable roles and required proof remain visible |
| Find a previous merge/checkpoint | [History and slice ledger](PROJECT_HISTORY.md#slice-ledger); detailed evidence in per-slice records |

## Parked / queued tracks

- **Slice 010b (FUB migration): IMPLEMENTATION.** 010a is deployed; live validation
  remains deferred. The reviewed 010b spec/brief are approved under D-063.
  [Current summary](SLICE_010_MIGRATION_SUMMARY.md); historical survey retained
  in [the ladder](SLICE_010_LADDER.md). New, empty Organization first and
  010a's source/credential contracts are accepted. Full inventory, mapping,
  imports and cutover need their own specifications and approvals.
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
- Known flake: `db_calls::a_second_correction_chains_onto_the_first_
  with_strictly_increasing_recorded_at` can misorder under full-suite
  load (microsecond timestamp ties in the history sort).

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
- The current shared-development release is 010a from `0735015`, deployed
  2026-09-11; see [the release record](../tasks/SLICE_010a_RELEASE.md). Source
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

- 2026-09-11 revised 010b plan: independent review returned READY-WITH-FIXES;
  two findings were corrected and targeted confirmation returned READY. All
  47 local document paths/anchors, whitespace and documentation-only scope
  checks passed. See the review record for dispositions. No 010b code, tests,
  browser walkthrough, live FUB call or deployment was performed.

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

1. Implement approved 010b in its isolated worktree: freeze concrete source/schema/
   wire details, complete backend and its checkpoint, then Web and final synthetic
   verification. No new hardware, shared queue platform or email relocation
   belongs in this slice.
2. Use readiness C gates to prepare for first real customer data; use V gates for
   the user's later authorized FUB validation. Identify dataset type and satisfy
   applicable prerequisites before connecting. No live call is scheduled here.
3. Review F-01/F-02/F-03 proposals as their triggers approach. First production
   planning owns recovery/service targets, deployment, identity/secrets and
   worker roles. Native support windows and tenant relocation come at their
   respective capabilities; do not expand D-050 now.
4. Existing user-deferred work remains: Slice 017 live sends/walkthrough, 009
   walkthrough steps 3–5, and Telnyx SIP password rotation. See live residuals
   and the owning verification records before acting.

## Approval currently required

- The foundations documentation assignment is authorized; its new architecture
  and policy proposals are not automatically accepted. Their owning specs must
  identify contract changes and acceptance under AGENTS §11/§16.
- 010a cleanup/integration and shared-development deployment are complete under
  D-060. Live FUB validation remains deferred by the user. 010b contracts, implementation and synthetic verification are approved under
  D-063. Runtime deployment, customer-data processing and later import/cutover
  remain outside scope. No repeated implementation authorization is needed.
- Slice 017 live sends remain the user's deferred action, not a new approval gate.
- Recovery targets, retention/erasure policy details and support-access policy
  remain open in the readiness plan and decision log. No values were invented.
- R1 (auto-hangup of a live call on identity change) remains a later product
  choice, not a blocker for this documentation milestone.
