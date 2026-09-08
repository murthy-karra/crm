# Perceived latency over the public tunnel — investigation (2026-09-07)

Measured at the user's request against the running development stack through
its public hostnames (`app.tarams.org` → Vite dev server on `:5173`,
`api.tarams.org` → `crm-api` on `:3000`, both via the `crm-dev` Cloudflare
tunnel; browser and tunnel connector both at colo SJC). Real Chrome driven by
`playwright-core` (headless, fresh profile), logged in as the seeded Acme
admin, three full runs; `curl` for the request floor. Nothing was changed in
the application. This refines the "PERCEIVED-LATENCY chunk" queued in
`docs/plans/PROJECT_STATE.md` and the tunnel section of
[PERF_BASELINE.md](PERF_BASELINE.md).

## 1. The request floor has risen and is structural

| Path (15 samples each) | min | p50 | p90 |
|---|---:|---:|---:|
| `https://api.tarams.org/api/health` | 89 ms | 194 ms | 241 ms |
| `https://app.tarams.org/api/health` (through the Vite proxy) | 116 ms | 247 ms | 432 ms |
| bare Cloudflare edge (`cdn-cgi/trace`, no tunnel) | 45 ms | 53 ms | 73 ms |
| loopback `127.0.0.1:3000/api/health` | 1 ms | 1 ms | 1 ms |

Every request crosses the edge twice (browser → SJC → tunnel → this Mac →
back), so the floor is about two bare edge round trips plus jitter from the
residential uplink. The August baseline's "~60 ms floor" is not reproducible
now; in-browser API calls measured 50–220 ms each with the server at ~1 ms.
This is not application code and no code change lowers it; only fewer
sequential round trips do.

**POST bodies pay more.** `POST /api/session` (login):

| Path | time to first byte |
|---|---:|
| loopback | 238–242 ms (Argon2id, by design; server log 265–271 ms) |
| tunnel, `curl` | 547–944 ms |
| tunnel, browser | 900–1150 ms |

A GET on the same path costs ~120 ms, so a request with a body carries an
extra 300–700 ms at the edge or connector (body inspection or an upload
round trip). Every mutation the UI makes pays this, which is why optimistic
updates matter more here than the backend numbers suggest.

## 2. Dev-mode Vite through the tunnel dominates every page load

| Step (3 runs) | module requests | DOMContentLoaded | data visible |
|---|---:|---:|---:|
| cold `/login` | 50 | 1.3–3.1 s | — (first `/api/me` fires only at DCL) |
| warm reload `/today` | 60 | 1.0–2.1 s | Today data 2.2–2.5 s |
| first client-side nav to `/people` | 9 (route chunk) | — | rows 0.39–0.76 s |

The shell alone is 50–60 unbundled module requests, each paying the floor;
the route-level lazy chunk for People is nine more requests that must finish
(300–530 ms) before the first data request is even sent. A production build
of the same code is 42 files in total (2.2 MB, largest `livekit-client`
531 KB), of which the shell needs a handful and each route one chunk. Serving
a production build through the tunnel is the single largest lever for "the
app feels slow to open" and for the first visit to each page. Measured
figures for that are pending (§5).

## 3. The login-to-Today waterfall

Click → `POST /api/session` (0.9–1.15 s, §1) → `GET /api/me` (~120 ms;
deliberate: the session lifecycle's one authoritative identity read for
multi-tab safety) → TodayView route chunk (12 modules, 300–470 ms) → four
parallel data calls (`/api/today`, `/today/sources`, `/today/feeds`,
`/realtime/token`, ~80–110 ms). Today data at **1.4–1.8 s after the click**.
Sequential network stages: four. A production build removes most of the
third stage; preloading the Today chunk while the login POST is in flight
removes the rest of it. Folding `/me` into the login response is possible but
trades away the lifecycle's authoritative read; not recommended without a
design decision.

## 4. What already works

| Interaction | measured | Loading flash? |
|---|---:|---|
| Filter change (My people shortcut) | 120–145 ms to new rows | **none** (`placeholderData: keepPreviousData` is already in place on the People query) |
| Open a Person preview | 140–250 ms to detail data | yes, "Loading" shown after ~25 ms for the duration |

The filter path is done. The preview shows a Loading state for one round trip;
hover prefetch of the detail, or a skeleton that keeps the row's name, would
remove the flash.

## 5. Ranked levers for the chunk (measured, not speculative)

1. **Serve a production build through the tunnel** (or a Vite preview of the
   build): 50–60 module requests → a handful; removes the 300–530 ms
   route-chunk stall on first visits. Expected effect: cold load from 2.5–4.5 s
   toward well under 1.5 s; Today data after login from 1.4–1.8 s toward
   ~1.2 s. Development on `localhost:5173` keeps dev mode. Verify by
   re-running the probe against the built output.
2. **Optimistic updates on stage and assignment mutations** (and tag
   apply/remove): mutations pay 0.5–1 s at the edge (§1) even though the
   server finishes in tens of milliseconds.
3. **Preload the Today route chunk during the login POST** and **hover-prefetch
   Person detail**: each removes one round trip from a felt interaction.
4. **Collapse the `/me` round trip after login** only as a recorded design
   change to the session lifecycle; the gain is ~120 ms.
5. Not a lever: backend query time (≈1 ms health, 50–200 ms end to end is
   all network). `CompressionLayer` at the origin would shrink only the
   Mac→edge leg (unchanged from August).

## 6. Method notes

Probe script kept in the coordinator's scratch space, not committed (it
reads the seed password from `CRM_DEV_SEED_PASSWORD` and never prints it).
Timings are Chrome's `request.timing()` (start offset relative to the
step's start, `responseEnd` as duration) and Navigation Timing; "Loading"
detection is a `MutationObserver` on `document.body` text. Runs were
sequential on a laptop also running Postgres in Docker, Vite, the API and the
coordinator's agents; absolute numbers are indicative (D-050), the ordering
of levers is not.
