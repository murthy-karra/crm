# Slice 010f3 — verification record

Implementation remains in progress on `codex/migration-010f3`. The bounded admitted
execution milestone is implemented and tested; staged preparation now has focused
DB and plan-shape evidence. Frozen transport and durable lifecycle receipts have focused functional evidence. Exact remainder, Web/browser acceptance, independent implementation
review and final integrated gates remain required. No review round has been used.

## Implemented execution checkpoint

`c5ae1d2` replaces the scaffold with typed catalog/Person units and exact accounting;
`e822241` integrates coordinator readiness `75b40fd`; `4dd5e69` adds the native
baseline/permit/rollback proofs; `ff19365` preserves captured choice order and
corrects the bitmap-index assertion. Migration `20261002000002` supplies unique
unit receipts/checkpoints and tightens the table/claim/Person/lease allowlist.

The admitted worker uses 010f1 Text/Number/Date/Choice interpretation, creates a
new choice field with its bounded options atomically, uses explicit option
mappings, and never overwrites/adopts changed/equal local cells or resurrects
removed tags. Native rows, shared claims, immutable encrypted unit results,
checkpoint and exact owner reservation settlement commit together. Catalog and
Person work advance through persisted keysets. Compatible original catalog
writers create/consult the shared claim in their native transaction. Preparation
now binds the selected report's actual snapshot/sequence and uses the same
`tag-group`/`tag-raw`/field/option HMAC purposes as 010f1.

## Executed evidence

All DB commands use the serialized private runner:
`python3 /private/tmp/crm-mobile005-010f3/run-db-check.py --lane migration --log NAME -- cargo test -p crm-api --test all FILTER -- --ignored --test-threads=1`.
The runner supplies isolated credentials and target
`/private/tmp/crm-mobile005-010f3/migration/target`; SQLx creates disposable synthetic
test databases. Logs below live in that same lane evidence directory. No shared
service build artifacts, source account, deployment or native QA stores changed.

| Check | Result | Tested source and evidence |
|---|---|---|
| `cargo check -p crm-app` | passed | Execution implementation compile: `worker-check.log`, `worker-check2.log`, `worker-check3.log`. |
| `cargo clippy -p crm-app --all-targets -- -D warnings` | passed | `admitted-worker-clippy2.log`; first `admitted-worker-clippy.log` reported explicit-scope helper argument-count lints, resolved with the established narrowly documented allow attributes. |
| Preparation, typed values, atomic catalog failure | 2 passed / 1 assertion failed, corrected | `admitted-worker-db1.log`: native decimal was correctly `123456.1250`; test incorrectly expected six displayed fractional digits. No native truncation/rounding failure. |
| Five admitted DB tests | passed | `admitted-worker-db2.log`: all four types plus tags, source-free execution, exact byte deltas/replay, unchanged Person/source identity rowsets, frozen equal/different/appeared/removed cells, catalog rollback/retry and missing-index readiness rejection. |
| Six admitted DB tests | passed | `admitted-worker-db3.log`: additionally proves wrong table/claim/unit/Org/lease and cancel fencing, and injected result failure after both catalog **and Person** native writes with zero partial native/claim/result/checkpoint/ledger changes. |
| Seven admitted tests including D-050 cardinality fixture | passed at `ff19365` | `admitted-worker-db5-perf.log`; command adds `--features perf-harness` and `--nocapture`. Prior `admitted-worker-db4-perf.log` had 6 passes and one test-helper failure because its index detector did not recognize a Bitmap Index Scan beneath a Bitmap Heap Scan. The observed production plan was indexed; helper corrected before rerun. |
| Exact original/admitted shared-claim reuse after readiness | passed at `1d2d2b7` | `admitted-original-claims-db3.log`; first two attempts paused before the additive identity-key guard correction. |
| `cargo fmt --all`, `git diff --check` | passed at checkpoints | Owned backend/test/document changes; final-tree formatting is still a coordinator gate. |
| Original metadata private-gate suite after shared claims | passed at `1d2d2b7` | All six in `original-metadata-composition-db.log`. |

