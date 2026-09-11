
WITH prefix AS (
SELECT p.id, p.last_contact_at AS last_contact_at
FROM person p
LEFT JOIN LATERAL (
  SELECT i2.source FROM inquiry i2
  WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
  ORDER BY i2.received_at DESC, i2.id DESC LIMIT 1
) latest_src ON ($5::text[] IS NOT NULL)
WHERE p.organization_id = $1
  AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
  AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3) OR ($4::boolean AND p.assigned_user_id IS NULL))
  AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
  AND ($6::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) > $21::timestamptz - make_interval(days => $6))
  AND ($7::int IS NULL OR COALESCE(p.created_at, '-infinity'::timestamptz) <= $21::timestamptz - make_interval(days => $7))
  AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
  AND ($9::int IS NULL OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > $21::timestamptz - make_interval(days => $9))
  AND ($10::int IS NULL OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= $21::timestamptz - make_interval(days => $10))
  AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
  AND ($12::int IS NULL OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > $21::timestamptz - make_interval(days => $12))
  AND ($13::int IS NULL OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= $21::timestamptz - make_interval(days => $13))
  AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
  AND ($15::int IS NULL OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > $21::timestamptz - make_interval(days => $15))
  AND ($16::int IS NULL OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= $21::timestamptz - make_interval(days => $16))
  AND ($17::boolean IS NULL OR (p.last_inbound_at IS NULL) = $17)
  AND ($18::boolean IS NULL OR (p.last_inbound_at IS NOT NULL) = $18)
  AND ($19::boolean IS NULL OR (EXISTS (SELECT 1 FROM contact_method cm3
       WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id AND cm3.kind = 'phone')) = $19)
  AND ($20::boolean IS NULL OR (EXISTS (SELECT 1 FROM contact_method cm4
       WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id AND cm4.kind = 'email')) = $20)
  -- docs/specs/SLICE_011d.md §2: same three derived predicates as
  -- filtered_summaries.sql, appended after the existing tail params.
  AND ($25::boolean IS NULL OR (EXISTS (
        SELECT 1 FROM inquiry ia
        WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
          AND ia.received_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
      )) = $25)
  AND ($26::boolean IS NULL OR (
        p.last_inbound_at IS NOT NULL
        AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
        AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
      ) = $26)
  AND ($27::boolean IS NULL OR (EXISTS (
        SELECT 1 FROM call c
        WHERE c.organization_id = p.organization_id
          AND c.person_id = p.id
          AND c.caller_user_id = $28
          AND c.status IN ('ended', 'failed')
          AND c.ended_at IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM contact_attempted root
              WHERE root.organization_id = c.organization_id
                AND root.causation_id = c.id
                AND root.corrects_id IS NULL
                AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
          )
      )) = $27)
  -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags (none-of).
  AND ($29::uuid[] IS NULL OR EXISTS (
        SELECT 1 FROM person_tag pt
        WHERE pt.organization_id = $1
          AND pt.person_id = p.id AND pt.tag_id = ANY($29)))
  AND ($30::uuid[] IS NULL OR NOT EXISTS (
        SELECT 1 FROM person_tag pt2
        WHERE pt2.organization_id = $1
          AND pt2.person_id = p.id AND pt2.tag_id = ANY($30)))
  AND ($31::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $31 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $32) AND (($33 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32)) OR ($33 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32)) OR ($32 = 'text' AND $33 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($34), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($32 = 'text' AND $33 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($34), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($32 = 'number' AND $33 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND ($35::text IS NULL OR v.number_value >= $35::numeric) AND ($36::text IS NULL OR v.number_value <= $36::numeric))) OR ($32 = 'number' AND $33 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND ($35::text IS NULL OR v.number_value >= $35::numeric) AND ($36::text IS NULL OR v.number_value <= $36::numeric))) OR ($32 = 'date' AND $33 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND ($37::date IS NULL OR v.date_value >= $37) AND ($38::date IS NULL OR v.date_value <= $38))) OR ($32 = 'date' AND $33 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND ($37::date IS NULL OR v.date_value >= $37) AND ($38::date IS NULL OR v.date_value <= $38))) OR ($32 = 'choice' AND $33 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND v.option_id = ANY($39))) OR ($32 = 'choice' AND $33 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $31 AND v.field_type = $32 AND v.option_id = ANY($39))))))
  AND ($40::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $40 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $41) AND (($42 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41)) OR ($42 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41)) OR ($41 = 'text' AND $42 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($43), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($41 = 'text' AND $42 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($43), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($41 = 'number' AND $42 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND ($44::text IS NULL OR v.number_value >= $44::numeric) AND ($45::text IS NULL OR v.number_value <= $45::numeric))) OR ($41 = 'number' AND $42 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND ($44::text IS NULL OR v.number_value >= $44::numeric) AND ($45::text IS NULL OR v.number_value <= $45::numeric))) OR ($41 = 'date' AND $42 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND ($46::date IS NULL OR v.date_value >= $46) AND ($47::date IS NULL OR v.date_value <= $47))) OR ($41 = 'date' AND $42 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND ($46::date IS NULL OR v.date_value >= $46) AND ($47::date IS NULL OR v.date_value <= $47))) OR ($41 = 'choice' AND $42 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND v.option_id = ANY($48))) OR ($41 = 'choice' AND $42 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $40 AND v.field_type = $41 AND v.option_id = ANY($48))))))
  AND ($49::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $49 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $50) AND (($51 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50)) OR ($51 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50)) OR ($50 = 'text' AND $51 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($52), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($50 = 'text' AND $51 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($52), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($50 = 'number' AND $51 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND ($53::text IS NULL OR v.number_value >= $53::numeric) AND ($54::text IS NULL OR v.number_value <= $54::numeric))) OR ($50 = 'number' AND $51 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND ($53::text IS NULL OR v.number_value >= $53::numeric) AND ($54::text IS NULL OR v.number_value <= $54::numeric))) OR ($50 = 'date' AND $51 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND ($55::date IS NULL OR v.date_value >= $55) AND ($56::date IS NULL OR v.date_value <= $56))) OR ($50 = 'date' AND $51 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND ($55::date IS NULL OR v.date_value >= $55) AND ($56::date IS NULL OR v.date_value <= $56))) OR ($50 = 'choice' AND $51 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND v.option_id = ANY($57))) OR ($50 = 'choice' AND $51 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $49 AND v.field_type = $50 AND v.option_id = ANY($57))))))
  AND ($58::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $58 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $59) AND (($60 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59)) OR ($60 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59)) OR ($59 = 'text' AND $60 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($61), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($59 = 'text' AND $60 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($61), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($59 = 'number' AND $60 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND ($62::text IS NULL OR v.number_value >= $62::numeric) AND ($63::text IS NULL OR v.number_value <= $63::numeric))) OR ($59 = 'number' AND $60 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND ($62::text IS NULL OR v.number_value >= $62::numeric) AND ($63::text IS NULL OR v.number_value <= $63::numeric))) OR ($59 = 'date' AND $60 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND ($64::date IS NULL OR v.date_value >= $64) AND ($65::date IS NULL OR v.date_value <= $65))) OR ($59 = 'date' AND $60 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND ($64::date IS NULL OR v.date_value >= $64) AND ($65::date IS NULL OR v.date_value <= $65))) OR ($59 = 'choice' AND $60 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND v.option_id = ANY($66))) OR ($59 = 'choice' AND $60 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $58 AND v.field_type = $59 AND v.option_id = ANY($66))))))
  AND ($67::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id = $67 AND cf.organization_id = $1 AND cf.archived_at IS NULL AND cf.field_type = $68) AND (($69 = 'is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68)) OR ($69 = 'is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68)) OR ($68 = 'text' AND $69 = 'contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($70), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($68 = 'text' AND $69 = 'not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($70), E'\\', E'\\\\'), '%', '\%'), '_', '\_') || '%' ESCAPE E'\\')) OR ($68 = 'number' AND $69 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND ($71::text IS NULL OR v.number_value >= $71::numeric) AND ($72::text IS NULL OR v.number_value <= $72::numeric))) OR ($68 = 'number' AND $69 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND ($71::text IS NULL OR v.number_value >= $71::numeric) AND ($72::text IS NULL OR v.number_value <= $72::numeric))) OR ($68 = 'date' AND $69 = 'range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND ($73::date IS NULL OR v.date_value >= $73) AND ($74::date IS NULL OR v.date_value <= $74))) OR ($68 = 'date' AND $69 = 'not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND ($73::date IS NULL OR v.date_value >= $73) AND ($74::date IS NULL OR v.date_value <= $74))) OR ($68 = 'choice' AND $69 = 'any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND v.option_id = ANY($75))) OR ($68 = 'choice' AND $69 = 'none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id = $1 AND v.person_id = p.id AND v.field_id = $67 AND v.field_type = $68 AND v.option_id = ANY($75))))))
  AND (CASE WHEN $23::boolean THEN p.id = ANY($22::uuid[]) ELSE NOT (p.id = ANY($22::uuid[])) END)
