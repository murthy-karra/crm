-- docs/specs/SLICE_016.md §5 statement (b): the task-only prefix. Only run
-- when neither the person-state statement nor the call prefix was
-- truncated. The viewer's earliest open, dated task due within 24 hours on
-- Persons NOT already retained (P ∪ call-only, passed as $4), one row per
-- Person, ordered due_at ASC, id ASC, limited to (200 - |retained|) + 1
-- ($5) — the extra row only sets truncated_task; it is never admitted.
-- Hydrated immediately (these are brand-new candidates), joined to person
-- for the summary. Literal Organization predicate, no dynamic SQL, no
-- rule-7 inquiry constraint, `now` bound as a parameter ($3). Reads
-- `task_org_assignee_due_open_idx`.
--
-- `latest_inquiry`/`PersonSummary.last_inquiry_at`: round-1 review fix A —
-- D-054 amends 011c §5 ("built-in items continue to have a real
-- InquiryRef") only for the zero-inquiry case ("a task item on a
-- zero-inquiry Person carries `latest_inquiry: null`, the list-only
-- precedent"); 011c §5's general rule otherwise stands, so a task-only
-- item on a Person who DOES have an inquiry carries that real
-- `InquiryRef`, exactly like every other Today statement. Hydrated via the
-- same `LEFT JOIN LATERAL ... ORDER BY received_at DESC, id DESC LIMIT 1`
-- pattern `source_candidates.sql` uses for `latest`. `inquiry_count` on
-- the returned `PersonSummary` reflects reality regardless, matching every
-- other Today statement.
-- `mine` is forced `MATERIALIZED` (PostgreSQL 12+ inlines a `WITH` by
-- default) so the ONLY access to the base `task` table this whole
-- statement performs is this one scan, matching literal
-- `organization_id`/`assignee_user_id` and a `due_at` upper bound —
-- exactly the shape `task_org_assignee_due_open_idx` exists for, and
-- bounded by the viewer's own qualifying tasks (dozens, per spec), never
-- by the Organization's total People or task volume. Without this fence,
-- an equivalent `DISTINCT ON`/self-`NOT EXISTS` expressed directly against
-- `task` gives the planner a second table access to plan (an inner or
-- outer side needing to re-derive "one row per Person") that it may
-- satisfy via `task_org_person_due_idx` (organization_id, person_id,
-- due_at, id — no assignee/open-only guard) instead, DEGENERATING into a
-- scan of every open, dated task in the Organization regardless of
-- assignee — measured on PostgreSQL 18's skip-scan-capable planner at a
-- 25,000-Person book ("Rows Removed by Filter: 25000", including inside a
-- per-outer-row correlated subquery re-evaluated per candidate) — the
-- exact super-linear-growth failure §11 forbids. Once `mine` is
-- materialized, "the earliest open task per Person" (`prefix`) is a
-- correlated `NOT EXISTS` self-anti-join over that already-small,
-- in-memory row set, never touching `task` or any index again.
WITH mine AS MATERIALIZED (
    SELECT t.person_id, t.id AS task_id, t.title, t.kind, t.due_at
    FROM task t
    WHERE t.organization_id = $1
      AND t.assignee_user_id = $2
      AND t.completed_at IS NULL
      AND t.deleted_at IS NULL
      AND t.due_at IS NOT NULL
      AND t.due_at <= $3::timestamptz + interval '24 hours'
),
prefix AS (
    SELECT
        m.person_id AS id,
        m.task_id,
        m.title,
        m.kind,
        m.due_at
    FROM mine m
    WHERE NOT (m.person_id = ANY($4::uuid[]))
      AND NOT EXISTS (
          SELECT 1 FROM mine m2
          WHERE m2.person_id = m.person_id
            AND (m2.due_at, m2.task_id) < (m.due_at, m.task_id)
      )
),
capped AS (
    SELECT * FROM prefix
    ORDER BY due_at ASC, id ASC
    LIMIT $5
)
SELECT
    capped.id AS "id!",
    p.first_name, p.last_name, p.created_at AS "created_at!",
    s.id AS "stage_id!", s.name AS "stage_name!",
    u.id AS "assigned_user_id?", u.display_name AS "assigned_user_display_name?",
    (SELECT cm.value FROM contact_method cm
       WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'email'
       ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS "primary_email?",
    (SELECT cm.value FROM contact_method cm
       WHERE cm.person_id = p.id AND cm.organization_id = p.organization_id AND cm.kind = 'phone'
       ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS "primary_phone?",
    (SELECT count(*) FROM inquiry i
       WHERE i.person_id = p.id AND i.organization_id = p.organization_id) AS "inquiry_count!",
    latest.id AS "latest_inquiry_id?",
    latest.source AS "latest_inquiry_source?",
    latest.received_at AS "latest_inquiry_received_at?",
    effective_attempt.id AS "last_attempt_id?",
    effective_attempt.channel AS "last_attempt_channel?",
    effective_attempt.outcome AS "last_attempt_outcome?",
    effective_attempt.occurred_at AS "last_attempt_occurred_at?",
    capped.task_id AS "task_id!",
    capped.title AS "title!",
    capped.kind AS "kind!",
    capped.due_at AS "due_at!"
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
ORDER BY capped.due_at ASC, capped.id ASC
