-- Slice 012 (docs/specs/SLICE_012.md §2; D-052): four trigger-maintained
-- last-activity columns on `person`, backfilled once. One transaction, in
-- this exact order: columns -> trigger functions and triggers -> backfill
-- -> index. Triggers are created BEFORE the backfill: `CREATE TRIGGER`
-- takes a SHARE ROW EXCLUSIVE lock on each history table, so it drains any
-- insert already in flight from a still-running old binary before the
-- backfill reads, and guarantees every insert committed after this
-- migration fires the trigger. Backfilling first would leave any row
-- committed between the backfill's snapshot and this migration's commit
-- permanently unreflected.
--
-- No new grants: `crm_app` already holds unrestricted `UPDATE ON person`
-- (migration 20260821000002); the trigger functions run as the invoking
-- role (no SECURITY DEFINER), so `crm_app`'s existing person UPDATE grant
-- is what lets an insert into a history table (which `crm_app` can only
-- SELECT/INSERT, never UPDATE) still update the Person row through the
-- trigger.

ALTER TABLE person
    ADD COLUMN last_inquiry_at  TIMESTAMPTZ,
    ADD COLUMN last_contact_at  TIMESTAMPTZ,
    ADD COLUMN last_inbound_at  TIMESTAMPTZ,
    ADD COLUMN last_outbound_at TIMESTAMPTZ;

-- Each trigger function matches both `id` and `organization_id`, so a fact
-- row whose Organization differs from the Person's updates nothing
-- (defence in depth; the composite FKs already forbid such a row where
-- they exist). Guarded `col IS NULL OR col < NEW.ts` (the WHERE form, not
-- GREATEST), so a non-advancing row (a backdated capture, a correction
-- that inherits its original's `occurred_at`, or a deduplicated outbound
-- capture) is a no-op that produces no dead tuple (spec §1 rule 3).
-- `AFTER INSERT` only: history tables have no application UPDATE/DELETE
-- path (spec §1 rule 4), so no other trigger event is needed.

CREATE FUNCTION person_touch_last_inquiry() RETURNS TRIGGER AS $$
BEGIN
  UPDATE person SET last_inquiry_at = NEW.received_at
   WHERE id = NEW.person_id AND organization_id = NEW.organization_id
     AND (last_inquiry_at IS NULL OR last_inquiry_at < NEW.received_at);
  RETURN NULL;
END $$ LANGUAGE plpgsql;

CREATE FUNCTION person_touch_last_contact() RETURNS TRIGGER AS $$
BEGIN
  UPDATE person SET last_contact_at = NEW.occurred_at
   WHERE id = NEW.person_id AND organization_id = NEW.organization_id
     AND (last_contact_at IS NULL OR last_contact_at < NEW.occurred_at);
  RETURN NULL;
END $$ LANGUAGE plpgsql;

-- `correspondence_captured.direction` ('inbound' | 'outbound') selects
-- which of the two columns this fact advances. `ELSIF ... = 'outbound'`
-- (round 1 review fix 7), not a bare `ELSE`: unreachable today (the
-- column's own CHECK constraint already limits it to these two values),
-- but fails closed at zero cost if that CHECK is ever widened — an
-- unknown third direction then advances neither column instead of being
-- silently routed to last_outbound_at.
CREATE FUNCTION person_touch_correspondence() RETURNS TRIGGER AS $$
BEGIN
  IF NEW.direction = 'inbound' THEN
    UPDATE person SET last_inbound_at = NEW.occurred_at
     WHERE id = NEW.person_id AND organization_id = NEW.organization_id
       AND (last_inbound_at IS NULL OR last_inbound_at < NEW.occurred_at);
  ELSIF NEW.direction = 'outbound' THEN
    UPDATE person SET last_outbound_at = NEW.occurred_at
     WHERE id = NEW.person_id AND organization_id = NEW.organization_id
       AND (last_outbound_at IS NULL OR last_outbound_at < NEW.occurred_at);
  END IF;
  RETURN NULL;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER inquiry_touch_person
    AFTER INSERT ON inquiry
    FOR EACH ROW EXECUTE FUNCTION person_touch_last_inquiry();

CREATE TRIGGER contact_attempted_touch_person
    AFTER INSERT ON contact_attempted
    FOR EACH ROW EXECUTE FUNCTION person_touch_last_contact();

CREATE TRIGGER correspondence_captured_touch_person
    AFTER INSERT ON correspondence_captured
    FOR EACH ROW EXECUTE FUNCTION person_touch_correspondence();

-- BEGIN PERSON_LAST_ACTIVITY_BACKFILL
-- One rewrite of each Person row. Re-run for the affected People after any
-- future history deletion or redaction (O-013); `db_person_last_activity.rs`
-- extracts this block by its markers and re-runs it over seeded history
-- with the columns nulled (spec §2, §8.1) — a `sqlx::test` database is
-- migrated empty, so this is the only way to exercise the backfill over
-- real data.
UPDATE person p
   SET (last_inquiry_at, last_contact_at, last_inbound_at, last_outbound_at) = (
       (SELECT max(received_at) FROM inquiry i
         WHERE i.person_id = p.id AND i.organization_id = p.organization_id),
       (SELECT max(occurred_at) FROM contact_attempted ca
         WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id),
       (SELECT max(occurred_at) FROM correspondence_captured cc
         WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
           AND cc.direction = 'inbound'),
       (SELECT max(occurred_at) FROM correspondence_captured cc
         WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
           AND cc.direction = 'outbound'));
-- END PERSON_LAST_ACTIVITY_BACKFILL

-- Declared in exactly this column order (spec §1 rule 6) so a forward scan
-- serves `source_candidates`' `last_contact_at ASC NULLS FIRST, id ASC`
-- ordering. No index on the other three columns (spec §1 rule 6: their age
-- predicates are not index-sargable under the generic plans prepared
-- statements use, so an index there would only add a non-HOT row rewrite
-- to every history insert).
CREATE INDEX person_org_last_contact_idx
    ON person (organization_id, last_contact_at ASC NULLS FIRST, id ASC);
