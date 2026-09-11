# Slice 010b — Implementation brief

**APPROVED FOR IMPLEMENTATION, 2026-09-11 (D-063).** The user approved the
reviewed specification, contracts and storage policy, then requested implementation.
Core-first sequencing is D-061. Live FUB validation remains deferred.

Specification: [SLICE_010b.md](../specs/SLICE_010b.md).
Application baseline: `0735015`, containing 010a, deployed to shared development
on 2026-09-11. Documentation baseline: main `a498a2c`, committed/pushed before
this approved revision. [010a release](SLICE_010a_RELEASE.md).
Source evidence: [public qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md).

## 1. Outcome and exclusions

Deliver a resumable, encrypted capture and deterministic preview for People,
users, stages, custom fields, notes and tasks. Every non-core family stays in
the report with an explicit not-captured/embedded-only state and next snapshot
work. Implementation and synthetic verification are authorized. Import, live-source
validation, runtime deployment and customer-data processing remain outside scope.

Exclude Person/Inquiry creation, stage/member/field mutations, contact-attempt
fabrication, actual merging, outbound communication, file/media fetching,
Operator tools, AI parsing, delta/cutover, erasure policy, new dependencies and
services. Do not silently change 010a's assessment response or probe profile.

## 2. Required reading and preconditions

- AGENTS.md; DECISION_LOG D-012–016, D-050, D-059–063 and O-012/O-013;
  architecture baseline; migration summary; approved version of the spec.
- Current migration reader, commands, crypto, store and worker under
  `backend/crates/crm-app/src/domain/migration/`; API routes and db_migration tests.
- Existing Person contact normalization, note/task/custom-field validators and
  import-ready provenance/tombstone rules. Do not reuse `contact::identify` to
  silently choose a duplicate winner.
- Current MigrationView session fences, direct credential submission and query
  lifecycle; scripts/check, check-db and sqlx-prepare.

The spec's allowance values/delegation are accepted under D-063. Before coding,
freeze endpoint pagination/representation profiles and exact
schema/HTTP DTOs, and apply 010a cross-job/fencing amendment pointers. Preserve
010a's existing retry-initiator behavior; the stricter source-initiator rule is
010b-only. See [the review record](SLICE_010b_REVIEW.md) for corrections and
independent review status under 04-review-plan.md.

The approved specification defines distinct source-work versus retained-read/preview
authority, semantic variants per representation, closed denial classification,
logical retained-byte reservations and revisioned budget increases. These are
approved contracts. No physical disk quota, automatic retention deletion or
production storage/backend selection follows from the synthetic-development
2 GiB/run and 4 GiB/Organization starting allowances. D-050's performance
envelope remains unchanged after removal of the draft entity-count stops.

## 3. Ownership and execution order

One primary implementation lane is sufficient. Create a
short-lived `codex/slice-010b-core-snapshot` branch/worktree from current main.
Approval is recorded in D-063. Follow MODEL_ROUTING for
the assigned implementation/review profiles; it does not itself start agents.

The implementation lane owns the sole additive database migration and necessary
generated SQLx metadata. Backend then Web, with a backend checkpoint before
Web starts. The coordinator owns project state, cross-spec pointers, evidence
and final integration. No concurrent writers to shared files or DB gate runs.

| Step | Work and completion evidence | Spec acceptance |
|---|---|---|
| 1. Qualify source requests | Pin page shapes, representation/comparison keys, lossless value encoding, next/offset semantics and denial matrix; synthetic fixtures for six families, task partitions, deleted users, Trash, allFields and note list/detail. Preserve live unknowns. | §8.3–4 |
| 2. Persistence and source engine | Seven logical record types including Organization storage ledger; immutable profile/original budgets, revisioned allowances/receipts, leases, byte reservations, atomic successful/negative settlement and checkpoint commits. | §8.1–7,12 |
| 3. Cross-job integration | Shared 010a/010b permit and Organization exclusion, connection/revocation fencing, retained-read authority independent of source eligibility; preserve 010a retry behavior. | §8.2,5–7 |
| 4. Deterministic preview and API | Persist encrypted destination/coverage inputs and source sequence boundary before generation; version comparison semantics. Build resumable pages and overlap-group counts/membership pagination without all-pairs expansion. Add budget and DB-only preview retry with current-admin authority. | §8.1–2,8,12 |
| 5. Backend checkpoint | Targeted tests and first bounded review/fix round; freeze wire/schema contract for Web. Do not claim compilation alone is completion. | §8.1–8,12 |
| 6. Web | Existing admin route; prepare/confirm, progress, pause/retry/cancel, budget review/increase, distinct source/preview resume actions, partial/previous reports and session/narrow-layout behavior. | §8.9 |
| 7. Integrate and verify | Second review/fix round, required full gates once, query plans and synthetic real-API/production-build browser walkthrough. Record deferred live validation separately. | §8.10–11 |

