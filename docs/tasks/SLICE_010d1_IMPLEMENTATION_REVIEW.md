# Slice 010d1 — Implementation reviews

**READY — both bounded implementation review/fix rounds complete, 2026-09-12.**
D-070 authorizes implementation and isolated synthetic checks. Final repository
gates, populated evidence and production-Web walkthrough have passed; see
[verification](SLICE_010d1_VERIFICATION.md) for exact artifact qualifications and
cleanup. This record preserves findings and intermediate handoffs; their earlier
pending statements describe those checkpoints, not the final status. No live
source, native timeline, deployment or Git-publication readiness is implied.

Target: `/Users/karrad/projects/crm-worktrees/slice-010d1`, branch
`codex/slice-010d1-history-capture`, base
`028d6133e1b7c3f81642275e030f63e98b2cca49`. Uncommitted documentation from the
prior audit/planning was copied intact; it is not newly implemented product code.

## Round 1 — backend components — READY for Web handoff

Root's independent backend review and final delta inspection are complete.
HC-R1-01 through HC-R1-04 are resolved. The credential-only regression passed
separately (1 test, 2.83s); root read its actual log and inspected the pause/release
code. The frozen client contract was handed to the Web writer. Two internal
argument-count lint corrections and the required populated collector/full gates
remain implementation verification work; this handoff is not final readiness.

The root-owned capability component was independently reviewed by
`audit_web_completion`, who authored the separate source parser but did not
author these reviewed files: `auth/workspace.rs`,
`scripts/migration-release-preflight` and its Python tests. The read-only review
found no actionable issues in exact-artifact/role matching, readiness freshness,
old-report handling, mixed selected/running API/worker capability, schema/count/
profile validation and durable confirmed-run detection after cancellation.

The review attributed the 25 passing Python tests to root. It ran no commands;
two new Rust readiness tests and actual DB startup checks were still pending.
This is component review only. Main backend/parser/HTTP/worker/storage and
synthetic QA harness review remain required before round 1 is complete.

Root independently inspected the parser, commands, store, worker, retained-read
queries, HTTP routes, migration and shared admission/crypto registration changes.
The backend was still being tightened and had not reached its freeze checkpoint.
Finding **HC-R1-01**: the parser inferred `source_hidden` from `showContent:false`
without qualified evidence for the new history GET endpoints. The parser author
confirmed this came from core-profile precedent. The accepted correction is
factual field presence only: `returned_in_raw` or `not_returned`; neither proves
complete or accessible content. Unknown flags/placeholders stay in exact raw
evidence. The parser owner is applying the correction within this review round;
focused rerun and final delta inspection are pending.

**HC-R1-02** (root, confirmed by backend owner): the new history worker reused
the legacy identity decoder, which could accept duplicate decoded JSON keys.
The history profile requires strict bounded parsing before identity binding. A
history-only validator and retained/live identity regression checks are being
added; legacy core/assessment contracts are preserved.

**HC-R1-03** (`audit_web_completion`, independent of QA-example author): the
synthetic recorder labeled identity requests `me`, but the production reader
requests `identity`. The QA owner is correcting that label so exact-source-scope
evidence reflects the production request. This changes evidence accuracy, not
source behavior. No browser runtime result was claimed before the correction.

**HC-R1-04** (root final delta inspection): with valid retained identity but a
damaged credential ciphertext, claim could return an error while leaving the
candidate queued. The accepted fix is an observable `retained_integrity_failed`
pause, releasing only that run's source reservation and making no source call.
The backend owner is applying it with a targeted credential-only corruption
control before the round-1 handoff.

Root inspected the HC-R1-01/02/03 corrections and read the actual focused logs:
11 parser tests passed (0.06s) and 15 DB tests passed (45.12s). This includes the
strict identity diagnostic and existing tenant/receipt/admission/cancellation
checks. These are interim focused results, not final-tree/full-suite evidence.
The independent QA example review found no further functional issues in its
isolation guards, fixed fixtures, source-only controls or recorder preservation.
Its disabled temporary client wording was also corrected: the harness uses no
HTTP FUB reader. It does not prove real release-artifact readiness, provider
transport/pacing or Centrifugo delivery.

## Round 2 — integrated behavior — READY

Web implementation and independent integrated inspection are complete; targeted
fixes and acceptance evidence remain in progress. No final readiness or
review-budget exception is claimed. The Web writer used the repository UI style
and the Tailwind skill, announced by root at handoff.

