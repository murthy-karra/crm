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
     p.last_inquiry_at as "last_inquiry_at?"
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
          OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $9))
     AND ($10::int IS NULL
          OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $10))
     AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
     AND ($12::int IS NULL
          OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $12))
     AND ($13::int IS NULL
          OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $13))
     AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
     AND ($15::int IS NULL
          OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > COALESCE($21, now()) - make_interval(days => $15))
     AND ($16::int IS NULL
          OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= COALESCE($21, now()) - make_interval(days => $16))
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
     -- docs/specs/SLICE_011d.md §2: awaiting_response — an inquiry after the
     -- effective last contact attempt (equivalently: exists an inquiry whose
     -- received_at exceeds the same p.last_contact_at this matrix already
     -- reads for the last_contact age axis).
     AND ($22::boolean IS NULL OR (EXISTS (
           SELECT 1 FROM inquiry ia
           WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
             AND ia.received_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
         )) = $22)
     -- docs/specs/SLICE_011d.md §2: client_replied_unanswered — the latest
     -- inbound correspondence exists and is later than both the effective
     -- last contact attempt and the latest outbound correspondence.
     AND ($23::boolean IS NULL OR (
           p.last_inbound_at IS NOT NULL
           AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
           AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
         ) = $23)
     -- docs/specs/SLICE_011d.md §2: awaiting_call_outcome — a call of the
     -- viewer's ($25) to the Person is ended/failed with a non-null
     -- ended_at and its root automatic contact attempt has no correction
     -- (exactly the compiled-in outcome_call membership).
     AND ($24::boolean IS NULL OR (EXISTS (
           SELECT 1 FROM call c
           WHERE c.organization_id = p.organization_id
             AND c.person_id = p.id
             AND c.caller_user_id = $25
             AND c.status IN ('ended', 'failed')
             AND c.ended_at IS NOT NULL
             AND EXISTS (
                 SELECT 1 FROM contact_attempted root
                 WHERE root.organization_id = c.organization_id
                   AND root.causation_id = c.id
                   AND root.corrects_id IS NULL
                   AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
             )
         )) = $24)
     -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags (none-of).
     AND ($26::uuid[] IS NULL OR EXISTS (
           SELECT 1 FROM person_tag pt
           WHERE pt.organization_id = $1
             AND pt.person_id = p.id AND pt.tag_id = ANY($26)))
  AND ($27::uuid[] IS NULL OR NOT EXISTS (
        SELECT 1 FROM person_tag pt2
        WHERE pt2.organization_id = $1
          AND pt2.person_id = p.id AND pt2.tag_id = ANY($27)))
  AND ($28::uuid IS NULL OR (
        EXISTS (SELECT 1 FROM custom_field cf
                WHERE cf.id = $28 AND cf.organization_id = $1
                  AND cf.archived_at IS NULL AND cf.field_type = $29)
        AND (
          ($30 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29))
          OR ($30 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29))
          OR ($29 = 'text' AND $30 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($31), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($29 = 'text' AND $30 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($31), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($29 = 'number' AND $30 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND ($32::text IS NULL OR v.number_value >= $32::numeric) AND ($33::text IS NULL OR v.number_value <= $33::numeric)))
          OR ($29 = 'number' AND $30 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND ($32::text IS NULL OR v.number_value >= $32::numeric) AND ($33::text IS NULL OR v.number_value <= $33::numeric)))
          OR ($29 = 'date' AND $30 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND ($34::date IS NULL OR v.date_value >= $34) AND ($35::date IS NULL OR v.date_value <= $35)))
          OR ($29 = 'date' AND $30 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND ($34::date IS NULL OR v.date_value >= $34) AND ($35::date IS NULL OR v.date_value <= $35)))
          OR ($29 = 'choice' AND $30 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND v.option_id = ANY($36)))
          OR ($29 = 'choice' AND $30 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $28 AND v.field_type = $29 AND v.option_id = ANY($36)))
        )
      ))
  AND ($37::uuid IS NULL OR (
        EXISTS (SELECT 1 FROM custom_field cf
                WHERE cf.id = $37 AND cf.organization_id = $1
                  AND cf.archived_at IS NULL AND cf.field_type = $38)
        AND (
          ($39 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38))
          OR ($39 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38))
          OR ($38 = 'text' AND $39 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($40), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($38 = 'text' AND $39 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($40), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($38 = 'number' AND $39 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND ($41::text IS NULL OR v.number_value >= $41::numeric) AND ($42::text IS NULL OR v.number_value <= $42::numeric)))
          OR ($38 = 'number' AND $39 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND ($41::text IS NULL OR v.number_value >= $41::numeric) AND ($42::text IS NULL OR v.number_value <= $42::numeric)))
          OR ($38 = 'date' AND $39 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND ($43::date IS NULL OR v.date_value >= $43) AND ($44::date IS NULL OR v.date_value <= $44)))
          OR ($38 = 'date' AND $39 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND ($43::date IS NULL OR v.date_value >= $43) AND ($44::date IS NULL OR v.date_value <= $44)))
          OR ($38 = 'choice' AND $39 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND v.option_id = ANY($45)))
          OR ($38 = 'choice' AND $39 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $37 AND v.field_type = $38 AND v.option_id = ANY($45)))
        )
      ))
  AND ($46::uuid IS NULL OR (
        EXISTS (SELECT 1 FROM custom_field cf
                WHERE cf.id = $46 AND cf.organization_id = $1
                  AND cf.archived_at IS NULL AND cf.field_type = $47)
        AND (
          ($48 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47))
          OR ($48 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47))
          OR ($47 = 'text' AND $48 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($49), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($47 = 'text' AND $48 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($49), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($47 = 'number' AND $48 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND ($50::text IS NULL OR v.number_value >= $50::numeric) AND ($51::text IS NULL OR v.number_value <= $51::numeric)))
          OR ($47 = 'number' AND $48 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND ($50::text IS NULL OR v.number_value >= $50::numeric) AND ($51::text IS NULL OR v.number_value <= $51::numeric)))
          OR ($47 = 'date' AND $48 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND ($52::date IS NULL OR v.date_value >= $52) AND ($53::date IS NULL OR v.date_value <= $53)))
          OR ($47 = 'date' AND $48 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND ($52::date IS NULL OR v.date_value >= $52) AND ($53::date IS NULL OR v.date_value <= $53)))
          OR ($47 = 'choice' AND $48 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND v.option_id = ANY($54)))
          OR ($47 = 'choice' AND $48 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $46 AND v.field_type = $47 AND v.option_id = ANY($54)))
        )
      ))
  AND ($55::uuid IS NULL OR (
        EXISTS (SELECT 1 FROM custom_field cf
                WHERE cf.id = $55 AND cf.organization_id = $1
                  AND cf.archived_at IS NULL AND cf.field_type = $56)
        AND (
          ($57 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56))
          OR ($57 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56))
          OR ($56 = 'text' AND $57 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($58), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($56 = 'text' AND $57 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($58), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($56 = 'number' AND $57 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND ($59::text IS NULL OR v.number_value >= $59::numeric) AND ($60::text IS NULL OR v.number_value <= $60::numeric)))
          OR ($56 = 'number' AND $57 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND ($59::text IS NULL OR v.number_value >= $59::numeric) AND ($60::text IS NULL OR v.number_value <= $60::numeric)))
          OR ($56 = 'date' AND $57 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND ($61::date IS NULL OR v.date_value >= $61) AND ($62::date IS NULL OR v.date_value <= $62)))
          OR ($56 = 'date' AND $57 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND ($61::date IS NULL OR v.date_value >= $61) AND ($62::date IS NULL OR v.date_value <= $62)))
          OR ($56 = 'choice' AND $57 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND v.option_id = ANY($63)))
          OR ($56 = 'choice' AND $57 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $55 AND v.field_type = $56 AND v.option_id = ANY($63)))
        )
      ))
  AND ($64::uuid IS NULL OR (
        EXISTS (SELECT 1 FROM custom_field cf
                WHERE cf.id = $64 AND cf.organization_id = $1
                  AND cf.archived_at IS NULL AND cf.field_type = $65)
        AND (
          ($66 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65))
          OR ($66 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65))
          OR ($65 = 'text' AND $66 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($67), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($65 = 'text' AND $66 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($67), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\'))
          OR ($65 = 'number' AND $66 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND ($68::text IS NULL OR v.number_value >= $68::numeric) AND ($69::text IS NULL OR v.number_value <= $69::numeric)))
          OR ($65 = 'number' AND $66 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND ($68::text IS NULL OR v.number_value >= $68::numeric) AND ($69::text IS NULL OR v.number_value <= $69::numeric)))
          OR ($65 = 'date' AND $66 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND ($70::date IS NULL OR v.date_value >= $70) AND ($71::date IS NULL OR v.date_value <= $71)))
          OR ($65 = 'date' AND $66 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND ($70::date IS NULL OR v.date_value >= $70) AND ($71::date IS NULL OR v.date_value <= $71)))
          OR ($65 = 'choice' AND $66 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND v.option_id = ANY($72)))
          OR ($65 = 'choice' AND $66 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $64 AND v.field_type = $65 AND v.option_id = ANY($72)))
        )
      ))
ORDER BY p.created_at ASC, p.id ASC
   LIMIT 501
