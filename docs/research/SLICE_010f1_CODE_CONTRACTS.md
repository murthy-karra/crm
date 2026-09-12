# Slice 010f1 — Retained tags/custom-field source and destination seams

Read-only discovery at main `f01c2e3`, 2026-09-11, following 010c deployment.
This supports the [draft spec](../specs/SLICE_010f1.md) and
[draft execution brief](../tasks/SLICE_010f1_IMPL.md); it approves neither.
No application test, database/runtime operation, live FUB request or new external
schema research was performed for this discovery. References below are current
code observations; proposal paragraphs are deliberately separate.

## 1. Authority and retained source

Read AGENTS, DECISION_LOG D-050/051/058/063–065, the architecture baseline and
prompt library 02-specify/03-plan. D-065's latest follow-up authorizes next-import
planning only. D-064's separate People, explicit stage approval and review hold
remain accepted; tags/fields, activation and delta semantics were not approved
by that People-import decision.

| Existing evidence | Consequence |
|---|---|
| [010b source record](SLICE_010b_FUB_SOURCE_CONTRACT.md), “Core source contracts” and “Remaining coverage” | People `allFields` and `/customFields` are captured; embedded tags do not prove a standalone tag catalog or complete source-account visibility |
| [010b schema manifest](SLICE_010b_SOURCE_SCHEMA_SHA256.json) | Saved public documentation is hash-identified, not a live-account fixture or immutable vendor version |
| [010b spec](../specs/SLICE_010b.md), §§3/5/remaining coverage; [concrete profile](../specs/SLICE_010b_CONTRACT.md) | Lossless capture plus bounded display projections; the latter may omit values and cannot authorize execution |
| [import_source.rs](../../backend/crates/crm-app/src/domain/migration/import_source.rs):18–28,67,102–142 | `ExtractedRecord` preserves canonical bytes and every top-level field as exact canonical JSON text; executable entity kinds currently People/Stage/User only |
| [import_worker.rs](../../backend/crates/crm-app/src/domain/migration/import_worker.rs):313–390 | Preparation selects only people/users/stages, then qualifies capture/ordinal/source ID/HMAC and all observations; a new customfields extractor must be added explicitly |
| [imports.rs](../../backend/crates/crm-app/src/domain/migration/imports.rs):434 | Parent eligibility requires People/users/stages exhaustion, not customfields exhaustion. Parent completion therefore cannot prove metadata readiness |
| [import_worker.rs](../../backend/crates/crm-app/src/domain/migration/import_worker.rs):1006–1057 | Execution resolves Person by Org/account/source identity and persists committed result/provenance; contact overlap is not identity |

During discovery the existing public People/customfields OAS files under
`/private/tmp/crm-010b-source/` matched their stored manifest hashes:
People `9a9b1049ec0ede0a1088ad15aa4c364b181f33c2dad9831b4c62c8bab6bba1aa`;
customfields `b339d002748633de0cc4e7f32adeb649ed6ef8e1c094d6af9992eabf971ee914`.
Those private paths are supplementary local evidence, not a required deployment
artifact. The committed manifest/source record remains the portable reference.
Some public response examples contain invalid JSON notation; synthetic fixtures
must be labelled synthetic, not repaired-and-relabelled live captures.

The customfields example has `id`, human `label`, stable machine `name`, `type`,
optional `isRecurring`, and string-array `choices` for `dropdown`. Example names
include `customBirthday` and `customLookingFor`; a recurring date exists in that
example. People values are selected from top-level machine-key properties, not
solely a nested `customFields` object. The People example includes a string-array
`tags`. No qualified source option IDs are supplied by this profile.

**Proposed seam:** a child extractor uses the same lossless Node/canonical rules,
rechecks People and customfields captures at the parent's exact boundary, and
retains exact per-field evidence. Do not extend the old parent family CHECKs or
mutate its manifest to smuggle in child work. The whole child is ineligible when
customfields capture is incomplete; tags-only continuation is deliberately not
part of this first proposal. Unrelated completed-with-gaps coverage stays visible.

## 2. Tag destination contracts

The accepted [011e tag specification](../specs/SLICE_011e.md):68–88 records both
caps, first-spelling semantics and native name rules; D-051 owns rename/delete.

| Exact current seam | Current behavior / risk |
|---|---|
| [tag/commands.rs](../../backend/crates/crm-app/src/domain/tag/commands.rs):19 | Unicode trim, 1–40 code points, no `char::is_control`; this is not a source spelling-preservation contract |
| Same file:31,152–203 | Org namespace advisory lock; operational workspace; case-insensitive create-or-get retains existing spelling; 200 tags per Org |
| Same file:252–314, especially 297 | Lock Person then tag; idempotent link; **20 tags per Person** even though the catalog cap is 200; publish only after changed commit |
| Same file:435,531 and D-051 | Rename/delete admin or creator while unused under lock; tags hard-delete, unlike custom-field archival |
| [tag/queries.rs](../../backend/crates/crm-app/src/domain/tag/queries.rs):176,257,290,324,398 | Native lower-name lookup/counts, insert tag and `ON CONFLICT DO NOTHING` link; persistence is available but not an import permission |
| [tag schema](../../backend/crates/crm-api/migrations/20260909000001_tag.sql):4–32 | Unique Org/lower(name); membership-backed nonnullable created/added-by actors; Person/tag link cascade; no source/origin columns |

