# Slice 010f2 — Implementation and verification

**Historical implementation checkpoint.** The subsequent
[release record](SLICE_010f2_RELEASE.md) records completed integration/deployment;
the [2026-09-12 audit](SLICE_010_COMPLETION_AUDIT_2026-09-12.md) independently
checks the completion claims. Statements below about an uncommitted worktree or
010f1 runtime describe the earlier checkpoint, not current operational status.

**IMPLEMENTED AND VERIFIED — 2026-09-11.** D-068 authorizes
this [specification](../specs/SLICE_010f2.md), [brief](SLICE_010f2_IMPL.md), owned
contracts and isolated synthetic verification. Runtime reconciliation has passed.
Commit, merge, push, deployment, live FUB/customer processing and activation are
outside this execution. Shared development remains on 010f1, `e36ce36`.

## Implementation and ownership

Baseline `cd3b0107392d4b40230bdd8996c12ec41be3d588`; worktree
`/Users/karrad/projects/crm-010f2`, branch `codex/slice-010f2-activity-import`.
The eight pending planning files on main were preserved; source and documentation
in this worktree remain uncommitted.

The implementation adds one retained notes/tasks child of a completed People
import, explicit historical-author/creator/active-assignee and task-kind mappings,
confirmed IANA source timezone, bounded HTML conversion with exact originals,
immutable plans/results/receipts, native INSERT permits, source-qualified identity,
atomic settlement/accounting, retry/cancel and bounded admin review. The existing
review hold continues to block ordinary agent use and outbound/Operator actions.

Changed areas are the new activity migration/domain/API modules; native review
queries and Person reader guards; startup/release preflight; Web activity APIs,
mapping/source/confirmation panels and Person review components; focused tests,
opt-in measurement harnesses and the guarded synthetic API example. Owned spec,
contract, erasure-inventory and status amendments are included. Dependencies are
pinned to html5ever 0.39.0 and chrono-tz 0.10.4 under the frozen conversion profiles.
See the [concrete contract](SLICE_010f2_CONTRACT.md).

The backend primary initially owned the sole migration and shared registrations;
root took remaining ownership when that agent reached its usage limit. Bounded
source, Web and measurement helpers had non-overlapping files. Root owns final
integration, migrations, runtime/DB audits and documentation. Assigned models were
inherited without override; billed usage is unavailable. No shared file had
concurrent product writers.

Both D-050 implementation review/fix rounds are closed. R1 found five issues,
all corrected with targeted regressions. R2 independently returned READY with no
P1/P2 findings and verified all 1,377 frozen hashes at start/end. Reviews were
static; they do not substitute for runtime verification. The later measurement,
visual-artifact and dependency investigations were bounded verification work,
not a third implementation review. See [review evidence](SLICE_010f2_IMPLEMENTATION_REVIEW.md).

## Environment and final checks

Private QA directory: `/private/tmp/crm-010f2-qa-694szdwe`, mode 0700; fresh
synthetic environment files mode 0600, without provider credentials. Owned
PostgreSQL 18.6 used port 55432, container `crm-010f2-postgres`, volume
`crm-010f2-postgres-data`; test and API databases are `crm_010f2_tests` and
`crm_010f2_qa`. Owned Centrifugo 6.9.2 used port 18082. API 3017 and production
Web preview 5187 are stopped; owned containers, volume and browser profiles
were removed after all checks completed.

Verified toolchain: Rust 1.98.0, sqlx-cli 0.8.6, Node 24.16.0, pnpm 11.22.0;
Chrome 152.0.7977.83 and Playwright-core 1.63.0. Build/dependency caches were
independent APFS clones. DB-backed execution and measurements were serialized.
Shared `crm_dev`, ports 5432/3000/5173/8000, API/Web PIDs 35303/35323 and
`development-*` containers were outside the implementation work.

Final closure sequence, including the corrected opt-in fixture, is sequential:

| Command | Result |
|---|---|
| `./scripts/sqlx-prepare` | [Passed](../design/qa/slice-010f2-2026-09-11/checks/final4-sqlx-prepare.log); compiler 14.90s; offline cache unchanged |
| `./scripts/check` | [Passed](../design/qa/slice-010f2-2026-09-11/checks/final4-check.log) in 56s: 935 Rust tests, 1,108 Web tests across 77 files, 5 doctests, 20 release-preflight and 11 email-worker tests; formatting/lint/build passed |
| `./scripts/check-db` | [Passed](../design/qa/slice-010f2-2026-09-11/checks/final4-check-db.log): 908/908; 442s total, 413.835s test execution; one slow existing test, no LEAK annotation |

