# Slice 019 — Typed custom fields on People

**Slice 019b amendment (approved 2026-09-10):**
[Custom-field filtering](SLICE_019b.md). The 019a exclusion of custom-field filtering in §§1/10 is now followed by approved rung 019b. Amends §§5/7 for membership-cache invalidation after value changes and dependent-cache refresh after definition/option changes; the realtime wire token is unchanged. Archive-based filter validity is specified in 019b §3.

**Status: APPROVED by the user on 2026-09-10 after independent review
(READY WITH CORRECTIONS; fifteen items applied, none a human decision).
Approval authorizes committing the planning documents and implementing in
the Slice 019 lane; the code commit, merge, push and deployment are asked
for separately.** Prepared against `main`
at `0212013` (D-058 recorded). Rung **019a** is this specification: one
large-M slice, one lane, backend then Web. Rung **019b** (custom-field
filter clauses) is scheduled but not specified here; it receives its own
specification and approval (D-058 §1). Brief:
[SLICE_019_IMPL.md](../tasks/SLICE_019_IMPL.md). Decisions:
[D-058](../decisions/DECISION_LOG.md) (scope, types, who edits,
archive-only, filtering deferred), D-051 (the tags precedent it diverges
from on delete), D-053 (content posture), D-050 (envelope), D-021 (typed
commands), D-029 (PII-free ledger), D-045 and UI_STYLE (visuals).
AGENTS §4.6 already classes custom-field definitions and values as
relational CRUD.

Follow Up Boss parity, verified from its API docs on 2026-09-10: a custom
field has `label`, `name` (the API key, e.g. `customClosePrice`), `type`
in {text, date, number, dropdown}, `choices` (dropdown only),
`isRecurring` (date only), `hideIfEmpty`, `orderWeight`; management is
account-owner only. 019a maps text, number, date and dropdown → choice;
`isRecurring` and `hideIfEmpty` are not modelled (additive later, §8).

---

## 1. Scope and product behaviour

An Organization admin defines **custom fields** for People under Manage →
Fields: a label, a type (text, number, date, single choice), and for a
choice field its options. Every member sees the fields on a Person's page
in a **Details** card and sets or clears a value per field. The Operator
reads the values. Fields and options are **archived**, never deleted:
archiving hides the field and keeps every value; un-archiving brings it
back.

In scope (019a): three tables; typed commands; the definition, option and
value routes; `custom_fields` on the Person detail read; a realtime change
token; the Operator `PersonDetail.custom_fields`; the Details card with
per-type inline editors; the Manage → Fields page (admin) with reorder,
rename, archive, restore and an options editor; tests; import-ready
columns.