**Proposed seam:** typed child catalog/link operations reuse native validation,
quota and lock semantics, with the child permit in §4. Existing tag names are
not renamed. Preserve exact spellings/duplicate ordinals and explicit aliases in
encrypted migration evidence. Actual DB collation uniqueness must be checked;
Rust lowercase is insufficient to promise a native insert. First approved source
spelling for creation is a declared proposal, not an already accepted FUB rule.
More than 20 planned-plus-existing links holds the Person's new tag-link set;
never choose the first 20. Independent eligible field values can still proceed.

Tags have no vendor ID in this capture. Any derived stable child identity is
application-owned and must be labelled as such, scoped to source account and
retained exact input; never present a hash/ordinal as a FUB tag ID. Long/invalid
source names stay encrypted and do not become unbounded native/indexed keys.
Use bounded tenant/account-scoped retained HMAC keys, with qualified source field
identity included for choices, exact encrypted equality checks and collision
rejection. Permanent tombstones must not contain raw tag/choice labels. These
are proposed child identity rules, not changes to original FUB source IDs.

## 3. Custom-field destination contracts

[Slice 019](../specs/SLICE_019.md):24–58 explicitly leaves recurrence,
hide-if-empty, defaults and option reorder outside ordinary v1 behavior; source
positions/settings must remain preserved/disclosed instead of silently inferred.

| Exact current seam | Current behavior / risk |
|---|---|
| [model.rs](../../backend/crates/crm-app/src/domain/custom_field/model.rs):8–78 | Immutable text/number/date/choice; HTTP typed values use text/decimal text/date/option UUID. No boolean, recurrence, multi-choice or hideIfEmpty behavior |
| [commands.rs](../../backend/crates/crm-app/src/domain/custom_field/commands.rs):19–85 | 50 live fields/50 live options; labels trimmed, 1–60 code points, controls/U+2028/U+2029 rejected; nonchoice has no options, choice creation requires 1–50 distinct valid options |
| Same file:214–291 | Operational guard, namespace lock, current active admin, native uniqueness and quota; field creation returns label conflict rather than create-or-get |
| Same file:308 onward | Definitions/options archived rather than deleted; values remain retained; no field-type mutation |
| [model.rs](../../backend/crates/crm-app/src/domain/custom_field/model.rs):190–249 | Number `^-?[0-9]{1,15}(\.[0-9]{1,4})?$`; text ASCII-edge-whitespace trim, 1–500 code points/no controls; date 1900-01-01 through 2200-12-31 |
| [commands.rs](../../backend/crates/crm-app/src/domain/custom_field/commands.rs):740–815 | Value command locks Person/field, validates active type/option, stores actor/origin/correlation and publishes IDs-only invalidation |
| [queries.rs](../../backend/crates/crm-app/src/domain/custom_field/queries.rs):157,353 | Ordinary definition/option creation does not populate migration source keys |
| Same file:578 | Existing public definition listing includes archived items/counts but omits source/external_key; it is not a complete executable destination freeze |
| Same file:740 | Value upsert **overwrites differing values** and changes attribution; not safe to call blindly for a no-overwrite import |
| [schema](../../backend/crates/crm-api/migrations/20260915000001_custom_field.sql):6–55 | Source/external_key both null or both present; unique Org/source/key includes archived definitions; options have no source ID/key; live labels unique case-insensitively |
| Same schema:57 onward | One value per Org/Person/field, type/FK constraints, NUMERIC(19,4), nullable updated-by only for migration; Person erasure cascades native values |

**Proposed seam:** a narrow typed import persistence layer creates matching
definitions/options and writes absent/equal-only values inside the child unit.
Reuse validators; do not invoke ordinary commands and weaken their review guard.
New fields use `source=fub` plus exact qualified machine name. Mapping an existing
field does not rewrite its source pair. Same-label, same-key/different-ID,
case-folding, archived and many-source-to-one-target conflicts need explicit holds.
Source-option choice labels are not sufficient authority without the selected
field and approved option mapping. Literal labels `None`/`N/A`/`null` are strings;
there is no observed permission to reinterpret them as null values.

The native external_key is unbounded TEXT in a full-value unique B-tree key.
The child must reject NUL/native-index oversize before lookup/insert while keeping
exact evidence. The spec proposes the existing import pattern's conservative
2,048-UTF-8-byte key bound, with composite-key proof; it is not a higher customer
quota or a new global custom-field validator. Normalized label limits do not
solve an oversized machine key. Existing compatible-target mapping need not copy
an unrepresentable key into native storage.

