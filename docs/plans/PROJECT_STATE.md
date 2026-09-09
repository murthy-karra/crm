# Project State

Last updated: 2026-09-09 (Slice 015 Notes complete: merged, runtime updated,
pushed, cleaned up; no slice active; LiveKit down, see Environment).

## Current phase

**SLICE 015 (NOTES) — COMPLETE: MERGED, RUNTIME UPDATED, PUSHED, CLEANED
UP (2026-09-09, each with the user's approval).** `main` at the merge
`fd5a184` plus records; pushed to `origin/main`. `crm_dev` migrated
(`20260912000001`); the dev API and `dev-web-prod` were not running and
were relaunched from `main` (API pid 77267 on `127.0.0.1:3000`, preview
on `5173`; logs under `/private/tmp/claude-501/dev-api.log` and
`dev-web-prod.log`); tunnel verified. Branch `slice-015-notes` and the
worktree deleted; `crm_slice015_qa` dropped. The live cross-tab
`note_changed` path was observed on the dev runtime (QA record addendum).
Evidence: [SLICE_015_VERIFICATION.md](../tasks/SLICE_015_VERIFICATION.md).
Deployment is not authorized. No slice active.

Previously: **LATER BATCH (2026-09-08) — COMPLETE AND MERGED TO LOCAL MAIN** at `3ff6c5f`
(with the user's approval; not pushed, not deployed). Source
`chore/later-batch-2026-09-08` at `5fe4231` (seven commits incl. the
round-1 fixes `28a3bf1` and the
[verification record](../tasks/LATER_BATCH_2026-09-08_VERIFICATION.md)).
Final-tree gates run once by the coordinator: `sqlx-prepare` clean, `check`
green (757 Rust, 656 Web), `check-db` 622 of 622 first run. Review round 1
of two: reviewer READY WITH FIXES, tester one blocking regression (fixed),
all applied; round 2 not needed. Runtime updated with approval: `crm_dev`
migrated (`20260911000001` applied; triggers only, the API needed no
restart), the production web server rebuilt and relaunched from `main`
(preview pid 28850, 20:09), branch and worktree `../crm-worktrees/later-1`
deleted. Open flakes carried forward: the `db_calls` correction-ordering
test (strict assertion kept, unreproduced in 21 runs) and the
`db_today_system_feed_evaluation` stage-clause test (one load failure, passes
isolated). `main` was pushed to `origin/main` on 2026-09-08 at `88df7f5`
with the user's approval; deployment is not authorized.

Previously: **LATER BATCH (2026-09-08) — LANE IN IMPLEMENTATION.** The user chose the
"worth a small batch soon" group from the LATER lists: `inquiry` append-only
triggers (one migration), the `db_calls` timing flake, splitting the three
largest test files, field-only `onSuccess` writes in the optimistic
mutations, and `isMutating` guards on the settle-invalidate and the realtime
invalidation. Brief: [LATER_BATCH_2026-09-08.md](../tasks/LATER_BATCH_2026-09-08.md);
no spec (recorded LATER items; no contract or behaviour decision). One lane
(Claude Sonnet 5) in `../crm-worktrees/later-1` on
`chore/later-batch-2026-09-08`. Progress: item 2 done (`2482b4c`, the
`db_calls` flake was a test asserting strict order on `recorded_at` alone
while the query already tie-breaks on `id`; test-only fix, gates green
757 / 650 / 617 of 617). Item 1 hit its checkpoint: a plain append-only
trigger blocks the `person` → `inquiry` cascade that D-015 §5 erasure relies
on; coordinator decision: a cascade-aware `reject_direct_mutation()`
(updates and direct deletes rejected, cascaded deletes allowed via
`pg_trigger_depth()`), recorded in the brief. **All five items complete**
(`c18d7e6`/`1b488a9` item 1 with five tests; `b4a4a04` item 3: the three
files split into nine, fixtures moved to `tests/common/`, test-name sets
identical per group and 729 = 729 overall; `cbdcbc7` items 4 and 5). The
lane caught two of its own bugs before landing: the trigger must check
`pg_trigger_depth() > 1` (a direct statement's own trigger already runs at
depth 1) and the settle guard must check `isMutating > 1` (TanStack v5 runs
`onSettled` before the success state change, so the calling mutation counts
itself). Lane final gates: `check` green (757 Rust, 653 Vitest); `check-db`
622 of 622 on the second run after one load-dependent failure in a moved
`db_today_system_feed_evaluation` test that passes 6 of 6 in isolation
(pre-existing shape, not in this batch's scope; classified in review).
Coordinator audit passed (22 files, all under tests, migrations and
`web/src`). **Review round 1 (of two) complete** on `cbdcbc7`: reviewer READY
WITH FIXES, tester one BLOCKING regression — two sibling mutations for the
same Person settling in the same tick both skip the invalidation (each sees
the other pending), so nothing refetches; fix: decide after the mutation's
own state flips (deferred check, invalidate when the count is zero, and
release the realtime hold by refetching active stale queries under the
Organization prefix from all four mutations). Also: the item 2 "fix" was
reverted to the spec-backed strict assertion (the flake was never reproduced
and remains open); `TRUNCATE inquiry` gains a test; the cascade guard also
requires the parent Person to be gone; a duplicated fixture removed.
Consolidated fix round dispatched. LATER: an in-trigger `DELETE FROM
inquiry` would pass the depth check (none exists; the header forbids one);
same-field rapid pairs show the earlier response until the later settles;
the hold is Organization-blind (one org active at a time);
`useLogContactMutation` still writes the whole `person` and has no
mutation key; the `db_today_system_feed_evaluation` load flake (moved test,
unchanged body, not reproduced in 5 isolated runs plus the trio).

Previously: **Slice 014 — COMPLETE AND MERGED TO LOCAL MAIN** at `ac270fb` (2026-09-08,
with the user's approval; not pushed, not deployed). Source
`slice-014-perceived-latency` at `3495f71` (six commits incl. the round-1
fixes `fa9bcab`, the walkthrough archive and the
[verification record](../tasks/SLICE_014_VERIFICATION.md)). Final gate run
once by the coordinator: `check` green (757 Rust, 650 Web); no `check-db`
needed (Web-only). Review round 1 of two: reviewer READY WITH FIXES, tester
no blocking finding, all fixes applied; round 2 not needed. Measured over
the tunnel in production mode: cold login DOMContentLoaded 0.22–0.58 s (was
1.3–3.1 s), warm Today data 0.17–1.36 s (was 2.2–2.5 s), 3 scripts before
DOMContentLoaded (was 50–60); walkthrough 9 of 9. **The tunnel now serves the
merged production build from the main checkout** (`./scripts/dev-web-prod`,
preview pid 62908, started 17:36); the slice-tree preview was stopped by
exact PID; the branch and worktree `../crm-worktrees/014` were deleted with
approval. Standing note: after a merge or pull, re-run
`./scripts/dev-web-prod` and reload; `./scripts/dev-web` is for HMR on
loopback when 5173 is free. `main` was pushed to `origin/main` on 2026-09-08
at `57dbde1` with the user's approval (carrying Slice 014 and its records);
deployment is not authorized.

Previously: **Slice 014 — APPROVED 2026-09-08, LANE IN IMPLEMENTATION (Web-only).**
[SLICE_014.md](../specs/SLICE_014.md) and its
[brief](../tasks/SLICE_014_IMPL.md) were drafted from the planner's analysis
(which corrected the investigation on one point: through the tunnel the
browser calls `api.tarams.org` directly, so production serving needs only
Vite's preview server on 5173 and no tunnel change; and found three of the
four 2026-08-29 FilterBar gaps already fixed by the 2026-09-06 pass),
independently reviewed READY WITH CORRECTIONS (the request-count gate
restated as before-DOMContentLoaded so the Today preload does not defeat it;
the caching claim corrected to what `vite preview` actually emits; tag
mutations write `data.tags` not `data.person`; a held-response Vitest for
the single-request claim; a shared `preloadTodayView` export), all applied,
and approved with both workflow confirmations: the tunnel is normally served
from the production bundle (`scripts/dev-web-prod`; dev mode stays for
loopback), and the running dev server (`pnpm run dev` pid 24542, Vite pid
24563, up since 2026-09-06) is stopped by exact PID at verification so the
production server can take port 5173. Four parts in order: A production
serving, B optimistic stage/assignment/tag mutations, C Today chunk preload
and data prefetch plus hover prefetch of Person detail, D FilterBar residue.
One lane (Claude Sonnet 5) in `../crm-worktrees/014` on
`slice-014-perceived-latency`; checkpoint after part A. Coordinator runs the
tunnel probe and walkthrough at the end.

Progress (2026-09-08): **part A complete** (`5301b5b`): `preview: { port }`
in `vite.config.ts` (Vite 8.2.1's preview resolver verified in source to
inherit host, strictPort, allowedHosts and proxy from `server`),
`scripts/dev-web-prod`, README and `.env.example` notes. Scratch-port
measurement of the production bundle: **3 script requests before
DOMContentLoaded** (was 50–60), DCL 50 ms on loopback, the login page renders
and `/api/me` reaches the API through the preview proxy. Web gate green (614
Vitest). Coordinator audit passed; parts B–D released. **Parts B–D complete**
(`1401008` optimistic stage/assignment/tag mutations with snapshot rollback
and settle-invalidate; `3e4fd5d` `preload.ts` shared by the router and
LoginView, `prefetchTodayData` from the guard, `onRowIntent` hover/focus
prefetch with a 150 ms dwell; `9f1ce13` FilterBar selected triggers, chip
chevron, Clear all only with a non-locked clause). Two real problems found
and fixed by the lane: a cached stage object leaking `position` into the
optimistic row, and the new prefetch reaching the live dev API from
`router.test.ts` until mocked. Web gate green, 643 Vitest. Coordinator audit
passed (15 files, all under `web/`). **Review round 1 (of two) complete** on
`9f1ce13`: reviewer READY WITH FIXES, tester no blocking finding; prefix
isolation, clean reference shapes, guard placement, the shared Today chunk
and the network-leak fix all verified. Consolidated fix round dispatched:
tag mutations invalidate the person key on any error; a transparent border
on the selected FilterBar trigger; one shared `fetchPerson`; test hardenings
(a vacuous other-row assertion, the assignment row write and rollback, a
real racing-invalidation test with a held stale GET, server tag order on
success, the 149/150 ms boundary and unmount, client mocks in the new test
files). LATER: field-only `onSuccess` writes for rapid mutation pairs,
insertion-order tag sorting vs collation, pointerleave timer clearing, the
unrelated-person invalidation transient. **Fix round complete** (`fa9bcab`:
all nine items; the stricter integration test caught a spurious extra
`GET /me` from a missing test-client `staleTime`; the racing-invalidation
test now holds a stale GET open past the mutation and passes
deterministically). Coordinator audit passed (19 files against `main`, all
under `web/` plus the four part-A files); **final gate run once on
`fa9bcab`: `check` green, 757 Rust / 650 Vitest.** The pre-approved tunnel
switch was performed on 2026-09-08 (pre-approved): the dev server (pids
24542/24563, up since 2026-09-06) stopped by exact PID; `scripts/dev-web-prod`
from the slice tree serves the production build on 5173 (preview pid 55779);
through the tunnel `/` answers `cache-control: no-cache` (`cf-cache-status:
DYNAMIC`) and hashed assets `max-age=14400`. **Probe re-run in production
mode (3 runs):** cold `/login` DCL 0.22–0.58 s (one jittery run 2.0 s) from
1.3–3.1 s; warm Today data 0.17–1.36 s from 2.2–2.5 s; login → Today
0.97–1.6 s of which the login POST is 0.76–1.1 s; filter change still
flash-free; 3 scripts before DOMContentLoaded (gate ≤ 10). **Walkthrough
9 of 9** in production mode over the tunnel (hover prefetch removes the
preview's Loading; optimistic stage/assignee hold through a 1.5 s delayed
response; tag apply/remove; locked chips; selected trigger "Stage · 2",
chevron, Clear all; popover 7 px under its chip when wrapped); archive
`docs/design/qa/slice-014-2026-09-08/`; the lane stalled on the walkthrough
script twice and the coordinator wrote and ran it. Verification record on
the branch: `docs/tasks/SLICE_014_VERIFICATION.md`. Next: the merge gate. **Standing note:** the tunnel is now served from the
production bundle; after a merge or pull, re-run `./scripts/dev-web-prod`
(from the main checkout once Slice 014 merges) and reload; use
`./scripts/dev-web` for HMR on loopback only when 5173 is free.

Previously: **PLANNING Slice 014 — perceived-latency chunk plus FilterBar UX polish
(Web-only).** The user chose the coordinator's suggestions 1 and 2 on
2026-09-08: `main` was pushed to `origin/main` at `32b36de` (carrying Slices
012 and 013, D-052 and the state records), and planning started for the held
perceived-latency chunk together with lane C (the four FilterBar UX gaps
recorded since 011a), so the FilterBar is touched once. Planner analysis
dispatched; a spec, one review round and one implementation gate follow.
Push and deployment beyond this are not authorized.

Previously: **Slices 012 and 013 — BOTH COMPLETE AND MERGED TO LOCAL MAIN.** Slice 013 merged at `9af47c1` (2026-09-08, with the user's
approval; not pushed, not deployed) from `slice-013-operator-filter` at
`151d38a` (ten commits after the rebase, incl. the
[verification record](../tasks/SLICE_013_VERIFICATION.md)). Final-tree gates
run once by the coordinator on the rebased tree: `sqlx-prepare` clean (no
new statements), `check` green (757 Rust, 614 Web), `check-db` 617 of 617
first run. Review round 1 of two: reviewer READY WITH FIXES, tester no
blocking finding, all fixes applied; round 2 not needed. No migration; the
old dev API (pid 12421) stopped by exact PID, the merged binary built and
`./scripts/dev-api` relaunched (pid 17478, binary of 13:25; health 200, the
Operator route answers 401 unauthenticated). Branch and worktree
`../crm-worktrees/013` deleted with approval. The Operator now has eight
tools. Lane C (FilterBar UX polish) remains held for the perceived-latency
chunk. `main` pushed 2026-09-08 (`32b36de`); deployment is not authorized.

**Slice 012 — COMPLETE AND MERGED TO LOCAL MAIN** at `26ddab7` (2026-09-08,
with the user's approval; not pushed, not deployed). Source
`slice-012-activity-columns` at `e32ffd7` (six commits incl. the round-1
fixes `77a51c8` and the [verification record](../tasks/SLICE_012_VERIFICATION.md)).
Final-tree gates run once by the coordinator under the shared lock:
`sqlx-prepare` clean, `check` green (718 Rust, 614 Web), `check-db` 603 of 603
first run. Review round 1 of two: reviewer READY WITH FIXES, tester no
blocking finding, all fixes applied; round 2 not needed. The shared
development runtime was updated with approval: `crm_dev` migrated
(`20260910000001` applied), the old API (pid 16112) stopped by exact PID, the
merged binary built and `./scripts/dev-api` relaunched. The branch and the
worktree `../crm-worktrees/012` were deleted. **Slice 013** rebased cleanly
onto `26ddab7` (`3d68a7c`; the three `tests/all.rs` registrations merged
without conflict); its round-1 fixes (`fe3321b`, `3d68a7c`: trait defaults
removed, alias dedup, empty-item drop, context-mismatch guard, six test
hardenings; lane gates 756 / 601 of 601) passed the coordinator audit; the
coordinator's once-only final-tree gates are running on the rebased tree.

Previously: **Slices 012 and 013 — APPROVED 2026-09-08, TWO PARALLEL LANES IN
IMPLEMENTATION.** After the ladder closed, the user chose to run the
"worth a small chunk soon" items in parallel worktrees ("Go"). Planner
analyses, then [SLICE_012.md](../specs/SLICE_012.md) (denormalized
last-activity columns on `person`; trigger-maintained per **D-052**; the
fourteen statements switch to the columns behind a frozen-text equivalence
gate; one migration; size M) and [SLICE_013.md](../specs/SLICE_013.md)
(Operator `filter_people` and `run_saved_list`, name-based, read-only,
D-046-faithful; size S), each with a brief, were drafted and independently
reviewed the same day: both READY WITH CORRECTIONS, all applied (012: create
the triggers before the backfill so no deploy-window row is lost; one index
not three; probe-gate placement; a testable backfill block. 013: a missed
closed code set in the count scheduler analogue, `get_today` drawer side
effect of `MAX_REFERENCES` 25, `validate_references` cut as redundant).
Lane A (012) in `../crm-worktrees/012` on `slice-012-activity-columns`; lane
B (013) in `../crm-worktrees/013` on `slice-013-operator-filter`; both from
`main` after the planning commit; one Claude Sonnet 5 writer each; Claude
Fable 5.1 coordinates. Ownership per the specs' §10; only `tests/all.rs` is
shared (alphabetical insertion, "keep both"). Merge order A then B. Lane C
(FilterBar UX polish) stays held for the perceived-latency chunk. Gate runs
across the two lanes are serialized by a directory lock because
`sqlx-prepare`/`check-db` use fixed throwaway database names.

Progress (2026-09-08): **lane A steps 1–3 complete** (`a503640`: migration in
the reviewed order, three triggers, marked backfill block, one NULLS FIRST
index, 11 invariant tests incl. real two-transaction concurrency, 3 schema
tests; `805699b`: the fourteen pre-switch statements frozen as a Rust module
under `tests/fixtures/statements_b45b04f/` and `db_statement_equivalence.rs`
green against the live text; lane gates `check` 717, `check-db` 602 of 602;
coordinator audit passed; one accepted implementation detail: `pub`
visibility on the Today source/feed statement functions so the equivalence
test can call them). Steps 4–5 (the read-side switch behind the equivalence
test, then performance evidence) released. **Lane B step 1 complete**
(`ab1657f`: input types, two trait methods with bridging defaults to be
removed in step 3, `FilterOutcome` views, both tool schemas and parsing with
`limit` defaulting to 10, regenerated snapshot, `kind_label` mirror test in
crm-api, fake backends; `check` 734 Rust / 614 Vitest; coordinator audit
passed). Steps 2–6 released with decisions: `RefBucket::Search` for both
tools; `validate()` failures after resolution are `invalid_arguments`, unknown
names are clarifications; names clipped and control-stripped at parse time.
**Lane A steps 4–5 complete** (`9f02ead` the read-side switch of all fourteen
statements with the equivalence test green throughout and the §8.6 pins
added; `df72926` the performance archive
`docs/design/perf/slice-012-2026-09-08/` with the harness committed behind
the `perf-harness` feature). Lane gates `check` 717, `check-db` 603 of 603
(three `db_calls` timing flakes under load, clean on re-run). Measured: seed
of 25k People with triggers 2.1 s; backfill 323 ms; paired regression all
seven rows within gate with payloads equal, `person_state` 353 → 22 ms,
`source_candidates` 46 → 2 ms, the `never` filter 22 → 5 ms; the gated
`waiting` probe runs once per gated candidate (20,333) with the index
descended 5,293 times. One fixture fix in `db_operator.rs` (a direct
`UPDATE inquiry SET received_at` backdate now keeps the column in step, the
same action the erasure runbook would take). Disclosed, not touched: an
011e-era `perf-harness` test binds 25 parameters to a 27-parameter
statement, broken at the branch point. Coordinator audit passed; review
round 1 (reviewer and tester) launched on `df72926`. **Lane B steps 2–6
complete** (`b0511e3` pure name resolver; `2a8279a` the adapter with the
request's `AuthContext` threaded into the backend constructor, bridging trait
defaults removed; `b815e04` dispatch, ledger names, `MAX_REFERENCES` 25,
declared span fields, prompt; `163b876` thirteen database tests;
`7da1b21` a real bug check-db caught: duplicate-named saved lists resolved
to the first match instead of a clarification, plus two fixture fixes).
Lane gates `sqlx-prepare` no new entries, `check` 751 Rust / 614 Vitest,
`check-db` 600 of 600. Two test files outside the boundary
(`db_today_source_operator.rs`, `db_today_source_settings.rs`) received
mechanical placeholder `AuthContext` fixtures because the constructor
signature changed; accepted. Coordinator audit passed; review round 1
(reviewer and tester) launched on `7da1b21`. **Review round 1 results:**
Slice 012 reviewer READY WITH FIXES and tester no blocking finding (byte-identity
of all fourteen switched statements verified against the base commit; fixes:
exercise the `last_inquiry`/`last_inbound` axes and the new `$5/$27` source
guard in the equivalence test, a cross-Organization backfill correlation
case, a blocking assertion in the concurrency test, a migration-order pin,
`ELSIF` in the correspondence trigger, an exact `db_operator.rs` fix-up, and
gate-2 plans re-captured under `plan_cache_mode = force_generic_plan`);
Slice 013 reviewer READY WITH FIXES (one required fix: the bridging trait
default bodies the lane reported removed are still present; coordinator
confirmed on the tree) with D-046, §5.2, telemetry and injection containment
verified; Slice 013 tester no blocking finding (fixes: deduplicate aliases
that resolve to one id instead of a strike — a revised coordinator decision;
drop empty name items; a fail-closed context-mismatch guard in
`run_saved_list`; cross-Organization same-name test with People on both
sides; admin-by-`list_id` test; a service unit for the counter reset; exact
span-field assertions incl. absent uuids; same-display-name members test).
Both fix rounds dispatched (lane A for 012, lane B for 013).
LATER recorded: `inquiry` lacks a `reject_mutation` trigger (migrator can
update it; `crm_app` cannot); every history insert now takes the Person row
lock (BEYOND_ENVELOPE; a future importer must insert in stable Person order);
the 011e-era perf-harness "25 vs 27 parameters" claim could not be reproduced
by the tester and awaits the lane's exact error or retraction.

Previous phase: **Slice 011e (tags) — COMPLETE. RUNG e2 MERGED TO LOCAL MAIN** at `b6dc49b`
(2026-09-07, with the user's approval; not pushed, not deployed). Source
`slice-011e-tag-clauses` at `1796e85` (seven commits: vocabulary `db2be0a`,
fourteen statements `2cecee3`, `invalid_tag` `23340d1`, Web `147ef64`,
performance `ad6f10a`, round-1 fixes `3d33ff7`, verification record
`1796e85`). Final-tree gates run once by the coordinator: `sqlx-prepare`
clean, `check` green (717 Rust, 614 Web tests), `check-db` 587 of 587 first
run. Review round 1 of two: reviewer READY WITH FIXES, tester no blocking
finding, all fixes applied; round 2 not needed. Performance (D-050): paired
regression unchanged with the clause absent; plan shape an index-only scan
on `person_tag_org_tag_person_idx` after the §4 binding amendment. Evidence
in the [verification record](../tasks/SLICE_011e_VERIFICATION.md) and
`docs/design/perf/slice-011e-2026-09-08/`. The branch and the worktree
`../crm-worktrees/011e-e2` were deleted with the user's approval; no 011e
branch remains and none existed on the remote. The shared development runtime
was updated the same evening (no migration in e2): the old API (pid 37778)
stopped by exact PID, the merged binary built and `./scripts/dev-api`
relaunched (pid 16112, binary of 22:06; health 200, the new filter kind
reaches authentication). **This completes the Slice 011 ladder** (011a, 011b,
011b-sort, 011c, 011d, 011e). `main` was pushed to `origin/main` on 2026-09-07 at
`aef151b` with the user's approval (the push carried 011e e1/e2, D-051, the
perceived-latency investigation and the state records); deployment is not
authorized.

Implementation history: the user passed the e2 Phase 6 gate on 2026-09-07;
one lane (Claude Sonnet 5) followed brief steps 1–5 with a coordinator audit
after step 2 and after step 5; Claude Fable 5.1 coordinated, took the
predicate-binding decision in review round 1, and amended spec §4/§8/§9.14
by pointer.

Previous rung: **RUNG e1 COMPLETE AND MERGED TO LOCAL MAIN** at
`51331e9` (2026-09-07, with the user's approval; not pushed, not deployed).
Source `slice-011e-tags` at `4af2e13` (five commits: backend `502f418`, Web
`e7530d2`, walkthrough `a563cce`, round-1 fixes `8d7c748`, verification
record `4af2e13`). The branch and the worktree `../crm-worktrees/011e-e1`
were deleted on 2026-09-07 with the user's approval; no 011e branch remains
locally and none ever existed on the remote. Final-tree gates run
once by the coordinator: `sqlx-prepare` clean, `check` green (704 Rust, 598
Web tests), `check-db` 566 of 566 first run. Review round 1 of two: reviewer
READY WITH FIXES, tester no blocking finding, all fixes applied; round 2 not
needed. Walkthrough 11 of 11. Full evidence in the
[verification record](../tasks/SLICE_011e_VERIFICATION.md). The shared
development runtime was updated the same evening with the user's approval:
`crm_dev` migrated (`20260909000001` applied), the old API (pid 89707,
binary of 15:51) stopped by exact PID, the merged binary built and
`./scripts/dev-api` relaunched. **Next rung: e2** (the `tags`/`not_tags`
clauses across the fourteen statements) per the brief; its Phase 6 gate is
the next approval. Specification approved and committed as `97b889f`.

Implementation history: the user passed the e1 Phase 6 gate on 2026-09-07
("proceed in a worktree"); one lane, one writer (Claude Sonnet 5, `implement`
profile) followed brief steps 1–6 with a coordinator audit after the backend
half and after each later commit; Claude Fable 5.1 coordinated.

Previous phase: **Slice 011d (tweakable built-in Today rules) — COMPLETE AND MERGED TO
LOCAL MAIN** at `b8b53e2` (2026-09-07, with the user's approval; not pushed,
not deployed). The two lane branches, all three worktrees and, on 2026-09-07 at the
user's request, the merged integration branch
`slice-011d-today-system-feeds` (was `77a8963`) were deleted; no 011d
branch remains locally or on the remote. Verification summary: 144 files against the previous
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

No slice is active. Last completed: the LATER batch
(`docs/tasks/LATER_BATCH_2026-09-08.md`, verification
`docs/tasks/LATER_BATCH_2026-09-08_VERIFICATION.md`, merged 2026-09-08),
Slice 014 (`docs/specs/SLICE_014.md`,
verification `docs/tasks/SLICE_014_VERIFICATION.md`, merged 2026-09-08),
Slices 012 (`docs/specs/SLICE_012.md`,
verification `docs/tasks/SLICE_012_VERIFICATION.md`) and 013
(`docs/specs/SLICE_013.md`, verification `docs/tasks/SLICE_013_VERIFICATION.md`),
both merged 2026-09-08. Previous: Slice 011e — Tags —
`docs/specs/SLICE_011e.md` (approved 2026-09-07),
companion `docs/specs/SLICE_011e_EXPLAINED.md`, brief
`docs/tasks/SLICE_011e_IMPL.md`, verification record
`docs/tasks/SLICE_011e_VERIFICATION.md`. Both rungs merged: e1 `51331e9`,
e2 `b6dc49b`. No slice is active; the next slice needs the user's request. Ladder:
docs/plans/SLICE_011_LADDER.md (011a → 011b → 011b-sort → 011c → 011d all
done → **011e**, the last rung).

Previous: Slice 011d — Tweakable built-in Today rules — `docs/specs/SLICE_011d.md`
(approved and delivered 2026-09-07; verification record
`docs/tasks/SLICE_011d_VERIFICATION.md`), companion `docs/specs/SLICE_011d_EXPLAINED.md`,
brief `docs/tasks/SLICE_011d_IMPL.md`.

## Current branch

`main` pushed to `origin/main` on 2026-09-07 (the push carried 011d's
`b8b53e2` and the state commits). No worktrees. The shared development
runtime was updated the same day with the user's approval: `crm_dev`
migrated (`20260908000001` applied), the old API (pid 54346, started
2026-09-06) stopped by exact PID, and `./scripts/dev-api` relaunched; the
new binary answers the 011d routes (401 unauthenticated, not 404). Earlier: `main`
at `929b6ab`, pushed to `origin/main` on 2026-09-06 (the push carried
011b's `2af023c`/`9d62e86` and 011c's `6117b4a`/`b4c4226`/`929b6ab`). The
011b and 011c slice branches were deleted locally after the merge; they never
existed on the remote. Deployment was not authorized. The shared
development runtime still runs the pre-011b binary and `crm_dev` lacks both
new migrations; restarting it needs `./scripts/db-migrate` first.

## Last accepted decision

D-052 (2026-09-08) — trigger-maintained derived columns are a read-model
mechanism; the history insert remains the only business mutation.
D-051 (2026-09-07, `97b889f`) — tag rename and delete by Organization admins,
plus the creator while the tag is unused; hard delete with `invalid_tag`
through the existing stale-reference paths. Any member creates and applies.
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
| 011d | Tweakable built-in Today rules: system feeds, three derived clauses, admin surface, change fact (D-049, D-050) | `b8b53e2` (integration `77a8963`), pushed 2026-09-07 |
| 011e-e1 | Tags model, commands, six routes, Person page and Tags page, Operator field (D-051) | `51331e9` (branch head `4af2e13`), local only |
| 011e-e2 | `tags`/`not_tags` clauses across the fourteen statements, `invalid_tag` paths, FilterBar chips, performance evidence | `b6dc49b` (branch head `1796e85`), pushed 2026-09-07 |
| 012 | Denormalized last-activity columns on Person, trigger-maintained (D-052); fourteen statements read the columns; equivalence gate; perf archive | `26ddab7` (branch head `e32ffd7`), local only |
| 013 | Operator `filter_people` and `run_saved_list` read-only tools, name-based, D-046-faithful, `MAX_REFERENCES` 25 | `9af47c1` (branch head `151d38a`), pushed 2026-09-08 |
| 014 | Production bundle through the tunnel (`dev-web-prod`), optimistic stage/assignment/tag mutations, Today chunk preload and data prefetch, hover prefetch of Person detail, FilterBar residue | `ac270fb` (branch head `3495f71`), pushed 2026-09-08 |
| — | LATER batch: `inquiry` append-only triggers (cascade-aware), three largest test files split into nine, field-only success writes and `isMutating` guards on the Person mutations | `3ff6c5f` (branch head `5fe4231`), pushed 2026-09-08 |
| 015 | Notes: `note` table (tombstone delete, import-ready), three commands and routes, `note` timeline kind, `note_changed`, Operator `PersonDetail.notes` (untrusted, history filtered), Person page composer with inline edit/delete (D-053) | `fd5a184` (branch head `00e2e67`), pushed 2026-09-09 |
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
implementation).** *Re-measured over the public tunnel on 2026-09-07 at the
user's request, in a real browser: see
[perceived-latency-2026-09-07.md](../design/perceived-latency-2026-09-07.md).
Headline: the per-request edge floor is now 90–250 ms (two edge hops, jitter;
not app code) and POST bodies pay 300–700 ms more; dev-mode Vite ships 50–60
module requests per load and a 9-module route chunk before People's first
data request (rows at 0.4–0.8 s); login → Today data 1.4–1.8 s over four
sequential stages; the filter path is already flash-free
(`keepPreviousData` landed). Ranked levers: production build through the
tunnel first, optimistic mutations second, chunk preload and detail
hover-prefetch third. The earlier notes below stand.* Scale baselines measured 2026-08-29 against a
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
- **LiveKit is DOWN (recorded 2026-09-08).** `livekit1.tarams.org` is
  offline; the user is bringing it back during the week of 2026-09-08.
  Until the user confirms it is back: place no live calls, run no
  telephony walkthroughs, and do not treat call failures as app defects.
  Call unit/DB tests are unaffected. Remove this line on restoration.
- Dev tunnel routing lives ONLY in the Cloudflare dashboard (D-025);
  `config.yml`'s ingress section is documentation. A fresh clone or
  recreated tunnel needs the three dashboard routes set by hand per
  README.
- The orphaned-dev-api hazard remains structural: "restart services"
  restarts only Docker; crm-api keeps running the old binary. Compare
  process start time vs binary mtime; kill by exact PID only. Bit us
  again 2026-08-29 (011a filters). Run ./scripts/db-migrate after
  checking out a branch with a new migration.
- dev-api and dev-web-prod currently run the post-015 build (relaunched
  2026-09-09 by the coordinator, detached with nohup; logs under
  `/private/tmp/claude-501/`).

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

1. **Slice 015 (Notes) is merged**; after the runtime update, observe the live cross-tab `note_changed` path on the
   shared dev runtime (the one walkthrough item the QA environment could
   not exercise). After 015: Tasks (the last CRM-core model),
   then the O-015 attachment-cap raise, then the small LATER batch.
2. **(Former text, kept for the candidate list.)** Slices 012–014 and the
   LATER batch done. Candidates for the next
   request, unordered: push `main`; the remaining LATER items (accessibility,
   test hardenings, performance levers, design residue; see the verification
   records) (person-state 503 test,
   equivalence pins, feeds page first-load error test, the `db_calls` timing
   flake, splitting the three largest test files, popover
   `aria-activedescendant`, `inquiry` append-only triggers, the exact-500
   boundary test, the invalid-source error detail); notes and tasks models
   (thesis core scope; the path back into the parked FUB migration, whose
   010f tags-import portion now has its destination model and whose
   importer must insert history in stable Person order per D-052).
   (Former text: e2 in progress, one lane,
   `implement` profile, backend then Web). The small LATER items from the 011d
   verification record can still be batched before or between the rungs:
   the person-state 503 test, two equivalence pins, the feeds page
   first-load error test, the `db_calls` timing flake (pre-existing, fails
   on `main` 1 in 3), and splitting the three largest test files.
2. Deployment remains a separate action needing approval; the shared
   development runtime and the remote are current as of 2026-09-07.
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

- None for Slice 015: merged, runtime updated, pushed and cleaned up on
  2026-09-09. Deployment is not authorized. The next slice or chunk needs
  the user's request.
- None for 011e: both rungs merged, the dev runtime updated and the branches
  cleaned up with approval on 2026-09-07. `main` pushed on 2026-09-07; deployment is not authorized. The next slice or chunk
  (candidates below) needs the user's request.
- None for 011d: merged, pushed, and the dev runtime updated with approval
  on 2026-09-07. Deployment is not authorized.
- All three 011c decisions were taken on 2026-09-06 (late evening): the §8
  planner amendment is approved, the Phase B pairing limitation accepted, and
  the local commit and merge performed, then the push. **Deployment is not
  authorized.** Updating the shared development runtime is a separate action.
- R1 (auto-hangup of a live call on identity change) is a product choice for
  a later slice, not blocking.
