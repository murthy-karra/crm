# Slice 010d1 — Implementation verification

**Release follow-up:** D-070 subsequently authorizes commit/merge/push, owned
cleanup and shared-development deployment. The implementation-time restrictions
below retain their historical scope; current release status is recorded in
[SLICE_010d1_RELEASE.md](SLICE_010d1_RELEASE.md).

**IMPLEMENTED AND VERIFIED within the approved synthetic scope — 2026-09-12.**
The approved historical capture/coverage contract is implemented in the isolated
worktree. Both bounded implementation reviews, repository gates, populated SQL
collector and production-Web/synthetic-API walkthrough are complete. This is
capture evidence, not native timeline import or live FUB qualification.

## Target and authority

D-070 accepts the complete [specification](../specs/SLICE_010d1.md),
[execution brief](SLICE_010d1_IMPL.md) and declared shared contracts. Implementation
is on `codex/slice-010d1-history-capture` in
`/Users/karrad/projects/crm-worktrees/slice-010d1`, based on main
`028d6133e1b7c3f81642275e030f63e98b2cca49`. Changes remain uncommitted. No commit,
merge, push, deployment, activation, real FUB access or customer-data operation
was performed. Shared development remains the existing 010f2 release.

Backend/database ownership was assigned to `audit_backend_completion`, source
parser/manifest and Web to `audit_web_completion`, and the synthetic API example
to `check_latest_delivery`. Root owned coordination, capability/preflight,
independent component inspection, final gates, browser verification and cleanup.
Each source file had one primary writer. Model settings were inherited without
overrides; per-task billed usage/cost is unavailable.

## Delivered behavior and files

- Nine additive history tables and typed Rust lifecycle/query/worker modules
  retain encrypted events, calls and text-message source evidence, observations,
  variants, links, receipts and owned reservations. Capture binds to a completed
  immutable People parent; it writes no native timeline facts.
- Fixed GET-only source requests use strict bounded parsing and honest terminal
  distinct-ID reconciliation. Coverage reports inaccessible totals as unknown,
  preserves gaps and field presence, and ends as `completed_with_gaps`.
- Actor-bound confirmations, current authority/connection checks, source admission,
  exact receipts, restart/retry/cancel handling and shared logical byte budgets
  support bounded recovery. Retained admin reads are source-free and use frozen
  keyset cursors with response/decryption bounds.
- Additive API routes, source capability/preflight checks and Web history panels
  implement proposal, confirmation, progress, coverage, records/variants,
  storage review, separate Resume and cancellation. The integration updates
  `MigrationView` and its existing source-busy coordination.
- Tests cover parsing, HTTP/domain/DB authority and failure paths, release
  readiness, populated query plans and Web lifecycle behavior. The synthetic
  `history_capture_qa` example is opt-in test support.

The concrete contract is [recorded here](SLICE_010d1_CONTRACT.md). Erasure/restore
inventory now includes all nine stores in
[production readiness](../plans/PRODUCTION_READINESS.md); those customer-data
readiness gates remain open.

## Final checks and artifact identity

[Gate logs and checksums](../design/qa/slice-010d1-2026-09-12/gates/README.md)
retain actual stdout, exit status, time and source manifests.

| Command | Actual result | Source qualification |
|---|---|---|
| `./scripts/sqlx-prepare` | Exit 0; 52.627s | Original final manifest `f559c658…` |
| `./scripts/check` | Exit 0; 98.553s; 949 Rust, 1,131 Web, five doctests, 25 preflight and 11 email-worker tests passed | Original final manifest; formatting, lint, shape/boundary checks and Web build also passed |
| `./scripts/check-db` | Exit 0; 542.25s; **934/934 DB tests passed** | Same backend/schema; includes all 26 new focused history DB tests |
| `./scripts/check` after HC-R2-W02 | Exit 0; 39.179s; same suite counts passed, no LEAK annotation | Final source manifest `2af03113…`; only the allowance CTA and existing test locator changed |

The two full non-document manifest digests are
`f559c6588beedae4b291390822d4100bf5080a1708cfa55fce5eb78f05351f75` and
`2af03113b4e654ba4490c2b0bf40c46b1d3e883e005fb26e1b31048911a8f590`.
All 1,037 entries were compared: the only difference is the label in
`HistoryCapturePanel.vue` and its test. Backend, schema, parser, QA binary and
SQLx metadata are unchanged. The earlier DB gate is not relabeled or counted as
a rerun after that Web-only correction.

