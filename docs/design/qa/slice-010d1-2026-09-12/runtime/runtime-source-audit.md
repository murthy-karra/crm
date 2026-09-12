# Slice 010d1 runtime source audit

The final three process recorders reconcile **74 synthetic GET reader calls:
15 identity, 27 core fixture, 32 history**. Every request completed; sequences
and counters reconcile below the recorder cap. No source calls occurred after
the replacement phase, including final access, disconnect and dialog checks.

| Epoch | All GETs | Identity | Core fixture | History |
|---|---:|---:|---:|---:|
| initial | 63 | 11 | 27 | 25 |
| low-budget | 0 | 0 | 0 | 0 |
| restored-budget | 11 | 4 | 0 | 7 |

History totals: 20 Events, 6 Calls, 6 Text Messages. The complete case used
1 identity + 7 collection reads; cancellation 1 + 2; overlap 1 + 2; denial 1 + 1;
retry 1 + 13; restored budget 1 + 7. Replacement made 3 identity reads and no
collection reads. Proposal, lost-confirmation replay, repeated retained reads
and the low-budget pause added no source calls in their recorded phases.

There were 19 browser attempts: {"failed": 3, "passed": 16}.
The JSON retains all attempts, including failures and reruns; source counts are
not reset or hidden across those attempts. Exact phase windows, checks and
counter deltas are recorded. Scoped external-request counts and recorded
business-publication deltas are zero.

History made **no detail, email, media or external-URL reads** and the source
seam made no non-GET operations. Core fixture setup legitimately made **six
note-detail reads**: three successful, three synthetic 404s. Do not extend the
zero-detail claim to that preparation. The fixture reader makes no real FUB
requests; these counters are not a general host-wide network audit.

Of the 41 post-setup calls, two are credential PUT identity validations
(replacement/restoration), correlated with API request/response windows. The
remaining **39 history-worker reads = 7 identity + 32 collection attempts**.
Subtracting the one discarded late response gives **38 expected retained capture
rows**, matching the coordinator's separate retained audit. The JSON preserves
the two credential-call sequence numbers and exact API log line references.

Six 429 responses preceded exact-path retries, each
**3.687683–3.997842 seconds after response completion**.
Saved control and runtime defaults specify two seconds; Retry-After is not
retained per response. Retry finished seven collection pages in thirteen
attempts with six 36-byte diagnostics (216 bytes). The browser observed
waiting_retry.

Cancellation returned HTTP 200 at 2026-09-12T17:34:31.185258Z while Events page two was in
flight. Its 40,987-byte response completed at 2026-09-12T17:34:34.749+00:00,
3.563742 seconds later. The recorder counts that response. The passed browser
check verifies it was discarded: cancelled state, unchanged sequence 2,
100 retained rows and zero reservation. The coordinator owns independent
retained database/byte-ledger proof; this audit queried no database.

API snapshots contain **no ERROR or HTTP 5xx**, no matches for 10 private
credential values and encoded forms, no source-payload matches among 60 synthetic-content
positive strings, and no authorization/cookie/private-key markers. One common
fixture word matches the synthetic startup banner in each log; those three
classified positives are retained in the JSON. There are
**18 WARN notices through replacement and 23 in the final log snapshots**.
All are PostgreSQL already/no-transaction-in-progress notices. Their cause and
baseline provenance were not established here; this is not a warning-free claim.
Exact hashes, messages and line numbers are recorded in the JSON.

This constructed recorder/log audit does not qualify live source coverage or
act as a packet trace. Recorder completion precedes settlement and cannot alone
prove retention, native invariants or byte accounting. Positive-string scans
have the documented coverage limits. No raw bodies, credentials, cookies or
environment files are copied. See runtime-source-audit.json for input hashes,
phase accounting, timing pairs and recorder limitations.
