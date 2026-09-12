# Slice 010d — Historical migration ladder

**010d1 DEPLOYED / VERIFIED IN SHARED DEVELOPMENT — 2026-09-12.** D-069 accepts
splitting capture from timeline import. D-070's complete 010d1 contracts,
implementation and isolated synthetic checks are complete; its follow-up
completed Git integration, cleanup and [shared-development release](../tasks/SLICE_010d1_RELEASE.md).
D-072 approves the reviewed [010d2 specification](SLICE_010d2.md), declared
contracts and isolated synthetic implementation, with user-selected metadata-first
exposure. 010d2 is [implemented and verified with isolated synthetic data](../tasks/SLICE_010d2_VERIFICATION.md)
in its uncommitted worktree; Git integration and deployment remain separate.
The original 010d1 planning baseline was main
`028d6133e1b7c3f81642275e030f63e98b2cca49`; original verification remains in
[the implementation evidence](../tasks/SLICE_010d1_VERIFICATION.md).

## Outcome

Preserve historical source evidence and bring qualified historical activity
into the migration review workspace with honest timestamps, attribution and
coverage. A successful collection request is not proof that all history is
available, and capture completion is not completed native import or cutover.

The existing core snapshot contains People, users, stages, custom fields, notes
and tasks. The completed 010c/f1/f2 imports use that immutable boundary. Historical
events/calls/texts require a separately confirmed source profile and capture;
they cannot be imported from summaries such as `lastCommunication`.

## Delivery sequence

| Rung | Deliverable | Exit condition |
|---|---|---|
| **010d1 — capture and coverage** | [Detailed specification](SLICE_010d1.md), [execution brief](../tasks/SLICE_010d1_IMPL.md): separately confirmed, encrypted, resumable capture of API-visible events/calls/text messages; bounded admin evidence and coverage report | Approved contracts, implemented synthetic source/DB/Web verification and independent review. Report collection enumeration, API restrictions and uncaptured content separately. No native history writes. |
| **010d2 — qualified historical timeline** | [Approved specification](SLICE_010d2.md) based on 010d1 evidence: explicit external-history types, source identity/actor/time rules, content placement/exposure, parent mapping, idempotent import and bounded timeline reads | [Implemented and verified](../tasks/SLICE_010d2_VERIFICATION.md) under D-072 with isolated synthetic data. Both complete readers are fenced and new review pages are bounded. Source is uncommitted and undeployed; native fact promotion, live qualification and activation remain separate. |
| **Further source/content work, when qualified** | Ordinary email/correspondence, recordings/media and any still-uncovered families | Prove supported source retrieval and satisfy their storage/privacy/consent prerequisites. Split further only where the evidence requires it. No claim that 010d1/2 silently completes these families. |
| **010e — deltas and activation** | Per-entity changes/deletions, repair, final reconciliation, source cutoff and explicit activation | Resolve Today/backlog/response semantics, communication restrictions and customer readiness before releasing the admin review hold. |

The [approved 010d2 specification](SLICE_010d2.md) and D-072 establish:

- Separate external FUB event, call and text record facts. They do not create
  native Inquiries, calls, contact attempts, correspondence or Today effects.
- Source record-created chronology with explicit unknown dates; source actor
  roles remain separate from the local import executor.
- Metadata-only display, deletable encrypted metadata, stable identities,
  conflicting-variant holds and permanent tombstones.
- An immutable parent/capture/interpretation anchor, bounded cross-family pages,
  existing paged notes/tasks and an independent reader compatibility boundary.

Readable bodies, live-source qualification, repair/deltas and activation remain
separate work. Do not invent a provider call lifecycle, map unknown outcomes to
success/failure, or assign deleted source users to the importing admin.

## Evidence and unresolved coverage

[Official source qualification](../research/SLICE_010d_FUB_SOURCE_CONTRACT.md)
records public GET availability and API visibility restrictions. It is not live
FUB validation. [Code discovery](../research/SLICE_010d_CODE_CONTRACTS.md) explains
the missing streams, native side effects and unbounded historical readers.

Ordinary email retrieval has not been qualified. Marketing campaign content is
not ordinary correspondence. D-062 requires new bulk email bodies/raw messages
outside PostgreSQL under a separately approved storage contract. Recordings and
transcripts retain O-012/O-002 prerequisites; no source URL is fetched by 010d1.
Full standalone tags and other migration gaps remain tracked in the
[migration summary](../plans/SLICE_010_MIGRATION_SUMMARY.md).

Live FUB validation remains user-deferred. First real customer data retains
D-015/O-012/O-013 and [readiness](../plans/PRODUCTION_READINESS.md) prerequisites.
The current review hold, ordinary Today/Operator/outbound restrictions and
Organization-wide PersonVisibilityScope remain in force throughout this ladder.
