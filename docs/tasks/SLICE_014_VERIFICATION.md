# Slice 014 — Verification record

Coordinator-owned evidence for [SLICE_014.md](../specs/SLICE_014.md) §8 under
the D-050 budget (two review rounds at most; performance reported, gated only
on structural facts).

Branch `slice-014-perceived-latency` from `main` at `731d3ce`, worktree
`../crm-worktrees/014`, one Web lane (Claude Sonnet 5), coordinated by Claude
Fable 5.1. Commits in order:

| Commit | Content | Lane gate (own tree) |
|---|---|---|
| `5301b5b` | Part A: `preview: { port }` (inheritance of host, strictPort, allowedHosts and proxy verified in Vite 8.2.1's `resolvePreviewOptions`), `scripts/dev-web-prod`, README and `.env.example` notes; scratch-port measurement: 3 script requests before DOMContentLoaded, DCL 50 ms on loopback | `check` green, 614 Vitest |
| `1401008` | Part B: `usePerson` passes `signal`; snapshot/restore/write helpers; optimistic stage, assignment, tag apply and remove with rollback and a single settle-invalidate; a cached-stage `position` leak into the optimistic row found and fixed | — |
| `3e4fd5d` | Part C: `preload.ts` shared by the router's Today record and LoginView; factored Today fetchers and `prefetchTodayData` from the guard; `DataTable.onRowIntent` and PeopleView's 150 ms dwell prefetch; the new prefetch was reaching the live dev API from `router.test.ts` under happy-dom until mocked (fixed in the same commit) | — |
| `9f1ce13` | Part D: FilterBar selected triggers with counts, chip chevron, Clear all only with a non-locked clause; `BUTTON_BASE` exported | `check` green, 643 Vitest, 757 Rust |
| `fa9bcab` | Round-1 fixes (below) | `check` green, 650 Vitest |

Coordinator file-list audit per checkpoint against `git diff --name-only`:
four files for part A (config, script, README, `.env.example`), fifteen files for parts B–D all under `web/`; no backend, `.sqlx`, migration, `sessionLifecycle.ts` or `/me` change; no new dependency.

### Criterion mapping (§8.1–8.11)

| § | Proof |
|---|---|
| 8.1 prod serving | `scripts/dev-web-prod` builds and serves on 5173; `https://app.tarams.org` loads and logs in; loopback proxy parity measured by the lane on a scratch port; headers recorded below |
| 8.2 probe | tunnel measurement below: 3 scripts before DOMContentLoaded (gate ≤ 10), the rest reported |
| 8.3 stage/assignment optimistic | `queries.test.ts`: optimistic value in the detail and in a filtered and an unfiltered People cache entry before resolution, only the matching row changed and a now-non-matching filtered row kept; every snapshot restored on rejection with the existing error text; success writes `data.person` and invalidates once; empty stages/members cache → no optimistic write, mutation still succeeds and invalidates; a cancelled `usePerson` query aborts its mocked request |
| 8.4 tag optimistic | `queries.test.ts`: apply shows the chip at once in lower-cased-name order; 409 rolls back with the 20-tags copy; remove rolls back on 404; 404 still invalidates tags and person; success writes `data.tags` |
| 8.5 racing invalidation | `queries.test.ts`: a simulated stale overwrite during the pending window never survives the mutation's own resolution |
| 8.6 preload and single request | `LoginView.test.ts` (spy on `preloadTodayView`), `router.test.ts` (prefetch once for a Today target incl. the root redirect; never for non-Today, platform-only or unauthenticated), `todayPrefetch.integration.test.ts` (deferred Today responses; exactly one request per endpoint as the mount joins the prefetch) |
| 8.7 hover prefetch | `DataTable.test.ts` (`onRowIntent` on pointerenter and focusin), `PeopleView.test.ts` (fake timers: 150 ms dwell, latest row wins across three rows, focusin parity, preview renders without "Loading person…" within the 30 s staleTime) |
| 8.8 FilterBar | `FilterBar.test.ts`: selected trigger with count only when applied; chevron inside the edit button with the "Edit …" name; Clear all appears and disappears as a non-locked clause joins or leaves a locked-only state (the former locked-only click test updated to assert absence); PeopleView and TodayFeedsView tests unchanged and green |
| 8.9 no Loading flash on filter change | existing test unchanged and green |
| 8.10 walkthrough | [`docs/design/qa/slice-014-2026-09-08/`](../design/qa/slice-014-2026-09-08/README.md): 9 of 9 scenarios pass in production mode over the tunnel (login → Today; hover prefetch then a preview with no Loading; optimistic stage and assignee holding through settle with the requests delayed 1.5 s; tag apply and optimistic remove; locked chips and no Clear all in the Today rules editor; selected trigger "Stage · 2", chevron and Clear all; popover 7 px under its chip in a wrapped row; caching headers) |
| 8.11 gates | final gate below |

### Review round 1 (of two)

Reviewer READY WITH FIXES, tester no blocking finding, both on `9f1ce13`.
Verified: the People key prefix `['org', orgId, 'people']` matches no other
factory key, so the optimistic sweep is Organization-scoped and cannot touch
`person`, `stages`, `members`, `tags` or the saved-list counts; snapshots are
taken after awaited cancels and restored atomically; row writes match by id
and never add or remove rows; `stageRefFromCache`, `actorRefFromCache` and
`tagRefFromCache` return clean `{id, name}` / `{id, display_name}` shapes;
pessimistic fallbacks skip the write and still invalidate once; `usePerson`
forwards `signal` and the abort test is real; the guard calls
`prefetchTodayData` only after every early return (public, pending,
unavailable, 401, platform-only, admin redirects) and `prefetchQuery` never
rejects navigation; the built `TodayView` chunk is referenced only from the
index chunk, so the router record and `LoginView` share one chunk; the
integration test genuinely holds all three Today responses open across the
mount; `hasClearableClause` uses the same predicate as `clearAll`; the
`router.test.ts` network-leak fix is complete and no other test can reach a
live API. Applied in `fa9bcab`:

- tag apply/remove invalidate the person key on any error, not only 404, so
  a timeout after a server commit heals;
- the selected FilterBar trigger keeps a transparent border so toggling does
  not shift layout by the glass control's 1px;
- one shared `fetchPerson` for `usePerson` and the hover prefetch (with
  `signal`);
- tests: the stage test's other row seeded with a third stage (the previous
  assertion was vacuous); the assignment optimistic write and rollback across
  detail, unfiltered and filtered rows; the racing-invalidation test
  rewritten to hold a stale GET open past the mutation's resolution; tag
  success writing the server's order; the 149/150 ms dwell boundary and the
  unmount timer; `LoginView.test.ts` mocks the client; the integration test
  fails loudly on unexpected paths.

