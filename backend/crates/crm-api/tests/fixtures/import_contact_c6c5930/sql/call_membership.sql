-- docs/specs/SLICE_011d.md §5 step 4a / §1 rule 4: for every retained P id,
-- the viewer's one qualifying call (ended DESC, id DESC — the most
-- recent), exactly the compiled-in outcome_call membership, carrying the
-- §1 rule 7 inquiry constraint — NARROWED by the call feed's full
-- customizable predicate matrix (identical axis semantics and parameter
-- positions to person/sql/filtered_summaries.sql and
-- queries::count_filtered_matches, including the three derived axes),
-- so an admin-added clause (stage, assigned_to, source, age windows, ...)
-- applies here exactly as it does to the two person-state feeds. With no
-- extra clauses beyond the anchor (`awaiting_call_outcome: true`), every
-- added predicate is NULL and this reduces exactly to the original
-- fixed-membership query. Used to APPEND the call_outcome_needed reason to
-- an already-ranked person-state item, never to re-rank it.
--
-- docs/specs/SLICE_012.md §4: reads the trigger-maintained p.last_*_at
-- columns instead of the four per-Person LATERAL max() probes; the
-- `latest_src` LATERAL (the latest inquiry's own source, untouched by
-- this slice — SLICE_012.md §1 rule 6 out-of-scope note) is unchanged.
WITH qualifying_call AS (
    SELECT DISTINCT ON (c.person_id)
        c.person_id AS id,
        c.id AS call_id,
        c.ended_at AS ended_at
    FROM call c
    WHERE c.organization_id = $1
      AND c.caller_user_id = $24
      AND c.status IN ('ended', 'failed')
      AND c.ended_at IS NOT NULL
      AND c.person_id = ANY($25::uuid[])
      AND EXISTS (
          SELECT 1 FROM inquiry i
          WHERE i.person_id = c.person_id AND i.organization_id = c.organization_id
      )
      AND EXISTS (
          SELECT 1 FROM contact_attempted root
          WHERE root.organization_id = c.organization_id
            AND root.causation_id = c.id
            AND root.corrects_id IS NULL
            AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
      )
    ORDER BY c.person_id, c.ended_at DESC, c.id DESC
)
SELECT
    qualifying_call.id AS "person_id!",
    qualifying_call.call_id AS "call_id!",
    qualifying_call.ended_at AS "ended_at!"
