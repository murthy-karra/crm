# Slice 014 — Perceived latency over the tunnel, and FilterBar polish

**Status: APPROVED by the user on 2026-09-08 after independent review, with
both §9 workflow confirmations given (the tunnel is normally served from the
production bundle; the running dev server is stopped by exact PID at
verification).** Approval authorizes implementation and tests in the Slice 014
lane, not commit, merge, push or deployment. Prepared against `main` at `8331f77` (Slices 012 and 013 merged
and pushed). Web-only. Brief: [SLICE_014_IMPL.md](../tasks/SLICE_014_IMPL.md).

The [perceived-latency investigation](../design/perceived-latency-2026-09-07.md)
measured the development stack through its public hostnames in a real browser:
every request pays a 90–250 ms edge floor, POST bodies pay 300–700 ms more,
and dev-mode Vite ships 50–60 unbundled module requests per page load plus a
nine-module route chunk before the People page sends its first data request.
The backend answers in about a millisecond. This slice takes the three levers
the investigation ranked, in that order, and folds in the small FilterBar
usability residue recorded since 011a so that component is touched once.
**Nothing changes on the wire** and nothing changes in the backend.

Authority: [D-016, D-017, D-018, D-023, D-045, D-050](../decisions/DECISION_LOG.md);
[UI_STYLE](../design/UI_STYLE.md); [002](SLICE_002.md) §10 (query-key
factory), [003](SLICE_003.md) §10, [011a](SLICE_011a.md) §6 (FilterBar
contract as amended 2026-09-06), [011b](SLICE_011b.md) §6, [011d](SLICE_011d.md)
§6 (locked-clause mode), [011e](SLICE_011e.md) §5 (tag chips and mutations);
the investigation note §5 (ranked levers) and §6 (method).

## 1. Scope and product behavior

In scope, four parts delivered in this order:

- **A. Production build served through the tunnel.** A `scripts/dev-web-prod`
  sibling of `scripts/dev-web` that builds `web/dist` and serves it on port
  5173 with Vite's preview server, so the existing tunnel route
  (`app.tarams.org → localhost:5173`) needs no change. Dev mode with HMR
  stays the loopback default.
- **B. Optimistic updates** for the mutations that pay the POST penalty on
  the Person page: stage change, assignment change, tag apply and remove.
- **C. Preload and prefetch.** The Today route chunk is imported while the
  login page is mounted; Today's data queries start from the router guard
  as soon as the identity is known, before the route chunk finishes; People
  rows prefetch the Person detail on hover or focus.
- **D. FilterBar residue.** Of the four gaps recorded on 2026-08-29, three
  were resolved by the 2026-09-06 UX pass (drafts never render as chips, the
  editor is an anchored glass popover, Clear all exists). What remains: a
  toolbar trigger looks the same whether or not its clause is applied; the
  chip has no visible click-to-edit cue; Clear all shows in locked-only mode
  where it does nothing; anchoring under a wrapped chip row is to be
  verified live.

Out of scope: collapsing the post-login `/me` read (a session-lifecycle
design decision, deliberate today); a `CompressionLayer` at the origin; any
backend change; the cloudflared or dashboard route configuration; serving
`web/dist` from the API (diverges from D-018's separate web and API
services); an optimistic Today-source toggle (its visible effect is Today
membership, not a field; LATER); preloading the People chunk on navigation
hover; mobile.

Product rules (safe defaults, veto-able; §9 lists two workflow confirmations
for the user):

1. **One server owns port 5173.** Both `dev-web` and `dev-web-prod` bind it
   with `strictPort`; exactly one runs at a time. `dev-web-prod` always
   builds first (`web/dist` is gitignored and goes stale). In prod mode a
   code change is invisible until the script is re-run; the README says so.
2. **Optimistic writes never change list membership.** A stage or assignee
   change is written into the Person detail and into the matching row of
   every cached People variant, but a row that no longer matches a filter
   leaves only when the settled refetch says so, so rows and the live count
   never disagree.
