# 010e6 — Recover People blocked before creation

**ACCEPTED FOR IMPLEMENTATION — D-090, 2026-09-15.** The user approved
this plan through Lavish after the 010e5 release. Its declared shared contracts
are owned implementation scope; materially different policy requires a decision. Companion: `.lavish/never-imported-people-plan.html`.

## Outcome in plain language

An administrator can find a Person skipped because their source stage or assigned
agent had no approved destination, fix that mapping, review the complete Person,
and create them once. Their original skipped result remains visible. Afterwards,
the administrator can run the existing metadata, activity and history import
steps against the successfully recovered group. Each step reports its own gaps.

“Recovery” here means creating a Person who never successfully entered this CRM.
It does not mean restoring a deleted Person or merging two similar contacts.

## What exists today

- [010e5](../specs/SLICE_010e5.md) repairs mappings on People already created.
- [010e3](../specs/SLICE_010e3.md) admits newly observed People, but explicitly
  excludes every source ID present in the original boundary, including holds.
  It also holds new records whose original inherited mapping is unavailable.
- Original and admitted creation share `migration_import_identity`, keyed by
  Organization, source account, family and exact source ID. Persistent erased or
  inconsistent identities must prevent recreation.
- [010e4](../specs/SLICE_010e4.md), [010f3](../specs/SLICE_010f3.md),
  [010f4](../specs/SLICE_010f4.md) and [010d3](../specs/SLICE_010d3.md) accept
  explicitly proven successful admission cohorts. They do not accept an arbitrary
  list of Person IDs. Extending that proof is part of this proposed slice.

Code inspected: `people_admission.rs`, `people_admission_worker.rs`, the admission
schema, `admitted_people_refresh_worker.rs`, and 010e5's typed mapping/provenance
contracts. The original exclusion and exact admission-result joins are deliberate
guards; removing an exclusion without new authority would be unsafe.

## 1. Scope and entry points

Work only inside an existing bound `migration_review` workspace with a completed
original People import and a current Organization administrator.

Two explicit anchors are eligible:

1. A qualified original manifest/result with a stage/assignee mapping hold and no
   successful original materialization.
2. A qualified held admission item/result, including a sealed all-held preview,
   whose creation was blocked by stage/assignee mapping.

One recovery root has one anchor owner and one retained source report. Anchor
records, their counts and raw source evidence stay immutable. A ready unconfirmed
admission preview may be atomically retired and linked to the new recovery;
confirmation and replacement must not both win. A terminal original import is
never reopened. Already successful People link to existing repair/refresh actions.

Explicit exclusions: missing/ambiguous identity evidence; original omissions
without a proven mapping hold; erased/missing formerly successful targets;
unsupported source shapes; source Trash/restricted records; other organizations;
source keys with no qualified evidence. These remain counted holds or exclusions.
An initial import that was never confirmed uses its existing mapping setup rather
than pretending a bound review workspace exists.

## 2. Source and ordering

Select one completed retained 010e1 report for the same parent/Organization/FUB
account. Use its newer snapshot for all core values and source catalogs. It must
be the latest confirmed admission boundary or a strictly later nonoverlapping
capture, and at least as new as the held admission anchor (when applicable).
Several reports over the identical frozen snapshot are one boundary, not newer.

Freeze report output revision, snapshot/final sequence, capture interval, parser
and engine versions, anchor IDs, exact source IDs and semantic evidence hashes.
Requalify raw accepted payload/ordinal/account/identity; a report display label
alone is never executable evidence. Require exhausted People/users/stages and
preserve lossless parsing, repeated-observation/conflict and size rules from 010e3.
If no eligible report is already retained, show that prerequisite; recovery never
silently fetches new FUB data.

Original-held candidates need positive proof of presence and the mapping hold at
the original boundary. They intentionally do not satisfy ordinary admission's
absence rule. Admission-held candidates retain independently qualified original
absence proof. Neither path may infer identity from a name, email, phone or date.
Missing or conflicting observations in the selected newer snapshot remain held.

Use the existing parent serialization and confirmed-boundary ordering. Freeze and
recheck the latest boundary at preview, confirmation and settlement. A competing
later admission invalidates an older unconfirmed recovery. Same-boundary recovery
is allowed only for the exact held selection or an exact cancelled remainder;
it must not widen ordinary admission's same-boundary rule. Successful recovery
records its own confirmation boundary and initializes its own baseline.

## 3. Mapping and complete preview

Group exact source stage/agent keys with affected candidate counts. Let the admin
choose an existing stage, an active existing member, explicit unassigned, or leave
unresolved. No best-guess destination, automatic member invitation, new stage,
pond expansion, or assignment fallback. Valid original approvals can be displayed
and retained, but explicit choices are frozen with their targets and revisions.

