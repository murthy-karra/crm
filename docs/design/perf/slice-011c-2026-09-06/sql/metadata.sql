-- Scratch-table source enumeration prototype. saved_list supplies live visible
-- list metadata; perf_011c_today_source only stands in for the unimplemented
-- today_work_source preference relation and exists only in the generated DB.
SELECT source.position, sl.id, sl.revision, sl.name, sl.filter
FROM perf_011c_today_source source
JOIN saved_list sl ON sl.id = source.list_id AND sl.organization_id = source.organization_id
WHERE source.organization_id = $1
  AND source.user_id = $2
  AND source.case_key = $3
  AND sl.deleted_at IS NULL
  AND (sl.scope = 'shared' OR sl.created_by_user_id = $2)
ORDER BY source.position ASC, sl.id ASC