3. **Optimistic only where the client can predict the result.** Stage,
   assignment, tag apply and tag remove are optimistic with rollback. Login
   and session changes, saved-list and Today-feed edits (revisions and 409
   reload flows), tag create, rename and delete (server-assigned ids and the
   D-051 verdict), log-contact (server-ordered history), calls and intake
   stay pessimistic. Create-then-apply of a new tag: the create is
   pessimistic, the apply that follows it is optimistic.
4. **Prefetch is bounded.** One dwell timer, latest row wins, at most one
   new detail request per dwell, deduplicated by the query cache; nothing
   crosses tabs (D-050's one-tab envelope).
5. **Rollback surfaces the existing errors.** A failed optimistic mutation
   restores every snapshot and renders the same inline error text the page
   shows today (stage, assignee, tag 409 and 404 copy).

## 2. Part A — production serving

`web/vite.config.ts` gains a `preview` block that sets only `port` (the one
field Vite does not inherit; the preview default is 4173). Host, `strictPort`,
`allowedHosts` (`CRM_WEB_ALLOWED_HOSTS`) and both proxy entries are inherited
from `server` by Vite's preview resolver, so `http://127.0.0.1:5173` keeps
working in prod mode, including the loopback realtime websocket path. Through the tunnel the browser does not use the proxy: the client
resolves `https://api.<rest>/api` whenever the hostname starts with `app.`,
so `app.tarams.org` talks to `api.tarams.org` directly, as it does in dev
mode. Vite 8.2.1's preview server supports `allowedHosts` and `proxy`
(`PreviewOptions` extends the common server options) and serves `dist` with
ETags and SPA fallback. No new dependency.

`scripts/dev-web-prod`: from the repo root, `cd web && pnpm run build &&
pnpm exec vite preview`, same shape as `scripts/dev-web`. README "Start the
applications" gains the prod-mode paragraph; `.env.example` notes that the
`CRM_WEB_*` variables also govern preview. Caching, stated honestly: preview serves `dist` with ETag and
Last-Modified and no `Cache-Control` header; hashed asset names make
Cloudflare's default edge caching of scripts, styles and fonts safe;
Cloudflare does not cache HTML by default, but a browser may treat a
recently fetched `index.html` as heuristically fresh, so after a rebuild a
plain navigation can briefly reference assets that no longer exist until a
reload revalidates. The README says "re-run the script, then reload". A true
HTML `no-cache` is a Cloudflare cache rule in the dashboard, out of scope
(LATER).

## 3. Part B — optimistic mutations

All four follow the TanStack pattern: `onMutate` cancels in-flight queries
for the affected keys, snapshots them, writes the optimistic value and
returns the snapshots; `onError` restores every snapshot; `onSuccess` writes
the server's authoritative value (`data.person` for stage and assignment,
`data.tags` into the detail's `tags` for tag apply and remove, whose routes
return `{tags, changed}`); `onSettled` invalidates as the mutation does
today. `usePerson`'s query function passes its abort `signal` so
cancellation actually aborts the request.

| Mutation | Optimistic write | Fallback |
|---|---|---|
| Stage | `person.stage` in the detail and in the matching row of every cached People variant, using the `StageRef` from the stages cache | if the stages cache is empty, stay pessimistic rather than invent a name |
| Assignment | `person.assigned_user` from the members cache, or `null` for Unassigned | same fallback on an empty members cache |
| Tag apply | append the `TagRef` from the tags cache into the detail's `tags`, sorted by lower-cased name then id (the server's `lower(name), id` order is authoritative on success) | 409 limit and 404 vanished roll back and render the existing inline copy; the 404 refetch path stays |
| Tag remove | filter the `TagRef` out of the detail | same |