Reuse 010e5's bounded editing and exact-key concepts, not its successful-refresh
binding rows: those rows require a Person and a refresh result that do not exist
yet. Recovery needs its own immutable, admission-owned initial approval evidence.
Future admitted refresh resolves that initial recovery approval, or a later valid
010e5 repair for the same Person/key. Invalid approval holds; it never falls back
to an unrelated source mapping.

The final preview shows every proposed name, email, phone, primary order, stage
and assignee, plus original hold, source dates, intended People/contact counts,
explicit unassigned count, exclusions and deferred family coverage. Confirmation
binds the exact sealed plan/digest/choice revision and displayed counts. Zero
eligible candidates is a valid review outcome with creation disabled.

## 4. Create exactly once and retry safely

Propose a distinct `mapping_recovery` mode in the existing admission lifecycle,
with a separately versioned recovery engine. The normal mode keeps its current
absence and inherited-mapping rules. Extend the existing admission worker; add
zero polling loops, executables or pods.

Before any creation, check the shared source identity plus historical successful
original/admission results. No identity row is not sufficient if successful
materialization evidence exists. A successful live identity yields “already
imported”; a tombstone, dangling identity or inconsistent success proof yields a
hold. Never adopt an existing CRM Person using contact similarity.

For an eligible item, preallocate a stable Person ID in its sealed manifest.
One short database transaction must recheck current admin/workspace, source and
target validity, quota reservation, plan, lease token/epoch, ordering and identity,
then atomically write Person, contacts, actual admission-origin identity, recovery
provenance/fact, successful result, initial baseline/approval and exact byte ledgers.
Retain the existing narrow native-column allowlist and null operational activity
clocks. Imported history must not simulate a fresh contact attempt or inquiry.

Database uniqueness is the final arbiter when two jobs try the same source ID.
Handle the conflict by inspecting the committed identity/result and returning a
qualified existing outcome; do not turn every uniqueness error into success.
If the process dies before commit, all item writes roll back. After commit, retry
finds the durable identity/result/receipt and does not create another Person or
fact. An expired lease can be reclaimed; the old worker cannot commit with its
old token. Kubernetes can restart capacity independently of this per-item lease.

Cancellation stops future items, preserves successes, and creates no duplicate
fact on retry. A confirmed cancelled run may offer only its exact unprocessed
eligible remainder, with inherited immutable choices, fresh current checks and a
new preview/confirmation. Settled holds require an explicit new recovery attempt;
they are not silently mixed into the unfinished remainder.

## 5. Follow-on imports are part of the acceptance boundary

A recovered Person is a real member of a recovery admission cohort, with mode
and origin displayed honestly. Do not fabricate an original 010c result or relabel
the original hold as successful. Add a typed `PersonRecovered` provenance/fact
representation, IDs-only in history with encrypted source content separately.

Extend the existing family selectors and typed qualification guards to accept
successful recovery admission results. Preserve their exact source policies:

- **Core refresh (010e4):** later qualified core report; recovery initial approval
  and baseline recognized, local edits protected; later 010e5 repairs still work.
- **Tags/custom fields (010f3):** the recovery report or strictly later qualified
  core capture; same catalog claims, option mapping and no coercion rules.
- **Notes/tasks (010f4):** same accepted role/kind/timezone and source qualification,
  shared identity/tombstone rules and local mutation protection.
- **History (010d3):** separately qualified completed history capture starting
  strictly after the recovery core snapshot completed; missing capture is a
  visible prerequisite. Existing metadata-only scope remains unchanged.

Each remains an independently previewed and confirmed command. The recovery
screen links these steps and shows per-family not-started/partial/held/completed
coverage. It never starts a cascading import, claims full migration from core
success, or activates ordinary/member/mobile access. Family work can only select
successful committed results of a terminal recovery; later successes cannot
expand an already confirmed family manifest.

## 6. Proposed shared-contract changes — approval checkpoint

D-090 accepts these changes under AGENTS.md §11. Their concrete implementation
contract is frozen before application code changes:

| Current contract | Proposed change and reason | Affected components / compatibility |
| --- | --- | --- |
| Admission requires proven original absence and inherited mapping | Explicit recovery mode/engine with positive held-anchor proof and reviewed initial mapping | Admission commands/store/worker; normal requests retain semantics |
| Admission creation has original mapping references | Exclusive original-approved or recovery-approved mapping ownership, immutable choice evidence and typed reader resolution | Additive DB/FKs/permits, admitted refresh and 010e5; no fake original/repair rows |
| Successful admission provenance implies newly observed creation | Recovery origin and IDs-only recovered fact, linked to old hold | Person provenance/history DTOs and Web; old rows remain ordinary admission |
| Family cohort qualifiers know current admission engine | Explicitly recognize recovery mode/results with the same exact identity proof | 010e4/010f3/010f4/010d3 guards, selectors, APIs and tests |
| No recovery editor or confirmation fields | New bounded recovery preparation/choice endpoints and exact mode-aware confirmation | Strict DTOs, receipts, cursors, no-store Web; legacy confirmation cannot approve recovery |
| Capability set has no recovery-aware readers/writers | Durable `fub-people-recovery-v1` requirement before first recovery confirmation | Startup and per-unit/reader fences, inventory, preflight, admin/migrator; incompatible binaries fail closed |

Proposed endpoints under `/api/migrations/fub/people-admissions`: POST
`/recoveries` accepts request ID, typed original/admission anchor and source report;
GET/POST `/{id}/recovery-mappings` page/edit exact keys with expected draft revision.
Existing plan/status/items/results/cancel endpoints receive explicit recovery
mode support. Resource authority derives from session Organization and stored
owners, never a browser-supplied trusted Organization or whole-book ID array.

Use existing bounds: root lists ≤20, mapping/item pages ≤50, choice requests
≤128 KiB/50 choices, resource responses ≤128 KiB, item/contact/result pages
≤256 KiB, field fragments ≤16 KiB, existing ≤64 MiB retained item ceiling.
Count/revision transport stays decimal strings; reject unsafe client conversion.
Cursor HMACs bind actor/Org/root/plan/choice revision/filter/traversal boundary.
Freeze exact SQL, FK XOR ownership, status codes, field names, lock order, byte
formulas and inventory hashes in the implementation contract before Web coding.

Additive schema defaults existing rows to ordinary admission. Do not backfill
recovery successes from holds. Preserve all old evidence/checksums. Rollback after
recovery use requires a recovery-compatible binary; old software cannot be used
as a recovery workaround. Raw/choice/manifest/provenance/baseline bytes are admitted,
reserved and charged once to the selected snapshot/Org; capacity failure pauses.
No quota increases, secret-manager changes or new source calls belong to this slice.

## 7. Implementation sequence and ownership

1. Review/accept this policy and declared amendments to 010e3/010e4/010e5 and the
   three admitted-family specifications. Freeze the concrete contract and proof
   inventory. Do not start application changes under planning authorization.
2. One primary backend/database writer implements additive mode/owners, candidate
   discovery, immutable mappings, lifecycle, single-item atomic creation and fences.
3. Extend initial mapping/baseline/provenance readers and every follow-on qualifier;
   complete backend contract integration before dependent Web work.
4. Add the admin recovery workflow and per-family handoff/coverage using current
   components, safe errors, durable receipts and access-loss cache clearing.
5. Independent review when a reviewer is authorized; at most two D-050 rounds.
   No independent review or new reviewer authorization is claimed by this plan.
6. Synthetic final checks, then a separate authorized release. Preserve shared
   runtime artifacts with isolated Rust/Web output and serial DB tests.

## 8. Required evidence

- Original and admission mapping holds, all-held preview replacement, old immutable
  outcomes; missing/null/exact long source keys, unavailable/ambiguous source proof.
- Cross-Org/member/CSRF/foreign target denials and actor/Org cache clearing.
- Existing successful identity; missing identity with successful result; erased or
  dangling identity; same email with different source IDs remains separate People.
- Two contenders for one ID; lost HTTP response; crash before/after atomic commit;
  takeover of expired lease, stale worker rejection, demotion, capacity exhaustion.
- Partial cancellation with successes, settled holds and unfinished eligible items;
  exact remainder, same-request replay, stale target/source and boundary races.
- Native counts/contacts/facts, identity/provenance and byte balances before/after.
- End-to-end recovered cohort through core refresh, tags/fields, notes/tasks and
  history, including later 010e5 repair and family-local edits/erasure conflicts.
- Old-format requests/rows continue ordinary behavior; incompatible startup,
  already-running writer, reader and family paths reject recovery workspaces.
- Real HTTP and production Web preview/confirm/reload/cancel/recovery at desktop
  and 390px. No live FUB/customer records required or authorized for verification.
- `scripts/check`, SQLx offline/migration checks, serial DB contracts/failure tests;
  25k realistic query plans and one paired Person/Today regression under D-050.

## 9. Remaining boundaries

No blanket recovery of arbitrary skipped data, identity repair, resurrection,
record merging, new-source data fetching, ongoing family synchronization, production
Kubernetes, worker supervisor/PgBouncer redesign, activation or native distribution.
Complete erasure/restore policies remain D-015/O-012/O-013 gates before customer use.
This plan closes a specific creation gap and makes its later family coverage visible.
