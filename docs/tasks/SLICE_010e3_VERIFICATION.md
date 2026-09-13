# Slice 010e3 verification evidence

## Isolated PostgreSQL lifecycle

`db_people_admission_execution::retained_new_people_admit_with_native_identity_provenance_and_facts` creates a real synthetic 010c review parent, captures a newer People stream, builds a real 010e1 report, then uses the admission command and worker. The fixture includes two original imported People, one original held Person, and one new Person with an email and phone.

The test verifies the sealed preview has exactly one eligible new source ID; exact confirmation settles it into one new native Person with two contacts, one global admission-origin identity, encrypted admission provenance, and one `person_admitted` fact. It also verifies the admitted Person has no Inquiry. The worker is driven through all four retained-source qualification phases before it builds the plan, so it does not fabricate a plan or result.

Passed 2026-09-13 against the disposable synthetic migrator database:

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all \
retained_new_people_admit_with_native_identity_provenance_and_facts \
-- --ignored --nocapture
```

This run exposed and corrected live errors in the original snapshot-boundary join, admission ledger numeric cast/release bind, planned contact permit binding, and original mapping revalidation.

The expanded focused suite passed on 2026-09-13 with five real PostgreSQL tests. It covers retained-source qualification and native settlement, original/identity/mapping holds, same-report cancellation and remainder preparation, a lowered live policy that pauses before any native write while preserving the cancel reserve, expired-lease fencing, and initiator-demotion fencing. The settlement test compares the live retained-byte ledger with the independent persisted-byte audit.

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all 'db_people_admission_execution::' \
-- --ignored --nocapture

5 passed; 0 failed
```

## Identity integrity, immutability, and hot-plan claim

`identity_tombstone_and_sealed_plan_items_are_immutable` uses a nil native identity target to model inconsistent or missing target identity evidence. It verifies the source is held rather than admitted, then selects the confirmed plan's exact `source_id=106` item and proves that a sealed plan cannot return to `building`, rewrite an item, or add a contact.

`hot_plan_25k_uses_sparse_eligible_claim_index` inserts 25,000 building-plan items with 50 eligible rows and 24,950 held rows. After `ANALYZE`, its real worker claim query (`admission`, `organization`, `plan`, eligible/unsettled, `ORDER BY id`, `FOR UPDATE`, `LIMIT 1`) uses `migration_people_admission_item_eligible_claim`; it therefore does not scan the held rows to claim the next eligible unit.

Both focused PostgreSQL tests passed on 2026-09-13:

```text
DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all \
identity_tombstone_and_sealed_plan_items_are_immutable \
-- --ignored --nocapture

1 passed; 0 failed

DATABASE_URL="$MIGRATION_DATABASE_URL" SQLX_OFFLINE=true \
CARGO_TARGET_DIR=/private/tmp/crm-010e3-target \
cargo test -p crm-api --test all \
hot_plan_25k_uses_sparse_eligible_claim_index \
-- --ignored --nocapture

1 passed; 0 failed
```

## Coordinator contract and atomicity checks

On 2026-09-13, the four `db_people_admission_contract` PostgreSQL tests passed
against genuine retained-source fixtures. They exercise the real executable
fingerprint/release-report reader (wrong capability, role, artifact, database,
expiry, gate and partial schema all fail closed), scoped application-role permits,
complete 56-contact and 63-KiB Unicode preview/provenance traversal, opaque cursor
scope, and the independent retained-byte accounting audit. A trigger-injected
failure at `person_admitted` rolls back the preceding Person, contacts, identity,
result and provenance writes. Retry commits one set, releases reservations, and
an exact lost-response confirmation replay returns the original receipt.

The additional HTTP regression
`admitted_person_review_http_includes_retained_partial_results_and_enforces_scope`
passed (1 test, 6.69 seconds). It opens both original and newly admitted People
through the production v1/v2 review, timeline, fact detail, notes, tasks and
inquiry routes after partial cancellation. Admission history has one admission
fact and zero Inquiries; ordinary members and a foreign Organization cannot read
it. This caught and fixed the review binding that previously required an original
import result for every Person. Original bindings remain unchanged; the additive
binding checks the current workspace, global identity, sealed admission item and
exact committed result.

## Actual browser acceptance

The coordinator used the production Vite build on Web5174 with the production
Axum router/application commands/worker on API3103. The API is a **test harness**:
local synthetic credentials, fixture encryption keys, recording publisher and
explicit `ReleaseReadiness::for_tests()`. This proves the browser/API workflow,
not production startup readiness, deployment or live FUB source qualification.
The separate contract test above exercises the real artifact readiness reader.

The first browser run exposed a nested preparation-reservation collision before
any native admission. After fixing reservation release within the transaction,
the workflow passed. Later browser checks found missing intended CRM values,
stale item dispositions during settlement and the Person review binding gap;
all were repaired and exercised again. The earlier isolated database is retained
as `crm_010e3_before_review_fix`, with private reconciliation evidence.

