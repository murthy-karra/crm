# Slice 010 — Follow Up Boss migration ladder (PARKED)

Status: **PARKED (user, 2026-08-28) — deliberately deferred before
any decision was taken.** Reason: too many FUB entities lack
destination models today (notes, tasks, custom fields, deals have no
CRM tables), so a migration now would import people/contacts/history
but classify much of a real FUB book "preserved, not yet
importable". Resume when more destinations exist — or when a design
partner's switch makes people+history alone worth shipping.

A full planner survey was completed 2026-08-27 and is preserved
below verbatim as the resume artifact: five S–M rungs (010a connect
+ inventory → 010b snapshot + preview report → 010c additive-only
people commit → 010d backdated call/text history → 010e delta +
reconciliation report) plus an open 010f+ family, with the safety
model (idempotent mapping table, preview-first, additive-only
no-rollback posture), credential handling, runtime shape, and
per-rung decision list. Key ground truths verified in code and still
true unless the code moves: `Origin::Migration` already exists;
stage defaults were seeded with FUB's nine for this purpose (D-019);
`receive_inquiry` supports backdated `received_at`; the capture
slice (009) established the backdated-fact discipline
(occurred_at/recorded_at, clamps, System-actor-on-behalf).

Undecided (were about to be asked when parked): no-rollback posture,
FUB-API-only scope, credential at-rest encryption. Ask them at
resume time — do not assume.

Walkthrough note for resume: a real FUB trial account is the right
instrument; availability and API facts marked TO-VERIFY in the
survey must be re-checked live at 010a spec time.

---

## Planner survey (2026-08-27, verbatim resume artifact)

Five rungs, each S–M, format per SLICE_007_LADDER (own spec, own
branch, own gates, rung specs written when the previous merges):

| Rung | Outcome | Size |
|---|---|---|
| 010a | Connect + inventory probe: admin pastes FUB API key (validated live, stored per credential decision), read-only entity counts, "what was discovered" report incl. will-import / preserved / cannot-access classification. Zero writes. | S–M |
| 010b | Snapshot + migration-preview report: paced worker (extraction-worker template) pages People/Stages/Users into an encrypted `migration_raw` store (separate table — D-042.6 structural-airtightness precedent); `migration_run` row (status/cursors/counts) drives polling UI; report v2 = dedup forecast via read-only `identify`, stage auto-map-by-name preview, FUB-user↔member email matching, unsupported-data inventory. Resumable via cursors; still zero CRM writes. | M |
| 010c | People commit: new typed domain command family (D-021) — per person: advisory lock + identify dedup → create (FUB stage auto-map/auto-create at end of position order; assignee by member-email match else unassigned+counted; ALL emails/phones via a widened multi-contact upsert) or match-existing (additive-only: contact methods added, names fill-if-empty, stage/assignee never touched) → one backdated Inquiry (`received_at`=FUB created, `source`=FUB source string VERBATIM, `source_external_id`=FUB id; provenance JSON as a `raw_payload` row `source='fub'`, new declared `PayloadFormat::FubPersonV1`, resolution resolved) → facts `Actor::System` + `Origin::Migration` + on_behalf_of=initiating admin + correlation_id=run id. `migration_mapping (org, entity_kind, fub_id) UNIQUE → crm_id` = idempotency (double-run → zero new rows, pinned). Commit behind ConfirmDialog showing 010b numbers. Per-rung decisions: Today-flood handling (recommended: one summary backdated attempt from FUB lastCommunication where present), merge policy, stage auto-create. | M |
| 010d | History commit: FUB Calls → backdated `contact_attempted` channel `call` (outcome vocabulary mapped, TO-VERIFY), TextMessages → channel `text`; new domain fn (log_contact_attempt hardcodes now() — do NOT widen its API); mapping-table dedup; clears Today for genuinely-worked people. Notes fetched + preserved encrypted, NOT imported (no destination; O-012). | M |
| 010e | Delta + cutover: changed-since fetch (updatedSince TO-VERIFY; fallback full re-page + mapping skip); additive updates only; fill-assignee-only-where-NULL after agents invited; final reconciliation report (discovered / created / matched / updated / preserved / inaccessible / failed, drillable) + cutover checklist. Webhook continuous sync out. | S–M |
| 010f+ | One rung per remaining entity as destinations ship: tags/custom fields (schema first), FUB email records → correspondence metadata (bodies O-012), tasks/appointments, deals (thesis §11 defers Deal model), notes bodies (O-012). Each from a fresh delta fetch. | S each |

Safety model: preview-first (010b IS the dry run); additive-only =
the no-rollback argument (never UPDATE non-empty, never DELETE,
never touch existing stage/assignee); verification counts per run
per entity; failures drillable, never silent; FUB 429/5xx =
transport class (pace, honor Retry-After, wait forever), malformed
entities = quality class (recorded, capped, reported).

Credential: `FubApiKey` newtype (IntakeToken shape: no
Display/PartialEq, redacted Debug, `reveal()` only at the auth
header build); recommended encrypted-at-rest via a
`seal_migration_credential` crypto sibling; admin-only; GET returns
status + last-4 only; deletable; never logged.

Runtime: in-process worker (extraction template; claim = one run,
inner per-page cursor commits so restart resumes) + polling status
row; `crm-admin fub-*` CLI subcommands over the same domain fns; no
separate service (D-002). Trait-seam FUB client with fixture fakes
(LeadExtractor precedent); sanitized real-response fixtures per the
007h discipline.

Attribution: `inquiry.source` keeps FUB's original string verbatim
(D-006/D-012 — "Zillow" stays "Zillow"); `origin='migration'` is
the import marker; correlation = run id chains the audit.

Tensions recorded: thesis §4.1's "notes, tasks, custom fields must
reconcile" is satisfied by preserved-and-reported until their models
exist (state at approval, not discovery); `inquiry.raw_payload_id`
has no FK (provenance rides `raw_payload` with the new format, not
`migration_raw`, to keep the column unambiguous).

TO-VERIFY at 010a spec time: FUB auth form (Basic, key as
username), endpoints/pagination, rate limits (~250 req/10s
tier-dependent), `updatedSince`, `lastCommunication` field, trial
account availability + API writability for walkthrough seeding.
