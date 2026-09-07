-- docs/specs/SLICE_011d.md §5 step 4a: for every retained P id, the viewer's
-- one qualifying call (ended DESC, id DESC — the most recent), exactly the
-- compiled-in outcome_call membership, carrying the §1 rule 7 inquiry
-- constraint. Used to APPEND the call_outcome_needed reason to an
-- already-ranked person-state item, never to re-rank it.
SELECT DISTINCT ON (c.person_id)
    c.person_id AS "person_id!",
    c.id AS "call_id!",
    c.ended_at AS "ended_at!"
FROM call c
WHERE c.organization_id = $1
  AND c.caller_user_id = $2
  AND c.status IN ('ended', 'failed')
  AND c.ended_at IS NOT NULL
  AND c.person_id = ANY($3::uuid[])
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