**HC-R2-01** (root compatibility inspection): a proposed-only future profile
correctly did not require compatible durable recovery, but Confirm could mark it
durable before the worker rejected its profile. Confirm and Resume now compare
profile/parser/schema before mutation; read actions and worker authority use the
same predicate. The A11 test owner is adding proposed-only and paused negative
controls. The populated collector had already started on the preceding binary;
its embedded hashes identify that binary. The added eligibility guard does not
change measured SQL/DDL/parser behavior; final evidence must distinguish this
delta rather than relabeling the measured artifact.

**HC-R2-02 — RESOLVED** (root worker lifecycle inspection): comparing a durable process UUID
with each claimant's UUID could make two compatible workers alternate identity
checks indefinitely without advancing a collection. The fix records the DB-clock
time of successful identity verification and a bounded startup boundary per
worker process. Compatible live workers can share later verification; a restarted
worker requires a new verification. A regression alternates independent sessions
and then introduces a restarted session. Its initial proof passed with two
identity calls, nine collection pages and 603 observations. Each process also
passes its own initial capability admission before reusing another worker's
verification; the claim carries this boundary through settlement. Root inspected
that correction and read its targeted negative-control pass (1 test, 3.33s).
This changes lifecycle DDL, so the populated collector ran again on the final
backend artifact; its collector case passed while the remaining batch checks were
finishing. The earlier collector output is preserved as interim evidence.

**HC-R2-W01 — RESOLVED** (`check_latest_delivery`, independent of the Web author):
an unselected active capture in an unpolled list could keep source controls
disabled after it completed. The fix refreshes the bounded loaded list while it
contains an active capture, with 2–30s failure backoff. A two-run regression
selects terminal B while A is active, proves busy state clears after A settles,
and proves observation reads and terminal polling remain stopped. The author
reports 32 focused tests, typecheck and targeted lint passing. The independent
reviewer inspected the exact code/test delta and marked integrated Web R2 READY;
it ran no commands and found no other actionable P1/P2 issues in exact request
recovery, authority fences, rendering, stable paging or the HTTP contract.
The real desktop/390px walkthrough remains pending.

The backend owner noted similar UUID handoff logic in the inherited core
`snapshot_worker.rs`. That older path is unchanged by this slice; no regression
proof or fix is claimed. It is recorded as a separate follow-up rather than
included in 010d1's completed behavior.

The earlier additive-upgrade test needed a fixture-construction compatibility
view for today's core admission lookup. The empty typed view is removed and
asserted absent before freezing the actual old-schema data. Original activity
migration preservation assertions remain, then the current additive schema is
applied and the same complete rows are compared again before startup. Root
inspected this narrow test-only correction and read its pass (1 test, 3.85s).
No production guard was weakened.

The A1 acceptance read-back found a missing explicit incomplete-parent negative.
The backend owner added a real proposed → queued → completed People lifecycle
test: both incomplete states reject history preparation with unchanged parent,
history rows, receipts, ledger and source counters; the same completed parent
then succeeds. Root inspected the test and read its pass (1 test, 2.98s).
This closes specified proof coverage within round 2, not an extra review round.

During independent QA preparation, the QA/collector agent inspected root's three
private scripts without running them. Root corrected the assumed disconnect
route, per-page retry expectations, duplicate-start process guard, Organization
row hashing, single-snapshot reconciliation, browser-error pass criteria and
demotion-test cleanup. The scripts pass syntax checks; runtime and screenshots
are still pending. These preparation corrections are not extra production
review rounds or evidence of a completed browser walkthrough.


**HC-R2-W02 — RESOLVED** (independent actual-image QA by
`check_latest_delivery`, also viewed by root): at 390px, the long allowance
confirmation CTA pushed Cancel partly outside the dialog viewport. The local
history-panel CTA is now “Increase allowances”; the dialog heading and exact
capture/budget preview retain its context. Only that label and its existing test
locator changed. Focused Web tests (32), typecheck, lint and build passed, followed
by root's full `./scripts/check` (949 Rust, 1,131 Web, five doctests, 25 preflight,
11 email-worker tests; exit 0, 39.179s, no LEAK annotation). The final 390px image
and measured button bounds show both actions fully inside the viewport. Cancel
closed the dialog without changing allowances; the new unconfirmed layout
proposal was then cancelled with zero source calls. The prior clipped image and
the terminal-run harness attempt are preserved. This completes the second round's
visual verification and correction; no third review was performed.

Final backend/source/schema hashes still match the full 934-test DB gate and
75,000-observation collector. Exact manifest comparison limits the later source
delta to the two CTA/test files. The desktop/390px walkthrough and final row/byte
reconciliation passed. Failed harness attempts, the response-cleanup recovery,
all source restrictions and physical query-work qualifications remain in the
[verification record](SLICE_010d1_VERIFICATION.md) and its linked artifact package.