Realtime (D-023): `person.changed` invalidations fire after commit, so a
refetch they trigger returns the new state and cannot revert an optimistic
value; the only hazard is a refetch already in flight at mutate time, which
`cancelQueries` removes, and `onSettled`'s invalidate corrects anything that
slips between. The two Selects stay disabled while pending; the displayed
value flips at once because it reads the cache.

## 4. Part C — preload and prefetch

- **Today chunk during login.** A tiny module exports
  `preloadTodayView = () => import('./views/TodayView.vue')`; the router's
  Today route record and `LoginView`'s `onMounted` both call it, so the lazy
  import resolves from the module cache and a test can spy on the export.
  In prod mode that is one chunk plus its few dependencies (the built Today
  chunk imports only the table, the log-contact dialog, the page header and
  two helpers, not the call library).
- **Today data before the chunk.** In the router guard, after the identity
  resolves and the target is Today with an Organization, a new
  `prefetchTodayData(queryClient, session)` in the queries module
  `prefetchQuery`s the Today, Today-sources and Today-feeds keys using the
  existing fetchers (factored into named functions). Keys stay in the
  factory (SLICE_002 §10). The session fence holds: the guard has already
  settled verification, and the fetch layer throws for private paths while
  verification is pending. Today's `refetchOnMount: 'always'` joins a fetch
  that is still in flight rather than issuing a second; over the tunnel the
  request floor (≥ 90 ms) exceeds the chunk import, so in practice the mount
  joins the prefetch. A completed prefetch followed by mount legitimately
  refetches; that is not a defect. The test holds the Today responses open
  until after mount and then asserts exactly one request per endpoint. The
  guard prefetches nothing for non-Today targets, platform-only sessions or
  its pending/unavailable early returns.
- **Hover and focus prefetch.** `DataTable` gains an optional `onRowIntent`
  callback fired on `pointerenter` and `focusin` (additive; other tables
  unaffected). `PeopleView` runs a single 150 ms timer (latest row wins,
  cleared on leave) and `prefetchQuery`s the Person detail. In-flight
  requests are not aborted on leave; the default 30 s `staleTime` means a
  click within it renders the preview without its Loading block.

## 5. Part D — FilterBar residue

- A toolbar trigger whose clause is applied shows a selected state (surface-2
  background, full text colour, a count such as "Stage · 2"); otherwise the
  UI_STYLE §5 glass control. The chip row remains the source of truth.
- The chip's edit button gains a 16px Lucide chevron (`aria-hidden`) after
  the text; the 40px target and the "Edit …" accessible name are unchanged.
- Clear all renders only when at least one non-locked clause is applied
  (today it also shows in the Today rules editor's locked-only state where it
  does nothing).
- Popover anchoring under a wrapped chip row is verified in the walkthrough;
  no change unless it drifts.

Locked-clause mode, URL sync, the live count and the empty-filter
normalization are untouched. No wire contract changes.

## 6. Contracts

None on the wire, in the schema or in the Operator. Declared for AGENTS §11
completeness: `DataTable` gains an optional prop (additive, Web-internal);
`vite.config.ts` gains a `preview` block; a new script and README text.

## 7. Failure behaviour and observability

A failed optimistic mutation rolls back and shows the existing error; a
prefetch failure is silent (the click path fetches normally). Prod mode
serves stale code until re-run, stated in the README. No new telemetry.

## 8. Acceptance criteria

1. `./scripts/dev-web-prod` builds and serves `web/dist` on 5173;
   `https://app.tarams.org` loads, logs in, and the realtime indicator does
   not show unavailable; `http://127.0.0.1:5173` also works in prod mode
   (proxy parity); `./scripts/check-tunnel` passes. (walkthrough)
2. **Probe, prod mode**, three runs with the investigation's method against
   `app.tarams.org`: cold `/login` module and script requests issued
   **before `DOMContentLoaded`** (the shell plus the login route chunk)
   **≤ 10** (structural gate, was 50–60); the Today preload's own request
   count reported separately; DCL, login → Today data, and People rows
   reported against the 2026-09-07 table, not gated (D-050); one `curl -I`
   each of `/` and a hashed asset through the tunnel recording
   `cache-control` and `cf-cache-status`. (measurement)
