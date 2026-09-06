\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE perf_seed_person (id uuid PRIMARY KEY, ordinal integer NOT NULL) ON COMMIT DROP;
INSERT INTO perf_seed_person (id, ordinal)
SELECT gen_random_uuid(), value FROM generate_series(1, 50000) AS value;

INSERT INTO organization (id, name, status, intake_slug, intake_token, intake_routing_mode)
VALUES (
  '11111111-1111-4111-8111-111111111111',
  'Slice 011b synthetic performance fixture',
  'active',
  'slice-011b-perf',
  'a1b2c3d4',
  'unassigned'
);
INSERT INTO stage (id, organization_id, name, position)
VALUES (
  '22222222-2222-4222-8222-222222222222',
  '11111111-1111-4111-8111-111111111111',
  'Synthetic',
  1
);

INSERT INTO person (id, organization_id, first_name, last_name, stage_id, created_at, updated_at)
SELECT seed.id,
       '11111111-1111-4111-8111-111111111111',
       'Synthetic',
       lpad(seed.ordinal::text, 5, '0'),
       '22222222-2222-4222-8222-222222222222',
       now() - ((seed.ordinal % 365) * interval '1 day'),
       now() - ((seed.ordinal % 365) * interval '1 day')
FROM perf_seed_person AS seed;

INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value, created_at)
SELECT '11111111-1111-4111-8111-111111111111',
       seed.id,
       'phone',
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       '+1555' || lpad(seed.ordinal::text, 7, '0'),
       now()
FROM perf_seed_person AS seed
WHERE seed.ordinal <= 49500;
COMMIT;

ANALYZE organization;
ANALYZE stage;
ANALYZE person;
ANALYZE contact_method;

SELECT 'people=' || count(*) FROM person;
SELECT 'with_phone=' || count(*) FROM contact_method WHERE kind = 'phone';
SELECT 'without_phone=' || (SELECT count(*) FROM person) - count(*) FROM contact_method WHERE kind = 'phone';