### Recorded LATER (D-050)

- A `qc.isMutating` guard before the settle-invalidate would remove a
  possible flicker when a second optimistic mutation starts while the first
  one's refetch is in flight (the final state is already correct).
- A Cloudflare cache rule for HTML `no-cache` on `app.tarams.org` (dashboard;
  out of scope) would remove the brief stale-`index.html` window after a
  rebuild.
- Optimistic Today-source toggle; People chunk preload on navigation hover.
- The People route still resolves as nine small chunks before its first data
  request (parallel, one round trip, but still ahead of the data);
  `modulepreload` hints for the People route or merging its vendor pieces
  would remove that hop.
- The login POST (0.76–1.1 s over the tunnel, 0.24 s at the origin) is now
  the largest single stage of login → Today; it is edge behaviour for
  request bodies, not application code.
- A mutation's `onSuccess` writes the whole server `person`, so a rapid
  stage-then-assignee pair can transiently show the first response's value
  for the second field until the second settles (final state converges);
  writing only the mutated field is the cheap mitigation.
- Optimistic tag insertion re-sorts the whole array by UTF-16 code units
  after lower-casing; Postgres `lower(name), id` under a linguistic collation
  can order accented names differently. Corrected on success; insertion at
  the comparator's index would leave server order intact.
- The dwell timer is cleared on the next row's intent and on unmount, not
  when the pointer leaves the table; a mouse crossing the table fires one
  prefetch for the last row (within rule 4's bound).
- A realtime `person.changed` for an unrelated Person invalidates the whole
  People prefix during a pending mutation; the row reverts transiently and
  `onSettled` heals it (same family as the `isMutating` guard).
- A stale stage or member name in the reference cache renders until
  `onSuccess` replaces it with the server value.
- The chevron can wrap under a very long chip; checked in the walkthrough.

### Tunnel measurement (coordinator, production mode, after the dev-server stop)

Performed 2026-09-08 after the pre-approved switch: the dev server (pids
24542/24563) stopped by exact PID; `scripts/dev-web-prod` from the slice tree
at `fa9bcab` serving on 5173 (preview pid 55779); the tunnel route unchanged.
Same probe script and method as the 2026-09-07 investigation (§6), three runs
in production mode against `app.tarams.org`, alice's seed login; the dev-mode
figures are the investigation's three runs.

| Step | Dev mode (2026-09-07) | Production mode (2026-09-08) |
|---|---|---|
| Cold `/login`: requests | 50–60 modules + assets | 24 total (HTML, chunks, icons, manifest, fonts, CSS, one API call); **3 scripts before DOMContentLoaded** on loopback (lane measurement), one `<script type="module">` plus `modulepreload` hints in the served HTML |
| Cold `/login`: DOMContentLoaded | 1.3–3.1 s | 0.22 s, 0.58 s, one jittery run 2.0 s |
| Cold `/login`: network idle | 2.5–4.5 s | 1.2 s, 1.3 s, one 4.9 s |
| Login click → Today data | 1.4–1.8 s | 0.97–1.6 s, of which `POST /api/session` 0.76–1.1 s (the edge floor for POST bodies, §1 of the investigation) |
| Warm `/today`: DOMContentLoaded / Today data | 1.0–2.1 s / 2.2–2.5 s | 0.07–0.37 s / 0.17–1.36 s (18 requests instead of 60) |
| First nav to `/people`: rows | 0.39–0.76 s after 9 sequential dev modules | 0.18–1.05 s; nine chunk requests still precede the data request but load in one parallel round trip |
| Filter change (My people) | 120–145 ms, no Loading flash | 68–277 ms, no Loading flash |
| Person preview by direct click | 140–250 ms, Loading shown | 74–212 ms; the probe clicks without a dwell, so hover prefetch is exercised in the walkthrough, not here |

Headers through the tunnel: `/` `cache-control: no-cache`, `cf-cache-status:
DYNAMIC`; a hashed asset `cache-control: max-age=14400`, `cf-cache-status:
MISS` on first fetch. Absolute numbers are laptop-and-residential-uplink
measurements and are reported, not gated (D-050); the structural gate (≤ 10
scripts before DOMContentLoaded) passes with 3.

### Final gate (coordinator, once, on `fa9bcab`, 2026-09-08)

`./scripts/check` run once from the worktree root under the shared gate lock:
all checks passed, 14 s: fmt, clippy, cargo check, crate fences, **757** Rust
tests, doc tests, Web lint/typecheck/**650** Vitest/build, email-worker tests.
`check-db` is not required (no backend, `.sqlx` or migration change). Push and
deployment are not performed or authorized by this record.

### Merge readiness

Source `slice-014-perceived-latency` at `fa9bcab` plus the QA archive and this
record; destination `main` (docs-only commits ahead of the branch base
`731d3ce`: project state records, so no code conflict is possible). No
migration; after the merge the production server should be restarted from the
main checkout (`./scripts/dev-web-prod`) so the tunnel serves the merged
build. Unresolved risks: the LATER list above.