3. Stage and assignment: the new value appears in the Person detail and in
   a filtered and an unfiltered cached People row before the response
   resolves, changing only the row with the matching id and leaving a
   now-non-matching filtered row in place; a rejected response restores
   every snapshot and renders the existing error text; success writes the
   server's `person` and invalidates once; with an empty stages or members
   cache no optimistic write happens and the mutation still succeeds and
   invalidates; a cancelled `usePerson` query aborts its request. (Vitest)
4. Tag apply shows the chip at once in lower-cased-name order; 409
   `person_tag_limit_reached` rolls back and shows the 20-tags copy; tag
   remove rolls back on 404; 404 still invalidates tags and person; success
   writes the server's `tags`. (Vitest)
5. A `person.changed` invalidation arriving during a pending stage mutation
   never displays the old stage after the mutation resolves. (Vitest)
6. Mounting `LoginView` calls `preloadTodayView` (spied); with the Today
   responses held open until after mount, login → Today issues exactly one
   request each to Today, Today sources and Today feeds; the guard prefetches
   nothing for a non-Today target or a platform-only session. (Vitest)
7. Hovering a People row for 150 ms issues one detail request; crossing
   three rows within 150 ms issues at most one; `focusin` behaves like
   hover; opening the preview within 30 s renders without the Loading block.
   (Vitest)
8. FilterBar: the trigger's selected state only when applied; the chevron
   inside the edit button with the "Edit …" name intact; Clear all hidden in
   locked-only mode; every existing FilterBar, PeopleView and TodayFeedsView
   test green, except that a test clicking Clear all in a locked-only state
   is updated to assert its absence. (Vitest)
9. Filter changes still show no Loading flash (the `placeholderData` path
   is unchanged). (Vitest, existing)
10. **Walkthrough** over `app.tarams.org` in prod mode: login → Today;
    People → hover a row → open the preview without Loading; change stage
    and assignee (instant, no revert); apply and remove a tag; the Today
    rules editor still shows locked chips; FilterBar chips, chevron and
    Clear all as above; popover anchored under a wrapped chip row.
11. `./scripts/check` green (Web lint, typecheck, Vitest, build). Independent
    reviewer and tester, read-only, at most two rounds.

## 9. Safe defaults and workflow confirmations

Safe defaults (veto-able): Vite preview over a separate static server;
`dev-web-prod` always builds; optimistic set exactly the four mutations;
pessimistic fallbacks on empty caches; 150 ms dwell and no abort on leave;
the selected-state styling and chevron; Clear all hidden in locked-only mode.

Two workflow confirmations for the user at the gate, not product decisions:
(1) the production bundle becomes the way the tunnel is normally served,
with dev mode kept for loopback work; (2) the currently running dev server
is stopped by exact PID (recorded in PROJECT_STATE, not here) when the slice
is verified so `dev-web-prod` can take the port. Recorded LATER: a
`qc.isMutating` guard before the settle-invalidate would remove a possible
flicker when a second optimistic mutation starts while the first one's
refetch is in flight (the final state is already correct).

## 10. Delivery

One Web lane, one writer, worktree `../crm-worktrees/014` on
`slice-014-perceived-latency` from `main`; size **M** (A: S, B: S–M, C: S,
D: S). Owns `web/**`, `scripts/dev-web-prod`, the README paragraph and the
`.env.example` comment; no backend, no `.sqlx`, no migration. Order A → B →
C → D, each with its Vitest before the next; the probe and walkthrough run
against the lane's own preview server on a scratch port (never 5173 while
the user's dev server holds it; the tunnel measurement happens once at the
end, after the user's confirmation 2). This specification authorizes nothing
until the user approves it after independent review; approval will authorize
implementation and tests, not commit, merge, push or deployment.
