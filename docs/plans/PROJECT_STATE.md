# Project State

Last updated: 2026-09-07 (Slice 011d complete and merged to local main;
not pushed).

## Current phase

**Slice 011d (tweakable built-in Today rules) — COMPLETE AND MERGED TO
LOCAL MAIN** at `b8b53e2` (2026-09-07, with the user's approval; not pushed,
not deployed). The two lane branches and all three worktrees were deleted
with that approval; the merged integration branch
`slice-011d-today-system-feeds` (`77a8963`) still exists locally and can be
deleted on request. Verification summary: 144 files against the previous
`main`. Final-tree gates run once by the coordinator:
`sqlx-prepare` clean, `check` green (699 Rust, 572 Web tests), `check-db`
541 of 541 on the second run after a pre-existing `db_calls` timing flake
that also fails on `main`. Review round 2: READY. Full evidence in the
[verification record](../tasks/SLICE_011d_VERIFICATION.md). Seven production
defects were found and fixed before merge (listed there). Implementation
history follows.
The user said "start 011d" on 2026-09-07. Integration branch
`slice-011d-today-system-feeds` from `main` at `66b44ff`; Lane B (Claude
Sonnet 5) in `../crm-worktrees/011d-lane-b` on `slice-011d-lane-b` doing brief
steps 1–3 (vocabulary, persistence, feed path behind the `Legacy | Feeds`
seam plus the equivalence suite), then stopping for the coordinator; Lane W
(Claude Sonnet 5) in `../crm-worktrees/011d-lane-w` on `slice-011d-lane-w`
doing steps 1–4 (types, chips, Today rules page, Today markers) with Vitest.
Claude Fable 5.1 coordinates.

Progress so far (2026-09-07):

- **Lane W steps 1–4 complete** on `slice-011d-lane-w` (`ab5f7b3`, `ef31a9b`):
  types and hooks mirroring spec §6, three boolean chips plus a locked-clause
  mode, the `/manage/today-feeds` page with preview/revert/typed-off
  confirmation and 409 reload, the Today Rules section and notices. Web gate
  green on the final lane tree (lint, typecheck, 560 Vitest tests, build).
  Coordinator audited the 20 changed files: all under `web/`, matching the
  report. Step 5 (browser walkthrough) waits for Lane B's routes. Two
  ten-minute agent stalls occurred; work was checkpointed and resumed.
- **Lane B steps 1–3 complete** on `slice-011d-lane-b` (`99e8d99`,
  `bd631c2`, `0280e1f`): the three clause kinds across all eleven statements
  with regenerated SQLx metadata; migration `20260908000001` (feed table,
  `today_feed_changed` fact, backfill) and org seeding; the `Legacy | Feeds`
  provider seam with `person_state.sql`, `call_membership.sql`,
  `call_only.sql`, and `system_feed_issues` on every `TodaySources` site;
  `db_today_feed_equivalence.rs` (5 tests, byte-identical `TodayList` JSON
  across a rich mixed fixture, two tenants, deactivated caller, a list
  source enabled, call feed disabled/enabled) plus every existing Today
  suite passing under `Feeds` by default. Gates on the lane tree: `check`
  (699 tests), `check-db` (471 of 471). Coordinator audited the 56 changed
  files: all under `backend/`. A machine-sleep interruption was resumed
  without loss. The `Legacy` provider stays until step 6.
- **Lane B corrections and step 4 complete** (`5ffeb2e`, `a4dc96b`,
  `c0f9bd6`): the three corrections verified by the coordinator (no issue for
  a disabled feed; per-axis parity tests in `db_people_filter.rs`; call-feed
  connection recovery with three failure-injection tests in
  `db_today_system_feed_call_failures.rs`); commands, preview, the six routes
  in `routes/today_feeds.rs`, the Operator field, telemetry, and
  `db_today_system_feed_commands.rs` (15 tests). Lane B found and fixed a
  real preview bug (read-only set before the `FOR SHARE` membership lock,
  which PostgreSQL rejects). Gates on the lane tree: `check` green,
  `check-db` 493 of 493.
