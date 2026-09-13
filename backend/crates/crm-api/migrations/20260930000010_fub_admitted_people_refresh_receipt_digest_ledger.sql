-- Receipt digests are durable bytea bindings and therefore physical ledger
-- bytes. Reconcile the previously omitted fixed-size field without rebuilding
-- any frozen preview.
CREATE TEMPORARY TABLE fub_admitted_people_refresh_receipt_digest_delta ON COMMIT DROP AS
SELECT r.id AS refresh_id,r.organization_id,r.newer_snapshot_id,
       COALESCE((SELECT sum(octet_length(x.digest)) FROM migration_admitted_people_refresh_receipt x WHERE x.refresh_id=r.id AND x.organization_id=r.organization_id),0)::bigint AS bytes
FROM migration_admitted_people_refresh r;
UPDATE migration_admitted_people_refresh r SET retained_bytes=retained_bytes+d.bytes
FROM fub_admitted_people_refresh_receipt_digest_delta d
WHERE r.id=d.refresh_id AND r.organization_id=d.organization_id AND d.bytes>0;
UPDATE migration_snapshot s SET retained_bytes=retained_bytes+d.bytes
FROM (SELECT organization_id,newer_snapshot_id,sum(bytes)::bigint AS bytes FROM fub_admitted_people_refresh_receipt_digest_delta WHERE bytes>0 GROUP BY organization_id,newer_snapshot_id) d
WHERE s.id=d.newer_snapshot_id AND s.organization_id=d.organization_id;
UPDATE migration_snapshot_storage s SET retained_bytes=retained_bytes+d.bytes
FROM (SELECT organization_id,sum(bytes)::bigint AS bytes FROM fub_admitted_people_refresh_receipt_digest_delta WHERE bytes>0 GROUP BY organization_id) d
WHERE s.organization_id=d.organization_id;
