# Intake throughput — capacity notes

Recorded 2026-08-27 (user question during the Slice 008 walkthrough:
"this could be a firehose"). Measurements from the live 007g/008
walkthroughs on the dev deployment. Not a work plan — a reference for
when volume ever approaches these numbers.

## Stage-by-stage ceilings

1. **Cloudflare edge (Email Routing → relay worker):** effectively
   unlimited at our scale — the relay is stateless and Cloudflare
   parallelizes invocations per message. No owned queue.
2. **`POST /inbound/email`:** ~35 ms end-to-end measured live (seal,
   store, parse, person upsert, facts — synchronous). Different orgs
   fully parallel, bounded by CPU/DB pool.
3. **Per-org serialization (deliberate):** one org's intake is
   single-writer behind the `intake:` advisory lock — the property
   that makes dedup race-free and rotation (D-041) fair. Ceiling
   ≈ 25–30 leads/second **per org** (~2M/day/org). Bursts past the
   3 s lock budget are NOT lost: 503 → the relay throws → the
   sending MTA retries for hours. SMTP is the backpressure queue; no
   owned buffering needed.
4. **LLM extraction (the narrowest path):** the in-process worker
   handles unrecognized-format mail ~1 lead/second sustained
   (~86k/day) — sequential attempts, ~1 s Groq latency each — plus
   Groq's own rate limits. Arrival-to-lead observed live: 2–10 s.
   (User-observed "minutes" during walkthroughs were upstream Gmail
   send-pacing, not this pipeline.)

## The worker: what/where/how to scale

An in-process tokio task inside crm-api (spawned at boot when a DB +
`GROQ_API_KEY` are configured; `lib.rs`) — NOT a separate deployment.
10 s poll; ≤50 claims/sweep with immediate re-sweep at the cap;
`FOR UPDATE SKIP LOCKED` claims with a 60 s lease (a dead worker's
row self-releases); 3-attempt quality cap with backoff; transport
failures wait indefinitely (spec 007f).

Scaling dials, in order of reach:

1. **API replicas (free today):** every crm-api replica runs a
   worker; SKIP LOCKED + the lease make N workers cooperate safely
   with zero coordination. K8s: scales with the API Deployment.
2. **In-process concurrency:** let one worker run several attempts
   concurrently instead of sequentially (bounded, e.g. per-provider
   limit). Moderate change to `worker.rs::run_once`.
3. **Dedicated worker deployment:** extract the spawn into its own
   binary for independent replica counts. Small refactor, only
   worthwhile when worker CPU/limits diverge from API sizing.

## The strategic answer to firehose volume

Firehose mail is portal mail, and portal mail is templated: every
pinned format (007h family) converts that entire stream from
~1/s-with-Groq-cost to milliseconds-and-free, with the LLM as the
long-tail net — the intended steady state. Realistic load check: a
50-agent org at a few thousand leads/day ≈ one lead per 20 s; the
tightest path above sits three orders of magnitude higher.

First dial to turn if extraction backlog ever grows: worker
concurrency (dial 1 or 2). Watch: `intake_extraction` ledger lag
(occurred_at − raw_payload.received_at).
