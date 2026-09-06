\set ON_ERROR_STOP on
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT
  p.id, p.first_name, p.last_name, p.created_at,
  s.id AS stage_id, s.name AS stage_name,
  u.id AS assigned_user_id, u.display_name AS assigned_user_display_name,
  (SELECT cm.value FROM contact_method cm
    WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
    ORDER BY cm.created_at ASC LIMIT 1) AS primary_email,
  (SELECT cm.value FROM contact_method cm
    WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
    ORDER BY cm.created_at ASC LIMIT 1) AS primary_phone,
  (SELECT count(*) FROM inquiry i
    WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS inquiry_count,
  (SELECT max(i.received_at) FROM inquiry i
    WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS last_inquiry_at
FROM person p
JOIN stage s ON s.id = p.stage_id
LEFT JOIN app_user u ON u.id = p.assigned_user_id
LEFT JOIN LATERAL (
  SELECT i2.source
  FROM inquiry i2
  WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
  ORDER BY i2.received_at DESC, i2.id DESC
  LIMIT 1
) latest_src ON true
LEFT JOIN LATERAL (
  SELECT max(i3.received_at) AS ts
  FROM inquiry i3
  WHERE i3.person_id = p.id AND i3.organization_id = p.organization_id
) last_inquiry_ts ON true
LEFT JOIN LATERAL (
  SELECT max(ca.occurred_at) AS ts
  FROM contact_attempted ca
  WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
) last_contact_ts ON true
LEFT JOIN LATERAL (
  SELECT max(cc.occurred_at) AS ts
  FROM correspondence_captured cc
  WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
    AND cc.direction = 'inbound'
) last_inbound_ts ON true
WHERE p.organization_id = '11111111-1111-4111-8111-111111111111'::uuid
  AND (NULL::uuid[] IS NULL OR p.stage_id = ANY(NULL::uuid[]))
  AND (NULL::uuid[] IS NULL OR p.assigned_user_id = ANY(NULL::uuid[])
       OR (NULL::boolean AND p.assigned_user_id IS NULL))
  AND (NULL::text[] IS NULL OR latest_src.source = ANY(NULL::text[]))
  AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (p.created_at IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_contact_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => NULL::int))
  AND (NULL::int IS NULL OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => NULL::int))
  AND (NULL::boolean IS NULL OR (last_inbound_ts.ts IS NULL) = NULL::boolean)
  AND (NULL::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM correspondence_captured cc2
    WHERE cc2.person_id = p.id AND cc2.organization_id = p.organization_id
      AND cc2.direction = 'inbound'
  )) = NULL::boolean)
  AND (true::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM contact_method cm3
    WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
      AND cm3.kind = 'phone'
  )) = true::boolean)
  AND (NULL::boolean IS NULL OR (EXISTS (
    SELECT 1 FROM contact_method cm4
    WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
      AND cm4.kind = 'email'
  )) = NULL::boolean)
ORDER BY p.created_at DESC, p.id ASC
LIMIT 501;
