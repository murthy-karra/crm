-- Slice 019a: typed custom fields on People. Ordinary relational CRUD
-- (AGENTS.md §4.6) — definitions and options are archived, never
-- deleted (D-058 §2); a value change is erasable CRUD cascaded with the
-- Person, never a history fact.

CREATE TABLE custom_field (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id    UUID NOT NULL REFERENCES organization (id),
    label              TEXT NOT NULL
        CHECK (char_length(label) BETWEEN 1 AND 60 AND label = btrim(label)
               AND label !~ '[[:cntrl:]]'),
    field_type         TEXT NOT NULL
        CHECK (field_type IN ('text', 'number', 'date', 'choice')),
    position           INTEGER NOT NULL,
    archived_at        TIMESTAMPTZ,
    created_by_user_id UUID NOT NULL,
    -- Import provenance (§8): both null, or both set.
    source             TEXT,
    external_key       TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((source IS NULL) = (external_key IS NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (id, organization_id),
    -- Lets a value row bind (field, organization, type) in one FK, so a
    -- type-mismatched value is unpersistable and a type change is
    -- refused by the database while any value exists (default
    -- ON UPDATE NO ACTION).
    UNIQUE (id, organization_id, field_type),
    FOREIGN KEY (organization_id, created_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
CREATE UNIQUE INDEX custom_field_org_live_label_key
    ON custom_field (organization_id, lower(label)) WHERE archived_at IS NULL;
CREATE UNIQUE INDEX custom_field_org_source_key
    ON custom_field (organization_id, source, external_key) WHERE source IS NOT NULL;

CREATE TABLE custom_field_option (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    field_id        UUID NOT NULL,
    label           TEXT NOT NULL
        CHECK (char_length(label) BETWEEN 1 AND 60 AND label = btrim(label)
               AND label !~ '[[:cntrl:]]'),
    position        INTEGER NOT NULL,
    archived_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (updated_at >= created_at),
    UNIQUE (id, field_id, organization_id),
    FOREIGN KEY (field_id, organization_id)
        REFERENCES custom_field (id, organization_id)
);
CREATE UNIQUE INDEX custom_field_option_live_label_key
    ON custom_field_option (field_id, lower(label)) WHERE archived_at IS NULL;

CREATE TABLE person_custom_field_value (
    organization_id    UUID NOT NULL,
    person_id          UUID NOT NULL,
    field_id           UUID NOT NULL,
    field_type         TEXT NOT NULL,
    text_value         TEXT
        CHECK (text_value IS NULL OR (char_length(text_value) BETWEEN 1 AND 500
               AND text_value = btrim(text_value, E' \t\r\n')
               AND position(E'\n' IN text_value) = 0)),
    number_value       NUMERIC(19, 4),
    date_value         DATE,
    option_id          UUID,
    -- NULL only for imported rows (the task.sql pattern).
    updated_by_user_id UUID,
    origin             TEXT NOT NULL,
    correlation_id     UUID NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, person_id, field_id),
    CHECK (num_nonnulls(text_value, number_value, date_value, option_id) = 1),
    CHECK (CASE field_type
             WHEN 'text'   THEN text_value   IS NOT NULL
             WHEN 'number' THEN number_value IS NOT NULL
             WHEN 'date'   THEN date_value   IS NOT NULL
             WHEN 'choice' THEN option_id    IS NOT NULL
             ELSE FALSE END),
    CHECK (updated_by_user_id IS NOT NULL OR origin = 'migration'),
    CHECK (updated_at >= created_at),
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE,
    FOREIGN KEY (field_id, organization_id, field_type)
        REFERENCES custom_field (id, organization_id, field_type),
    FOREIGN KEY (option_id, field_id, organization_id)
        REFERENCES custom_field_option (id, field_id, organization_id),
    FOREIGN KEY (organization_id, updated_by_user_id)
        REFERENCES organization_membership (organization_id, user_id)
);
CREATE INDEX person_custom_field_value_field_idx
    ON person_custom_field_value (organization_id, field_id);

GRANT SELECT, INSERT, UPDATE ON custom_field TO crm_app;
GRANT SELECT, INSERT, UPDATE ON custom_field_option TO crm_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON person_custom_field_value TO crm_app;