ORDER BY p.last_contact_at ASC NULLS FIRST, p.id ASC
LIMIT $24
)
SELECT p.id, p.first_name, p.last_name, p.created_at,
       s.id AS stage_id, s.name AS stage_name,
       u.id AS "assigned_user_id?", u.display_name AS "assigned_user_display_name?",
       (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
        ORDER BY cm.created_at ASC LIMIT 1) AS "primary_email?",
       (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
        ORDER BY cm.created_at ASC LIMIT 1) AS "primary_phone?",
       COALESCE((SELECT count(*) FROM inquiry i WHERE i.person_id = p.id AND i.organization_id = p.organization_id), 0)::bigint AS "inquiry_count!",
       latest.id AS "latest_inquiry_id?", latest.source AS "latest_inquiry_source?",
       latest.received_at AS "latest_inquiry_received_at?",
       effective_attempt.id AS "last_attempt_id?", effective_attempt.channel AS "last_attempt_channel?",
       effective_attempt.outcome AS "last_attempt_outcome?", effective_attempt.occurred_at AS "last_attempt_occurred_at?",
       prefix.last_contact_at AS "last_contact_at?"
FROM prefix
JOIN person p ON p.id = prefix.id AND p.organization_id = $1
JOIN stage s ON s.id = p.stage_id AND s.organization_id = p.organization_id
LEFT JOIN app_user u ON u.id = p.assigned_user_id
LEFT JOIN LATERAL (
  SELECT i.id, i.source, i.received_at FROM inquiry i
  WHERE i.person_id = p.id AND i.organization_id = p.organization_id
  ORDER BY i.received_at DESC, i.id DESC LIMIT 1
) latest ON true
LEFT JOIN LATERAL (
  SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at FROM contact_attempted ca
  WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
    AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = ca.id)
  ORDER BY ca.occurred_at DESC, ca.id DESC LIMIT 1
) effective_attempt ON true
ORDER BY prefix.last_contact_at ASC NULLS FIRST, prefix.id ASC
