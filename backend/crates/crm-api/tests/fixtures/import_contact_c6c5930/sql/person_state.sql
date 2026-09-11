-- docs/specs/SLICE_011d.md §5 step 2: the ONE static person-state
-- statement covering both person-state feeds (unanswered_inquiry,
-- client_replied). Binds both feeds' complete NULL-guarded predicate
-- matrices (identical axis semantics to person/sql/filtered_summaries.sql,
-- §2's three derived axes included), their enabled flags and freshness
-- windows (bound as make_interval(hours => $h)). Shared per-Person
-- aggregates (latest inquiry, last contact/inbound/outbound max, the
-- earliest-qualifying-inquiry "waiting" probe) are computed ONCE and read
-- by both matrices. Membership is (enabled_a AND matrix_a) OR (enabled_b
-- AND matrix_b), plus the §1 rule 7 inquiry constraint (latest.id IS NOT
-- NULL). Reply wins precedence over inquiry when both hold (fresh/
-- order_key computed from feed B's basis). Ordering: fresh DESC, then the
-- winning basis's timestamp ASC, then id ASC, LIMIT 201 — computed BEFORE
-- the limit, in the `qualifying`/`capped` CTEs. Only the effective-attempt
-- correction chain, latest inquiry ref (already carried from `capped`),
-- inquiry count and summary columns are hydrated in the OUTER query over
-- the at-most-201 `capped` prefix — the structural fix for the §8 planner
-- hazard: the expensive per-Person effective-attempt anti-join LATERAL
-- never runs against the whole Organization, only the capped prefix.
--
-- docs/specs/SLICE_012.md §4 (read-side switch): the four LATERAL max()
-- probes are gone — every matrix predicate and the shared aggregates now
-- read the trigger-maintained p.last_*_at columns directly. Two pins:
-- (1) the `waiting` probe's gate ("exists an inquiry after X" ⇔ "max > X")
-- is placed INSIDE the probe subquery's own WHERE, where it references
-- only outer (person-row) columns and is pseudo-constant, letting the
-- planner fold it into a One-Time Filter that skips the scan entirely for
-- a Person who cannot qualify, rather than in the LATERAL's ON clause (a
-- join qual evaluated after the probe already ran). (2) The `latest`
-- LATERAL inside `ranked` is needed only for the two feeds' own `source`
-- clause now (its id/received_at hydration moved to the outer query, over
-- the capped prefix, exactly as source_candidates.sql already does), so it
-- is guarded on EITHER feed's source parameter being bound ($5 OR $27).
-- Feed A's `fresh` reads p.last_inquiry_at (the denormalized maximum IS
-- the latest inquiry's received_at, by the column's own invariant).

