# Performance Baseline — 2026-08-29

Measured load-test results for the dev stack, recorded so future
changes (denormalization, 011c feeds, pool sizing, index work) are
compared against numbers instead of memory. Re-run with
`./scripts/perf` (see "Reproducing" below) after any change that could
move these.

## Method and caveats — read before comparing

- **All data was created through the live HTTP API** (D-021: no direct
  DB writes) into one dedicated Organization, "Perf Test Realty".
  `./scripts/dev-bootstrap` wipes it like everything else.
- **The API binary was a DEBUG build** (`./scripts/dev-api` =
  `cargo run`, dev profile). Absolute numbers are conservative; a
  release build serializes/decodes faster. The *shape* of every finding
  (what is flat, what scales with what) is build-independent because
  the dominant costs are inside Postgres.
- Environment: M1 Max MacBook Pro; Postgres 18 in Docker (OrbStack),
  loopback; client = Python `requests` (sequential for latency tables,
  100 OS threads for concurrency — the client itself saturates before
  ~a few hundred req/s, so STRESS-phase throughput is a lower bound).
- The 2026-08-29 dataset had a **pathologically skewed assignment**: the
  seed relied on `actor_default` routing, which concentrated ~60k
  people on the admin's book. Accidentally useful — it measured the
  worst-case Today. `./scripts/perf seed` now round-robins assignment
  explicitly; expect per-viewer numbers to differ on a fresh seed.

## Dataset

100,000 people, 67,037 contact attempts, 104,950 inquiries (repeat
inquiries for ~5%), 103 members. Books: admin 60,560; m1 19,750.
Seeding sustained **104–127 leads/s** for 15+ minutes through the full
intake pipeline (per-org advisory lock serialized) with zero
degradation over time — write throughput is not a concern.

## Latency tables (loopback, 15 iterations, debug build)

### At 5,000 people (thin history)

| Query | avg ms | rows |
|---|---|---|
| People unfiltered | 18.1 | 500 TRUNC |
| assigned_to / source / has_phone / last_contact filters | 19–21 | 500 |
| 4-clause combo | 24.4 | 500 |
| Today (admin) | 97.0 | 200 TRUNC |

### At 100,000 people (67k attempts of history)

| Query | avg ms | p95 | verdict |
|---|---|---|---|
| People unfiltered | 23.1 | 28.7 | flat |
| assigned_to me | 20.6 | 25.9 | flat |
| source zillow | 25.2 | 27.8 | flat |
| last_contact within 7d | 22.4 | 22.8 | flat |
| has_phone false | 42.8 | 47.3 | mild degrade |
| **last_contact never** | **233.6** | 257.2 | degrades (absence-proving) |
| **4-clause combo (incl. a never)** | **317.8** | 338.8 | degrades |
| **Today (admin, 60,560-person book)** | **965.8** | 1012.8 | linear in book × history |
| Today (member, 19,750-person book) | 590.4 | 606.1 | same curve |

Positive, indexable predicates are flat from 5k → 100k. Absence-proving
filters (`never`, `has_x false`) and Today degrade.

## Why Today costs ~1 second (EXPLAIN ANALYZE, admin book)

The query narrows to the viewer's book first (indexable outer WHERE),
then runs ~5 LATERAL probes per person (latest inquiry, last
uncorrected attempt, waiting inquiry, last inbound/outbound
correspondence). Each probe is a few-microsecond index descent; the
cost is the multiplication: **60,560 people × ~5 probes ≈ 300k index
descents ≈ 1s**. Cumulative timings from the actual plan: 48 ms after
scanning the book, 288 ms after the latest-inquiry probe, 416 ms after
last-attempt, 701 ms after all probes, 1,109 ms total. `LIMIT 201`
cannot help: membership and ordering depend on the probe results, so
every candidate must be evaluated before the top 201 are known.

A real agent's book is hundreds to ~2,000 people (≈ 5–60 ms even
today); the 1s case is concentrated books, admin-scale views, and —
critically — 011c, which re-runs this pattern per marked list on every
Today render.

## Concurrency (100 agents, one org, 100k people, pool = sqlx default 10)

| Phase | served | p50 | 503s |
|---|---|---|---|
| STRESS (no think time) | 36 req/s | ~2.3–2.8 s ALL endpoints | 201 (9%) |
| REALISTIC (2–5 s think time, ~24 req/s) | healthy | people/filters 23–60 ms; Today 612 ms | 18 (1.2%) |

The wall is **capacity = pool ÷ query time**: 10 connections behind
~1s Today queries ≈ 10 heavy queries/s; excess demand queues to the 2s
acquire timeout and 503s. Even realistic load produced user-visible
errors. Fast queries stayed fast at p50 whenever they weren't queued
behind a Today. Logins (Argon2id) averaged 236 ms sequential — by
design.

## Tunnel path (browser-realistic network costs)

~60 ms floor per request (Mac ↔ Cloudflare edge, twice each way);
People ≈ 90–140 ms browser-realistic; payloads compressed at the edge
(229 KB → 27 KB — browsers get this automatically; an origin
`CompressionLayer` would additionally shrink only the Mac→edge leg).
Dev-mode Vite through the tunnel ships hundreds of unbundled modules,
each paying the floor — the largest "app feels slow to open"
contributor, absent in any production build.

## Conclusions → recorded actions

1. **Denormalized last-activity columns** (`person.last_contact_at` /
   `last_inbound_at` / `last_outbound_at` / `latest_inquiry_at`,
   maintained at write time): collapses Today's probes and the
   `never`/absence filters to indexed column tests (~1s → tens of ms,
   ~40x capacity). Queued in PROJECT_STATE; land before or with 011c.
2. **Explicit pool sizing**: `PgPoolOptions.max_connections` is the
   sqlx default 10 (`crm-api/src/state.rs`); size deliberately, and
   plan a PgBouncer/CNPG-Pooler tier when API replicas multiply.
3. **Perceived-latency chunk (web)**: `keepPreviousData`, optimistic
   mutations, hover prefetch, waterfall collapse — the network floor,
   not the backend, governs feel. Queued in PROJECT_STATE.
4. Fixed-matrix static SQL filtering is vindicated at 100k; no change.

## Reproducing

```sh
# API must be running; source .env for CRM_DEV_SEED_PASSWORD.
./scripts/perf seed --target 100000     # ~15 min at ~110/s, resumable via --start
./scripts/perf bench                    # 15-iter latency table
./scripts/perf agents --agents 100      # STRESS + REALISTIC, 60s each
```

Compare against the tables above; note the debug-build and
assignment-skew caveats when comparing absolute numbers.
