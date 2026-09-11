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