The [final source manifest](../design/qa/slice-010f2-2026-09-11/checks/final4-source-sha256.json)
freezes 1,016 non-document source/configuration files; every hash matched after
the final gates and cleanup. Manifest SHA256:
`e0649d8e40cbaac17d09f0de8f2027cde61f0cb9d604cdd181a243084bdc40be`.
The preceding full sequence passed the same service-free tests and all 908 DB
tests in 458s total (427.076s test execution), with one slow existing scale test
and no LEAK annotation. Perf-target Clippy passed after the opt-in fixture fix.
The final closure repeated the required scripts so that even that test-only
correction is included in the final frozen tree.

Earlier failed attempts remain evidence:

- The first DB gate expected two result-issue rows for two successfully imported
  tasks. The correct issue count is zero. The test assertion was corrected; its
  focused rerun and later complete DB suites passed.
- The SQL collector initially generated a 500-character title ending in a space;
  the native constraint correctly rejected it. Only the inert generator changed.
- The paired harness initially failed during fixture setup for the same trailing
  whitespace reason, before either HTTP server or any timing sample. A non-space
  suffix corrected its opt-in fixture. No completed comparison was discarded.
- One earlier passing DB run had a nextest LEAK annotation for an authority-loss
  test. No surviving test process was found; its isolated nextest follow-up passed
  in 2.957s without the annotation. The later full run also had no such annotation.

Focused source, conversion, role/tenant, receipt, local-edit/tombstone, private
permit, retry, lease/cancel and sibling-budget suites passed. R1's six targeted
regressions passed. The populated-upgrade test passed: actual migrations through
010f1, ordinary notes/open/completed tasks and real retained snapshot/preview/
proposed-People rows survive the new migration unchanged across 21 old tables;
ordinary Person detail remains complete. Its one inert cancelled metadata row
proves preservation only, not legal metadata-child execution.

## Actual queries and one paired reader comparison

The [measurement evidence](../design/qa/slice-010f2-2026-09-11/measurements/README.md)
retains actual SQL, EXPLAIN plans, source hashes and all paired raw samples.
All 92 final probes passed row bounds with zero temporary spill blocks.
Measurements exposed avoidable whole-plan work, which root corrected by adding
matching indexes and starting selective filters from immutable issue projections.
Missing negative detail lookup dropped from 2,504 shared blocks to 2; rare issue
filters to 76; empty filters to 1 with zero parent-row lookups. The focused real
result-filter regression and final collector passed. Only the new unreleased
migration changed; the disposable QA database was recreated, never checksum-patched.

The query book has 25,000 People and 50 members, with 25,502 notes and 51,003 tasks.
One Person has 503 notes (501 at 10,000 characters), 503 open tasks and 502 completed
tasks. Each family traverses in 11 pages, no duplicates, at most 50 items, maximum
47,304 response bytes; full-note reads return 10,000 characters. Readers add zero
source calls. The real small source path and inert bulk ciphertext/cardinality
seeds are explicitly distinguished; this is not large-source qualification or
hardware capacity evidence.

The one actual paired operational Person-detail comparison passed in 20.88s.
Both arms share the executable, fixture, HTTP/authentication stack and clock;
three affected function bodies are frozen from `cd3b010`, with independently
verified provenance. Five warmups and 40 measured requests per arm alternate
AB/BA without overlap. All 90 responses are HTTP 200, complete, byte/JSON equal
at 33,955 bytes, with expected entry counters. All fixture counts remain unchanged.
Baseline/current p95 are 14.145/15.906ms; current passes the specified 39.145ms
limit (baseline p95 + max(25ms, 10%)). This is not a full historical-binary or
concurrency/capacity comparison.

## Real API, production Web and exact reconciliation

All 18 browser phases passed. The [runtime record](../design/qa/slice-010f2-2026-09-11/runtime/README.md)
links the safe trace, screenshots, failed harness attempts and exact audits.
Prerequisites were three fresh Organizations, named admins/members, inactive
historical actors and helper admins, created through ordinary invitations and
retained snapshot/People-import commands. No activity row was directly seeded.
The initial synthetic reader counter of 201 was frozen; every activity epoch
added zero source calls and zero publications, including after source disconnect.

| Case | State | Native notes | Native tasks | Immutable results |
|---|---|---:|---:|---:|
| Complete | Completed | 54 | 7 | 70 |
| Partial cancellation | Cancelled | 27 | 7 | 42 |
| Storage recovery | Completed | 54 | 7 | 70 |

The exact read-only oracle checks complete bodies/titles, Unicode codepoints,
source-qualified IDs, Person binding, author/creator/assignee/completion-actor
independence, microsecond times, both DST-derived UTC instants, identities and
results. Complete cases have exactly 61 applied and 9 held results. Cancellation
retains a strict settled subset. Parent, workspace and sibling full-row hashes
match their pre-activity baseline. All 25 unrelated business tables per
Organization are unchanged, including Today, Operator and communication state.

