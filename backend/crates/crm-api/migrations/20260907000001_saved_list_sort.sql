-- Slice 011b-sort: a saved list's sort order is part of its definition
-- (D-048). Two nullable columns: NULL means the default order
-- (`created.desc`). Deliberately no tombstone constraint — the older
-- binary's delete updates only name, filter, revision and deleted_at, so a
-- constraint tying sort to deleted_at would make every rollback-era delete
-- of a sorted list fail; a lingering sort token on a tombstone leaks
-- nothing and is never read.

ALTER TABLE saved_list
    ADD COLUMN sort_key TEXT,
    ADD COLUMN sort_direction TEXT,
    ADD CONSTRAINT saved_list_sort_key_check
        CHECK (sort_key IS NULL OR sort_key IN ('created','name','stage','assignee')),
    ADD CONSTRAINT saved_list_sort_direction_check
        CHECK (sort_direction IS NULL OR sort_direction IN ('asc','desc')),
    ADD CONSTRAINT saved_list_sort_pair_check
        CHECK ((sort_key IS NULL) = (sort_direction IS NULL));
