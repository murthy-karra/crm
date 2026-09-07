-- Slice 011c: per-viewer saved-list preferences for the Today queue. These
-- are ordinary configuration rows; membership is evaluated from the current
-- saved definition at read time.

ALTER TABLE saved_list
    ADD CONSTRAINT saved_list_organization_id_id_key UNIQUE (organization_id, id);

CREATE TABLE today_work_source (
    organization_id UUID NOT NULL,
    user_id UUID NOT NULL,
    list_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, user_id, list_id),
    FOREIGN KEY (organization_id, user_id)
        REFERENCES organization_membership (organization_id, user_id),
    FOREIGN KEY (organization_id, list_id)
        REFERENCES saved_list (organization_id, id)
);

CREATE INDEX today_work_source_organization_list_idx
    ON today_work_source (organization_id, list_id);

-- Source preferences are target-state configuration. Application callers can
-- read, enable and disable them, but never update rows in place or truncate
-- another actor's preferences.
GRANT SELECT, INSERT, DELETE ON today_work_source TO crm_app;
