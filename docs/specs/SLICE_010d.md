# Slice 010d — Historical migration ladder

**010d1 APPROVED / IMPLEMENTATION IN PROGRESS — 2026-09-12.** D-069 accepts
splitting capture from timeline import. D-070 approves the complete 010d1
contracts, implementation and isolated synthetic checks. Later 010d2 semantics
and implementation require their own specification and approval. Baseline main:
`028d6133e1b7c3f81642275e030f63e98b2cca49`.

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
| **010d2 — qualified historical timeline** | A later detailed specification based on 010d1 evidence: explicit external-history types, source identity/actor/time rules, content placement/exposure, parent mapping, idempotent import and bounded timeline reads | Reviewed semantics and contracts before implementation; faithful facts without fabricated live calls, inquiries or communication attempts. Both legacy Person and 010f2 core readers must be bounded/fenced before adding history volume. |
| **Further source/content work, when qualified** | Ordinary email/correspondence, recordings/media and any still-uncovered families | Prove supported source retrieval and satisfy their storage/privacy/consent prerequisites. Split further only where the evidence requires it. No claim that 010d1/2 silently completes these families. |
| **010e — deltas and activation** | Per-entity changes/deletions, repair, final reconciliation, source cutoff and explicit activation | Resolve Today/backlog/response semantics, communication restrictions and customer readiness before releasing the admin review hold. |

010d2 is deliberately not an implementation-ready contract. It must decide:

- Which FUB event types actually establish an Inquiry, and which source times
  establish occurrence rather than vendor record creation. Source labels and
  source actors stay distinct from the import executor.
- Whether qualified history remains separate external facts or populates native
  Inquiry/contact/correspondence truth, which changes counts, filters and D-052
  maxima. Existing operational commands have side effects and cannot be used as
  historical persistence adapters.
- Readable text/body policy and erasure linkage; source-only and held records;
  stable identity, variants, tombstones and independently frozen capture input.
- Bounded cross-family history/order/cursors, compatibility with existing paged
  notes/tasks, and the separate history-reader release boundary.

The recommendation is separate typed external historical facts first; it remains
a proposal for 010d2. Do not implement a generic unrestricted JSON event store,
invent a provider call lifecycle, map unknown outcomes to success/failure, or
assign deleted source users to the importing admin.

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