- **Coordinator decision (2026-09-07):** the call feed's two statements
  bound no filter matrix, so extra clauses on that feed were ignored, which
  contradicts spec §1 rule 4. Decision: extend both call statements with the
  full matrix (spec §5 "feed C matrix params"), not restrict validation.
  Assigned to Lane B with the remaining §9 coverage gaps (deleted-stage and
  unsupported-JSON fallback evaluation tests, preview timeout 503, Operator
  parity under customized/disabled/fallback feeds) and then step 5
  performance evidence paired against `Legacy`.

- **Lane B round 3 complete** (`401c18e`, `549243a`, `145335a`, `97cdbec`):
  call feed bound to the full filter matrix; fallback, preview-timeout and
  Operator-parity coverage; step 5 evidence at
  `docs/design/perf/slice-011d-2026-09-07/` (paired Legacy vs Feeds serial p95
  204 ms vs 177 ms, payload-identical apart from `system_feed_issues`; the
  011c matrix all complete; person-state EXPLAIN a nested-loop anti join with
  index use with and without the merge-join toggle). Gates on the lane tree:
  `check` green, `check-db` 504 of 504.
- **Lanes merged** into `slice-011d-today-system-feeds` at `3496d71` via the
  third worktree `../crm-worktrees/011d-integration` (113 files, no
  conflicts).
- **D-050 applied** (committed on main as `1d951a6` by a peer session; spec
  §8 pointer `ae449ad`): step 5 gates only on the paired regression and the
  person-state plan shape, both already met; the 1/10/20 matrix and pool wait
  are trend data; the merge-join toggle question is closed as keep both
  settings; at most two review-then-fix rounds.
- **Review round 1 (of two) complete** on the merged tree. Reviewer:
  equivalence gate CONFIRMED by SQL analysis and both suites; READY WITH
  FIXES. One BLOCKING defect: `person_state.sql` projects a nullable boolean
  into a non-null decode, so a customized feed with `unassigned` plus one
  unassigned replied Person would 503 the whole Organization's Today. Plus:
  call-feed statements use `now()` instead of the bound clock; Update
  validates references before the revision check (422 before 409); no
  HTTP-level route tests; five §9.2 cases missing. Tester: the same clock and
  precedence defects, the 199/200/201 × call-only cap case, a mislabeled
  failure test, a weak revert-fact test, a vacuous telemetry test, and
  several cheap boundary/idempotency tests; Operator parity had no gap.
  Beyond-envelope items (concurrency 20, pool wait) recorded as trend only.
- **Lane B fix round assigned** with every in-envelope finding, the
  call-statement EXPLAINs D-050 asks for, and then step 6: delete the
  `Legacy` provider and freeze its SQL under `tests/fixtures/today_f51bff8/`.
- **Lane W step 5 assigned**: browser walkthrough on the merged tree in a
  scratch QA runtime (011c pattern, scratch database, the user's dev
  processes untouched). Two Web items from the tester wait for its report:
  a session-identity fence on the preview dialog, and the 409 flow's draft
  handling (coordinator choice: keep the reload but say so explicitly in the
  notice, and keep the editor open if the refetch fails).

Planning history follows. On 2026-09-06 the user asked to look at 011d. The read-only planner
analysed the rung against the code and found that the ladder's pre-declared
d1/d2 seam does not exist (the three Today arms are one statement with one
precedence rule and one cap). Offered three cuts, the user chose **one L rung
with parallel backend and web lanes** (D-049). Claude Fable 5.1 then wrote
[SLICE_011d.md](../specs/SLICE_011d.md), its
[companion](../specs/SLICE_011d_EXPLAINED.md) and the two-lane
[brief](../tasks/SLICE_011d_IMPL.md); the independent reviewer returned READY
WITH CORRECTIONS with no blocking decision, and all eight corrections were
applied (migration version, rule 7 on the call feed, the call-only sentinel
gating, the invalid-definition example, 403-before-400 precedence, `Feed.filter`
semantics under `filter_error`, a `Legacy | Feeds` provider seam for the
equivalence and paired-perf gates, `unavailable` precedence over `partial`).
The user approved the specification with its seven §1 safe defaults on
2026-09-07, held implementation briefly, then started it the same day.

Previous phase, for context:

**Slice 011c (saved lists feed Today) — COMPLETE, MERGED AND PUSHED.**
The user approved the §8 planner amendment, accepted the Phase B pairing
limitation and approved the local commit and merge on 2026-09-06 (late
evening). Both 011b and 011c were then pushed to `origin/main` at the user's
request; deployment was not authorized.

How it got here: the Codex lanes (Astra / Terra, `xhigh`) specified, built,
reviewed and browser-walked the slice, then ran out of usage on the evening of
2026-09-06 with the tree uncommitted and three items open (Phase B HTTP
performance, final-tree gates, independent acceptance review). The user asked
Claude to finish. Claude Fable 5.1 took the coordinator, sole-gate-runner and
acceptance-reviewer roles; Claude Sonnet 5 lanes made the bounded corrections;
the lane ledger is in the [implementation brief](../tasks/SLICE_011c_IMPL.md#takeover-on-2026-09-06-evening-codex-usage-exhausted)
and every result in the [verification record](../tasks/SLICE_011c_VERIFICATION.md).

What the takeover found and did:

- The tree as left passed `check-db` (422 of 422) and the Web gate but failed
  clippy on three test-file lints; a telemetry test was never registered; the
  Phase B harness did not compile. All fixed; the harness lints clean.
- Acceptance review found no blocking backend or Operator defect. The Web
  session-privacy review found a P1 availability lockout (an auth attempt that
  never settles left every tab paused for ever with no exit) plus three small
  router/copy defects; all fixed with tests (C011C-I17–I20) and the lockout
  recovery verified live. One pre-existing telephony gap is a residual (R1).
- **Phase B exposed a pre-existing Today planner hazard:** once autovacuum
  fills the visibility map, PostgreSQL 18 replans the built-in query's
  per-Person effective-contact probe into a Merge Anti Join that scans the
  whole corrections index per Person (~2.2 s instead of ~0.2 s for a 30k-Person
  book; the frozen original query shows 2.6 s). Run 1 failed on it. A
  transaction-local `SET LOCAL enable_mergejoin = off` beside the approved JIT
  setting pins the fast plan with no result change (frozen-fixture parity
  tests pass) and no effect on the source statements; run 2 with it passed
  every final-arm criterion (522/522 complete, all p95 caps met, five-source
  concurrency-20 p95 2,710 ms against 4,500, pool headroom ≥ 414 ms). It is
  recorded in [spec §8](../specs/SLICE_011c.md) as the fourth planning change,
  approved by the user after the evidence was in. Evidence, both runs retained:
  [Phase B archive](../design/perf/slice-011c-http-2026-09-06/README.md).
- Final-tree gates were run once by the coordinator after run 2 and passed
  (`sqlx-prepare`; `check` with 675 Rust and 446 Web tests; `check-db` 423 of
  423); see the verification record for the actual results.

QA setup incident (Codex phase): a bootstrap command used the shared migration
URL and rewrote the existing development owner's local credential hash and
timestamp. The previous password's equivalence is unknown and its hash cannot
be restored. Exact effects are recorded in
[the verification record](../tasks/SLICE_011c_VERIFICATION.md#shared-development-credential-incident).
Do not describe shared development data as untouched for this slice. The QA
runtime and its generated databases were cleaned up at the takeover's end.

The user approved **five Today sources per agent** and **available work with
an explicit notice when one source fails** (D-047). 011b-sort remains separately
queued; it is not a functional prerequisite for 011c. The approved specification
preserves built-in Today work, private-list visibility and deterministic order,
and addresses the measured cost of evaluating filters against a large history.

## Just completed: Slice 011b-sort

Started 2026-09-06 (late evening) at the user's request. The planner's
recommendation is reconciled into a draft specification
[SLICE_011b_SORT.md](../specs/SLICE_011b_SORT.md) and brief
[SLICE_011b_SORT_IMPL.md](../tasks/SLICE_011b_SORT_IMPL.md); independent
review returned READY-WITH-FIXES and the eight corrections are applied. The
user took the one genuine decision, **D-048**: sort is part of the list
definition, with clickable headers plus an "Added" column as the accepted
control. The user approved implementation the same evening. Branch
`slice-011b-sort` from `main` at `31c9980`: two Claude Sonnet 5 lanes built the
backend and Web halves in parallel, the coordinator passed the 100k
performance gate (sorted p95 20–132 ms against the same-run 343.6 ms
four-clause baseline; custom plans retained; no lever needed), independent
review and adversarial analysis found no P1/P2 defect and their test and
hardening items were applied by two fix lanes, and the final gates passed
once on the final tree (Rust 689 + 5 doctests, Web 518, database 459 of 459).
Full evidence: [SLICE_011b_SORT_VERIFICATION.md](../tasks/SLICE_011b_SORT_VERIFICATION.md).
Implementation commit `bd23f42`, merged to `main` as `d52a0ad` and pushed to
`origin/main` on 2026-09-06 at the user's request; the slice branch was deleted
locally and never existed on the remote. Deployment was not authorized.

## Previous completed slice

**Slice 011b (saved lists) — COMPLETE AND MERGED TO LOCAL MAIN.**
The user authorized starting the slice with **Astra / ultra** for
specification, coordination, and independent review, and **Terra / ultra**
for implementation, tests, and fixes. This supersedes the older
Sonnet/Fable assignment for this slice. The spec and implementation brief
were drafted against `1635fc4`, which includes the 011a filter UX
fixes (`0117b87`) and D-045 workspace/Person-preview changes. Independent Astra/ultra specification review is **READY** after corrections.
The user approved the slice after reading the plain-language companion;
implementation, tests and fixes are authorized.

The implementation now passes the full repository and database gates, the
synthetic browser walkthrough and independent source/performance review.
Personal/shared lists, counts, copy/edit/delete flows, privacy and recovery are
implemented in `2af023c` and merged to local `main`. The user approved this
commit and merge on 2026-09-06; no push or deployment was performed. See
[verification evidence](../tasks/SLICE_011b_VERIFICATION.md) for actual results
and limits, including the 50k-Person query plans.

Official FUB guidance was rechecked on 2026-09-06: creating lists from
People filters, explicit save/update, admin management of shared lists,
independent duplication, and deletion of only the definition all remain
supported patterns. D-046, not an inferred FUB policy, is authoritative
for personal-list privacy and the separate limits.

Decisions taken this phase (user, 2026-08-29):

- **Per-list sort stays OUT of 011b** and becomes its own small
  follow-up rung immediately after 011b (v1 restricted to non-derived
  columns). Ladder amendment recorded at 011b spec approval as **011b-sort**.
  Rationale recorded: variable ORDER BY vs the fixed-matrix
  static-SQL discipline, and sort determines WHICH 500 rows survive
  truncation on >500-match lists.

Decisions accepted at restart (user, 2026-09-06; **D-046**):

- Personal list names and criteria are creator-only, including from admins.
  Organization-wide Person visibility is unchanged.
- Shared lists ≤200/Organization plus personal lists ≤50/creator/Organization;
  no combined cap. These count definitions, not People matching a list.

Also 2026-08-29: a three-agent docs-freshness audit ran over the whole
docs tree; the spec supersession-pointer chain verified fully intact
(zero missing pointers). All findings were fixed with per-item user
approval: README rewritten to current state (feature summary, real
directory list, gate-speedup check steps, Email intake section, 4 new
env-table rows, runtime-neutral Docker wording); D-013 amended to
bless .env.example's non-credential defaults; O-005 deduped and O-005/
O-007 marked resolved, O-014 annotated with shipped status, D-023 §4
supersession note added, accepted-decisions-continue-below pointer
added; thesis §8 (D-043) and §16 (achieved) annotated; ARCHITECTURE_
BASELINE gained an "Amendments since baseline" section + the contracts/
correction; ZITADEL dev-vs-prod parentheticals added (AGENTS §3/§4.2,
baseline); status headers fixed on SLICE_007_LADDER / SLICE_011_LADDER /
SLICE_006c_PLAN / type-safety-hardening; orphan CRM_ENVIRONMENT deleted
from .env.example. Uncommitted, awaiting the commit gate.

Also this session (2026-08-29): the user's "011a filters don't work"
report was root-caused to the KNOWN orphaned-dev-api hazard — the
running crm-api predated the 011a merge and silently ignored
`?filter=`. Killed by PID, relaunched via ./scripts/dev-api, filter
path verified live (garbage filter 400s; assigned_to narrows
correctly). 011a FilterBar UX intuitiveness gaps noted for a later
polish pass: draft chips look active while filtering nothing,
detached editor panel, undiscoverable chip-click-to-edit, no
clear-all.

## Current slice

Slice 011d — Tweakable built-in Today rules — `docs/specs/SLICE_011d.md`
(approved and delivered 2026-09-07; verification record
`docs/tasks/SLICE_011d_VERIFICATION.md`), companion `docs/specs/SLICE_011d_EXPLAINED.md`,
brief `docs/tasks/SLICE_011d_IMPL.md` (Lane B backend owns the migration and
SQLx; Lane W web). Planned integration branch `slice-011d-today-system-feeds`
from `main` at `f51bff8`. Ladder: docs/plans/SLICE_011_LADDER.md (011a → 011b →
011b-sort → 011c all done → **011d** → 011e tags).

## Current branch

`main` at `b8b53e2` (the 011d merge), local only; `origin/main` is still at
`929b6ab` (the 011c push). No worktrees. The shared development runtime
still runs the pre-011d binary and `crm_dev` lacks migration
`20260908000001`; run `./scripts/db-migrate` before restarting it. Earlier: `main`
at `929b6ab`, pushed to `origin/main` on 2026-09-06 (the push carried
011b's `2af023c`/`9d62e86` and 011c's `6117b4a`/`b4c4226`/`929b6ab`). The
011b and 011c slice branches were deleted locally after the merge; they never
existed on the remote. Deployment was not authorized. The shared
development runtime still runs the pre-011b binary and `crm_dev` lacks both
new migrations; restarting it needs `./scripts/db-migrate` first.

## Last accepted decision

D-050 (2026-09-07, `1d951a6`) — operating envelope (25k People, 50 members,
5 concurrent Today loads, one active tab), two review rounds per slice, and
performance gating on paired regression plus plan shape only.
D-049 (2026-09-06) — Slice 011d ships as one L rung with parallel backend and
web lanes; a one-time exception to the S–M rung rule, not a change to it.
D-048 (2026-09-06) — a saved list's sort order is part of its definition.
D-047 (2026-09-06) — up to five Today sources per agent and explicit partial
availability. D-046 preserves creator-only personal lists and separate
shared/personal caps. D-045 governs the current white/glass Web design; D-044 establishes
the Elysium CRM identity. D-043 remains the slice-shaping product decision:
smart lists are first-class and FUB-shaped; lists feed Today; built-in
Today logic becomes org-tweakable system feeds; the filter model IS the
Today configuration language. Plus the three ladder-acceptance decisions
in SLICE_011_LADDER.md (order a→e; D-043 §3 strict; Source = latest inquiry)
and the 2026-08-29 sort-rung decision above.

## Slice ledger

All entries are complete, merged to main and pushed (011b and 011c were pushed
together on 2026-09-06).

| Slice | What | Merge |
|---|---|---|
| 000 | Foundation (workspace, health/ready, compose, scripts) | `e5182d1` |
| 001 | Identity/sessions (Argon2id, HMAC tokens, role split) | `587a087` |
| — | Tunnel + CORS (app./api.tarams.org; later D-024/D-025) | `3b6df76` |
| 002 | Intake + People/history + web stack (D-017) | 2026-08-21 |
| 003 | Realtime (Centrifugo, D-023) + Today | 2026-08-22 |
| 004 | Administration (platform admin, invitations, D-026/27) | 2026-08-22 |
| 005 | Read-only AI Operator (crm-operator, 5 tools, D-028/29) | 2026-08-22 |
| 006 | Calling (LiveKit/Telnyx) | `332e78a` |
| 006a | crm-app extraction | `a17aed3` |
| 006b | Operator start_call (propose→confirm, D-034) | `3f36d25` |
| 006c | Call outcome (D-032/D-033, low tier) | `58ecad8` |
| 007a | Intake address | `81af77f` |
| 007b | Inbound email endpoint | `4b3462a` |
| 007c | System actor + unattended routing (D-035) | `fe0b99b` |
| 007d | First pinned email format (D-036) | `a75b9a8` |
| 007e | Unresolved workbench (D-037) | 2026-08-25 |
| 007f | LLM extraction via Groq (D-038) | 2026-08-25 |
| 007g | Real receiving: Cloudflare Email Routing + worker (D-039) | `9604f76` |
| 007h1 | Forwarded-wrapper unwrap, Gmail inline (D-040) | `105f730` |
| — | Type-safety hardening ladder, 8/8 chunks (S1…S2) | `069f55a` |
| 008 | Intake routing modes / round-robin (D-041) | `defdab1` |
| 009 | Correspondence capture v1 (D-042; largest slice, 78 files) | `807d7c2` |
| 011a | Filter vocabulary + ad-hoc People filtering (D-043) | `4aee12d` |
| 011b | Personal and shared saved People lists (D-046) | `9d62e86` (implementation `2af023c`) |
| 011c | Saved lists feed Today (D-047; §8 planner amendment approved) | `929b6ab` (implementation `6117b4a`) |
| 011b-sort | Per-list sorting for saved People lists (D-048) | `d52a0ad` (implementation `bd23f42`) |
| 011d | Tweakable built-in Today rules: system feeds, three derived clauses, admin surface, change fact (D-049, D-050) | `b8b53e2` (integration `77a8963`), local only |
| — | Gate-speedup chunk (check 35m→79s, check-db 37m→~2m) | 2026-08-28 |
| — | Test-binary consolidation (40 files → 1 binary) | `6427ee8` |

Closing-state documents: docs/design/type-safety-hardening.md (ladder
closing state + residuals), docs/tasks/GATE_SPEEDUP.md (gate-speedup
resume artifact), docs/design/intake-throughput.md (intake capacity
notes).

## Parked / queued tracks

- **Slice 010 (FUB migration): PARKED** (user, 2026-08-28) — too many
  FUB entities lack destination models (notes/tasks/custom
  fields/deals). Full resume artifact: docs/plans/SLICE_010_LADDER.md.
  The three ladder-level decisions (rollback posture, scope,
  credential at-rest) were NOT taken — ask at resume. Building
  notes/tasks/deals models is itself a path back; 011e (tags)
  re-opens the 010f tags-import portion.
- **Remote gates (gate-speedup phase 2): DEFERRED** pending local
  phase-1 results (now in: local gates are ~3 min — pressure is low).
  Survey recorded so it is not re-litigated: first choice GitHub
  Actions + self-hosted runner on the user's 64-core machine (origin
  github.com/murthy-karra/crm, gh authenticated; hosted default
  runners too small; GitHub larger runners need a paid Team org;
  Depot/Blacksmith-class vendors are the cheap escape hatch with no
  workflow rewrite; Buildkite the only non-Actions product seriously
  weighed). Re-verify vendor pricing at spec time.

**QUEUED: PERCEIVED-LATENCY chunk (web, not yet approved for
implementation).** Scale baselines measured 2026-08-29 against a
"Perf Test Realty" org seeded via the live API (dev DB only; wiped by
the next dev-bootstrap), first at 5k people, then at **100k people +
66,589 contact attempts + ~5k repeat inquiries** (the mature-FUB-team
case; write path held 104–107 leads/s across the whole 15-minute
seed, no degradation). At 100k: core filters stay FLAT (people
unfiltered 23 ms, assigned_to 21 ms, source 25 ms, last_contact-
within-7d 22 ms — the fixed matrix + indexes hold); ABSENCE-proving
filters degrade (has_phone-false 43 ms; last_contact-never 234 ms;
4-clause combo with a never clause 318 ms); **Today = 966 ms admin /
590 ms member** (linear in org size × history — ~97 ms at 5k). The
ladder's "fine to ~50k" holds for filters but NOT for Today (~500 ms
at 50k extrapolated). Consequence: the recorded denormalized
last-activity-columns lever (person.last_contact_at /
last_inbound_at / last_inquiry_at maintained at write time) now has a
measured trigger and should be its own small chunk BEFORE or WITH
011c (which multiplies Today's cost); it also collapses the
never-filters to indexed column tests. Tunnel-path measurements
(2026-08-29): ~60 ms edge floor per request; browser-realistic
People ≈ 90–140 ms; payloads edge-compressed 229 KB→27 KB (origin
CompressionLayer would shrink only the Mac→edge leg). Since the
backend is flat and fast, perceived speed work is web-side:
`placeholderData: keepPreviousData` on people/filter queries (kills
the Loading… flash on every chip edit), optimistic updates on
stage/assignment mutations, hover prefetch of person detail, collapse
the me→org-queries waterfall (2 sequential RTTs over the tunnel), and
prod-build web serving for the tunnel (dev Vite ships hundreds of
unbundled modules through it). Bundles naturally with the FilterBar
UX polish items (draft-chip affordance, anchored editors, clear-all).
100-AGENT CONCURRENCY TEST (same org, 2026-08-29): STRESS (no think
time) saturates the sqlx-default 10-connection pool — every request
queues to ~2.3–2.8 s and 9% 503 via the 2 s acquire timeout, Today the
biggest consumer; REALISTIC (2–5 s think time, ~24 req/s) is healthy
at p50 (people/filters 23–60 ms) but Today p50 612 ms / p95 1.8 s and
a 1.2% 503 rate — 100 active agents in one 100k-person org is past
comfort TODAY. Root cause is capacity = pool(10) / Today(~1 s);
the denormalization chunk multiplies capacity ~40x and is the fix;
explicit pool sizing (max_connections currently sqlx default 10,
state.rs) is the cheap secondary lever. Login (Argon2id) 236 ms avg
sequential — by design, fine. THE HARNESS IS COMMITTED: ./scripts/perf
(seed | bench | agents) + docs/design/PERF_BASELINE.md (full tables,
EXPLAIN anatomy of Today, method caveats: debug build, skewed books,
Python client) — re-run bench after any query/index/pool change and
compare against the baseline doc.

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
- Dev tunnel routing lives ONLY in the Cloudflare dashboard (D-025);
  `config.yml`'s ingress section is documentation. A fresh clone or
  recreated tunnel needs the three dashboard routes set by hand per
  README.
- The orphaned-dev-api hazard remains structural: "restart services"
  restarts only Docker; crm-api keeps running the old binary. Compare
  process start time vs binary mtime; kill by exact PID only. Bit us
  again 2026-08-29 (011a filters). Run ./scripts/db-migrate after
  checking out a branch with a new migration.
- dev-api currently runs the post-011a binary (restarted 2026-08-29).

## Backlog (deferred product tracks — full notes in the decision log)

- **O-014 email epic**: remaining products — send (O-006),
  transactional, migration reconstruction. Gmail restricted-scope
  CASA assessment is the schedule-driver — start paperwork early.
- **O-013 "Delete my data"**: Person erasure on O-012 crypto-shred;
  must be addressed before the first external customer holds real
  consumer data.

- **O-015 blob size / storage / retention** (recorded 2026-08-29):
  raw MIME incl. attachments lives in Postgres BYTEA today, capped at
  1.4 MiB by the email worker — too small for real-estate disclosure
  packets (5–20 MB), so legitimate client mail with a signed PDF is
  BOUNCED. Cap raise is actionable NOW and independent of the rest.
  Settled in the entry: whole-message relocation to object storage
  (not per-attachment extraction) when recordings force that slice;
  junk stripping REJECTED (error asymmetry + content_hmac determinism
  trap); infrequent-access tier via bucket lifecycle rules, not deep
  archive. Genuinely open: the cap value, and RETENTION (legal weight,
  sequenced with O-013).
- **O-008 AI next-step suggestions**: after every communication and
  daily; reminder only; no work before the communication slices.
- O-006 (outbound messaging consent) blocks the SMS slice; O-002
  (recording consent) blocks recording features.

## Latest verification

- 2026-09-06, 011b final implementation tree on
  `codex/slice-011b-saved-lists`, base `1635fc4`: Terra ran
  `./scripts/check` (30s; 651 Rust tests, 5 doctests, 388 Web tests,
  9 email-worker tests, lint/type checking/build) and then
  `./scripts/check-db` (139s; SQLx prepare check and 379 DB tests), all passing.
  Astra source/performance review found no remaining actionable finding.
  Coordinator completed the isolated browser walkthrough and verified both
  existing People queries plus all 147 prior SQLx files unchanged. Full
  [evidence and criterion mapping](../tasks/SLICE_011b_VERIFICATION.md) includes
  the 50k dense/sparse plans and the native-confirm automation limitation.
- 2026-09-06, 011b documentation phase at `1635fc4`: independent
  Astra/ultra review READY after A1/A2 and R1–R3 corrections (typed version
  validation, membership freshness, dirty-copy preservation, uncertain
  create handling and count concurrency). Coordinator verified all nine
  relative Markdown file links in the five changed documents and
  `git diff --check`. No application/DB tests were run for this docs-only
  change; implementation and performance evidence remain future work.
- 2026-08-28, test-binary consolidation on `main`: coordinator's own
  final-tree run — check 14s warm; check-db 2:11 (363/363). Test
  reconciliation keyed on (file, test-name): 439/363 before = after,
  exact.
- 2026-08-28, 011a on `slice-011a-filter-vocabulary` before merge:
  lane gates green post-fix (check; check-db 49 blocks 0 failed;
  filter unit 65; db_people_filter 31; web 300); coordinator
  final-tree check + check-db green (own run).
- 2026-08-29, live against the running dev stack: post-011a filter
  path verified (garbage `?filter=` → 400; assigned_to filter → 4/16
  people, single assignee).

## Next recommended action

1. **Next rung when requested: 011e (tags)**, the last rung of the ladder;
   write its spec just in time per the ladder rule. Before that, the small
   LATER items from the 011d verification record can be batched: the
   person-state 503 test, two equivalence pins, the feeds page first-load
   error test, the `db_calls` timing flake (pre-existing, fails on `main`
   1 in 3), and splitting the three largest test files.
2. Updating the shared development runtime (`./scripts/db-migrate`, then
   restart the API by exact PID), pushing `main` (011d not on the remote)
   and deployment are separate actions needing approval.
   The equivalence gate (Lane B step 3) and the merge-join toggle question
   (step 5) return to the coordinator. The shared development runtime is
   already migrated and serving the merged 011c code.
2. Post-merge polish landed on main (2026-09-06): the People page explains
   list creation when reached from Lists (three steps plus a recorded
   walkthrough video in a wide dialog), and a direct load of any protected URL no longer bounces to
   Today when session verification settles mid-navigation (replay now waits
   for the initial navigation); a named list's header now wraps its toolbar
   below a long title instead of squeezing the title. `playwright-core` was
   added as a Web dev dependency only for the guide recording script.
3. Follow-ups this slice surfaced, unordered: R1 (call host not fenced on a
   session boundary, pre-existing, telephony files); the Today built-in query
   hazard deserves a durable fix in 011d or the queued denormalization chunk
   rather than relying on a planner toggle for ever; the Web reviewer's three
   uncovered criterion-10 test scenarios (fake-clock timer, real-wiring
   same-actor relogin, cancellation race); `scripts/check-db` does not
   schema-check test-target queries that `scripts/sqlx-prepare` now caches.
   011b-sort remains separately queued.
2. Updating the shared development runtime, pushing and deployment are
   separate actions; the shared API/database still predate 011b.
3. Later, unordered: 009 walkthrough steps 3–5;
   Telnyx SIP rotation; delete
   `slice-011a-filter-vocabulary` (needs approval).

## Approval currently required

- None for 011d: merged and cleaned up with approval on 2026-09-07. Push
  of `main` (carries 011d) and deployment are not authorized; deleting the
  merged `slice-011d-today-system-feeds` branch needs a word.
- All three 011c decisions were taken on 2026-09-06 (late evening): the §8
  planner amendment is approved, the Phase B pairing limitation accepted, and
  the local commit and merge performed, then the push. **Deployment is not
  authorized.** Updating the shared development runtime is a separate action.
- R1 (auto-hangup of a live call on identity change) is a product choice for
  a later slice, not blocking.