`snapshot_compare.rs` [source](../../backend/crates/crm-app/src/domain/migration/snapshot_compare.rs):47–74,
270–420 already suggests type-compatible fields and flags native limits, recurrence
and exact-number issues. It consumes clipped preview projections. Reuse validator
knowledge, never treat that preview/candidate object as executable raw evidence.
Numeric strings, recurrence omission and malformed choices still require the
explicit conservative proposed rules; public schemas do not settle every live
value shape. Unknown shapes can be held without inventing semantics.

## 4. Parent persistence and workspace boundary

[010c migration](../../backend/crates/crm-api/migrations/20260918000001_fub_people_import.sql):
`migration_workspace` (62) uniquely binds the original Org/import/plan and
`crm-workspace-v1`; source families (76) and identity families (113) are closed.
People identity has no cascading target FK, intentionally preserving tombstones.
Results (125), receipts (142), reservations (150) and encrypted provenance belong
to that original import. Do not reuse its completed lease or rewrite these rows.

The DB write guard (217–237) permits an original running import token only for
Person/contact/stage and initial import facts. Tags, person_tag, custom_field,
custom_field_option and person_custom_field_value are ordinary guarded tables.
Thus an admin role or `Origin::Migration` cannot presently write child metadata.
[workspace_http.rs](../../backend/crates/crm-api/src/auth/workspace_http.rs):25–49
preserves admin migration/provenance 403 before generic held-member GET 409;
allowed admin reads still acquire complete authoritative shared read permits.

**Proposed seam:** additive child root/plan/qualified source/mapping/manifest/
identity/result/receipt/reservation persistence with composite tenant references
and source-account scope. A child-only private token must prove completed parent,
unchanged binding, confirmed child plan, active admin and live lease, then limit
native operations to planned metadata. Reuse the workspace lock namespace/mode,
current membership revalidation and two-second bounded row waits. No new review
mode, general permit, broker or per-Person visibility policy is needed.

The source-free worker can reuse patterns in [imports.rs](../../backend/crates/crm-app/src/domain/migration/imports.rs):203–344
(guard/ledger/receipts),788 (confirmation replay),1010–1335 (keyset/field/provenance)
and [import_worker.rs](../../backend/crates/crm-app/src/domain/migration/import_worker.rs):779 onward
(atomic units). Child ownership must remain separate from source/preview/parent
reservations. A dedicated cancellation reservation avoids exhaustion blocking a
control action. Existing release readiness/preflight must certify the selected
010f1 API/worker artifacts; keep `crm-workspace-v1` and compatible-only recovery,
not a replacement deployment system.

## 5. UI, verification and capacity evidence

Current [imports API](../../web/src/api/imports.ts),
[PeopleImportPanel](../../web/src/components/migration/PeopleImportPanel.vue),
[field inspector](../../web/src/components/migration/ImportFieldViewer.vue) and
[Person provenance](../../web/src/components/migration/PersonImportProvenance.vue)
provide reuse patterns for dirty choices, named confirmation, exact request
replay, scoped cursors and bounded full-value inspection. Keep the shared
workspace focus/visibility refresh, current-role cache fence and member waiting
view. New metadata responses are never authoritative session-mode updates.

Existing proof fixtures: [db_import_source.rs](../../backend/crates/crm-api/tests/db_import_source.rs),
[db_import_gate.rs](../../backend/crates/crm-api/tests/db_import_gate.rs),
[db_import_http.rs](../../backend/crates/crm-api/tests/db_import_http.rs),
[db_tags.rs](../../backend/crates/crm-api/tests/db_tags.rs),
[db_custom_fields.rs](../../backend/crates/crm-api/tests/db_custom_fields.rs).
Reuse the raw-qualified shared import fixture, then add actual customfields
captures. The synthetic scale fixture in
[db_import_plans.rs](../../backend/crates/crm-api/tests/db_import_plans.rs) is
explicitly plan-only, not valid executable source evidence.

Capacity arithmetic must include **20 tags per Person**, not merely the 200-tag
Org catalog cap. At 25k People: at most 500k native tag links, 1.25M live-field
values, 2,500 live options; historical/held migration evidence adds separate bytes.
This corrects the initial discovery estimate of 5M links, which ignored the
per-Person limit. It is not measured PostgreSQL disk capacity. Prove per-unit
fan-out/reservation bounds, SQL row/index access and representative aggregate
bytes without raising quotas or mandating an all-maxima fixture.

Use actual new/changed claim/source identity/dependency/mapping/manifest/result/
field SQL for D-050 EXPLAIN evidence at 25k People/50 members with a documented
worst realistic metadata distribution. Include dense shared tags and rare/empty
filters; distinguish bounded keyset-tail traversal from repeated whole-book scans.
No CRM reader SQL change is expected: additive imported values use existing reads.
Require the paired reader regression only if that expectation changes. Full
repository gates and synthetic browser checks still apply after implementation;
none have been run for this planning-only record.
