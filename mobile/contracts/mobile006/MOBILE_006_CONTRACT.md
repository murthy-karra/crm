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
