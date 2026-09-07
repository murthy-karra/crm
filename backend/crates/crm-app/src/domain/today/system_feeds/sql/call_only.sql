-- docs/specs/SLICE_011d.md §5 step 4b / §1 rule 4: only run when the
-- person-state statement was NOT truncated. The call-only prefix — People
-- with a qualifying call (one per Person, most recent by ended_at DESC, id
-- DESC, the compiled-in outcome_call membership) who are NOT already in
-- the retained P set, ordered ended_at ASC, id ASC (oldest outcome-needed
-- first), limited to (200 - |P|) + 1 — NARROWED by the call feed's full
-- customizable predicate matrix (identical axis semantics and parameter
-- positions to person/sql/filtered_summaries.sql and
-- queries::count_filtered_matches, including the three derived axes), so
-- an admin-added clause also applies here. With no extra clauses beyond
-- the anchor, every added predicate is NULL and this reduces exactly to
-- the original fixed-membership query. Carries the §1 rule 7 inquiry
-- constraint. Hydrated immediately (unlike person_state.sql's capped
-- prefix) because these are brand-new candidates, not already-fetched
-- Persons.
WITH prefix AS (
    SELECT DISTINCT ON (c.person_id)
        c.person_id AS id,
        c.id AS call_id,
        c.ended_at AS ended_at
    FROM call c
    WHERE c.organization_id = $1
      AND c.caller_user_id = $24
      AND c.status IN ('ended', 'failed')
      AND c.ended_at IS NOT NULL
      AND NOT (c.person_id = ANY($25::uuid[]))
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
),
matched AS (
    SELECT prefix.id, prefix.call_id, prefix.ended_at
    FROM prefix
    JOIN person p ON p.id = prefix.id AND p.organization_id = $1
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
    LEFT JOIN LATERAL (
        SELECT max(cc5.occurred_at) AS ts
        FROM correspondence_captured cc5
        WHERE cc5.person_id = p.id AND cc5.organization_id = p.organization_id
          AND cc5.direction = 'outbound'
    ) last_outbound_ts ON true
    WHERE ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
      AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
           OR ($4::boolean AND p.assigned_user_id IS NULL))
      AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
      AND ($6::int IS NULL
           OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => $6))
      AND ($7::int IS NULL
           OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => $7))
      AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
      AND ($9::int IS NULL
           OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $9))
      AND ($10::int IS NULL
           OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $10))
      AND ($11::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = $11)
      AND ($12::int IS NULL
           OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $12))
      AND ($13::int IS NULL
           OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $13))
      AND ($14::boolean IS NULL OR (last_contact_ts.ts IS NULL) = $14)
      AND ($15::int IS NULL
           OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $15))
      AND ($16::int IS NULL
           OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $16))
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
      AND ($21::boolean IS NULL OR (EXISTS (
            SELECT 1 FROM inquiry ia
            WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
              AND ia.received_at > COALESCE(last_contact_ts.ts, '-infinity'::timestamptz)
          )) = $21)
      AND ($22::boolean IS NULL OR (
            last_inbound_ts.ts IS NOT NULL
            AND last_inbound_ts.ts > COALESCE(last_contact_ts.ts, '-infinity'::timestamptz)
            AND last_inbound_ts.ts > COALESCE(last_outbound_ts.ts, '-infinity'::timestamptz)
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
),
capped AS (
    SELECT * FROM matched
    ORDER BY ended_at ASC, id ASC
    LIMIT $26
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
    capped.call_id AS "call_id!",
    capped.ended_at AS "ended_at!"
FROM capped
JOIN person p ON p.id = capped.id AND p.organization_id = $1
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
ORDER BY capped.ended_at ASC, capped.id ASC
