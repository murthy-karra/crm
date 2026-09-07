\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE perf_seed_person (id uuid PRIMARY KEY, ordinal integer NOT NULL UNIQUE) ON COMMIT DROP;
INSERT INTO perf_seed_person (id, ordinal)
SELECT md5('crm-011c-person-' || value::text)::uuid, value
FROM generate_series(1, 50000) AS value;

-- Fixed timestamps and IDs make the fixture reproducible. Source result IDs are
-- public synthetic UUIDs only; no shared development row is read or modified.
INSERT INTO organization (id, name, status, intake_slug, intake_token, intake_routing_mode)
VALUES ('11111111-1111-4111-8111-111111111111',
        'Slice 011c isolated performance fixture', 'active', 'slice-011c-perf',
        'a1b2c3d4', 'unassigned');
INSERT INTO app_user (id, email, display_name) VALUES
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1', 'conc@example.invalid', 'Concentrated Viewer'),
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2', 'representative@example.invalid', 'Representative Viewer'),
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3', 'partial@example.invalid', 'Partial Viewer'),
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4', 'empty@example.invalid', 'Empty Viewer'),
  ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa5', 'other@example.invalid', 'Other Viewer');
INSERT INTO organization_membership (organization_id, user_id, role, status)
SELECT '11111111-1111-4111-8111-111111111111', id,
       CASE WHEN id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1'::uuid THEN 'admin' ELSE 'member' END,
       'active'
FROM app_user;
-- `perf_011c_today_source` is a generated-DB-only stand-in for the proposed
-- preference relation. It lets the prototype include live/visible saved-list
-- metadata enumeration without pre-creating a production migration.
CREATE TABLE perf_011c_today_source (
  organization_id uuid NOT NULL,
  user_id uuid NOT NULL,
  case_key text NOT NULL,
  position integer NOT NULL,
  list_id uuid NOT NULL,
  PRIMARY KEY (organization_id, user_id, case_key, list_id)
);
GRANT SELECT ON perf_011c_today_source TO crm_migrator;
INSERT INTO saved_list
  (id, organization_id, created_by_user_id, scope, name, filter, revision,
   create_request_id, create_fingerprint, created_at, updated_at, deleted_at)
SELECT md5('crm-011c-list-' || u.id::text || '-' || n::text)::uuid,
       '11111111-1111-4111-8111-111111111111', u.id, 'personal',
       'Synthetic source ' || n::text,
       CASE n
         WHEN 1 THEN '{"version":1,"clauses":[{"kind":"stage","stage_ids":["22222222-2222-4222-8222-222222222222"]},{"kind":"source","sources":["zillow"]}]}'::jsonb
         WHEN 2 THEN '{"version":1,"clauses":[{"kind":"stage","stage_ids":["22222222-2222-4222-8222-222222222222"]},{"kind":"source","sources":["zillow"]},{"kind":"has_phone","value":true}]}'::jsonb
         WHEN 3 THEN '{"version":1,"clauses":[{"kind":"stage","stage_ids":["22222222-2222-4222-8222-222222222222"]},{"kind":"last_contact","age":{"op":"never"}}]}'::jsonb
         WHEN 4 THEN '{"version":1,"clauses":[{"kind":"stage","stage_ids":["22222222-2222-4222-8222-222222222222"]},{"kind":"source","sources":["zillow"]},{"kind":"last_contact","age":{"op":"never"}}]}'::jsonb
         WHEN 5 THEN '{"version":1,"clauses":[{"kind":"stage","stage_ids":["22222222-2222-4222-8222-222222222222"]},{"kind":"has_phone","value":false}]}'::jsonb
       END, 1,
       md5('crm-011c-list-request-' || u.id::text || '-' || n::text)::uuid,
       decode(md5('crm-011c-list-fingerprint-a-' || u.id::text || '-' || n::text) || md5('crm-011c-list-fingerprint-b-' || u.id::text || '-' || n::text), 'hex'),
       timestamptz '2026-09-06 12:00:00+00', timestamptz '2026-09-06 12:00:00+00', NULL
FROM app_user u CROSS JOIN generate_series(1, 5) n;
INSERT INTO perf_011c_today_source (organization_id, user_id, case_key, position, list_id)
SELECT '11111111-1111-4111-8111-111111111111', u.id, cases.case_key, cases.position,
       md5('crm-011c-list-' || u.id::text || '-' || cases.list_ordinal::text)::uuid