The old stale migration-grant blocker is resolved: the current isolated tests
apply `20261002000002` and can read immutable original metadata identity evidence.
No UPDATE privilege or row UPDATE lock was added to that immutable table.

## D-050 execution-query evidence

`admitted-worker-db5-perf.log` records six exact production SQL statements with
`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` on a 25,000-Person/50-member synthetic
book with four typed values and one tag per admitted Person. The enlarged rows
are explicitly inert cardinality fixtures, not billable/processable encrypted
execution input; real preparation/execution is separately proved on real retained
synthetic captures.

The late Person query seeks directly beyond the persisted checkpoint through
`migration_admitted_metadata_manifest_work` (one row, five shared hit blocks,
zero filtered rows). Person operations use their unit index; receipt probes use
the unique root/Org/unit index; native value/tag baselines use their composite
primary keys. The seven-row catalog naturally receives a small sequential scan,
with seven indexed receipt probes instead of scanning all Person results.
Measured times are reported in the log, never used as absolute laptop gates.

The copied manifest/operation/result retained-column sum is **49,773,796 bytes**;
those three physical relations total **124,837,888 bytes** in this fixture. These
are different measures and exclude other evidence/native tables; neither number
is a customer quota or production capacity prediction. Preparation query evidence is recorded below. Coordinator owns the one paired
Person/Today regression after integration; this lane did not duplicate it.

## Remaining implementation and evidence

The staged checkpoint adds schema `20261002000004`: source observations are
checked in capture/ordinal order, at most 100 indexed records per step; fields
and options use one descriptor per step; tag spelling follows captured source
order; cohort manifests use admission-result keysets and retain missing targets;
Person field operations advance through bounded mapping keysets. Each step
reserves at most 64 MiB, accumulates exact charged-column deltas without rescanning
the settled prefix, then commits its lease/checkpoint and all three ledgers
atomically. Handover readiness commits before later preview work. Typed list,
detail and plan readers require current tenant admin authority.

`admitted-preparation-db1.log` passed the eight existing admitted tests after the
initial staging refactor. `admitted-preparation-db2.log` passed one and failed two
new fixture assertions: an empty custom-field page advanced before the injected
source fault, and upstream provenance FKs prevented synthetic Person deletion.
The corrected tests inject the first nonempty step and deliberately remove only
test-database native-reference constraints while preserving immutable evidence.
`admitted-preparation-db3.log` passed all four focused preparation tests, including
the seventeenth conflicting observation and exact failed-step rollback/retry.

Its six D-050 preparation plans use a 25,000-Person/50-member inert cardinality
fixture. Capture, record, source-field and capture-ordered tag lookups use their
indexes; the four-field mapping query scans its seven-row catalog. Inspection
found that the cohort's correlated EXISTS hashed all 25,000 identities despite
its indexed outer keyset. The changed lateral LIMIT 1 query in
`admitted-preparation-db4.log` instead uses `migration_admitted_metadata_admission_work`,
`person_pkey` and `migration_import_identity_target`: one result row, 9 shared
hits and 2 reads (observed 0.05 ms, not an absolute gate). The test originally
required the identity PK specifically and rejected this equivalent bounded
index. Its actual plan is retained in
`backend/crates/crm-api/tests/fixtures/admitted_metadata_cohort_plan.json`; the
corrected assertion is verified against that recorded plan, without another
EXPLAIN. The other five unchanged preparation plans reuse db3 evidence. The old
execution performance case is skipped in subsequent functional checks.

`admitted-preparation-check2.log`, `admitted-preparation-clippy.log` and
`admitted-preparation-clippy2.log` passed. `admitted-preparation-check.log` records
an initial command launched outside the backend Cargo workspace. `admitted-preparation-db5.log` passes ten functional/recorded-plan tests; its
new cross-tenant fixture omitted required Organization intake fields. The
intermediate `admitted-preparation-db6-admin.log` records the second missing
field; the complete corrected fixture passes in
`admitted-preparation-db7-admin.log`. Together these establish all eleven
functional/recorded-plan checks on the staged implementation. No application
change was needed for the fixture corrections. The db5 command used
`--include-ignored` and skipped both EXPLAIN fixtures; db7 selected only the
current-tenant-admin reader test.

