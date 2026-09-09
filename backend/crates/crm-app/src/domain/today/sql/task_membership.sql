-- docs/specs/SLICE_016.md §5 statement (a): "Membership for retained ids".
-- For every retained Person id (P ∪ call-only, i.e. `builtins` as they
-- stand right before this statement runs), the viewer's earliest open,
-- dated task with due_at <= now + 24 hours — filtered by ASSIGNEE before
-- choosing the earliest (the WHERE clause binds t.assignee_user_id = $2
-- before the DISTINCT ON ever considers a row), one row per Person. No
-- dynamic SQL, no rule-7 inquiry constraint, literal Organization
-- predicate, `now` bound as a parameter ($3). Reads
-- `task_org_assignee_due_open_idx` (organization_id, assignee_user_id,
-- due_at, id WHERE completed_at IS NULL AND deleted_at IS NULL AND
-- due_at IS NOT NULL).
SELECT DISTINCT ON (t.person_id)
    t.person_id AS "person_id!",
    t.id AS "task_id!",
    t.title AS "title!",
    t.kind AS "kind!",
    t.due_at AS "due_at!"
FROM task t
WHERE t.organization_id = $1
  AND t.assignee_user_id = $2
  AND t.completed_at IS NULL
  AND t.deleted_at IS NULL
  AND t.due_at IS NOT NULL
  AND t.due_at <= $3::timestamptz + interval '24 hours'
  AND t.person_id = ANY($4::uuid[])
ORDER BY t.person_id, t.due_at ASC, t.id ASC
