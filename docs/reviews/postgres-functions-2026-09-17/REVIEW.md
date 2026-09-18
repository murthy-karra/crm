# PostgreSQL function and trigger responsibility review

Reviewed source: `82b185d`, 2026-09-17. Review only; no application, migration,
permission, or database state changed.

**Yes: business policy exists in PL/pgSQL, concentrated in migration.** The
clearest examples are remainder-work selection, migration eligibility and state
transitions, and budget-admission procedures. This is more than append-only
constraints or derived columns. It is largely intentional enforcement added by
migration slices, rather than evidence of an unauthorized second API. Rust still
initiates the reviewed operations through its commands/workers.

**I found no trigger that independently creates CRM business facts or chooses
normal CRM actions** such as assigning a Person, changing their stage, completing
a task, receiving an inquiry, or deciding a call outcome. That does not establish
a general security proof; this review concerns placement of responsibility.

## Scope and evidence

Read-only `pg_proc`/`pg_trigger` inspection of the shared-development database:

- 160 application functions: 150 PL/pgSQL and 10 SQL.
- 445 non-internal triggers, invoking 115 distinct functions.
- 34 functions execute as `SECURITY DEFINER`; that alone is not a defect.
- All 111 applied migration SHA-384 checksums match the checkout. The last is
  `20261008000034`; application routines are in `public`.
- Every deployed function was inventoried and structurally classified by its
  effective body, write targets, trigger bindings and purpose. Detailed source
  and Rust call-path review focused on the policy-bearing groups below. This is
  not an exhaustive proof of every SQL predicate or a concurrency/security audit.
- Test-only fault/delay/probe functions were identified separately; they are not
  installed application policy. The temporary `_mint_capture_token_backfill`
  helper is dropped during its migration and is not a deployed routine.

Artifacts:

- [Function index](function-index.md): every function and its responsibility.
- [Inventory](inventory.json): signatures, categories, source references,
  definer status and effective-definition hashes.
- [Effective definitions](effective-functions.sql): actual `pg_get_functiondef`
  output, including later migration patches. **Audit export, not a migration.**
- [All trigger bindings](triggers.csv): table, trigger, target function and DDL.

No application test suite or database mutation experiment was run for this
read-only architectural review. Existing passing E2E tests do not resolve the
architectural questions below.

## Architectural boundary

AGENTS §4.8 and D-008 require meaningful business mutations to pass through typed
Rust commands. They do not, by themselves, prohibit every SQL helper called by a
command. D-052 explicitly permits trigger-maintained read models and says
**business facts may not be written by triggers**. D-015 supports immutable facts
and erasable content. D-064/D-065 intentionally require review-only workspaces,
admin access, private import permits and compatible-runtime protection. D-092
accepts combined-refresh behavior and concrete SQL/guard implementation work.

Consequently, a domain-aware rejection trigger is not automatically an
architectural violation. The issue is how much policy the database now owns,
where it duplicates Rust, and whether an ordinary-looking write has additional
business meaning.

## Findings and recommended disposition

### 1. Migration remainder selection is a workflow algorithm in PL/pgSQL

**Highest refactoring priority; not a demonstrated runtime failure.**

`crm_family_refresh_remainder_next` branches over eight preparation stages,
chooses cohort/pages/source/mappings/manifest work, and decides which completed
catalog results are inherited versus which unfinished operations remain eligible.
`crm_family_refresh_remainder_copy_guard` and
`crm_family_refresh_remainder_progress_guard` enforce that same sequence, eligible
dispositions and count transitions.

Rust's `remainder_copy::run_once` calls this function to obtain its next work item,
then contains its own stage-to-table switch and stage advancement. PostgreSQL is
therefore doing workflow selection, not simply validating a foreign key. Changing
the recovery algorithm requires coordinated Rust and SQL edits.

