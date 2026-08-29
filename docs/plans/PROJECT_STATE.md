# Project State

Last updated: 2026-08-29 (file restructured: stale per-slice
narratives compressed into the ledger below; every still-operational
residual preserved under "Live residuals". Full history remains in
this file's git history and in the per-slice specs.)

## Current phase

**Slice 011b (saved lists) — SPEC PHASE, in progress (2026-08-29).**
Planner ground-truth survey complete (schema/command/HTTP/web
precedents, viewer-relative `me` confirmed workable for shared lists,
stale-reference posture, count-query shape). FUB details re-verified
per D-043's spec-time instruction: FUB personal (unshared) smart
lists are creator-only; admins fully manage shared lists; a saved
sort order is part of a FUB list's definition.

Decisions taken this phase (user, 2026-08-29):

- **Per-list sort stays OUT of 011b** and becomes its own small
  follow-up rung immediately after 011b (v1 restricted to non-derived
  columns). Ladder amendment to be recorded when the 011b spec is
  approved. Rationale recorded: variable ORDER BY vs the fixed-matrix
  static-SQL discipline, and sort determines WHICH 500 rows survive
  truncation on >500-match lists.

Decisions still pending (asked, awaiting the user):

- List-cap shape. Recommended: shared ≤200/org, personal ≤50/owner,
  no combined cap (caps are counts of saved lists — engineering
  guardrails bounding the Lists-index/Today evaluation cost, not
  product limits).
- Personal-list privacy. Recommended: owner-only, admins included
  (matches FUB; private-then-open is reversible, the reverse is not).

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

Slice 011b — Saved lists — spec being drafted (docs/specs/
SLICE_011b.md does not exist yet). Ladder:
docs/plans/SLICE_011_LADDER.md (011a done → **011b** → 011c
lists-feed-Today → 011d tweakable built-ins → 011e tags; plus the
newly-decided sort rung after 011b).

## Current branch

`main` (`6427ee8`, pushed to origin). Working tree clean apart from
this planning edit. Undeleted merged branches:
`slice-011a-filter-vocabulary` (deletion not yet approved).

## Last accepted decision

D-043 (2026-08-28) — smart lists are first-class and FUB-shaped;
lists feed Today; built-in Today logic becomes org-tweakable system
feeds; the filter model IS the Today configuration language. Plus the
three ladder-acceptance decisions in SLICE_011_LADDER.md (order a→e;
D-043 §3 strict; Source = latest inquiry) and the 2026-08-29 sort-rung
decision above.

## Slice ledger (all COMPLETE, merged to main, pushed)

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

## Live residuals and follow-ups

Carried forward; everything else previously listed here was resolved
and now lives only in git history.

**For the 011b spec (recorded 2026-08-28, not blocking):**
- M7 positive span pin (`filter_kinds` values) was skipped in 011a.
- Dedicated web pins missing (indirect coverage only):
  `router.go(-1)` rehydrate; zero-clauses-URL-canonicalize;
  fractional-days-truncate.
- "20 clauses accepted" ceiling is unconstructible (10 kinds ×
  one-per-kind) — record as closed/wontfix; the two reachable
  ceilings are pinned.

**Deferred live walkthroughs (user's choice, test-pinned meanwhile):**
- 011a §8 walkthrough deferred at the commit gate (2026-08-28) —
  2026-08-29's filter verification covered the API path live; the
  browser walkthrough remains open.
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
- **O-008 AI next-step suggestions**: after every communication and
  daily; reminder only; no work before the communication slices.
- O-006 (outbound messaging consent) blocks the SMS slice; O-002
  (recording consent) blocks recording features.

## Latest verification

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

1. Finish the two pending 011b decisions (caps shape, privacy) →
   draft docs/specs/SLICE_011b.md → independent review
   (crm-reviewer) → user approval → implementation gate (Option A:
   Sonnet implements, Fable coordinates — the proven workflow).
2. At 011b spec approval, amend SLICE_011_LADDER.md to add the
   per-list-sort rung after 011b.
3. Later, unordered: 011a browser walkthrough + 009 steps 3–5;
   Telnyx SIP rotation; FilterBar UX polish pass; delete
   `slice-011a-filter-vocabulary` (needs approval).

## Approval currently required

- The two 011b spec decisions above (caps shape; personal-list
  privacy).
- Nothing else outstanding: all merged slices were committed, merged,
  and pushed with explicit user approval at their gates.
