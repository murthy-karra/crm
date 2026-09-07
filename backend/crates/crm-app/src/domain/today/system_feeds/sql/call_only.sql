-- docs/specs/SLICE_011d.md §5 step 4b: only run when the person-state
-- statement was NOT truncated. The call-only prefix — People with a
-- qualifying call (one per Person, most recent by ended_at DESC, id DESC,
-- the compiled-in outcome_call membership) who are NOT already in the
-- retained P set, ordered ended_at ASC, id ASC (oldest outcome-needed
-- first), limited to (200 - |P|) + 1. Carries the §1 rule 7 inquiry
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
      AND c.caller_user_id = $2
      AND c.status IN ('ended', 'failed')
      AND c.ended_at IS NOT NULL
      AND NOT (c.person_id = ANY($3::uuid[]))
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
capped AS (
    SELECT * FROM prefix
    ORDER BY ended_at ASC, id ASC
    LIMIT $4
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