Evidence: [remainder migration](../../../backend/crates/crm-api/migrations/20261008000032_fub_family_refresh_remainder.sql),
function at line 122; [Rust remainder worker](../../../backend/crates/crm-app/src/domain/migration/family_refresh/remainder_copy.rs),
lines 135–157.

Recommendation: make the phase/work-selection policy explicit in the typed Rust
workflow. Retain database checks for frozen ownership, tenant boundaries, immutable
confirmed inputs, exact predecessor/result relationships and atomic checkpoint
updates. Decide deliberately whether the ordered-walk proof must remain in SQL;
do not remove it merely to reduce function count.

### 2. Migration eligibility and lifecycle policy are implemented in both layers

`crm_family_refresh_mutation_allowed` knows permitted note/task update columns,
source representations, conflicting source variants, native revisions, completed-
by attribution and current execution position. `crm_mapping_repair_owned_write`
implements candidate eligibility, inherited choices, mapping authority and
settlement rules. `crm_people_recovery_owned_write` and
`crm_people_recovery_item_proof` encode which held People qualify for recovery and
which stage/assignee approvals count. Plan/bundle guards also encode terminal
states, lease rules and confirmation expiry limits.

These are real business/workflow rules even when implemented as validation. They
are materially broader than generic immutable-row or tenant checks. The large
JSON field allowlists and repeated state/role/source predicates make drift between
worker code, preview behavior and commit-time rejection harder to review.

Evidence: [combined refresh guard](../../../backend/crates/crm-api/migrations/20261008000001_fub_family_refresh.sql),
line 279, extended by later execution/ownership migrations;
[mapping repair](../../../backend/crates/crm-api/migrations/20261006000001_fub_people_mapping_repair.sql),
line 210; [People recovery](../../../backend/crates/crm-api/migrations/20261007000001_fub_people_recovery.sql),
lines 128 and 292. The inventory links the other guards.

Recommendation: distinguish immutable authorization/ownership proof from source
interpretation and eligibility policy. Keep the former fail-closed at commit;
centralize the latter in Rust where possible. Preserve parity tests for any
intentionally duplicated checks. No guard removal is recommended without a
replacement proof for stale binaries, revoked executors and unconfirmed writes.

### 3. Reservation functions are multi-table stored procedures with policy

`crm_family_refresh_reserve`, `settle` and `reclaim` insert/delete reservations and
update plan, bundle, Organization and source budget ledgers. They also enforce
admin eligibility, leases, control/unit modes, byte limits and special cases for
executor revocation and cancellation. They are invoked directly by Rust commands
and workers; the accounting workflow is partly delegated to PL/pgSQL.

The atomic accounting is valuable and is operational rather than lead-management
policy. However, exception paths such as the permitted revoked-executor settlement
are workflow policy, not just a derived counter.

Evidence: [budget procedures](../../../backend/crates/crm-api/migrations/20261008000009_fub_family_refresh_history_budget.sql),
lines 15, 52 and 84; [revocation patch](../../../backend/crates/crm-api/migrations/20261008000025_fub_family_refresh_revocation.sql);
[cancellation patch](../../../backend/crates/crm-api/migrations/20261008000026_fub_family_refresh_lifecycle.sql);
[Rust reservation caller](../../../backend/crates/crm-app/src/domain/migration/family_refresh/preparation.rs),
lines 99–106.

Recommendation: explicitly treat these as a reviewed atomic storage-ledger API if
retained. Prefer Rust-owned admission/lifecycle decisions and a narrow SQL
accounting operation with invariant checks. Do not move the accounting updates
into separate, non-atomic calls.

### 4. Imported-history display deletion has durable erasure semantics

`crm_history_import_display_change` can create/update an erased identity when a
display row is deleted. `crm_admitted_history_suppress` marks the identity erased
and clears encrypted manifest content. `crm_family_history_erasure` clears later
display ciphertext and adjusts retained-byte ledgers. This prevents resurrection,
but deleting a display row is consequently not ordinary rebuildable-view cleanup.

