SELECT count(*) as "count!"
           FROM (
             SELECT p.id
             FROM person p
             JOIN stage s ON s.id = p.stage_id
             LEFT JOIN LATERAL (
                 SELECT i2.source
                 FROM inquiry i2
                 WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
                 ORDER BY i2.received_at DESC, i2.id DESC
                 LIMIT 1
             ) latest_src ON true
             WHERE p.organization_id = $1
               AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
               AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
                    OR ($4::boolean AND p.assigned_user_id IS NULL))
               AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
               AND ($6::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => $6))
               AND ($7::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => $7))
               AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
               AND ($9::int IS NULL
                    OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > now() - make_interval(days => $9))
               AND ($10::int IS NULL
                    OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= now() - make_interval(days => $10))
               AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
               AND ($12::int IS NULL
                    OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > now() - make_interval(days => $12))
               AND ($13::int IS NULL
                    OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= now() - make_interval(days => $13))
               AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
               AND ($15::int IS NULL
                    OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > now() - make_interval(days => $15))
               AND ($16::int IS NULL
                    OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= now() - make_interval(days => $16))
               AND ($17::boolean IS NULL OR (p.last_inbound_at IS NULL) = $17)
               AND ($18::boolean IS NULL OR (p.last_inbound_at IS NOT NULL) = $18)
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
               -- docs/specs/SLICE_011d.md §2: same three derived predicates
               -- and identical positions as filtered_summaries.sql, offset
               -- by the absence of that statement's reference_now param.
               AND ($21::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM inquiry ia
                     WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
                       AND ia.received_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
                   )) = $21)
               AND ($22::boolean IS NULL OR (
                     p.last_inbound_at IS NOT NULL
                     AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
                     AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
                   ) = $22)
               AND ($23::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM call c
                     WHERE c.organization_id = p.organization_id
                       AND c.person_id = p.id
                       AND c.caller_user_id = $24
                       AND c.status IN ('ended', 'failed')
                       AND c.ended_at IS NOT NULL
                       AND EXISTS (
                           SELECT 1 FROM contact_attempted root
                           WHERE root.organization_id = c.organization_id
                             AND root.causation_id = c.id
                             AND root.corrects_id IS NULL
                             AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
                       )
                   )) = $23)
               -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags
               -- (none-of), same positions as filtered_summaries.sql.
               AND ($25::uuid[] IS NULL OR EXISTS (
                     SELECT 1 FROM person_tag pt
                     WHERE pt.organization_id = $1
                       AND pt.person_id = p.id AND pt.tag_id = ANY($25)))
               AND ($26::uuid[] IS NULL OR NOT EXISTS (
                     SELECT 1 FROM person_tag pt2
                     WHERE pt2.organization_id = $1
                       AND pt2.person_id = p.id AND pt2.tag_id = ANY($26)))
             LIMIT 501
           ) capped