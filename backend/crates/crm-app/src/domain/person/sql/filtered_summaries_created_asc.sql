SELECT
     p.id, p.first_name, p.last_name, p.created_at,
     s.id as stage_id, s.name as stage_name,
     u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
     (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
        ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
     (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
        ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
     (SELECT count(*) FROM inquiry i
        WHERE i.person_id = p.id AND i.organization_id = p.organization_id) as "inquiry_count!",
     (SELECT max(i.received_at) FROM inquiry i
        WHERE i.person_id = p.id AND i.organization_id = p.organization_id) as "last_inquiry_at?"
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
       SELECT max(i3.received_at) as ts
       FROM inquiry i3
       WHERE i3.person_id = p.id AND i3.organization_id = p.organization_id
   ) last_inquiry_ts ON true
   LEFT JOIN LATERAL (
       SELECT max(ca.occurred_at) as ts
       FROM contact_attempted ca
       WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
   ) last_contact_ts ON true
   LEFT JOIN LATERAL (
       SELECT max(cc.occurred_at) as ts
       FROM correspondence_captured cc
       WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
         AND cc.direction = 'inbound'
   ) last_inbound_ts ON true
   WHERE p.organization_id = $1
     AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
     AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
          OR ($4::boolean AND p.assigned_user_id IS NULL))
     AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
     AND ($6::int IS NULL
          OR COALESCE(p.created_at, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $6))
     AND ($7::int IS NULL
          OR COALESCE(p.created_at, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $7))
     AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
     AND ($9::int IS NULL
          OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $9))
     AND ($10::int IS NULL
          OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $10))
     AND ($11::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = $11)
     AND ($12::int IS NULL
          OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $12))
     AND ($13::int IS NULL
          OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $13))
     AND ($14::boolean IS NULL OR (last_contact_ts.ts IS NULL) = $14)
     AND ($15::int IS NULL
          OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $15))
     AND ($16::int IS NULL
          OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $16))
     AND ($17::boolean IS NULL OR (last_inbound_ts.ts IS NULL) = $17)
     AND ($18::boolean IS NULL OR (EXISTS (
           SELECT 1 FROM correspondence_captured cc2
           WHERE cc2.person_id = p.id AND cc2.organization_id = p.organization_id
             AND cc2.direction = 'inbound'
         )) = $18)
     AND ($19::boolean IS NULL OR (EXISTS (
           SELECT 1 FROM contact_method cm3
           WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
             AND cm3.kind = 'phone'
         )) = $19)
     AND ($20::boolean IS NULL OR (EXISTS (
           SELECT 1 FROM contact_method cm4
           WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
             AND cm4.kind = 'email'
         )) = $20)
   ORDER BY p.created_at ASC, p.id ASC
   LIMIT 501
