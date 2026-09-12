# Slice 010d1 — Execution brief

**Release follow-up:** D-070 subsequently authorizes commit/merge/push, owned
cleanup and shared-development deployment. The implementation-time restrictions
below retain their historical scope; current release status is recorded in
[SLICE_010d1_RELEASE.md](SLICE_010d1_RELEASE.md).

**APPROVED FOR IMPLEMENTATION — D-070, 2026-09-12.** The user said “Ok go for it”
after the full plan and READY review were presented. Complete policy/shared
contracts, implementation and required isolated synthetic verification are
authorized. Baseline main `028d6133e1b7c3f81642275e030f63e98b2cca49`;
at that approval checkpoint, shared development remained the verified 010f2 release.

## Outcome and inputs

After approval, implement [010d1](../specs/SLICE_010d1.md): an independently
confirmed same-account history capture bound to completed 010c People, encrypted
raw evidence, honest coverage and bounded admin inspection. Preserve core parent,
siblings and review workspace. No native history, source writes, email/media
retrieval, activation, Operator/native clients, source/customer operations or
deployment. The [010d ladder](../specs/SLICE_010d.md) keeps those later dependencies
explicit; capture is not finished historical import.

Read AGENTS, decisions D-012/015/050/059–069, architecture, complete 010b/c/f1/f2
specs/concrete contracts, the new source/code research, and prompts 05/06.
Public source research qualifies documented expectations only. Live validation
is still user-deferred. Synthetic source fixtures must be labeled constructed,
not vendor captures, particularly where documentation examples are malformed.

## Ownership, dependencies and order

After implementation approval, create one short-lived
`codex/slice-010d1-history-capture` worktree from then-current main. One primary
writer owns backend, shared HTTP/source/storage contracts and the **sole database
migration lane**, including SQLx metadata and test registrations. The coordinator
owns spec/status/amendment pointers, review dispositions and final-tree evidence.
Use a single lane by default; disjoint Web support may start only after the client
contract is frozen and backend reviewed. At most three worktrees; one primary
writer per worktree; never concurrent DB test runs in a checkout.

Use the advisory `design` profile for fidelity/authorization/storage planning
and preserve the actual assigned model. Independent reviewers inspect frozen
trees. Escalate contract/policy changes to the coordinator and user under AGENTS
§11/§16; a failing fixture does not authorize a lossy fallback or expanded source.

| Step | Owned deliverable and dependency | Acceptance checkpoint |
|---|---|---|
| 1. Freeze concrete contract | New `SLICE_010d1_CONTRACT.md`: exact endpoint/query/parser profiles and public schema manifest, DTO/errors/acknowledgements, states/receipts, source identity, byte inventory/bounds, cursor/keyset and capability fields | A1–A8/A11/A12; no unresolved source fallback or implied native fact semantics |
| 2. Source and persistence | Separate history tables and reader request type; exact captures/observation indexes/projections, counters, parent binding, crypto purposes and owned reservations | A1–A3/A6/A7/A12; prove 16-MiB reservation or stop and report the needed contract correction before changing it |
| 3. Lifecycle and API | Typed proposal/confirm/retry/cancel/budget, fenced worker, symmetric assessment/core/history admission and connection invalidation, bounded retained reads, preflight/startup capability | A4–A8/A11; retained reads without source authority; no old worker claims history jobs |
| 4. Backend review/fix | First bounded independent implementation review, focused source/DB/HTTP/CLI tests, accepted corrections and frozen client contract | A1–A9/A11/A12; baseline parent/native/sibling reconciliation retained |
| 5. Web | History card/confirmation, active progress, budget/resume/cancel, source coverage/records/variants and stale-response/identity fences | A7/A8/A10; truthful partial/completed-with-gaps states and bounded tables |
| 6. Integrated review/verification | Second bounded independent implementation review/fix, sequential full gates, actual query plans and synthetic real-API/production-Web walkthrough | A1–A12; final hashes, exact counters, evidence and owned resource cleanup |

D-050 allows two implementation review/fix rounds; the planning review here is
separate. Do not start an unbounded review cycle or claim remaining findings
resolved without proof. No calendar estimate before source volume/visibility is
measured; the sequence above is the executable dependency order.

## File and contract boundary

- New `history_capture*` modules under `backend/crates/crm-app/src/domain/migration/`
  own request parsing, commands, store, worker and paged queries. Additive
  `FubReader` method and production/fixture implementations reuse transport rules.
  Keep `fub-core-v1` enums and original requests unchanged. A history run never
  goes into the old core worker's claim table unless a separately reviewed
  compatibility change explicitly replaces this design.
- One new migration in `backend/crates/crm-api/migrations/`: tenant-keyed run/
  capture/observation/coverage/reservation stores, composite parent keys, exact
  identity constraints, bounded fields/indexes and durable confirmed requirement.
  No applied SQL edits or native domain insert permits. Sole owner generates
  `backend/.sqlx/` metadata.