FROM app_user u
CROSS JOIN (VALUES
  ('one_dense', 1, 1),
  ('one_absence', 1, 3),
  ('five_overlap', 1, 1),
  ('five_overlap', 2, 2),
  ('five_overlap', 3, 3),
  ('five_overlap', 4, 4),
  ('five_overlap', 5, 5)
) AS cases(case_key, position, list_ordinal);
INSERT INTO stage (id, organization_id, name, position)
VALUES ('22222222-2222-4222-8222-222222222222',
        '11111111-1111-4111-8111-111111111111', 'Synthetic', 1);

INSERT INTO person (id, organization_id, first_name, last_name, stage_id, assigned_user_id, created_at, updated_at)
SELECT seed.id,
       '11111111-1111-4111-8111-111111111111',
       'Synthetic', lpad(seed.ordinal::text, 5, '0'),
       '22222222-2222-4222-8222-222222222222',
       CASE
         WHEN seed.ordinal <= 30000 THEN 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1'::uuid
         WHEN seed.ordinal <= 32000 THEN 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2'::uuid
         WHEN seed.ordinal <= 32100 THEN 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3'::uuid
         WHEN seed.ordinal <= 45000 THEN 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa5'::uuid
         ELSE NULL
       END,
       timestamptz '2026-09-06 12:00:00+00' - ((seed.ordinal % 365) * interval '1 day'),
       timestamptz '2026-09-06 12:00:00+00' - ((seed.ordinal % 365) * interval '1 day')
FROM perf_seed_person seed;