The frozen transport is now implemented for headers, sources, mappings, aliases,
records, results, issues, segments and Person provenance; confirmation checks the
counted revision/digest/acknowledgments. Durable preparation, replan, confirm,
retry and cancel receipts retain full response envelopes. Exact remainder is the
remaining transport/lifecycle implementation. New reader hot-query plans and
final integrated payload/retention checks remain required.

Required remaining tests include failed handover rollback and rerun accounting,
old already-running writer rejection, multi-cohort collisions/tombstones/archival,
source variants/incomplete evidence, budget/key/expiry failures, full-tag-set
limits, HTTP no-store and source-free reader negatives, desktop/390px browser
workflow and preservation reconciliation. Final `scripts/check`,
`scripts/sqlx-prepare`, `scripts/check-db`, cross-slice upgrade checks remain coordinator-owned and sequential. The coordinator
completed the single required same-build Today pair and eleven Mobile statements
at `83f4fc8`; see `MOBILE_005_010f3_PERFORMANCE.md`.


## Immutable replacement plans and cancellation capacity checkpoint

The canonical replan request now starts a distinct building plan, with at most
50 explicit patches. A persisted `(kind,id)` choice step inherits prior choices
by exact source key, freezes new destination/claim baselines, and precedes cohort
baselines. Changing reports starts fresh choices. Plans own snapshot/report/output
revision/capture bindings, and receipts retain their own snapshot for decryption
after a later source change. Earlier mapping evidence is retained unchanged.
Cancellation capacity is reserved at initial preparation, moved transactionally
when selecting another snapshot, and spent directly by cancellation; cancellation
does not need a new allowance at an exhausted Organization limit.

`admitted-replan-check.log` and `admitted-replan-check2.log` passed. The first
functional database run, `admitted-replan-db1.log`, passed all ten existing tests.
The second run passed source-change/inheritance/replay, but exposed two fixture
expectations: the immutable original snapshot allowance cannot be lowered, and
preparation now retains 64 KiB cancellation capacity. Its broad feature filter
also selected the separate UI server's opt-in guard, which correctly rejected
running without its explicit harness setting before creating that fixture. Later
runs use `db_admitted_metadata::` and exclude both unchanged performance tests.
`admitted-replan-db3.log` verifies twelve tests; its cancellation proof found an
actual SQL INT4-to-i64 decoding error in receipt byte measurement. Both query and
proof now explicitly use BIGINT. The focused corrected cancellation result and
new choice-query D-050 evidence are recorded with the final checkpoint below.

`admitted-replan-db4-cancel.log` passes the corrected protected-capacity test;
together with db3 it establishes all thirteen functional/recorded-plan checks.
`admitted-replan-clippy.log` passes `cargo clippy -p crm-app --all-targets --
-D warnings`. `admitted-replan-db5-plans.log` runs only the three new choices
statements on the 25,000-Person/50-member fixture with about 25,000 mapping rows.
The late mapping keyset uses `migration_admitted_metadata_mapping_prepare`, one
row with 3 shared hits and 1 read. Exact inherited-source lookup and destination
uniqueness each return one row with 4 shared hits. No settled-prefix scan occurs.
The source/observation inventory contains 25,003 rows each: 18,501,713 source
logical bytes plus 800,096 observation logical bytes, versus 40,984,576 physical
relation bytes. These are explicitly inert cardinality copies, not billable
execution or a production quota projection. Existing execution/cohort SQL proof
is reused; no unchanged EXPLAIN or paired Today benchmark was repeated here.

The focused runners use the migration target and serialized database helper in
`/private/tmp/crm-mobile005-010f3/`; all successful and failed logs remain under
its `migration/` directory. The tested code is the immutable-replanning commit
containing this verification update. Full frozen response/readers, counted
confirmation and exact remainder are still in progress; no final integration or
browser completion is claimed by this checkpoint.

