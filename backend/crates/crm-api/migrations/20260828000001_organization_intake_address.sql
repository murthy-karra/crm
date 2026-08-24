-- Slice 007a (docs/specs/SLICE_007a.md §3): every Organization becomes
-- addressable for email lead intake — an immutable DNS-label-safe slug
-- and an unguessable token, rendered into an address by configuration
-- (never stored as text). Backfill here: migrations are the sanctioned
-- non-application writer (D-021). No UPDATE grant on either column this
-- rung (rotation is a later rung).
ALTER TABLE organization
    ADD COLUMN intake_slug  TEXT NOT NULL DEFAULT '',
    ADD COLUMN intake_token TEXT NOT NULL DEFAULT '';

UPDATE organization
   SET intake_slug  = 'org-' || left(id::text, 8),
       intake_token = substr(md5(random()::text || id::text), 1, 8)
 WHERE intake_slug = '';

ALTER TABLE organization
    ALTER COLUMN intake_slug DROP DEFAULT,
    ALTER COLUMN intake_token DROP DEFAULT,
    ADD CONSTRAINT organization_intake_slug_format
        CHECK (intake_slug ~ '^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$'),
    ADD CONSTRAINT organization_intake_token_format
        CHECK (intake_token ~ '^[a-z0-9]{8}$');

CREATE UNIQUE INDEX organization_intake_slug_idx ON organization (intake_slug);