This is domain-specific suppression/privacy behavior. The historical migration
specification explicitly requires display suppression and no reconstruction of
erased content, so it should not be described as an accidental policy invention.

Evidence: [original display deletion](../../../backend/crates/crm-api/migrations/20260922000001_fub_history_timeline.sql),
line 351; [admitted suppression](../../../backend/crates/crm-api/migrations/20261005000001_fub_admitted_history.sql),
line 311; [refresh erasure](../../../backend/crates/crm-api/migrations/20261008000009_fub_family_refresh_history_budget.sql),
line 106; [010d2 erasure requirements](../../specs/SLICE_010d2.md).

Recommendation: make the irreversible meaning explicit in erasure/suppression
commands, repository APIs and documentation. Retain database propagation as a
privacy safeguard if desired. Never let a generic projection-rebuild operation
DELETE these rows under the assumption they can be recreated.

### 5. Text-patched function definitions obscure the effective policy

Several later migrations load `pg_get_functiondef`, edit strings and execute the
result. For example, lifecycle/revocation migrations alter settlement exceptions;
execution patches add phase/cancellation checks; the remainder migration patches
a set of existing guards in a loop. Reading only the original `CREATE FUNCTION`
or counting explicit definitions misses the active behavior.

Evidence: migrations
[25](../../../backend/crates/crm-api/migrations/20261008000025_fub_family_refresh_revocation.sql),
[26](../../../backend/crates/crm-api/migrations/20261008000026_fub_family_refresh_lifecycle.sql),
[28](../../../backend/crates/crm-api/migrations/20261008000028_fub_family_refresh_execution.sql),
[31](../../../backend/crates/crm-api/migrations/20261008000031_fub_family_refresh_history_execution.sql),
[32](../../../backend/crates/crm-api/migrations/20261008000032_fub_family_refresh_remainder.sql).

Recommendation: maintain reviewable canonical effective definitions and verify
freshly migrated definitions against them. Prefer complete definitions in future
forward migrations when practical. Do not edit already-applied migrations.

## What should remain in PostgreSQL

No responsibility-placement finding for these mechanisms:

- `reject_mutation` / `reject_direct_mutation`: immutable-history protection.
- `person_touch_*`: maxima derived from committed history, explicitly accepted
  by D-052. They do not create contact credit or history facts.
- `crm_mobile_*`: revision/cursor/catalog invalidation and optimistic-concurrency
  tokens, not user-facing decisions about what action to perform.
- `crm_history_review_*`, imported-history head/count projections and mapping
  order/count projections: derived state maintained from authoritative rows.
- Tenant/ownership relationships, immutable confirmed inputs, atomic compare-and-
  swap and ledger balance checks. Domain vocabulary alone does not make these
  inappropriate database constraints.
- Workspace review/compatibility enforcement is intentional defense in depth.
  `crm_workspace_read` does own an admin-versus-member policy decision in SQL;
  its Rust entry point is `auth::workspace::read_check`, not a client-supplied
  authorization result. Preserve this safety property during any refactor.

## Inventory by responsibility

Categories describe purpose, not counts of defects. Several migration guards mix
policy and valid integrity checks and must be reviewed before relocation.

| Responsibility | Functions |
|---|---:|
| Append-only integrity | 2 |
| Derived projections and revisions | 33 |
| Storage measurement and accounting integrity | 16 |
| Workspace authorization and compatibility fences | 13 |
| Migration eligibility, ownership and workflow guards | 72 |
| Migration work selection | 6 |
| Reservation workflow and accounting | 3 |
| Erasure and suppression propagation | 9 |
| Query and fingerprint helpers | 6 |
| **Total** | **160** |

Suggested next implementation scope: start with the combined-refresh remainder
algorithm and document the SQL/Rust ownership boundary. Preserve existing
integrity/authorization/privacy tests and add behavior-equivalence checks before
moving policy. A blanket removal of triggers would discard useful protection
without addressing where decisions actually belong.