## Frozen headers, readers and lifecycle receipts checkpoint

`20261002000006` adds bounded polling counters and source-spelling alias
references. Preparation now records the confirmed admission plan and complete
settled cohort count, uses the selected capture's actual start time for later
report qualification, and persists an actor/request-bound encrypted response.
Cancelled unconfirmed roots release the unique active cohort slot while their
plans and receipts remain unchanged. Confirmed roots retain uniqueness.

HTTP preparation returns `201`; plan replacement, confirm, retry and cancel
return `202`. Header/result/record/mapping/source/alias/issue/segment/provenance
DTOs follow the frozen contract. Pages use authenticated Organization/owner/
plan-generation/filter/endpoint cursors, at most fifty items, 128 KiB per item
and 512 KiB envelopes. Segments preserve UTF-8 boundaries. Confirm binds revision,
digest, workspace and counted acknowledgments; receipts include their own exact
three-ledger byte charge and replay before fresh capacity/readiness checks.
Configured policy ceilings reach both HTTP operations and the production worker.

`admitted-transport-db1.log` passed thirteen existing functional/recorded-plan
checks after the full header/counter/receipt implementation. On the expanded
transport tree, `admitted-transport-db2.log` passed all fifteen, including fresh
preparation after unconfirmed cancellation and lossless source segments, distinct
mapping pages, cross-endpoint/filter/stale-plan cursor rejection, provenance and
terminal cached counts. Both skip the two unchanged D-050 fixtures. The API test
check first found a fixture-only attempt to clone the non-Clone page DTO in
`admitted-transport-check6.log`; constructing independent requests corrected it.
Earlier `admitted-transport-check1.log` found the large JSON macro recursion
limit; splitting the bounded header object resolved it. Check4 found an Arc
reference type mismatch at the release argument, corrected with `as_deref`.
`admitted-transport-clippy.log` passes app all-target Clippy with warnings denied.

The final issue-counter addition retains cohort/mapping reasons, charges each
new issue code once, and uses a column-limited count-update grant. A populated
original handover INSERT's extra eleventh bind was removed; the coordinator owns
its new real populated-handover failure/rollback/rerun and legacy lease proofs.
The coordinator also owns outer HTTP/admin/no-store/body-size tests, readiness
inventory, desktop/390px browser acceptance and final integrated gates. No full
slice completion or independent review pass is claimed by this checkpoint.

The final issue-counter/readiness transport tree compiles and its missing-native
cohort proof passes in `admitted-transport-db3-issues.log` (one test, 5.19s).
This explicitly checks a nonempty bounded issue response after the retained hold.

## Exact remainder and legacy permit checkpoint

`20261002000007` and the remainder worker add an automatically queued, unique
successor for a cancelled confirmed run. Mapping dependencies reference original
committed result IDs; immutable source evidence remains with its original plan.
Each remaining catalog mapping and never-settled Person baseline/operation is
copied under a persisted keyset in bounded transactions, re-encrypted for the new
owner without refreshing source, choices or native comparisons. Copying itself
can be cancelled/retried; cancelling a partly copied successor retains the full
original remaining scope for the next successor. Successful/held terminal People
are excluded with cached exact counters. Source/result readers preserve original
evidence provenance through these references.

The final missing route, `POST /{id}/remainder`, now returns `202` and a durable
actor-bound response receipt. Exact replay precedes fresh readiness/capacity.
`admitted-remainder-check1.log` passes the API test compile. Both focused real-DB
cases pass in `admitted-remainder-db1.log` (2 tests, 11.94s): partial Person cancel,
cancel during copying, unique three-Person settlement across the chain, retained
catalog dependency reuse with no new claims, unchanged predecessor results, and
an equal Text cell appearing after the original confirmation held under the
original baseline while the other three typed cells apply.

The coordinator's unchanged legacy permit regression exposed SQL's unspecified
boolean evaluation order when an empty JSON proof is present before readiness.
The additive migration now parses only within an explicit ready branch, catches
malformed JSON, and uses the parsed value for containment, preserving the 000003
identity-key helper. The unchanged test passes in
`admitted-remainder-db2-legacy.log` (1 test, 5.50s). This establishes actual valid
pre-handover legacy behavior and rejection of its old proof after handover.

