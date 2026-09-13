# Mobile 003 / 010e3 — Coordinator planning review

**Planning review — 2026-09-13.** D-078 subsequently accepts both specifications
and the occurrence-time recommendation for implementation. The findings below
retain the historical planning assessment; no implementation test is claimed.
This is a coordinator code-to-spec review, not an independent agent review or
implementation acceptance test. Read-only inspection used main closeout
`619c1b33b15dcd8f331561e16178b3e107984a3b`; proposals live on
`codex/mobile003-010e3-planning`. D-077 authorizes this planning work.

Reviewed: [Mobile 003 spec](../specs/MOBILE_003_OFFLINE_CONTACT_LOGGING.md),
[mobile briefs](MOBILE_003_IMPL.md), [010e3 spec](../specs/SLICE_010e3.md),
[migration brief](SLICE_010e3_IMPL.md) and
[coordinated plan](../plans/MOBILE_003_010e3_PARALLEL_LAUNCH.md). Authority was
AGENTS, the decision log, architecture baseline and the accepted Mobile 001/002,
010c/010e1/010e2 contracts. Prior chat is not the contract source.

## Findings and disposition

| Tag / finding | Current evidence | Planning disposition |
|---|---|---|
| CONTRACT — occurrence time changes an accepted clock boundary | [Mobile 001 §3](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md) keeps device time as untrusted metadata; Mobile 002 retains server business clocks. D-022 compares contact occurrence to Inquiry time. | Mobile 003 §3 declares a new-kind-only reported-time exception, future rejection and separate server recording time. This remains the one outstanding product choice; it is not silently accepted. |
| CONTRACT — retrying the ordinary contact command duplicates facts | [log_contact_attempt.rs](../../backend/crates/crm-app/src/domain/commands/log_contact_attempt.rs) explicitly owns a transaction and is non-idempotent. | Mobile 003 §§2/4 requires a shared typed transaction core plus atomic mobile fact/receipt; preserve ordinary wrapper behavior and old bytes. |
| SEEN_HERE — receipt paths assume note/task | [operations.rs](../../backend/crates/crm-app/src/domain/mobile/operations.rs) has note/task visibility dispatch and note/task revision/publication branches. | Explicit contact resource visibility, null revision and event dispatch are required. Accepted replay precedes fresh time checks; a missing target cannot free the consumed operation ID. |
| SEEN_HERE — contact acceptance need not change Person mobile revision | [revision migration](../../backend/crates/crm-api/migrations/20260923000001_mobile_offline_foundation.sql) watches downloaded fields/components; [Today SQL](../../backend/crates/crm-app/src/domain/today/source_candidates.sql) uses contact-derived dates. | Mobile 003 §5 requires acceptance-triggered Today reconciliation even with equal Person revision. Pending work does not locally hide Today entries; no fake revision or new history replication. |
| TRUST — newly observed is not a creation certificate | [core_change_source.rs](../../backend/crates/crm-app/src/domain/migration/core_change_source.rs) emits `first_observation_is_not_creation` and non-exhaustive evidence arrays. | 010e3 §3 independently qualifies complete raw observations and proves exact absence from the retained original boundary. Original-present held records are excluded from admission. |
| CONTRACT — separate admission identity could duplicate an original Person | [original import schema](../../backend/crates/crm-api/migrations/20260918000001_fub_people_import.sql) owns the global source PK, old plan/manifest links and target tombstones. | 010e3 §4 extends that same registry with explicit admission origin/FKs and immutable result. Original rows remain unchanged; missing targets stay consumed; no fabricated original result or second uniqueness namespace. |
| BOUNDARY — old children/provenance require successful original results | [imports.rs](../../backend/crates/crm-app/src/domain/migration/imports.rs), the original result/contact FKs and 010e2 eligibility bind the original import. | New admission provenance/history rendering is explicit. Existing metadata/activity/history/refresh eligibility stays original-only; new-Person coverage is visibly incomplete until its own following specification. |
| TRUST — review permits can accidentally authorize another writer | The latest 010e2 release records the correction keeping old import contact permissions INSERT-only; review-mode mobile writes remain denied. | 010e3 §5 requires a private exact-item/new-target INSERT permit and negative token-borrowing tests. Coordinator serializes guard changes and combined acceptance. |
| BOUNDARY — cancellation must not forget settled identities or source order | The accepted 010e2 source-boundary/remainder policy preserves prior settlements. | Admission gets its own monotonic boundary and owned reservations; exact-input remainder plans never reopen a cancelled run or refresh settled People. |
| BOUNDARY — four deliverables cannot become four concurrent worktrees | AGENTS §12 caps short-lived implementation worktrees at three. Shared facts/registration and fixed-name DB gates can collide despite Git separation. | Mobile backend integrates/closes before both native lanes start. Migration has one backend/Web writer. Shared patches and DB gates are serialized; no fourth implementation lane. |

The raw-input wording was tightened during review: 16 MiB is a bounded input
unit, not a multiplier available to every descriptor in a checkpoint. Preparation
must also progress through excluded pages. Retained-byte accounting explicitly
includes defined identity/fact overhead and does not charge shared raw captures
again. These are requirements for later proof, not measured implementation results.

## Scenario walkthroughs

These are design checks, not executed application tests:

- A Monday contact uploaded Wednesday cannot satisfy a Tuesday Inquiry under the
  recommended policy. A later existing contact maximum stays unchanged by an
  older accepted fact. A clock ahead of the server yields recoverable rejection;
  an uncertain previous upload cannot be replaced with a fresh operation ID.
- A committed contact with a lost response replays its original receipt without
  another fact; a successful receipt with unchanged Person revision still causes
  Today reconciliation. An unrelated real contact uses another draft/operation.
- A source Person absent from a qualified original boundary and present in the
  newer capture can be admitted with inherited mappings. A similar email does
  not merge People. An original held record, uncertain baseline or missing
  identity target is held/excluded instead of recreated.
- Partial cancellation retains admitted Persons, identities and provenance. An
  explicit exact-input remainder preview counts those as already admitted and
  can admit the rest once; older or overlapping new inputs cannot regress state.
- Admitted People remain in admin review with a visible dependent-family gap.
  Their insertion cannot give mobile, Operator or ordinary mutations a bypass.
  Original provenance/history continue working with unchanged contracts.

## Readiness and validation

Planning artifacts and ownership are complete. Mobile 003's occurrence-time
recommendation awaits the user's answer; both specifications' declared shared
contracts need implementation acceptance under AGENTS §11. There is no conflict
between existing accepted documents requiring a code change now: the time
exception and admission contracts are explicitly proposals. Exact DTO/schema
fixtures are the approved implementation owners' next deliverables once accepted.

Validation performed for this docs-only change: `git diff --check`; a local
Markdown relative-file/heading-link check across all 12 changed/new documents;
and changed-path inspection confirming only `docs/` files. Entry-point/current
roadmap wording was corrected to show 010e2 released and 010e3 proposed. No
application tests, database migrations, builds, source processing or deployment
were run or claimed. No implementation agent was launched for this planning pass.

Later acceptance is precisely the specs' focused checks, actual iOS/Android and
Web/API workflows, relevant D-050 measurements and final integrated gates. The
released Mobile 002 / 010e2 evidence is reusable only for unchanged behavior.
