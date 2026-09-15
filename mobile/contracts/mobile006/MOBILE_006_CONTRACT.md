# Mobile006 metadata contract

`mobile-v1` adds the opt-in capability set `update_person_metadata`,
`metadata_revisions`, and `metadata_catalog`.

`POST /api/mobile/v1/operations` accepts kind `update_person_metadata` with:

```json
{"person_id":"uuid","expected_metadata_revision":"1","expected_catalog_revision":"1","actions":[{"kind":"add_tag","tag_id":"uuid"},{"kind":"set_field","field_id":"uuid","value":{"number":"123.4500"}}]}
```

Actions are `add_tag`, `remove_tag`, `set_field`, or `clear_field`; repeated tag
or field targets and more than 50 actions are malformed. A response has resource
type `person_metadata` and its committed revision is the resulting metadata
revision. Fresh mismatched Person or catalog tokens return `revision_conflict` or
`catalog_revision_conflict`. Exact receipt replay precedes either check.

`POST /api/mobile/v1/reconciliations` adds optional `include_metadata:true`.
The response identifies representation `metadata-v1` and a catalog URL. Catalog
pages are `GET /api/mobile/v1/reconciliations/{id}/metadata/catalog/{tags|fields|options}`.
Current conflict review is `GET /api/mobile/v1/people/{person_id}/metadata`.
All requests are bound by `X-Mobile-Context`, Organization, actor, generation,
and catalog revision. A changed catalog invalidates an opted-in generation.

## Read representations

The metadata component is requested as
`GET /api/mobile/v1/reconciliations/{generation_id}/people/{person_id}/metadata`.
It returns `generation_id`, `person_id`, `section:"metadata"`, broad `revision`,
`metadata_revision`, `catalog_revision`, complete `tags`, complete typed `values`,
and `complete:true`. A component is accepted only when its manifest broad revision,
manifest metadata revision, and the opted-in catalog revision still match.

Catalog pages have exact common fields `generation_id`, `section`, `revision`,
`items`, `next_cursor`, and `complete`. `tags` rows are `{id,name}`; `fields` rows
are `{id,label,field_type,position,archived_at}`; `options` rows are
`{id,field_id,label,position,archived_at}`. Values use one externally tagged value
shape: `{"text":"..."}`, `{"number":"12.3400"}`, `{"date":"2026-09-14"}`, or
`{"option_id":"uuid"}`. Decimal and date values are strings.

`GET /api/mobile/v1/people/{person_id}/metadata` returns the same current tags and
values plus `context_id`, `person_revision`, `metadata_revision`, and
`catalog_revision`. It is a no-store review read and cannot seal or replace a
generation.

## Bounds and failures

Every operation body is at most 128 KiB. Catalog and normal component pages contain
at most 100 rows and serialize to at most 512 KiB. Snapshot admission reads at most
10,001 tags, fields, or options per class and rejects a catalog whose measured
temporary JSON representation exceeds 8 MiB with `422 over_limit`; it never clips
a catalog and calls it complete. A single oversize catalog page/component returns
the same closed `over_limit` response.

Fresh Person-token mismatches return `409 revision_conflict`; fresh catalog-token
mismatches return `409 catalog_revision_conflict`; changed catalog/generation
traversals return `409 generation_changed`. Unknown or malformed IDs, targets,
actions, duplicate targets, malformed tokens, and empty patches return
`400 malformed_request` before a mutation. Validly shaped unavailable/deleted or
archived targets return `422 invalid_metadata`. A receipt retry with its exact
bytes returns the original receipt before either current-token check; altered bytes
return `409 operation_payload_mismatch`.

Sealing still uses `POST /api/mobile/v1/reconciliations/{id}/seal` with `{}`. An
opted-in seal additionally validates the current metadata catalog revision and all
manifest metadata revisions. It returns the established sealed generation response
or a `generation_changed` conflict; it never seals a partial metadata traversal.