Remaining implementation/verification: actual Person fan-out bounds and visible
oversized evidence holds, explicit undeclared custom-key and duplicate machine-name
outcomes, new reader/remainder SQL plans at D-050 size, full budget/key/expiry/source
variant matrix, coordinator browser acceptance and final integrated review/gates.

## Final evidence bounds and source fidelity checkpoint (in verification)

The additive `20261002000008` introduces the manifest's `oversized` flag and a
32-byte blind field-name key with a scoped partial index. Preparation now checks
capture descriptors before fetching ciphertext: captures beyond 4 MiB yield a
visible retained evidence hold without opening the body. Missing/invalid retained
representations remain visible; duplicate source machine names hold both field
mappings, and undeclared custom keys become counted held value operations.
Choice options reference their complete immutable parent definition rather than
copying it into every option envelope; compatible claim comparisons and readers
resolve that exact parent, retaining the original 010f1 type/choice fidelity.

Native baseline size is counted with indexed row aggregates before constructing
JSON. An oversized native baseline freezes a compact hold; oversized derived
Person work retains its staged evidence and settles compact grouped counts with
no native mutation. Regular Person bounds include actual baseline/operation
bytes. Catalog bounds include option children only for an atomic create-matching
field. The per-Person target alignment runs once after all field batches, while
settled catalog units leave the pending execution index atomically with results.

`admitted-fidelity-check1.log` and `check2.log` passed API test compilation.
`admitted-fidelity-db1.log` passed 20 functional/recorded-plan checks and failed
one newly added native stress seed because its 4,000-character Text value violated
the native 500-character limit. The corrected valid 35,000-cell Unicode seed
passes in `admitted-fidelity-db2-native-bound.log` (1 test, 63.10s), proving a
visible >64-MiB baseline hold, exact grouped outcome counts and every native cell
unchanged. The first pass also covers a deliberately unreadable oversized capture,
duplicate/undeclared keys, 120-option normalized evidence, typed execution,
original claims, atomic rollback and exact remainder.

Final cardinality/SQL evidence and the expiry/current-policy/key retry regression
are still running. The first final D-050 fixture attempt failed a test-only
predecessor FK seed before any EXPLAIN; `admitted-fidelity-d050-final1.log` is
retained. Cardinality envelopes are inert synthetic copies, never executed or
reported as a billable migration run. The coordinator separately owns and has
reported passing real browser acceptance, the outer HTTP denial matrix and
readiness variants; those checks are not duplicated in this lane.

The final functional implementation passes all 21 focused functional and
recorded-plan checks in `admitted-fidelity-db3-functional.log` (111.08s), skipping
only the two previously proved D-050 fixtures and the unchanged 35,000-cell
stress test whose dedicated pass is recorded above. This includes the complete
normalized source/mapping/operation/observation/issue/receipt/root/plan logical
byte inventory, expired confirmation rejection with no charge, current-policy
reduction rolling back the entire next unit and reservation release, wrong-key
pause with no effects, and explicit retry preserving the confirmed plan verbatim.
App all-target Clippy with warnings denied passes in
`admitted-fidelity-clippy.log` (8.74s).

`admitted-fidelity-d050-final2.log` records fifteen actual new/changed hot SQL
plans at 25,000 People/50 members. Descriptor/name lookup, sparse pending catalog,
native/operation bounds, filtered readers, provenance, alias and remainder use
scoped indexes, with execution times 0.013–0.246ms. Its final assertion expected
one particular operation-read index, but PostgreSQL correctly chose the existing
manifest/Organization/kind/source unique index and sorted only five scoped rows
(0.039ms); the assertion was widened to accept that valid plan. Tail-only evidence
continues with the predecessor lookup, a nonempty late remainder-mapping probe,
one-time seal bound and logical/physical inventory; the earlier unchanged plans
are not repeated.
