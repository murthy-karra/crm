-- D-050 single plan-shape pass. Session-local tables preserve actual deployed
-- columns, constraints and indexes. Copy encrypted sample widths, never decrypt;
-- copied ciphertext is deliberately NOT qualified comparison evidence.
-- 25k People + 25k notes (list + detail) + 50k tasks = 100k source entities,
-- 125k observations and 26k capture pages. No persisted application data changes.
\set ON_ERROR_STOP on
BEGIN;
SELECT id AS report, organization_id AS org, baseline_snapshot_id AS snapshot
FROM public.migration_core_change_report WHERE state='completed' ORDER BY created_at DESC LIMIT 1 \gset
CREATE TEMP TABLE migration_core_change_group (LIKE public.migration_core_change_group INCLUDING ALL);
CREATE TEMP TABLE migration_core_change_variant (LIKE public.migration_core_change_variant INCLUDING ALL);
CREATE TEMP TABLE migration_core_change_note_key (LIKE public.migration_core_change_note_key INCLUDING ALL);
CREATE TEMP TABLE migration_snapshot_capture (LIKE public.migration_snapshot_capture INCLUDING ALL);
CREATE TEMP TABLE migration_snapshot_record (LIKE public.migration_snapshot_record INCLUDING ALL);
INSERT INTO migration_core_change_group
SELECT gen_random_uuid(),:'report'::uuid,:'org'::uuid,
 CASE WHEN g<=25000 THEN 'people' WHEN g<=50000 THEN 'notes' ELSE 'tasks' END,
 g::text,g::text,t.nonce,t.ciphertext,
 CASE WHEN g%10=0 THEN 'changed' ELSE 'unchanged' END,t.output_nonce,t.output_ciphertext
FROM generate_series(1,100000) g CROSS JOIN LATERAL
 (SELECT nonce,ciphertext,output_nonce,output_ciphertext FROM public.migration_core_change_group WHERE report_id=:'report'::uuid AND disposition IS NOT NULL LIMIT 1) t;
INSERT INTO migration_core_change_variant
SELECT id,report_id,organization_id,side::smallint,'fub-tasks-v1',decode(md5(source_key)||md5(source_key),'hex'),1
FROM migration_core_change_group CROSS JOIN generate_series(0,1) side;
INSERT INTO migration_core_change_note_key
SELECT report_id,organization_id,0,decode(md5(source_key)||md5(source_key),'hex'),source_id
FROM migration_core_change_group WHERE family='notes';
INSERT INTO migration_snapshot_capture
SELECT gen_random_uuid(),:'snapshot'::uuid,:'org'::uuid,
 CASE WHEN g<=250 THEN 'people' WHEN g<=500 THEN 'notes' WHEN g<=1000 THEN 'tasks_open' ELSE 'note_detail' END,
 g,g,decode(md5(g::text)||md5(g::text),'hex'),'synthetic-plan-v1',200,t.raw_byte_len,t.nonce,t.ciphertext,'synthetic-plan','success',false,true,clock_timestamp()
FROM generate_series(1,26000) g CROSS JOIN LATERAL
 (SELECT raw_byte_len,nonce,ciphertext FROM public.migration_snapshot_capture WHERE snapshot_id=:'snapshot'::uuid AND stream='people' LIMIT 1) t;
INSERT INTO migration_snapshot_record
SELECT gen_random_uuid(),:'snapshot'::uuid,:'org'::uuid,c.id,c.sequence,
 CASE WHEN g<=100000 THEN ((g-1)%100)::integer ELSE 0 END,
 CASE WHEN g<=25000 THEN 'people' WHEN g<=50000 OR g>100000 THEN 'notes' ELSE 'tasks' END,
 g::text,'synthetic-plan-v1',t.semantic_hmac,t.projection_nonce,t.projection_ciphertext,false
FROM generate_series(1,125000) g
JOIN migration_snapshot_capture c ON c.sequence=CASE WHEN g<=100000 THEN ((g-1)/100)+1 ELSE g-100000+1000 END
CROSS JOIN LATERAL (SELECT semantic_hmac,projection_nonce,projection_ciphertext FROM public.migration_snapshot_record WHERE snapshot_id=:'snapshot'::uuid LIMIT 1) t;
ANALYZE migration_core_change_group;
ANALYZE migration_core_change_variant;
ANALYZE migration_core_change_note_key;
ANALYZE migration_snapshot_capture;
ANALYZE migration_snapshot_record;
SELECT id AS group_id FROM migration_core_change_group ORDER BY id OFFSET 50000 LIMIT 1 \gset
SELECT id AS capture_id FROM migration_snapshot_capture WHERE sequence=100 \gset
\echo capture_keyset
EXPLAIN (ANALYZE,BUFFERS)
SELECT * FROM migration_snapshot_capture WHERE snapshot_id=:'snapshot'::uuid AND organization_id=:'org'::uuid AND sequence>12000 AND sequence<=26000 ORDER BY sequence LIMIT 1;
\echo capture_observations
EXPLAIN (ANALYZE,BUFFERS)
SELECT ordinal,family,source_id,representation,semantic_hmac FROM migration_snapshot_record WHERE capture_id=:'capture_id'::uuid AND snapshot_id=:'snapshot'::uuid AND organization_id=:'org'::uuid ORDER BY ordinal LIMIT 101;
\echo source_group_lookup
EXPLAIN (ANALYZE,BUFFERS)
SELECT id,nonce,ciphertext FROM migration_core_change_group WHERE report_id=:'report'::uuid AND organization_id=:'org'::uuid AND family='people' AND source_key='12000';
\echo variant_upsert
EXPLAIN (ANALYZE,BUFFERS)
INSERT INTO migration_core_change_variant(group_id,report_id,organization_id,side,representation,semantic_hmac)
VALUES(:'group_id'::uuid,:'report'::uuid,:'org'::uuid,0,'fub-tasks-v1',decode(repeat('12',32),'hex'))
ON CONFLICT(group_id,side,representation,semantic_hmac) DO UPDATE SET occurrences=migration_core_change_variant.occurrences+1 RETURNING occurrences;
\echo note_request_lookup
EXPLAIN (ANALYZE,BUFFERS)
SELECT source_id FROM migration_core_change_note_key WHERE report_id=:'report'::uuid AND organization_id=:'org'::uuid AND side=0 AND request_hmac=decode(md5('35000')||md5('35000'),'hex');
\echo comparison_keyset
EXPLAIN (ANALYZE,BUFFERS)
SELECT * FROM migration_core_change_group WHERE report_id=:'report'::uuid AND organization_id=:'org'::uuid AND id>:'group_id'::uuid ORDER BY id LIMIT 50;
\echo published_filtered_page
EXPLAIN (ANALYZE,BUFFERS)
SELECT id,output_nonce,output_ciphertext FROM migration_core_change_group WHERE report_id=:'report'::uuid AND organization_id=:'org'::uuid AND family='people' AND disposition='changed' AND id>:'group_id'::uuid ORDER BY id LIMIT 51;
\echo published_unfiltered_page
EXPLAIN (ANALYZE,BUFFERS)
SELECT id,output_nonce,output_ciphertext FROM migration_core_change_group WHERE report_id=:'report'::uuid AND organization_id=:'org'::uuid AND id>:'group_id'::uuid ORDER BY id LIMIT 51;
ROLLBACK;
