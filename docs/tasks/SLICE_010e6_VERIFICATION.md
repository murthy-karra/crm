# 010e6 — Verification

**PASS — implementation verification complete, not a release record.** Branch `codex/010e6-recovery`, base
`512bc4a`. Primary owns all evidence below. Production/shared development and
installed native stores are not modified by these checks.

## Isolation

Rust `CARGO_TARGET_DIR=/private/tmp/crm-010e6-target-20260915`; separate SQLx target
`/private/tmp/crm-010e6-sqlx-target`. Production Web output is
`/private/tmp/crm-010e6-web-dist-20260915`. Database tests run serially in synthetic
SQLx databases; the separate migration inventory database is
`crm_010e6_schema_20260915`. Browser fixture is synthetic database `crm_010e6_qa`,
API 3107 and Web preview 5187. No live FUB request or shared-runtime replacement.

## Verification evidence

- Independent implementation review: READY, final round 2. See
  [review](SLICE_010e6_IMPLEMENTATION_REVIEW.md); no third review.
- Twelve recovery DB checks passed, including original/admission-held creation,
  exact healthy/missing identity, immutable hold history, HTTP role/tenant/target
  denial, bounded authenticated cursors, receipt replay, long stage-key fragments,
  crash rollback, expired lease takeover, partial cancellation/exact remainder,
  capability/schema fences and every follow-on family. This run precedes final
  stale-preparation and exact-acknowledgement/capacity assertions.
  `/private/tmp/010e6-recovery-final.log`.
- Two production mapping-page EXPLAIN probes passed on 25,000 inert candidates,
  keys and choices, including a late page. Choices use the key/version index and
  both mapping and choice probes visit fewer than 100 rows. This is index-shape
  evidence, not worker throughput or production latency.
  `/private/tmp/010e6-query-plans.json`.
- Web focused family tests: 81 passed. Typecheck and lint passed.
  `/private/tmp/010e6-web-family-tests.log`, `/private/tmp/010e6-web-types3.log`.
- `scripts/check` passed in 124 seconds: 995 Rust tests, 1,312 Web tests,
  55 preflight tests, formatting, Clippy, production-shape check, crate graph
  fences, doctests, production Web build and email-worker checks.
  `/private/tmp/010e6-check-final.log`. Later edits are ignored-test fixture and
  assertion corrections; the production Rust/Web tree is unchanged.
- SQLx prepare `--check --workspace` passed against the isolated complete schema.
  It retains the existing potentially-unused-cache warning; no cache was deleted.
  `/private/tmp/010e6-sqlx-prepare.log`.
- Broad serial database run: 72/75 passed initially. Three failed checks were test
  setup/expectation issues described below; corrected reruns cover them separately.
  `/private/tmp/010e6-db-broad.log`. Eight subsequent checks passed, including
  native/imported history scope/counts/cursors, a 25k contact-detail plan, exact
  unsupported-engine rejection and recovery HTTP scope/cursor/replay checks.
  `/private/tmp/010e6-db-final-corrections.log`.
- Corrected metadata 25k fixture passed (46.190 seconds), completing all three
  original broad-run failures through corrected reruns. Person-detail pair also
  passed (91.069 seconds); this initial invocation omitted the opt-in Today flag
  and is not combined Person/Today evidence. `/private/tmp/010e6-paired-and-metadata.log`.
- Final standard workspace/all-target Clippy passed after test-only corrections,
  38.88 seconds. `/private/tmp/010e6-clippy-final.log`.
- Real production-Web browser walkthrough passed against synthetic API 3107 and
  Web 5187. At 390×844, mapped two original holds, built the full preview and
  cancelled it; both items remained cancelled with zero settled. At 1280×900,
  prepared a fresh recovery, mapped Later to Lead and agent 3 to explicitly
  unassigned, inspected name/email/contacts, acknowledged exact counts (2 People,
  1 contact, 2 unassigned), confirmed and observed 2 settled. Reload preserved
  completed results and Person links. Person detail showed Lead, Unassigned,
  the synthetic email and “Person recovered after mapping hold,” with review
  hold still active. Metadata handoff selected the exact recovered cohort and
  left preparation separate; history handoff required a qualified capture.
  Logout returned only the sign-in form. Browser viewport reset and QA tab closed.
- Combined Person/Today run with `CRM_MOBILE006_PAIRED_TODAY=1` passed in
  96.427 seconds. This single complete combined run was required because the
  first invocation omitted Today. Person p95: baseline 17.085 ms, current
  20.550 ms, limit 42.085 ms; complete response equality passed. Today p95:
  baseline 57.961 ms, current 59.418 ms, limit 82.961 ms; all DTOs equal and
  completion passed. `/private/tmp/010e6-paired-complete.log` and
  `/private/tmp/crm-010e6-person-today-complete/`. These are isolated fixture
  comparisons sharing the current authorization stack, not production latency.

## Failures caught and corrected

Test development exposed PL/pgSQL name ambiguity in the refresh approval guard
(`kind`) and admission-anchor trigger (`old`). Both were renamed. The first lease
fault-injection setup correctly hit the new compatibility fence and now stamps its
explicit synthetic transaction. A missing-identity corruption fixture cannot use
superuser replication settings under the migrator; it temporarily disables only
that synthetic table's user triggers. Long stage labels exposed an overly strict
catalog qualification check: creation-only restrictions now preserve explicit
mapping to existing stages, matching the existing parser contract. These earlier
failures are not represented as passing evidence.

The broad gate also found an old metadata scale fixture copying a successful
admission result onto unrelated synthetic People, which D-085 correctly rejects.
It now creates inert matching admission item/result tuples for each scale Person,
restoring the temporary item-build guard before EXPLAIN. No application integrity
constraint was weakened. The unknown-engine test now removes the replacement
mode/engine constraint during its rollback-only fault injection. The first CSRF
assertion incorrectly expected origin interception (403); the established policy
uses SameSite=Lax cookies and JSON-only commands. The corrected test proves a
cross-site simple `text/plain` request is rejected without a mutation or CORS grant.
The full gate also required updating the history inventory assertion from 12 to
13 supported fact types. Failures are retained in the preceding logs.

An additional, nonstandard all-target Clippy invocation with `perf-harness`
failed on unused helpers in standalone pre-existing performance binaries
(`/private/tmp/010e6-clippy-extra-perf-failed.log`). The repository's standard
Clippy command omits that feature and passed; the required aggregate performance
binary compiles and executes separately. No unrelated helper suppression was added.

## Tested source and cleanup

The uncommitted source tree is inventoried in
`/private/tmp/crm-010e6-tested-source.json` (42 changed/new backend, Web and script
files; SHA256 `c0d49eef46be505144fe04f38e886a2450818998b8c9a8a55c161bdb1f3505a6`).
Final `git diff --check` passed. QA API3107 and Web5187 listeners stopped;
browser tab closed and viewport reset. Synthetic databases and private evidence
remain retained. No commit, merge, push or deployment was performed.