Out of scope: filter clauses, saved-list or Today-rule use, and sorting by
a custom field (019b for clauses; sorting never); a People-list column or
a preview change; boolean, multi-choice, money, URL or phone types;
recurring dates and hide-if-empty; required fields and defaults; a
per-field permission; a derived API key on definitions (the import binds
FUB's `name` to `external_key`; nothing else needs a key); option
reordering (creation order; the import sets positions); history facts for
value changes (AGENTS §4.6); Operator write tools (a later D-057-style
rung); the FUB import itself (SLICE_010 010f uses the columns in §8);
mobile.

## 2. Persistence (one additive migration, backend-owned)

`crm-api/migrations/20260915000001_custom_field.sql` (20260914 is taken
by Slice 018).

```sql
CREATE TABLE custom_field (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id    UUID NOT NULL REFERENCES organization (id),
    label              TEXT NOT NULL
        CHECK (char_length(label) BETWEEN 1 AND 60 AND label = btrim(label)
               AND label !~ '[[:cntrl:]]'),
    field_type         TEXT NOT NULL
        CHECK (field_type IN ('text', 'number', 'date', 'choice')),
    position           INTEGER NOT NULL,
    archived_at        TIMESTAMPTZ,
    created_by_user_id UUID NOT NULL,
    -- Import provenance (§8): both null, or both set.
    source             TEXT,
    external_key       TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((source IS NULL) = (external_key IS NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (id, organization_id),
    -- Lets a value row bind (field, organization, type) in one FK, so a
    -- type-mismatched value is unpersistable and a type change is
    -- refused by the database while any value exists (default
    -- ON UPDATE NO ACTION).
    UNIQUE (id, organization_id, field_type),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
CREATE UNIQUE INDEX custom_field_org_live_label_key
    ON custom_field (organization_id, lower(label)) WHERE archived_at IS NULL;
CREATE UNIQUE INDEX custom_field_org_source_key
    ON custom_field (organization_id, source, external_key) WHERE source IS NOT NULL;

CREATE TABLE custom_field_option (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    field_id        UUID NOT NULL,
    label           TEXT NOT NULL
        CHECK (char_length(label) BETWEEN 1 AND 60 AND label = btrim(label)
               AND label !~ '[[:cntrl:]]'),
    position        INTEGER NOT NULL,
    archived_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (updated_at >= created_at),
    UNIQUE (id, field_id, organization_id),
    FOREIGN KEY (field_id, organization_id)
        REFERENCES custom_field (id, organization_id)
);
CREATE UNIQUE INDEX custom_field_option_live_label_key
    ON custom_field_option (field_id, lower(label)) WHERE archived_at IS NULL;

CREATE TABLE person_custom_field_value (
    organization_id    UUID NOT NULL,
    person_id          UUID NOT NULL,
    field_id           UUID NOT NULL,
    field_type         TEXT NOT NULL,
    text_value         TEXT
        CHECK (text_value IS NULL OR (char_length(text_value) BETWEEN 1 AND 500
               AND text_value = btrim(text_value, E' \t\r\n')
               AND position(E'\n' IN text_value) = 0)),
    number_value       NUMERIC(19, 4),
    date_value         DATE,
    option_id          UUID,
    -- NULL only for imported rows (the task.sql pattern).
    updated_by_user_id UUID,
    origin             TEXT NOT NULL,
    correlation_id     UUID NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, person_id, field_id),
    CHECK (num_nonnulls(text_value, number_value, date_value, option_id) = 1),
    CHECK (CASE field_type
             WHEN 'text'   THEN text_value   IS NOT NULL
             WHEN 'number' THEN number_value IS NOT NULL
             WHEN 'date'   THEN date_value   IS NOT NULL
             WHEN 'choice' THEN option_id    IS NOT NULL
             ELSE FALSE END),
    CHECK (updated_by_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (updated_at >= created_at),
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (field_id, organization_id, field_type)
        REFERENCES custom_field (id, organization_id, field_type),
    FOREIGN KEY (option_id, field_id, organization_id)
        REFERENCES custom_field_option (id, field_id, organization_id),
    FOREIGN KEY (organization_id, updated_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
CREATE INDEX person_custom_field_value_field_idx
    ON person_custom_field_value (organization_id, field_id);

GRANT SELECT, INSERT, UPDATE ON custom_field TO crm_app;
GRANT SELECT, INSERT, UPDATE ON custom_field_option TO crm_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON person_custom_field_value TO crm_app;
```

Rules the schema encodes: a value's type must equal its field's type
(the three-column FK; `MATCH SIMPLE` lets a NULL `option_id` or
`updated_by_user_id` skip its FK); an option must belong to the field it
is stored against; a non-member cannot be an author; the Person cascade
erases values (D-015 §5; SLICE_002 §2's erasable set and O-013's runbook
gain the table); definitions and options have no `DELETE` grant
(archive-only, D-058 §2); `origin` is `web_session` or `migration` as on
`task`. Tests pin these by SQLSTATE (23514 for a CHECK, 23503 for an FK),
not by constraint name: the long FK's auto-generated name exceeds 63
characters.

**Numbers cross the Rust boundary as text.** The workspace enables no
decimal type for sqlx and `Cargo.*` is not owned, so the number value is
`Number(String)` validated by a pure function (`^-?[0-9]{1,15}(\.[0-9]{1,4})?$`,
else `invalid_value`), bound as `CAST($n::text AS numeric)` and read as
`trim_scale(number_value)::text` (Postgres 18). Canonical wire form:
trailing zeros trimmed, so `"12.50"` returns as `"12.5"`; a JSON number
rather than a string is 400.

Typed columns rather than JSONB on `person`: every 019b clause becomes one
static NULL-bound predicate per type; clear is a row delete and erasure is
the cascade; `PersonSummary` and the People rows stay byte-identical
(011e rule 6); the composite FK gives a type guarantee no JSONB CHECK can.

Limits (D-050, command-enforced 409s): 50 live definitions per
Organization; 50 live options per field; text 500 characters; numbers as
above; dates are calendar dates between 1900-01-01 and 2200-12-31.

## 3. Typed commands (new `crm-app/src/domain/custom_field/`)

`model.rs`, `commands.rs`, `queries.rs`, `error.rs` in the `tag/`
layout. Static SQL only; every statement in the offline cache. Seven
commands.

**Definition commands, admin only.** Each takes the per-Organization
advisory lock `custom_fields:<organization_id>` (the tag lock pattern),
re-reads the actor's membership `FOR SHARE` and requires `status =
'active'` and `role = 'admin'` (→ `Forbidden`), then acts:

- `CreateCustomField { label, field_type, options: Vec<String> }` →
  `position` = max(live) + 1; options created in order for `choice`
  (non-empty, ≤ 50, distinct case-insensitively → `InvalidValue`
  otherwise; forbidden for the other types → `TypeMismatch`);
  `LimitReached` at 50 live fields; `LabelTaken` on a live
  case-insensitive duplicate.
- `UpdateCustomField { field_id, label, archived: bool }` — full
  replace (the 016 `UpdateTask` shape) → `changed`. Rename applies to a
  live or archived row; the live-label uniqueness check runs only when
  the row is, or becomes, live (→ `LabelTaken`). Archiving sets
  `archived_at` and keeps every value; un-archiving clears it and sets
  `position` = max(live) + 1 so the live order stays `1..n`. The type
  is immutable (no field in the command; the database also refuses a
  change while any value exists).
- `ReorderCustomFields { field_ids }` — the full order of every live
  field, no more, no fewer (→ `InvalidValue`); positions rewritten
  `1..n`.
- `AddCustomFieldOption { field_id, label }` (choice fields only →
  `TypeMismatch`; `OptionLimitReached`; `OptionLabelTaken`; permitted
  on an archived field) → `position` = max(live) + 1.
- `UpdateCustomFieldOption { field_id, option_id, label, archived }` —
  the same full-replace semantics as the field. Archiving an option
  keeps the values that hold it; they render with the archived option's
  label and the editor offers only live options plus "keep current".

**Value commands, any active member** (D-058 §2; the `AssignPerson` /
`ChangePersonStage` / `AddPersonTag` rule; no membership re-read beyond
the session, as in `add_person_tag_attempt`):

- `SetPersonCustomFieldValue { person_id, field_id, value }` where
  `value: CustomFieldValue` is `Text(String) | Number(String) |
  Date(NaiveDate) | Option(CustomFieldOptionId)`. Steps: `lock_person`
  in the session Organization (→ `NotFound`); load the field `FOR SHARE`
  in the Organization (→ `NotFound`; archived → `FieldArchived`);
  variant versus `field_type` (→ `TypeMismatch`); validate (trim and
  the text rules, the number pattern, the date range → `InvalidValue`);
  for `Option`, the option must be live and belong to this field (→
  `UnknownOption`, byte-identical for archived, foreign or nonexistent);
  upsert on the primary key with `WHERE row IS DISTINCT FROM excluded` →
  `changed`, bumping `updated_at` and `updated_by_user_id` only on
  change; `origin` = `web_session`, `correlation_id` from the context.
  Returns the Person's full value list.
- `ClearPersonCustomFieldValue { person_id, field_id }` → loads the
  field in any state for the 404, then `DELETE`; `changed` = rows
  affected; permitted on an archived field (the Web does not show
  archived fields).

Both publish `person.changed { custom_field_changed }` only when
`changed`. Command structs carrying a value or label implement a
redacting `Debug` (SLICE_016 §3). Values are never logged.

Queries: `list_definitions(org)` (live and archived, with `person_count`
= rows holding a value, and the options); `values_for_person(org,
person)` in field position order for live fields with a set value,
carrying the option label even when the option is archived.

## 4. HTTP (SLICE_016 §4 shape; declared additive; SLICE_002 §5 gains rows by pointer)

All routes require an authenticated session with an active Organization
(`AuthContext`); platform-only sessions are 401 on every route. Definition
and option **writes** take `OrgAdminContext` (403 `forbidden` for a
non-admin at the extractor) **and** the command re-checks the role under
the lock. Handlers order extractors `Path, OrgAdminContext, Json`, so the
precedence is: malformed path uuid 400 → 401 → 403 (admin writes only) →
400 body → 404 `not_found` → 409 → 422 → 503 `unavailable`. Bodies observe
the house 128 KiB cap; malformed JSON is 400 `malformed_request`.
`PUT /api/custom-fields/order` is a static segment beside `{field_id}`;
the router gives the static route priority, so `order` never reaches the
uuid extractor (a test pins it, §12).

`CustomField` = `{"id","label","field_type","position","archived_at":
ts|null,"person_count","options":[{"id","label","position","archived_at":
ts|null}]}` (options ordered `position, id`; `[]` for non-choice types).
`Value` = `{"field_id","label","field_type","value": {"text": string} |
{"number": string} | {"date": "YYYY-MM-DD"} | {"option_id": uuid},
"option_label": string|null, "updated_at"}`. The `value` body is the outer
struct `{"value": …}` with `deny_unknown_fields`; the inner externally
tagged enum rejects zero, two or unknown keys by construction.

| Route | Auth | Success | Errors |
|---|---|---|---|
| `GET /api/custom-fields` | member | 200 `{"fields":[CustomField]}` live first by `position, id`, then archived by `archived_at DESC, id` | 401, 503 |
| `POST /api/custom-fields` `{"label","field_type","options"?:[string]}` | admin | 201 `{"field":CustomField}` | 400, 403, 409 `custom_field_limit_reached`, 409 `custom_field_label_taken`, 422 `type_mismatch` (options on a non-choice type), 422 `invalid_value` |
| `PUT /api/custom-fields/order` `{"field_ids":[uuid]}` | admin | 200 `{"fields":[CustomField]}` | 400, 403, 422 `invalid_value` (not exactly the live set) |
| `PUT /api/custom-fields/{field_id}` `{"label","archived"}` | admin | 200 `{"field","changed"}` | 400, 403, 404, 409 `custom_field_label_taken` |
| `POST /api/custom-fields/{field_id}/options` `{"label"}` | admin | 201 `{"field"}` | 400, 403, 404, 409 `option_limit_reached`, 409 `option_label_taken`, 422 `type_mismatch` |
| `PUT /api/custom-fields/{field_id}/options/{option_id}` `{"label","archived"}` | admin | 200 `{"field","changed"}` | 400, 403, 404, 409 `option_label_taken` |
| `PUT /api/people/{person_id}/custom-fields/{field_id}` `{"value": <typed>}` | member | 200 `{"custom_fields":[Value],"changed"}` | 400, 404 `not_found` (Person or field; byte-identical across Organizations), 409 `field_archived`, 422 `type_mismatch`, 422 `invalid_value`, 422 `unknown_option` |
| `DELETE /api/people/{person_id}/custom-fields/{field_id}` | member | 200 `{"custom_fields":[Value],"changed"}` | 404 |

Eight routes. `GET /api/people/{id}` gains top-level `"custom_fields":
[Value]` beside `tags`: set values on **live** fields only, in field
position order, assembled in `routes/people.rs::get_person` beside `tags`
and `tasks` (no change to `crm-app/src/domain/person/`). The Web renders
the empty rows from `GET /api/custom-fields`. `GET /api/people` rows are
byte-identical (011e rule 6). Error envelopes never echo input. New
`ApiError` variants land beside `TagLimitReached`.

## 5. Realtime (SLICE_003 §6 pointer)

`PersonChange` gains `CustomFieldChanged` (wire `custom_field_changed`),
published on the existing `person.changed` event after a set or clear that
changed a row; ids only. The backend token table gains the pair. The
Web's closed token union gains the member, and the handler gains a
`custom_field_changed` arm that invalidates only `queryKeys.person(orgId,
personId)` (the `note_changed` precedent), because a value touches
nothing else in 019a; 019b widens it. Definition and option changes
publish nothing (the tag rename precedent): the mutating client
invalidates its own `customFields` query; other tabs refetch normally
(D-050's one-tab envelope).

## 6. Operator (SLICE_005 §5 pointer)

`PersonDetail` gains `custom_fields: Vec<CustomFieldView { label:
UntrustedText, value: UntrustedText }>`: live fields with a set value in
position order (≤ 50 by the cap); the label because it is admin-authored
text, the value rendered canonically (text as is; number as its trimmed
decimal string; date as `YYYY-MM-DD`; choice as the option label), each
through the untrusted wrapper. The rendered system prompt's untrusted-text
parenthetical gains "custom field labels and values", pinned by a new
assertion in the prompt tests. `PersonCard`, the tool count, the snapshot
and the crate fences are unchanged. No write tool (§1). LATER: fifty
500-character values are a 25k-character worst case in the model's view
(AGENTS §5.3); acceptable at the envelope, no cap now.

## 7. Web (UI_STYLE and D-045 bind)

- **Person page, Details card** after Contact methods: every live field
  in position order, label left, editor right. Text: input, Enter or
  blur saves, Escape reverts. Number: input with `inputmode="decimal"`,
  client-side pattern check, the string sent verbatim, the server's
  canonical form rendered back. Date: the calendar-date input. Choice:
  the `Select` with the live options, a "keep current" entry when the
  held option is archived, and a Clear entry; the stage `Select` at
  `PersonDetailView.vue` ~1388 is the pattern including its inline
  `describeApiError` line. Each row saves independently with its own
  pending and error state; a 409 `field_archived` or a 404 refetches
  definitions and the Person. Empty state when no live field exists:
  "No custom fields yet." and, for admins, a link to Manage → Fields.
  Pessimistic mutations on the person-mutation key, settling through the
  existing Person settle helper (so they take part in the realtime hold
  and do not collide with the Slice 014 optimistic mutations).
  `queryKeys.customFields(orgId)` carries the 10 s `staleTime` precedent
  so the definitions are fetched once per navigation burst, not per
  Person.
- **Manage → Fields** at `/manage/fields`, `requiresOrgAdmin: true` (the
  Members and Intake pattern; Tags is deliberately a member route and is
  not the precedent), with the nav entry in the admin block of
  `AppShell.vue`: a table of live fields (label, type, people with a
  value), inline rename, up and down reorder (no drag), Archive through
  `ConfirmDialog` naming the count (sends `archived: true`), an options
  editor for choice fields (add, rename, archive, restore; creation
  order), an "Archived" section with Restore (sends `archived: false`).
  "New field" form: label, type, and for choice an initial options list.
  Types `CustomField`, `CustomFieldOption`, `PersonCustomFieldValue`.
- Nothing on the People list or the preview.

## 8. Import readiness (SLICE_010 010f; schema only)

`custom_field.source` / `external_key` (FUB `name`, e.g.
`customClosePrice`), `person_custom_field_value.origin = 'migration'`
with `correlation_id` = run id and `updated_by_user_id` NULL. A future
`ImportCustomField` maps FUB `type` (`dropdown` → `choice`),
`orderWeight` → `position`, `choices` → options matched
case-insensitively by label (positions in FUB's order), `isRecurring` →
an additive column when needed, `hideIfEmpty` → dropped. Values keyed by
`(person, field)` are idempotent on re-run. No import code in 019.

## 9. Authorization, tenant isolation, failure and observability

Literal `organization_id` predicate in every statement; composite FKs
make a cross-Organization value, a foreign option, a type-mismatched value
or a non-member author unpersistable. Definition writes: extractor 403
first, then the command's `FOR SHARE` membership re-read under the
Organization lock (a concurrently demoted admin gets 403 and writes
nothing; the tag command test at `db_tags.rs` ~772 is the precedent, so
one such test on one command suffices). Value writes: any active member,
`lock_person` in the session Organization. Platform admins have no tenant
access. Unknown or foreign ids on any mutation are 404 `not_found`,
byte-identical. `person_count` and archived labels are visible to every
member, as `GET /api/tags` already exposes counts.

Failure: 503 `unavailable` on database failure; an archive racing a value
PUT yields 409 `field_archived` which the client resolves by refetch;
set-to-same and clear-when-absent are `changed: false` with no
publication; a value on an option archived since the page loaded is
refused 422 `unknown_option` and the editor refetches.

Observability: spans `custom_field.create|update|reorder`,
`custom_field_option.add|update`, `person_custom_field.set|clear`, all
`skip_all`, with `organization_id`, `actor_id`, `person_id`, `field_id`,
`field_type`, `outcome`, and `value_chars` for text; **labels and values
never appear in spans, logs, error envelopes, the ledger or the realtime
payload**; the Operator ledger records Person ids only (D-029).

## 10. Contract declaration and amendment ownership

Declared additive changes (AGENTS §11): three tables; the eight routes in
§4; `GET /api/people/{id}` gains `custom_fields`; `PersonChange` gains
`custom_field_changed`; Operator `PersonDetail` gains `custom_fields`
(model-facing; the tool snapshot is unchanged); the `ApiError` variants.
Pointer lines: SLICE_002 §2 (the erasable CRUD set gains
`person_custom_field_value`) and §5 (routes), SLICE_003 §6 (realtime),
SLICE_005 §5 (Operator), SLICE_011_LADDER "explicitly not in this
ladder" and SLICE_010_LADDER 010f (D-058). `GET /api/people`,
`PersonSummary`, the fourteen filter statements and the tool definitions
are untouched.

## 11. Performance (D-050)

The detail read adds one primary-key-prefix statement per Person page
load. `GET /api/custom-fields` adds one grouped count over
`person_custom_field_value` by field, index-only on `(organization_id,
field_id)`, at most 1.25M entries at the envelope (25k People × 50
fields); it runs whenever the definitions list is fetched, which the
Web's `staleTime` limits to once per navigation burst. The lane reports
the `EXPLAIN` of the count at a seeded 25k × 5 shape once. No statement in
the People, saved-list or Today paths changes. Reported, not gated.

## 12. Acceptance criteria and required tests

- **DB (`crm-api/tests/db_custom_fields.rs`, registered in
  `tests/all.rs`; the `db_tags.rs` pattern):** (TRUST) cross-Organization
  field, option and Person on every mutation → 404 byte-identical; a
  non-admin on every definition write → 403 with no write (one loop over
  the admin routes, with a positive control); the demoted-admin race on
  one definition command; (CONTRACT) raw-SQL inserts of a
  cross-Organization value, a value whose `field_type` disagrees with the
  field, an option of another field, a two-column value and a
  zero-column value fail with SQLSTATE 23514 or 23503; `UPDATE
  custom_field SET field_type` fails while a value exists and succeeds
  when none does; grants pinned (no `DELETE` on the two definition
  tables; `DELETE` on values); archive hides the field from the detail
  read and from the Operator view while the row and its values survive,
  un-archive reverses with `position` = max + 1, `person_count`
  unchanged across both; (BOUNDARY) a value holding an archived option
  still carries `option_label` on the detail read; setting an archived
  or foreign option → 422 `unknown_option` byte-identical; setting on an
  archived field → 409; limits → 409 at 50 and 50; label uniqueness
  case-insensitive among live rows and an un-archive clash → 409; type
  mismatch per type; (CONTRACT) number canonical form (`"12.50"` in,
  `"12.5"` out; `"1e5"`, `"1.23456"`, `"1000000000000000"`, a JSON
  number → 422 or 400); date range; set and clear idempotency with
  `changed` and no publication on `false`; Person delete cascades values;
  `db_schema.rs` enumerations gain the three tables.
- **HTTP:** per-route precedence; 401 for platform-only sessions on every
  route; (CONTRACT) `PUT /api/custom-fields/order` reaches the reorder
  handler and `PUT /api/custom-fields/not-a-uuid` is 400; the `value`
  body rejects two keys, zero keys and unknown keys with 400; `GET
  /api/people` byte-identical.
- **Operator:** `custom_fields` present with both fields wrapped; archived
  and unset fields absent; a sentinel in a value never reaches spans or
  the ledger (the CaptureWriter test); the prompt parenthetical
  assertion; crate fences green.
- **Web (Vitest):** Details card renders live fields in order, empty
  state for members and the admin link; each editor type saves and
  renders the returned value, Escape reverts, an error shows inline;
  archived-option "keep current"; the `custom_field_changed` arm
  invalidates only the Person query; Fields page: create per type,
  rename, reorder, archive with the confirm naming the count, restore,
  options editor. The generic admin route guard is not re-tested.
- **Gates:** `sqlx-prepare`, `check`, `check-db`, once on the final tree
  by the coordinator.
- **Walkthrough (coordinator, QA runtime):** alice (admin) creates
  "Budget" (number), "Anniversary" (date), "Lead temperature" (choice:
  Cold, Warm, Hot) and "Referrer" (text); carol (member) sets all four on
  a Person and clears one; alice archives "Referrer" and its value
  disappears from the page while the count stays; restore brings it back
  with the value; carol cannot open Manage → Fields; bob (Best Realty)
  sees no Acme fields; the Operator answers "what is Grace's budget"
  from `custom_fields`.

## 13. Safe defaults adopted (veto-able; not re-litigated in-lane)

Four types (D-058); any active member sets values (D-058); archive-only
(D-058); **definitions and options are managed by Organization admins
only** (FUB parity; D-051's creator path has no analogue because a
definition is Organization schema); no derived key column; label
uniqueness case-insensitive among live rows; type immutable after
creation; limits 50 / 50 / 500 / the number pattern / the date window;
dates as calendar `DATE`; numbers as trimmed decimal strings on the wire;
the detail read carries set values on live fields only; 404 rather than
an `unknown_field` code on mutation paths (`invalid_field` is reserved for
019b's filter 422); `custom_field_changed` as a new token with a
Person-only invalidation; the admin extractor plus the command re-check;
full-replace `PUT` for rename, archive and restore; un-archive appends
to the live order; options in creation order with no reorder; an
archived option keeps rendering where held; option writes permitted on
an archived field; Operator label and value both untrusted; `hideIfEmpty`
and `isRecurring` not modelled; no People-list column.

## 14. Delivery

One lane (Claude Sonnet 5), one writer, branch `slice-019-custom-fields`
from `main` in `../crm-worktrees/019`, backend first with a checkpoint
after the migration, commands, routes, Operator read and DB tests are
green on targeted runs, then the Web. **Review round 1 covers the
backend at the checkpoint and round 2 the Web plus the round-1 fixes**,
so the two-round cap (D-050) maps onto the checkpoint. Size: large M.

Ownership: `backend/crates/crm-app/src/domain/custom_field/**` (new);
`backend/crates/crm-app/src/realtime/events.rs` (the variant and the token
table); `backend/crates/crm-api/src/routes/custom_fields.rs` (new), the
router registration, `routes/people.rs` (the value routes and the
`get_person` addition), `src/error.rs` (the variants);
`backend/crates/crm-api/src/operator/backend.rs`,
`backend/crates/crm-operator/src/views.rs`, `prompts/system.md` (the
parenthetical) and the prompt assertion in `service.rs` tests; the new
migration; `.sqlx/`; `backend/crates/crm-api/tests/db_custom_fields.rs`,
`tests/all.rs`, `db_schema.rs` (enumerations), `db_operator.rs` (the
sentinel round); `web/src/api/{types.ts,queries.ts}` (`queryKeys` lives
in `queries.ts`), `web/src/realtime/events.ts`, `web/src/router.ts`,
`web/src/components/AppShell.vue` (the nav entry),
`web/src/views/PersonDetailView.vue` and test, `web/src/views/FieldsView.vue`
and test (new). Not owned: `crm-app/src/domain/person/`, the fourteen
filter statements and their `.sqlx` entries, `PersonSummary`,
`crm-operator/src/tools.rs` and the snapshot, `docs/`, `Cargo.*`, any
other migration. The coordinator runs review and test analysis (two
rounds at most, D-050), the once-only final-tree gates, the walkthrough,
and the commit and merge gates.