FROM qualifying_call
JOIN person p ON p.id = qualifying_call.id AND p.organization_id = $1
LEFT JOIN LATERAL (
    SELECT i2.source
    FROM inquiry i2
    WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
    ORDER BY i2.received_at DESC, i2.id DESC
    LIMIT 1
) latest_src ON true
WHERE ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
  AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
       OR ($4::boolean AND p.assigned_user_id IS NULL))
  AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
  AND ($6::int IS NULL
       OR COALESCE(p.created_at, '-infinity'::timestamptz) > $26::timestamptz - make_interval(days => $6))
  AND ($7::int IS NULL
       OR COALESCE(p.created_at, '-infinity'::timestamptz) <= $26::timestamptz - make_interval(days => $7))
  AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
  AND ($9::int IS NULL
       OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > $26::timestamptz - make_interval(days => $9))
  AND ($10::int IS NULL
       OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= $26::timestamptz - make_interval(days => $10))
  AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
  AND ($12::int IS NULL
       OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > $26::timestamptz - make_interval(days => $12))
  AND ($13::int IS NULL
       OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= $26::timestamptz - make_interval(days => $13))
  AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
  AND ($15::int IS NULL
       OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > $26::timestamptz - make_interval(days => $15))
  AND ($16::int IS NULL
       OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= $26::timestamptz - make_interval(days => $16))
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
        SELECT 1 FROM call c2
        WHERE c2.organization_id = p.organization_id
          AND c2.person_id = p.id
          AND c2.caller_user_id = $24
          AND c2.status IN ('ended', 'failed')
          AND c2.ended_at IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM contact_attempted root2
              WHERE root2.organization_id = c2.organization_id
                AND root2.causation_id = c2.id
                AND root2.corrects_id IS NULL
                AND NOT EXISTS (SELECT 1 FROM contact_attempted x2 WHERE x2.corrects_id = root2.id)
          )
      )) = $23)
  -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags (none-of).
  AND ($27::uuid[] IS NULL OR EXISTS (
        SELECT 1 FROM person_tag pt
        WHERE pt.organization_id = $1
          AND pt.person_id = p.id AND pt.tag_id = ANY($27)))
  AND ($28::uuid[] IS NULL OR NOT EXISTS (
        SELECT 1 FROM person_tag pt2
        WHERE pt2.organization_id = $1
          AND pt2.person_id = p.id AND pt2.tag_id = ANY($28)))
  AND ($29::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$29 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$30) AND (($31='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30)) OR ($31='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30)) OR ($30='text' AND $31='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($32),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($30='text' AND $31='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($32),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($30='number' AND $31='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND ($33::text IS NULL OR v.number_value >= $33::numeric) AND ($34::text IS NULL OR v.number_value <= $34::numeric))) OR ($30='number' AND $31='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND ($33::text IS NULL OR v.number_value >= $33::numeric) AND ($34::text IS NULL OR v.number_value <= $34::numeric))) OR ($30='date' AND $31='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND ($35::date IS NULL OR v.date_value >= $35) AND ($36::date IS NULL OR v.date_value <= $36))) OR ($30='date' AND $31='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND ($35::date IS NULL OR v.date_value >= $35) AND ($36::date IS NULL OR v.date_value <= $36))) OR ($30='choice' AND $31='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND v.option_id=ANY($37))) OR ($30='choice' AND $31='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$29 AND v.field_type=$30 AND v.option_id=ANY($37))))))
  AND ($38::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$38 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$39) AND (($40='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39)) OR ($40='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39)) OR ($39='text' AND $40='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($41),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($39='text' AND $40='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($41),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($39='number' AND $40='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND ($42::text IS NULL OR v.number_value >= $42::numeric) AND ($43::text IS NULL OR v.number_value <= $43::numeric))) OR ($39='number' AND $40='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND ($42::text IS NULL OR v.number_value >= $42::numeric) AND ($43::text IS NULL OR v.number_value <= $43::numeric))) OR ($39='date' AND $40='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND ($44::date IS NULL OR v.date_value >= $44) AND ($45::date IS NULL OR v.date_value <= $45))) OR ($39='date' AND $40='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND ($44::date IS NULL OR v.date_value >= $44) AND ($45::date IS NULL OR v.date_value <= $45))) OR ($39='choice' AND $40='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND v.option_id=ANY($46))) OR ($39='choice' AND $40='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$38 AND v.field_type=$39 AND v.option_id=ANY($46))))))
  AND ($47::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$47 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$48) AND (($49='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48)) OR ($49='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48)) OR ($48='text' AND $49='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($50),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($48='text' AND $49='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($50),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($48='number' AND $49='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND ($51::text IS NULL OR v.number_value >= $51::numeric) AND ($52::text IS NULL OR v.number_value <= $52::numeric))) OR ($48='number' AND $49='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND ($51::text IS NULL OR v.number_value >= $51::numeric) AND ($52::text IS NULL OR v.number_value <= $52::numeric))) OR ($48='date' AND $49='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND ($53::date IS NULL OR v.date_value >= $53) AND ($54::date IS NULL OR v.date_value <= $54))) OR ($48='date' AND $49='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND ($53::date IS NULL OR v.date_value >= $53) AND ($54::date IS NULL OR v.date_value <= $54))) OR ($48='choice' AND $49='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND v.option_id=ANY($55))) OR ($48='choice' AND $49='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$47 AND v.field_type=$48 AND v.option_id=ANY($55))))))
  AND ($56::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$56 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$57) AND (($58='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57)) OR ($58='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57)) OR ($57='text' AND $58='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($59),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($57='text' AND $58='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($59),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($57='number' AND $58='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($60::text IS NULL OR v.number_value >= $60::numeric) AND ($61::text IS NULL OR v.number_value <= $61::numeric))) OR ($57='number' AND $58='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($60::text IS NULL OR v.number_value >= $60::numeric) AND ($61::text IS NULL OR v.number_value <= $61::numeric))) OR ($57='date' AND $58='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($62::date IS NULL OR v.date_value >= $62) AND ($63::date IS NULL OR v.date_value <= $63))) OR ($57='date' AND $58='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($62::date IS NULL OR v.date_value >= $62) AND ($63::date IS NULL OR v.date_value <= $63))) OR ($57='choice' AND $58='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND v.option_id=ANY($64))) OR ($57='choice' AND $58='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND v.option_id=ANY($64))))))
  AND ($65::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$65 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$66) AND (($67='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66)) OR ($67='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66)) OR ($66='text' AND $67='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($68),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($66='text' AND $67='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($68),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($66='number' AND $67='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($69::text IS NULL OR v.number_value >= $69::numeric) AND ($70::text IS NULL OR v.number_value <= $70::numeric))) OR ($66='number' AND $67='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($69::text IS NULL OR v.number_value >= $69::numeric) AND ($70::text IS NULL OR v.number_value <= $70::numeric))) OR ($66='date' AND $67='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($71::date IS NULL OR v.date_value >= $71) AND ($72::date IS NULL OR v.date_value <= $72))) OR ($66='date' AND $67='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($71::date IS NULL OR v.date_value >= $71) AND ($72::date IS NULL OR v.date_value <= $72))) OR ($66='choice' AND $67='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND v.option_id=ANY($73))) OR ($66='choice' AND $67='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND v.option_id=ANY($73))))))