- Existing migration `reader.rs`, `store.rs`, assessment/snapshot admission,
  connection replacement/disconnect, shared source pacing and byte ledger change
  only where required for symmetric history participation. Test all directions;
  checking exclusivity only inside the new command is insufficient.
- API route/module/state/startup registration and worker scheduling; existing
  `ReleaseReadiness` plus `scripts/migration-release-preflight` and its tests own
  the new capture capability. No Person/timeline/Today read-contract amendment.
- Web new history API/types/components/query keys plus migration view integration;
  preserve current session/workspace fences. Read UI_STYLE and relevant styling
  instructions when implementing. No Operator or native mobile write path.
- New focused pure/source/DB/HTTP/Web/CLI tests. Primary writer owns shared
  `all.rs` registrations. Reuse existing synthetic API/QA-server helpers and
  ephemeral infrastructure; no new production service or general import framework.
- Coordinator amendments: 010a/b source-lifecycle and accounting pointers;
  010c/f1/f2 independent capture boundary; capability/erasure inventory, migration
  summary/current state, contract/review/verification records. Preserve historical
  delivery evidence and the 2026-09-12 completion audit unchanged.

## Required checks and prerequisites

During implementation run focused tests for each acceptance row. Include actual
foreign resources and current-role changes; lost response replay with exact actor/
request/body identity; changed source account/user/credential; mixed old/new source
jobs and disconnect races; every pagination mode/failure; invalid IDs/large numbers/
duplicate keys/variants/group references; quota/lease/crash/cancel interactions;
frozen capture-sequence traversal during writes and zero raw/body/log exposure.
Test nonidentical overlapping pages (IDs 1–100 then 100–199 with total 200),
terminal invalid/duplicate-ID uncertainty and the advancing-versus-diagnostic
capture basis; occurrence totals alone must never settle stream exhaustion.

Construct a deterministic synthetic fixture with ≥25k rows in each family,
≥500 rows for one Person, empty/rare filters, duplicate and changed observations,
unknown/zero Person IDs, excluded/erased parent identities and group participants.
Retain exact expected-vs-actual capture counts, distinct identities, variant and
link counts, plaintext/ciphertext/accounted bytes, unchanged parent/sibling/native
rows and zero provider/outbound/realtime-business side effects. Inspect actual
SQL plans for the new queue, source identity, keyset pages and filtered pages;
prove queries and decryption are bounded rather than relying on post-fetch limits.

After the two bounded reviews, on the frozen final tree, run sequentially:

```sh
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

Use repository-pinned Rust/Node/pnpm and documented isolated Postgres/Centrifugo.
`check` is service-free; DB scripts require the disposable fixture and exclusive
DB slot. Never reset/seed shared `crm_dev`, print credentials, call live FUB or
count skipped checks as passes. Keep command logs, exact counts and tree hashes.

Add an opt-in collector named `db_history_capture_plans` using actual application
SQL, registered by the primary writer, and retain successful output from:

```sh
cd backend
SQLX_OFFLINE=true cargo test -p crm-api --test all --features perf-harness --locked db_history_capture_plans:: -- --ignored --nocapture --test-threads=1
```

Use D-050's 25k-People/50-member envelope. No unrelated Today/timeline load test.
If an existing shared reader is changed, include it in **one paired** baseline/
final check using the same fixture/build settings, equal expected payloads and
the established relative p95 bound max(25 ms, 10%); do not rerun benchmarks without
a demonstrated regression. New page size/decrypt bounds and query plan shape are
mandatory independently of timing.

Finally use a real synthetic API and production Web build: proposal makes zero
source calls; named confirmation starts the fake source; all three streams/coverage
restrictions, linked/unlinked/variant data, paging, retry/backoff, storage pause/
increase/explicit Resume, cancel/in-flight commit, lost response replay, reload,
disconnect, source revision replacement, admin demotion and Org switch. Inspect
desktop/390px screenshots and dialogs. A fake source recorder must prove the
exact allowed GETs and absence of email/detail/media/source writes. Record zero
real-source calls separately from synthetic request counts.

Preserve sanitized evidence, concrete contract, current erasure inventory and
verification report; clean only owned sessions/processes/containers/volumes.
Do not modify the current shared-development runtime or renew release evidence.

## Planning handoff and approval

Independent plan review is [READY](SLICE_010d1_REVIEW.md), with the terminal
identity-reconciliation finding resolved and targeted confirmation recorded.
D-070 accepts the complete reviewed contract and implementation under AGENTS
§11/§16. The earlier D-069 split approval remains the sequencing decision.
Source/customer operations, activation, Git publication and deployment remain
outside this brief's implementation approval. Later 010d2 planning must use
actual capture findings and return its own semantics/read-contract choices.