The original full check had one passing nextest LEAK annotation on
`session::login_with_non_json_content_type_returns_400`. Its isolated follow-up
passed (1 test, 0.021s test time) without LEAK; the later full check also had none.
No cause or production fix is inferred. The DB gate had one slow test, no
failure/LEAK; its SQLx check reported potentially unused cached queries without
failing. The existing LiveKit 531kB build chunk advisory remains. Complementary
DB/pure-test skips in each runner are not included in pass totals.

[Backend evidence](../design/qa/slice-010d1-2026-09-12/backend/README.md) preserves
11 parser passes, 26 focused history DB passes, actual transport evidence,
standard/opt-in lint, formatting, additive-upgrade and QA-build results. The
final batch passed 25 focused tests plus the populated collector in 326.32s;
the later incomplete-parent test passed separately in 2.98s and in the full DB
gate. The additive-upgrade test passed in 3.85s. Its earlier missing-relation
failure and narrow fixture correction are preserved, along with other interim
attempts. Interim tests are not substituted for final artifact evidence.

## Acceptance read-back

| ID | Verified result and evidence |
|---|---|
| A1 | Actual completed same-account People parent accepted; real proposed and queued parent states, foreign resources and mismatched bindings rejected with exact no-write controls. Browser proposal made zero source calls. Parent/sibling/workspace row hashes stayed unchanged. |
| A2 | Strict parser/production transport tests and recorder cover fixed historical paths and parameters, redirects, URL/token rejection and bounds. Historical capture made no detail/email/media requests or source writes. Public schema is pinned; live pagination remains unqualified. |
| A3 | Parser and DB tests preserve exact encrypted raw evidence, large IDs, duplicate-key rejection, variants, checkpoint/total rules and terminal uniqueness. Browser overlap retained 200 occurrences/199 distinct IDs and paused rather than claiming enumeration. |
| A4 | Domain/HTTP/DB controls prove current role, actor, tenant, connection revision and symmetric source admission. Browser demotion removed the view; foreign existing capture returned 404. Replacement fenced the old run and required a separately confirmed current-user capture. |
| A5 | Lost confirmation response replayed the exact request and receipt without extra state/bytes. Browser cancellation retained the first 100 records and discarded a delayed response. Six actual synthetic 429s exercised Retry-After and recovery. Alternating workers/restart tests prove progress and each process's readiness admission. |
| A6 | Reservation/cap/encryption/sibling/budget-race tests and actual octet-length collector checks reconcile shared logical storage. Lowered deployment ceiling paused at sequence zero with zero source calls; increased allowance left the run paused until explicit Resume. Terminal runs released reservations; paused runs kept their dedicated cancellation reserve. |
| A7 | Fixture coverage distinguishes occurrences, unique identities, conflicts, invalid IDs, parent linkage and returned field presence. Denied events showed zero observations with unknown total/inaccessible count, not false zero inaccessible records. Browser inspected variants and unlinked metadata. |
| A8 | Populated collector retained 25,000 records per family and 501 per family for one Person, traversed 1,500 pages and inspected 14 actual SQL plans. Browser froze sequence 2 while sequence 3 committed; the same cursor/page remained identical. Tenant/cursor/erasure tests and actual response sizes enforce bounded retained reads. |
| A9 | Before authority-test mutations, only the nine history tables and shared ledger changed across 108 tenant tables. Final reconciliation preserves all People/relationship-history/Today/Operator/parent/sibling/workspace rows; exact role/credential test changes are separately accounted for. Synthetic business publication deltas remained zero during capture. |
| A10 | Production Web and real API commands passed proposal/confirm, uncertain replay, progress/frozen paging, full coverage, variants/unlinked, partial cancel, restriction/retry, budget/pause/Resume, replacement, demotion/Org switch and disconnect. Desktop/390px images were actually inspected. HC-R2-W02 clipping was corrected and rechecked with both action bounds inside 390px. |
| A11 | Preflight unit/CLI and actual-artifact DB controls reject stale/forged/missing capability, incompatible profile/parser/schema, keys and mixed candidates. Durable confirmed cancelled runs remain capability-relevant. Restart and per-process admission have positive and negative controls. |
| A12 | Ciphertext/AEAD, key isolation, corruption and log-sentinel tests passed; retained endpoints expose metadata, no source-body or media reader. Nine new stores are explicitly inventoried. This does not close real-data erasure/restore readiness. |

## Populated and browser evidence

The final constructed collector used 25,000 People/50 members. Only the initial
People were imported through ordinary commands; the remaining People are an
explicit cardinality fixture. It retained 75,000 observations through 750
collection GETs and one identity GET, then traversed all observations through
1,500 authorized pages. Maximum measured page/record sizes were 51,965/1,001
bytes; SQL fetched at most 51 observations and decrypted at most 50. All six
embedded production/schema hashes matched source independently.