-- 45,000 Persons have a phone; the remaining 5,000 support an actual
-- absence-proving has_phone=false source. Email is intentionally independent.
INSERT INTO contact_method (id, organization_id, person_id, kind, value, normalized_value, created_at)
SELECT md5('crm-011c-phone-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', seed.id, 'phone',
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       timestamptz '2026-09-06 12:00:00+00'
FROM perf_seed_person seed WHERE seed.ordinal % 10 <> 0;
INSERT INTO contact_method (id, organization_id, person_id, kind, value, normalized_value, created_at)
SELECT md5('crm-011c-email-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', seed.id, 'email',
       'synthetic-' || seed.ordinal::text || '@example.invalid',
       'synthetic-' || seed.ordinal::text || '@example.invalid',
       timestamptz '2026-09-06 12:00:00+00'
FROM perf_seed_person seed WHERE seed.ordinal % 5 <> 0;

-- 45,455 historical inquiries. Eighty percent use zillow (a dense source);
-- the no-inquiry remainder supports LastInquiry Never. Recent second inquiries
-- create >200 concentrated and representative built-ins and exactly 100 partial.
INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, message, received_at, created_at)
SELECT md5('crm-011c-inquiry-old-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', seed.id,
       md5('crm-011c-raw-' || seed.ordinal::text)::uuid,
       CASE WHEN seed.ordinal % 10 < 8 THEN 'zillow' ELSE 'website' END,
       'fixture-' || seed.ordinal::text, NULL,
       timestamptz '2026-09-06 12:00:00+00' - interval '30 days' - ((seed.ordinal % 20) * interval '1 hour'),
       timestamptz '2026-09-06 12:00:00+00'
FROM perf_seed_person seed
WHERE seed.ordinal % 11 <> 0 OR seed.ordinal BETWEEN 32001 AND 32100;
INSERT INTO inquiry (id, organization_id, person_id, raw_payload_id, source, source_external_id, message, received_at, created_at)
SELECT md5('crm-011c-inquiry-waiting-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', seed.id,
       md5('crm-011c-raw-waiting-' || seed.ordinal::text)::uuid,
       'zillow', 'waiting-' || seed.ordinal::text, NULL,
       timestamptz '2026-09-06 10:00:00+00', timestamptz '2026-09-06 12:00:00+00'
FROM perf_seed_person seed
WHERE seed.ordinal BETWEEN 1 AND 100
   OR (seed.ordinal BETWEEN 30001 AND 32000 AND seed.ordinal % 5 = 0)
   OR seed.ordinal BETWEEN 32001 AND 32100;

-- The 30,000-Person concentrated book has exactly 100 waiting built-ins and
-- otherwise-contacted history, so it exercises a spare B plus expensive source
-- scans on the same connection. The 2,000-Person representative book has >200
-- waiting built-ins. Remaining no-contact People keep LastContact Never history-heavy. Every seventeenth root has one correction at the same
-- occurred_at, preserving the production effective-contact equivalence.
INSERT INTO contact_attempted
  (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
   occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome)
SELECT md5('crm-011c-contact-root-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', 'system', NULL, NULL, 'fixture',
       timestamptz '2026-09-06 12:00:00+00' - ((seed.ordinal % 20 + 1) * interval '1 day'),
       timestamptz '2026-09-06 12:00:00+00',
       md5('crm-011c-contact-correlation-' || seed.ordinal::text)::uuid, NULL, NULL,
       seed.id, 'call', 'no_answer'
FROM perf_seed_person seed
WHERE (seed.ordinal > 100 OR seed.ordinal > 30000)
  AND NOT (seed.ordinal BETWEEN 32001 AND 32100)
  AND NOT (seed.ordinal BETWEEN 30001 AND 32000 AND seed.ordinal % 5 = 0);
INSERT INTO contact_attempted
  (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
   occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id, channel, outcome)
SELECT md5('crm-011c-contact-correction-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', 'system', NULL, NULL, 'fixture',
       timestamptz '2026-09-06 12:00:00+00' - ((seed.ordinal % 20 + 1) * interval '1 day'),
       timestamptz '2026-09-06 12:00:00+00',
       md5('crm-011c-contact-correction-correlation-' || seed.ordinal::text)::uuid, NULL,
       md5('crm-011c-contact-root-' || seed.ordinal::text)::uuid,
       seed.id, 'call', 'reached'
FROM perf_seed_person seed
WHERE (seed.ordinal > 100 OR seed.ordinal > 30000)
  AND NOT (seed.ordinal BETWEEN 32001 AND 32100)
  AND NOT (seed.ordinal BETWEEN 30001 AND 32000 AND seed.ordinal % 5 = 0)
  AND seed.ordinal % 17 = 0;

-- 7,128 inbound correspondence records and matching raw rows give the existing
-- client-replied lateral probes representative history without customer data.
INSERT INTO correspondence_raw (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)
SELECT md5('crm-011c-correspondence-raw-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111',
       timestamptz '2026-09-06 12:00:00+00' - interval '12 hours',
       decode('00', 'hex'), decode('00', 'hex'),
       decode(md5('crm-011c-correspondence-hmac-a-' || seed.ordinal::text) || md5('crm-011c-correspondence-hmac-b-' || seed.ordinal::text), 'hex'), 1, true
FROM perf_seed_person seed WHERE seed.ordinal % 7 = 0 AND seed.ordinal > 30000
  AND seed.ordinal NOT BETWEEN 32001 AND 32100;
INSERT INTO correspondence_captured
  (id, organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,
   occurred_at, recorded_at, correlation_id, causation_id, corrects_id, person_id,
   agent_user_id, direction, message_id, thread_key, via, correspondence_raw_id, backdated)
SELECT md5('crm-011c-correspondence-' || seed.ordinal::text)::uuid,
       '11111111-1111-4111-8111-111111111111', 'system', NULL, NULL, 'fixture',
       timestamptz '2026-09-06 12:00:00+00' - interval '12 hours',
       timestamptz '2026-09-06 12:00:00+00',
       md5('crm-011c-correspondence-correlation-' || seed.ordinal::text)::uuid, NULL, NULL,
       seed.id, 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa5', 'inbound',
       'fixture-' || seed.ordinal::text || '@example.invalid', NULL, 'cc',
       md5('crm-011c-correspondence-raw-' || seed.ordinal::text)::uuid, false
FROM perf_seed_person seed WHERE seed.ordinal % 7 = 0 AND seed.ordinal > 30000
  AND seed.ordinal NOT BETWEEN 32001 AND 32100;
COMMIT;
ANALYZE organization;
ANALYZE app_user;
ANALYZE organization_membership;
ANALYZE stage;
ANALYZE person;
ANALYZE contact_method;
ANALYZE inquiry;
ANALYZE contact_attempted;
ANALYZE correspondence_raw;
ANALYZE correspondence_captured;

SELECT 'fixture_people=' || count(*) FROM person;
SELECT 'fixture_inquiries=' || count(*) FROM inquiry;
SELECT 'fixture_contact_attempts=' || count(*) FROM contact_attempted;
SELECT 'fixture_contact_corrections=' || count(*) FROM contact_attempted WHERE corrects_id IS NOT NULL;
SELECT 'fixture_inbound=' || count(*) FROM correspondence_captured WHERE direction = 'inbound';
SELECT 'builtin_concentrated_candidates=' || count(*) FROM person WHERE assigned_user_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1' AND id IN (
  SELECT person_id FROM inquiry WHERE received_at = timestamptz '2026-09-06 10:00:00+00');
SELECT 'builtin_representative_candidates=' || count(*) FROM person WHERE assigned_user_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2' AND id IN (
  SELECT person_id FROM inquiry WHERE received_at = timestamptz '2026-09-06 10:00:00+00');
SELECT 'builtin_partial_candidates=' || count(*) FROM person WHERE assigned_user_id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3' AND id IN (
  SELECT person_id FROM inquiry WHERE received_at = timestamptz '2026-09-06 10:00:00+00');
