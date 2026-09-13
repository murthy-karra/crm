# Mobile 004 contract checkpoint

The frozen implementation fixtures are [mobile/contracts/mobile004/MOBILE_004_CONTRACT.md](../../mobile/contracts/mobile004/MOBILE_004_CONTRACT.md).

Mobile 004 keeps `mobile-v1` and adds only `change_person_stage`,
`stage_revisions`, and `stage_catalog` capabilities. Reconciliation defaults
`include_stage_catalog` to false. Opt-in responses pin a positive catalog
revision, link a max-100-row `(position,id)` stage page, and revalidate at every
page and seal; catalog mutation returns `409 generation_changed`.

Stage operations have exact payload `{person_id,stage_id,expected_stage_revision}`.
They return a durable `person_stage` receipt with a nonnull committed stage
revision. Replay precedes fresh baseline checks; revision comparison precedes
same-target no-op. Current-stage reads are authorized/context-bound conflict
baselines only. The detailed fixtures also define opaque/malformed/foreign and
review-hold errors, receipt/check constraints, trigger path and lock order.