The 14 actual plans had no observation-table sequential scan or temporary spill.
Physical work is not constant: a rare excluded filter examined 25,000 family
index entries, and an empty conflicts filter examined 75,000 entries plus
same-identity probes. This is correctness and query-plan evidence, not production
capacity, p95 latency or vendor completeness proof.

The production-Web walkthrough used three constructed Organizations, private
loopback PostgreSQL 18.6/Centrifugo 6.9.2 and an opt-in API example with injected
FUB reader, readiness and publisher. All three launched API process records and the final executable agree on SHA256
`d859036fe9417e130c19fdd26b75fea3687f475cd061e1887d923b81215cb4dc`.
The backend handoff had separately recorded a compiled-only executable
`fc82cf60…`; that earlier build is not claimed as the browser-run binary.
No real FUB HTTP request was possible through that reader. Production transport
and actual release-artifact guards are tested separately; the browser harness
does not prove real provider pacing or Centrifugo delivery.

[Runtime evidence and reproduction](../design/qa/slice-010d1-2026-09-12/runtime/REPRODUCTION.md)
retain successful and failed attempts. One completed walkthrough needed an
owned Chrome shutdown to release harness response-body cleanup; a retained-only
repeat then exited cleanly. Two access attempts failed on login synchronization;
the completed attempt proves the actual role/tenant boundaries. The first
post-fix dialog attempt correctly found no allowance controls on a terminal run;
a new unconfirmed proposal supplied the final layout check and was cancelled
without source work. These are harness corrections, not additional review rounds
or hidden product passes. The earlier clipped-dialog image is retained alongside
its correction.

The three process recorders contain 74 synthetic GETs: 15 identity, 27 core
fixture and 32 historical collection reads. Of 41 post-setup calls, two validate
credential replacement/restoration; 39 belong to history workers. One late
cancelled response was discarded, matching 38 retained captures. Historical
capture made no detail/email/media reads; ordinary core fixture preparation
included six note-detail reads. The six retry waits measured 3.688–3.998 seconds
after failure completion against the configured two-second delay. Counters are
an injected-reader audit, not a host-wide network trace.

API logs contain no ERROR/HTTP 5xx or scanned credential/source-payload matches.
They do contain 23 PostgreSQL already/no-transaction-in-progress notices. The
[source/log audit](../design/qa/slice-010d1-2026-09-12/runtime/runtime-source-audit.md)
retains exact lines and scan qualifications. These observations do not establish
cause or prove every notice benign; the existing bounded SQLx cancellation
follow-up remains open before production cutover.

Across the final browser fixture there were nine history runs, 38 retained
captures, 1,509 observations and 1,508 identities. The first six runs' counters
remained identical through later authority checks. Before those checks, the 98
non-history/non-ledger tables were identical. Finally, 94 tables remained
identical and exactly four authority tables changed: four deliberate role events,
two credential-replacement receipts, the associated membership row and connection
rows. All 12 member roles/statuses were checked against setup and restored.
Completed/cancelled runs reserved zero bytes; three deliberately paused runs
retained 8KiB control reserves each. Every Organization ledger delta exactly
matched its history-run retained/reserved sums.

## Reviews, cleanup and remaining limits

Both authorized [implementation review/fix rounds](SLICE_010d1_IMPLEMENTATION_REVIEW.md)
are complete with HC-R1-01–04, HC-R2-01/02 and HC-R2-W01/W02 resolved. The final
visual correction and acceptance proof additions belong to round 2.

Owned API/Web processes were stopped after checking exact process identity.
The isolated Docker project, volume and network were removed; ports
13010/15173/55432/18082 are closed and no owned Chrome profile has a process.
Shared container IDs/ports and shared listener PIDs are unchanged. Private
credentials, browser profiles, scratch files and the worktree `.env` were removed
at 18:12:22 UTC after archive verification. The [cleanup receipt](../design/qa/slice-010d1-2026-09-12/runtime/cleanup-receipt.json)
and root [checksums](../design/qa/slice-010d1-2026-09-12/SHA256SUMS) record closeout.
The worktree and its uncommitted source/build caches are retained for review and
integration. Three main-checkout status documents now point to this worktree;
2,107 other tracked/pending files were unchanged during that scoped status sync.

Live FUB pagination/account visibility, native timeline import (010d2),
email/media, deltas, activation and customer-data readiness remain outside this
completion. The inherited core snapshot worker's process-UUID handoff concern
is an unproven follow-up, not a reproduced or fixed result here. Existing SQLx
cancellation and audited call-history ordering follow-ups remain separately open.