WITH ranked AS (
    SELECT
        p.id,
        ((
        ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
        AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
             OR ($4::boolean AND p.assigned_user_id IS NULL))
        AND ($5::text[] IS NULL OR latest.source = ANY($5))
        AND ($6::int IS NULL
             OR COALESCE(p.created_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $6))
        AND ($7::int IS NULL
             OR COALESCE(p.created_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $7))
        AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
        AND ($9::int IS NULL
             OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $9))
        AND ($10::int IS NULL
             OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $10))
        AND ($11::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $11)
        AND ($12::int IS NULL
             OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $12))
        AND ($13::int IS NULL
             OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $13))
        AND ($14::boolean IS NULL OR (p.last_contact_at IS NULL) = $14)
        AND ($15::int IS NULL
             OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $15))
        AND ($16::int IS NULL
             OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $16))
        AND ($17::boolean IS NULL OR (p.last_inbound_at IS NULL) = $17)
        AND ($18::boolean IS NULL OR (p.last_inbound_at IS NOT NULL) = $18)
        AND ($19::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM contact_method cm_phone_a
              WHERE cm_phone_a.person_id = p.id AND cm_phone_a.organization_id = p.organization_id
                AND cm_phone_a.kind = 'phone'
            )) = $19)
        AND ($20::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM contact_method cm_email_a
              WHERE cm_email_a.person_id = p.id AND cm_email_a.organization_id = p.organization_id
                AND cm_email_a.kind = 'email'
            )) = $20)
        AND ($21::boolean IS NULL OR (waiting.received_at IS NOT NULL) = $21)
        AND ($22::boolean IS NULL OR (
              p.last_inbound_at IS NOT NULL
              AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
              AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
            ) = $22)
        AND ($23::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM call c_a
              WHERE c_a.organization_id = p.organization_id
                AND c_a.person_id = p.id
                AND c_a.caller_user_id = $47
                AND c_a.status IN ('ended', 'failed')
                AND c_a.ended_at IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM contact_attempted root_a
                    WHERE root_a.organization_id = c_a.organization_id
                      AND root_a.causation_id = c_a.id
                      AND root_a.corrects_id IS NULL
                      AND NOT EXISTS (SELECT 1 FROM contact_attempted x_a WHERE x_a.corrects_id = root_a.id)
                )
            )) = $23)
        -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags (none-of),
        -- feed A's own tag_ids parameters.
        AND ($52::uuid[] IS NULL OR EXISTS (
              SELECT 1 FROM person_tag pt_a
              WHERE pt_a.organization_id = $1
                AND pt_a.person_id = p.id AND pt_a.tag_id = ANY($52)))
        AND ($53::uuid[] IS NULL OR NOT EXISTS (
              SELECT 1 FROM person_tag pt2_a
              WHERE pt2_a.organization_id = $1
                AND pt2_a.person_id = p.id AND pt2_a.tag_id = ANY($53)))
        AND ($56::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$56 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$57) AND (($58='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57)) OR ($58='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57)) OR ($57='text' AND $58='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($59),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($57='text' AND $58='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($59),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($57='number' AND $58='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($60::text IS NULL OR v.number_value >= $60::numeric) AND ($61::text IS NULL OR v.number_value <= $61::numeric))) OR ($57='number' AND $58='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($60::text IS NULL OR v.number_value >= $60::numeric) AND ($61::text IS NULL OR v.number_value <= $61::numeric))) OR ($57='date' AND $58='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($62::date IS NULL OR v.date_value >= $62) AND ($63::date IS NULL OR v.date_value <= $63))) OR ($57='date' AND $58='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND ($62::date IS NULL OR v.date_value >= $62) AND ($63::date IS NULL OR v.date_value <= $63))) OR ($57='choice' AND $58='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND v.option_id=ANY($64))) OR ($57='choice' AND $58='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$56 AND v.field_type=$57 AND v.option_id=ANY($64))))))
        AND ($65::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$65 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$66) AND (($67='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66)) OR ($67='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66)) OR ($66='text' AND $67='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($68),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($66='text' AND $67='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($68),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($66='number' AND $67='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($69::text IS NULL OR v.number_value >= $69::numeric) AND ($70::text IS NULL OR v.number_value <= $70::numeric))) OR ($66='number' AND $67='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($69::text IS NULL OR v.number_value >= $69::numeric) AND ($70::text IS NULL OR v.number_value <= $70::numeric))) OR ($66='date' AND $67='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($71::date IS NULL OR v.date_value >= $71) AND ($72::date IS NULL OR v.date_value <= $72))) OR ($66='date' AND $67='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND ($71::date IS NULL OR v.date_value >= $71) AND ($72::date IS NULL OR v.date_value <= $72))) OR ($66='choice' AND $67='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND v.option_id=ANY($73))) OR ($66='choice' AND $67='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$65 AND v.field_type=$66 AND v.option_id=ANY($73))))))
        AND ($74::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$74 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$75) AND (($76='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75)) OR ($76='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75)) OR ($75='text' AND $76='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($77),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($75='text' AND $76='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($77),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($75='number' AND $76='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND ($78::text IS NULL OR v.number_value >= $78::numeric) AND ($79::text IS NULL OR v.number_value <= $79::numeric))) OR ($75='number' AND $76='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND ($78::text IS NULL OR v.number_value >= $78::numeric) AND ($79::text IS NULL OR v.number_value <= $79::numeric))) OR ($75='date' AND $76='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND ($80::date IS NULL OR v.date_value >= $80) AND ($81::date IS NULL OR v.date_value <= $81))) OR ($75='date' AND $76='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND ($80::date IS NULL OR v.date_value >= $80) AND ($81::date IS NULL OR v.date_value <= $81))) OR ($75='choice' AND $76='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND v.option_id=ANY($82))) OR ($75='choice' AND $76='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$74 AND v.field_type=$75 AND v.option_id=ANY($82))))))
        AND ($83::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$83 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$84) AND (($85='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84)) OR ($85='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84)) OR ($84='text' AND $85='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($86),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($84='text' AND $85='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($86),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($84='number' AND $85='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND ($87::text IS NULL OR v.number_value >= $87::numeric) AND ($88::text IS NULL OR v.number_value <= $88::numeric))) OR ($84='number' AND $85='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND ($87::text IS NULL OR v.number_value >= $87::numeric) AND ($88::text IS NULL OR v.number_value <= $88::numeric))) OR ($84='date' AND $85='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND ($89::date IS NULL OR v.date_value >= $89) AND ($90::date IS NULL OR v.date_value <= $90))) OR ($84='date' AND $85='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND ($89::date IS NULL OR v.date_value >= $89) AND ($90::date IS NULL OR v.date_value <= $90))) OR ($84='choice' AND $85='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND v.option_id=ANY($91))) OR ($84='choice' AND $85='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$83 AND v.field_type=$84 AND v.option_id=ANY($91))))))
        AND ($92::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$92 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$93) AND (($94='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93)) OR ($94='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93)) OR ($93='text' AND $94='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($95),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($93='text' AND $94='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($95),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($93='number' AND $94='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND ($96::text IS NULL OR v.number_value >= $96::numeric) AND ($97::text IS NULL OR v.number_value <= $97::numeric))) OR ($93='number' AND $94='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND ($96::text IS NULL OR v.number_value >= $96::numeric) AND ($97::text IS NULL OR v.number_value <= $97::numeric))) OR ($93='date' AND $94='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND ($98::date IS NULL OR v.date_value >= $98) AND ($99::date IS NULL OR v.date_value <= $99))) OR ($93='date' AND $94='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND ($98::date IS NULL OR v.date_value >= $98) AND ($99::date IS NULL OR v.date_value <= $99))) OR ($93='choice' AND $94='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND v.option_id=ANY($100))) OR ($93='choice' AND $94='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$92 AND v.field_type=$93 AND v.option_id=ANY($100))))))
      )) AS matrix_a,
        ((
        ($24::uuid[] IS NULL OR p.stage_id = ANY($24))
        AND ($25::uuid[] IS NULL OR p.assigned_user_id = ANY($25)
             OR ($26::boolean AND p.assigned_user_id IS NULL))
        AND ($27::text[] IS NULL OR latest.source = ANY($27))
        AND ($28::int IS NULL
             OR COALESCE(p.created_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $28))
        AND ($29::int IS NULL
             OR COALESCE(p.created_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $29))
        AND ($30::boolean IS NULL OR (p.created_at IS NULL) = $30)
        AND ($31::int IS NULL
             OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $31))
        AND ($32::int IS NULL
             OR COALESCE(p.last_inquiry_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $32))
        AND ($33::boolean IS NULL OR (p.last_inquiry_at IS NULL) = $33)
        AND ($34::int IS NULL
             OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $34))
        AND ($35::int IS NULL
             OR COALESCE(p.last_contact_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $35))
        AND ($36::boolean IS NULL OR (p.last_contact_at IS NULL) = $36)
        AND ($37::int IS NULL
             OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) > $46::timestamptz - make_interval(days => $37))
        AND ($38::int IS NULL
             OR COALESCE(p.last_inbound_at, '-infinity'::timestamptz) <= $46::timestamptz - make_interval(days => $38))
        AND ($39::boolean IS NULL OR (p.last_inbound_at IS NULL) = $39)
        AND ($40::boolean IS NULL OR (p.last_inbound_at IS NOT NULL) = $40)
        AND ($41::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM contact_method cm_phone_b
              WHERE cm_phone_b.person_id = p.id AND cm_phone_b.organization_id = p.organization_id
                AND cm_phone_b.kind = 'phone'
            )) = $41)
        AND ($42::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM contact_method cm_email_b
              WHERE cm_email_b.person_id = p.id AND cm_email_b.organization_id = p.organization_id
                AND cm_email_b.kind = 'email'
            )) = $42)
        AND ($43::boolean IS NULL OR (waiting.received_at IS NOT NULL) = $43)
        AND ($44::boolean IS NULL OR (
              p.last_inbound_at IS NOT NULL
              AND p.last_inbound_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
              AND p.last_inbound_at > COALESCE(p.last_outbound_at, '-infinity'::timestamptz)
            ) = $44)
        AND ($45::boolean IS NULL OR (EXISTS (
              SELECT 1 FROM call c_b
              WHERE c_b.organization_id = p.organization_id
                AND c_b.person_id = p.id
                AND c_b.caller_user_id = $47
                AND c_b.status IN ('ended', 'failed')
                AND c_b.ended_at IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM contact_attempted root_b
                    WHERE root_b.organization_id = c_b.organization_id
                      AND root_b.causation_id = c_b.id
                      AND root_b.corrects_id IS NULL
                      AND NOT EXISTS (SELECT 1 FROM contact_attempted x_b WHERE x_b.corrects_id = root_b.id)
                )
            )) = $45)
        -- docs/specs/SLICE_011e.md §4: tags (any-of) / not_tags (none-of),
        -- feed B's own tag_ids parameters.
        AND ($54::uuid[] IS NULL OR EXISTS (
              SELECT 1 FROM person_tag pt_b
              WHERE pt_b.organization_id = $1
                AND pt_b.person_id = p.id AND pt_b.tag_id = ANY($54)))
        AND ($55::uuid[] IS NULL OR NOT EXISTS (
              SELECT 1 FROM person_tag pt2_b
              WHERE pt2_b.organization_id = $1
                AND pt2_b.person_id = p.id AND pt2_b.tag_id = ANY($55)))
        AND ($101::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$101 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$102) AND (($103='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102)) OR ($103='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102)) OR ($102='text' AND $103='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($104),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($102='text' AND $103='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($104),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($102='number' AND $103='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND ($105::text IS NULL OR v.number_value >= $105::numeric) AND ($106::text IS NULL OR v.number_value <= $106::numeric))) OR ($102='number' AND $103='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND ($105::text IS NULL OR v.number_value >= $105::numeric) AND ($106::text IS NULL OR v.number_value <= $106::numeric))) OR ($102='date' AND $103='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND ($107::date IS NULL OR v.date_value >= $107) AND ($108::date IS NULL OR v.date_value <= $108))) OR ($102='date' AND $103='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND ($107::date IS NULL OR v.date_value >= $107) AND ($108::date IS NULL OR v.date_value <= $108))) OR ($102='choice' AND $103='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND v.option_id=ANY($109))) OR ($102='choice' AND $103='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$101 AND v.field_type=$102 AND v.option_id=ANY($109))))))
        AND ($110::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$110 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$111) AND (($112='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111)) OR ($112='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111)) OR ($111='text' AND $112='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($113),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($111='text' AND $112='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($113),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($111='number' AND $112='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND ($114::text IS NULL OR v.number_value >= $114::numeric) AND ($115::text IS NULL OR v.number_value <= $115::numeric))) OR ($111='number' AND $112='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND ($114::text IS NULL OR v.number_value >= $114::numeric) AND ($115::text IS NULL OR v.number_value <= $115::numeric))) OR ($111='date' AND $112='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND ($116::date IS NULL OR v.date_value >= $116) AND ($117::date IS NULL OR v.date_value <= $117))) OR ($111='date' AND $112='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND ($116::date IS NULL OR v.date_value >= $116) AND ($117::date IS NULL OR v.date_value <= $117))) OR ($111='choice' AND $112='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND v.option_id=ANY($118))) OR ($111='choice' AND $112='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$110 AND v.field_type=$111 AND v.option_id=ANY($118))))))
        AND ($119::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$119 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$120) AND (($121='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120)) OR ($121='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120)) OR ($120='text' AND $121='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($122),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($120='text' AND $121='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($122),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($120='number' AND $121='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND ($123::text IS NULL OR v.number_value >= $123::numeric) AND ($124::text IS NULL OR v.number_value <= $124::numeric))) OR ($120='number' AND $121='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND ($123::text IS NULL OR v.number_value >= $123::numeric) AND ($124::text IS NULL OR v.number_value <= $124::numeric))) OR ($120='date' AND $121='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND ($125::date IS NULL OR v.date_value >= $125) AND ($126::date IS NULL OR v.date_value <= $126))) OR ($120='date' AND $121='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND ($125::date IS NULL OR v.date_value >= $125) AND ($126::date IS NULL OR v.date_value <= $126))) OR ($120='choice' AND $121='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND v.option_id=ANY($127))) OR ($120='choice' AND $121='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$119 AND v.field_type=$120 AND v.option_id=ANY($127))))))
        AND ($128::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$128 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$129) AND (($130='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129)) OR ($130='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129)) OR ($129='text' AND $130='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($131),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($129='text' AND $130='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($131),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($129='number' AND $130='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND ($132::text IS NULL OR v.number_value >= $132::numeric) AND ($133::text IS NULL OR v.number_value <= $133::numeric))) OR ($129='number' AND $130='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND ($132::text IS NULL OR v.number_value >= $132::numeric) AND ($133::text IS NULL OR v.number_value <= $133::numeric))) OR ($129='date' AND $130='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND ($134::date IS NULL OR v.date_value >= $134) AND ($135::date IS NULL OR v.date_value <= $135))) OR ($129='date' AND $130='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND ($134::date IS NULL OR v.date_value >= $134) AND ($135::date IS NULL OR v.date_value <= $135))) OR ($129='choice' AND $130='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND v.option_id=ANY($136))) OR ($129='choice' AND $130='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$128 AND v.field_type=$129 AND v.option_id=ANY($136))))))
        AND ($137::uuid IS NULL OR (EXISTS (SELECT 1 FROM custom_field cf WHERE cf.id=$137 AND cf.organization_id=$1 AND cf.archived_at IS NULL AND cf.field_type=$138) AND (($139='is_set' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138)) OR ($139='is_not_set' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138)) OR ($138='text' AND $139='contains' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($140),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($138='text' AND $139='not_contains' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND lower(v.text_value) LIKE '%' || replace(replace(replace(lower($140),E'\\',E'\\\\'),'%','\%'),'_','\_') || '%' ESCAPE E'\\')) OR ($138='number' AND $139='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND ($141::text IS NULL OR v.number_value >= $141::numeric) AND ($142::text IS NULL OR v.number_value <= $142::numeric))) OR ($138='number' AND $139='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND ($141::text IS NULL OR v.number_value >= $141::numeric) AND ($142::text IS NULL OR v.number_value <= $142::numeric))) OR ($138='date' AND $139='range' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND ($143::date IS NULL OR v.date_value >= $143) AND ($144::date IS NULL OR v.date_value <= $144))) OR ($138='date' AND $139='not_range' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND ($143::date IS NULL OR v.date_value >= $143) AND ($144::date IS NULL OR v.date_value <= $144))) OR ($138='choice' AND $139='any_of' AND EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND v.option_id=ANY($145))) OR ($138='choice' AND $139='none_of' AND NOT EXISTS (SELECT 1 FROM person_custom_field_value v WHERE v.organization_id=$1 AND v.person_id=p.id AND v.field_id=$137 AND v.field_type=$138 AND v.option_id=ANY($145))))))
      )) AS matrix_b,
        p.last_inquiry_at AS latest_inquiry_received_at,
        p.last_inbound_at AS last_inbound_at,
        waiting.received_at AS waiting_received_at
    FROM person p
    LEFT JOIN LATERAL (
        SELECT i.source
        FROM inquiry i
        WHERE i.person_id = p.id AND i.organization_id = p.organization_id
        ORDER BY i.received_at DESC, i.id DESC
        LIMIT 1
    ) latest ON ($5::text[] IS NOT NULL OR $27::text[] IS NOT NULL)
    LEFT JOIN LATERAL (
        SELECT i2.received_at
        FROM inquiry i2
        WHERE p.last_inquiry_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
          AND i2.person_id = p.id AND i2.organization_id = p.organization_id
          AND i2.received_at > COALESCE(p.last_contact_at, '-infinity'::timestamptz)
        ORDER BY i2.received_at ASC
        LIMIT 1
    ) waiting ON true
    WHERE p.organization_id = $1
      AND p.last_inquiry_at IS NOT NULL
),
qualifying AS (
    SELECT
        r.id,
        -- F1 fix: for an unassigned Person under an assigned_to matrix that
        -- includes `unassigned`, the assigned-term OR chain
        -- (`p.assigned_user_id = ANY($3) OR ($4::boolean AND
        -- p.assigned_user_id IS NULL)`) can itself evaluate to NULL rather
        -- than true/false (NULL OR (false AND true) = NULL), which makes
        -- the whole matrix_a/matrix_b AND-chain NULL — `true AND NULL` is
        -- NULL, not false. COALESCE(..., false) here makes by_inquiry/
        -- by_reply TOTAL booleans (never NULL), matching their `"!"`-forced
        -- non-null projection in the outer SELECT; no admitted row changes,
        -- since a NULL matrix never qualified a candidate before either.
        (COALESCE($48::boolean, false) AND COALESCE(r.matrix_a, false)) AS by_inquiry,
        (COALESCE($49::boolean, false) AND COALESCE(r.matrix_b, false)) AS by_reply,
        r.latest_inquiry_received_at,
        r.last_inbound_at,
        r.waiting_received_at
    FROM ranked r
    WHERE (COALESCE($48::boolean, false) AND COALESCE(r.matrix_a, false))
       OR (COALESCE($49::boolean, false) AND COALESCE(r.matrix_b, false))
),
capped AS (
    SELECT
        q.*,
        CASE WHEN q.by_reply THEN q.last_inbound_at
             WHEN q.by_inquiry THEN q.waiting_received_at
             ELSE NULL END AS order_key,
        CASE WHEN q.by_reply THEN (q.last_inbound_at > $46::timestamptz - make_interval(hours => $51))
             WHEN q.by_inquiry THEN (q.latest_inquiry_received_at > $46::timestamptz - make_interval(hours => $50))
             ELSE false END AS fresh
    FROM qualifying q
    ORDER BY fresh DESC, order_key ASC, q.id ASC
    LIMIT 201
)
SELECT
    capped.id,
    p.first_name, p.last_name, p.created_at,
    s.id AS stage_id, s.name AS stage_name,
    u.id AS "assigned_user_id?", u.display_name AS "assigned_user_display_name?",
    (SELECT cm.value FROM contact_method cm
       WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
       ORDER BY cm.created_at ASC LIMIT 1) AS "primary_email?",
    (SELECT cm.value FROM contact_method cm
       WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
       ORDER BY cm.created_at ASC LIMIT 1) AS "primary_phone?",
    (SELECT count(*) FROM inquiry i
       WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS "inquiry_count!",
    latest.id AS "latest_inquiry_id?",
    latest.source AS "latest_inquiry_source?",
    latest.received_at AS "latest_inquiry_received_at?",
    effective_attempt.id AS "last_attempt_id?",
    effective_attempt.channel AS "last_attempt_channel?",
    effective_attempt.outcome AS "last_attempt_outcome?",
    effective_attempt.occurred_at AS "last_attempt_occurred_at?",
    capped.by_inquiry AS "by_inquiry!",
    capped.by_reply AS "by_reply!",
    capped.fresh AS "fresh!",
    capped.order_key AS "order_key?"
FROM capped
JOIN person p ON p.id = capped.id
JOIN stage s ON s.id = p.stage_id
LEFT JOIN app_user u ON u.id = p.assigned_user_id
LEFT JOIN LATERAL (
    SELECT i.id, i.source, i.received_at
    FROM inquiry i
    WHERE i.person_id = p.id AND i.organization_id = p.organization_id
    ORDER BY i.received_at DESC, i.id DESC
    LIMIT 1
) latest ON true
LEFT JOIN LATERAL (
    SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at
    FROM contact_attempted ca
    WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
      AND NOT EXISTS (SELECT 1 FROM contact_attempted x2 WHERE x2.corrects_id = ca.id)
    ORDER BY ca.occurred_at DESC, ca.id DESC
    LIMIT 1
) effective_attempt ON true
ORDER BY capped.fresh DESC, capped.order_key ASC, capped.id ASC