If source qualification disproves a proposed profile or changes an approved
contract, stop that dependent work and report current/proposed contracts and
compatibility under AGENTS §11; do not invent a fallback or drop source data.

## 4. File boundaries

Owned after approval:

- Migration domain: new snapshot/preview modules, closed reader interface and
  necessary existing command/worker changes. Existing crypto gains separate
  purposes; no intake blob format or key-policy rewrite.
- One `backend/crates/crm-api/migrations/` file with an unused version chosen
  against the actual head; `backend/.sqlx/` only if generated by required queries.
- Migration API routes, closed error mappings, necessary state/startup/config
  names, direct-domain and DB/HTTP tests registered in the existing test binary.
- Web migration API/queries, MigrationView and focused test files; no navigation
  redesign, new state-management layer or business-form refactoring.
- Spec/brief/source qualification/verification and required pointer amendments.
  Coordinator alone edits PROJECT_STATE.md.

The implementer lists actual files before checkpoints. Any dependency, service,
native/Operator contract or unowned-domain change must be justified separately.

## 5. Checks and evidence

While developing, run focused Rust library tests for snapshot parsers/crypto and
focused Web MigrationView tests, then the filtered DB/HTTP tests using the existing
nextest harness and disposable test databases. Final concrete test filters belong
in the verification record once test names exist; do not invent passing commands.

On the final backend/Web tree, run sequentially:

```sh
./scripts/sqlx-prepare
./scripts/check
./scripts/check-db
```

Use the installed Node 24.16.0 / pnpm 11.22.0 runtime and existing developer
configuration. Never point test reset/bootstrap at `crm_dev`. Do not log URLs
containing passwords or put source credentials in arguments, fixtures or screenshots.

Inspect `EXPLAIN (ANALYZE, BUFFERS)` for claims, latest/stream/checkpoint,
record-version, overlap-key/group membership and preview page queries over the
D-050 book. A shared office phone must not cause all-pairs materialization or
repeated full-group scans; keyset pages preserve access to every candidate.
Use representative dense notes/tasks and small synthetic reservation/ceiling
values to exercise exhaustion without multi-GiB fixtures. Existing
People/Today SQL is unowned; no 019b benchmark rerun unless an actual hot-path
change triggers D-050's one paired benchmark.

Synthetic walkthrough must use the production Web build against the real API
with a test-injected source reader. Capture authorization/session transitions,
recovery, partial reports and no business writes. Do not add a production
environment switch or HTTP route that activates the fake reader.

At most two review/fix rounds. Record exact code hashes, changed-file inventory,
checks/counts, failed attempts, plan evidence, screenshots, unresolved issues,
model/effort when available and external-validation status in
`docs/tasks/SLICE_010b_VERIFICATION.md` during implementation. Distinguish planned checks from executed results.

## 6. Planning handoff

Completed: scoped draft specification, current-code inspection, source-profile
research, execution/ownership sequence and observable acceptance criteria.
The four coordinator findings and two independent findings are revised with
corresponding contracts, negative cases and execution steps;
[review status](SLICE_010b_REVIEW.md) remains explicit.
The user accepted the remaining allowance/delegation policy and approved the
specification/brief under D-063. Next: freeze concrete profile/schema/DTO details,
implement backend, complete its checkpoint, then implement Web and final checks.
Live FUB testing remains deferred. Planning itself created no application code.