The final browser runtime applies the combined fresh schema to `crm_010e3_qa`.
Its private copied test executable SHA-256 is
`155c9a739e38ad95f6a6ab89d9ca0c9babb6b16070603d84379b44bf4c4c29f0`.
The browser preview had six source groups: two eligible, one already imported,
one present at the original boundary, two held, and 57 intended contacts.
Actual confirmation, one worker unit, cancellation, Person review and a fresh
same-report remainder preview were exercised through the UI. Live item status
changed to **Admitted** without Reload. The cancelled run retained its committed
Person and immutable history/provenance.

Desktop (1280 CSS pixels) and 390-CSS-pixel browser layouts were visually
inspected. The retained `sourceUrl` was reconstructed exactly from four UTF-8
fragments (offsets 0, 16383, 32766, 49149; total 63028 bytes), both before
confirmation and from Person admission provenance. Ordered contacts traversed
0–49 then 50–55, with the final paging control disabled. The Person displayed
its admission fact and initial stage/assignment facts, zero Inquiries, and the
continuing administrator review boundary.

The earlier completed cancellation/remainder browser run produced exactly two
native admitted People, two identity/results/provenance/admission facts and zero
Inquiries; both runs released all reservations. Exact canonical row-byte MD5
fingerprints for original People, contacts, import parent, mappings and results
matched before and after. Private evidence and reproducible preservation SQL are
under `/private/tmp/crm-010e3-qa/`; no customer content or credential is committed.

## Paired ordinary Person read

The existing paired harness passed on a quiet machine with 25,000 People and 50
members, 40 measured requests and five warmups per arm. Baseline p95 was
21.402042 ms; current p95 was 22.497625 ms, below the 46.402042-ms allowed limit.
All paired response bytes, JSON, entry points and fixture counts matched.

The first measured run overlapped Rust compilation and failed its p95 gate
(55.60 ms baseline, 104.44 ms current, 80.60-ms limit). Its output is retained;
a quiet rerun was justified by that observed concurrent load, not discarded as
a pass. The harness preserves its three frozen `cd3b010` reader bodies. Its
shared helper manifest was updated to the unchanged pre-Mobile003 `619c1b3`
read-check hash; it does not claim to compare two complete historical binaries.
Final paired JSON is `person-paired-quiet/person-detail-paired.json` under the
private evidence directory. This is a regression check, not production capacity.

The final fresh-schema browser run also completed the remainder and opened both
new People and the original Person. Its reconciliation found two admission
identities/results/provenance/facts, 57 admitted contacts, zero Inquiries, and
three distinct People sharing the same email (including the original). Both
runs had zero reserved bytes. The same five original row fingerprints matched
exactly again. Full-profile reload succeeded and Logout returned the sign-in
screen with decrypted review content removed. The fixture and runtime are
isolated from API3000/Web5173/demoAPI3101 and installed ordinary native stores.

The four additional [adversarial PostgreSQL checks](SLICE_010e3_ADVERSARIAL_VERIFICATION.md)
passed: corrupted capture/ordinal/HMAC/original baseline, oversized IDs and
malformed contacts, stale or expired exact plans, and mapping invalidation
between confirmation and execution. These enforce holds/pauses before output.

## Final paging and database closeout

The browser executable above predates the final SQL-only paging/index refinement.
A 25k-row query-plan fixture exposed a full-plan scan for sparse disposition
pages. The read now resolves cancellation under the existing shared run lock,
uses indexed stored-disposition ranges, and merges at most two bounded ranges
for the virtual cancelled view. Public response and cursor contracts are
unchanged. The final regression traverses eligible, cancelled and settled views
with one-item pages, including a partially completed cancelled run.

All 15 affected admission database cases passed on the final query and schema.
Fresh-schema SQLx `prepare --check --workspace` and workspace Clippy with warnings
denied also passed. The complete regular database inventory has 1,009 distinct
passing cases: 136 from the initial run and 873 from a precise resumed selection.
That selection includes the repaired legacy contact timing test, which now
observes actual lock contention and uses one clock domain. The original command
did stop on that test; this record does not report the stopped invocation as a
successful full run. `final-gate-inventory.json` under the private evidence
directory records test counts and SHA-256 log hashes. The final 15 are a subset
of those 1,009 cases, not additional cases.

The final [25k-Person query-plan fixture](SLICE_010e3_QUERY_VERIFICATION.md)
passed on the final compiled source: 1/1 in 14.18 seconds. Earlier final setup
attempts timed out while OrbStack was suspended during macOS background sleep;
the passing run followed a temporary wake helper, without a service restart.
This plan proof verifies examined-row bounds, not production throughput.