All 12 durable activity stores were independently recomputed: measured and
retained totals match; native ledger costs equal result-native costs; child,
snapshot and Organization reservations are zero. Native cost and physical
relation allocation are reported separately from logical retained allowances;
repeated Unicode compression and fixture allocation cannot support per-agent
storage estimates.

The browser proves explicit mapped/unmapped roles, kind and timezone choices,
dirty-choice discard, source-only acknowledgement, readable HTML and exact-source
inspection, held-subset confirmation, real lost-response UI replay, native paging/
full content/provenance/reload, stale cursor rejection, cancellation, budget
pause/increase/explicit Resume, source disconnect, member hold and admin demotion.
The lost response was dropped only after real server acceptance; the identical
UI retry returned 202. Root independently found one matching receipt and one
queued child with zero native work before draining. A private harness had awaited
200 incorrectly; recovery added no confirmation request.

Cursor evidence separates the actual authenticated limit-25 cursor returning
409 from the unchanged Web default-50 focus refresh clearing pages/open content.
The storage probe lowered deployment headroom to exactly 19,262,579 retained plus
reserved bytes and paused with zero native rows. Restoring the deployment ceiling
and explicitly increasing the run allowance from 256 to 512MiB did not resume it;
only explicit Resume did. It does not prove exhaustion of the original allowance.

Desktop and 390px screenshots were visually inspected. Settled full-note/source
and confirmation dialogs fit; later loaded native/paging images replace early
loading captures. The uncertain-retry transition image and initial DST viewport
limits are qualified. No blocking layout issue was found. No page errors or
external requests were recorded; expected login/authority/barrier/network-abort
console errors are retained rather than called zero.

## Compatibility and observed telemetry

The guarded API executable SHA256 is
`784117db51cabcff436737ab4031d51c4525048111179e6f44f17bf89fbe479b`.
An independently hashed historical 010f1 binary was never launched. Its actual
DB/hash preflight allowed launch before activity and rejected launch afterward
with three durable activity bindings, including the cancelled child. Both probes
inventory only the quiesced disposable QA target, not the shared or production fleet.

A concurrent read during confirmation returned 503 because the exclusive
workspace barrier raised `workspace_busy`; it succeeded after confirmation.
The private driver now awaits the actual confirmation response before its next
read. This exclusion was recorded, not hidden as a successful initial attempt.

The API also emitted transaction-state notices during cancelled/aborted browser
requests. Inspection of pinned SQLx 0.8.6 identifies possible begin/drop cancellation
paths, but the notices lack enough request/backend correlation to establish the
historical cause. One observed live transaction snapshot had no transaction older
than 0.102s; all final native/business/ledger audits pass. These facts do not prove
every historical notice benign. The [bounded dependency report](../design/qa/slice-010f2-2026-09-11/runtime/sqlx-cancellation-dependency-report.md)
and [production-readiness follow-up](../plans/PRODUCTION_READINESS.md#observed-transaction-cancellation-notices)
retain the possible paths and attribution limits; no transaction-manager redesign
or dependency upgrade is claimed by this slice.

## Closure and remaining scope

Final source-hash, private-credential, content-log, whitespace/link and
owned-resource cleanup checks passed. The [log audit](../design/qa/slice-010f2-2026-09-11/runtime/runtime-log-audit.json)
found zero occurrences of five synthetic content markers across four positive
API startup controls; it retains the eight no-transaction and three already-in-
transaction notices. This is a scoped synthetic-content check, not certification
of arbitrary production logging. Explicit evidence inventories record copied
file hashes and credential scans.

The [cleanup audit](../design/qa/slice-010f2-2026-09-11/runtime/cleanup-reconciliation.json)
records stopped API/Web/browser processes and removal of the two owned containers,
their PostgreSQL volume and four browser profiles. Main's eight planning-file
hashes and HEAD, shared container identities/start times/ports/mounts and the
existing 010f1 release were preserved. The worktree and private evidence remain.
The [closure check record](../design/qa/slice-010f2-2026-09-11/checks/closure-checks.json)
records the final source, link, whitespace and credential verification.

Live FUB validation remains user-deferred. Synthetic evidence does not authorize
customer data, close privacy/erasure or production-readiness gates, activate the
workspace, or qualify remaining source families. Release integration and deployment
remain separate work. The implementation stays uncommitted in its worktree.

Subsequent D-068 follow-up authorizes integration and shared-development release.
The statements above describe the completed implementation checkpoint; subsequent
Git, cleanup and runtime results belong to [the release record](SLICE_010f2_RELEASE.md).
