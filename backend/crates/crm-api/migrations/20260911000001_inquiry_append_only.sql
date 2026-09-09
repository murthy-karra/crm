-- LATER (012, recorded in docs/specs/SLICE_012.md's "Recorded LATER (D-050)"):
-- `inquiry` had no `reject_mutation` trigger, so the migrator role could
-- update/delete it directly (`db_operator.rs` did, hand-maintaining
-- `person.last_inquiry_at` next to the backdate). This closes that gap —
-- but not with the fact tables' unconditional `reject_mutation()`.
--
-- Why `inquiry` differs from the four D-015 §8 fact tables: the fact
-- tables reference erasable rows (person, inquiry, raw_payload) by bare
-- UUID with no FK specifically so they survive a Person's erasure with
-- orphaned ids (20260821000004_inquiry_and_facts.sql's own header
-- comment; D-015 §5: "delete the Person correlation row … History remains
-- intact with orphaned IDs"). `inquiry` is the opposite kind of row —
-- documented as "erasable CRUD" in that same migration, carrying the raw
-- lead message and a hard `FOREIGN KEY (person_id, organization_id)
-- REFERENCES person (id, organization_id) ON DELETE CASCADE` so it is
-- erased WITH the Person, per D-015 §5's erasure contract. An
-- unconditional `BEFORE DELETE FOR EACH ROW` trigger (the fact tables'
-- pattern) fires on rows deleted by that cascade too — confirmed
-- empirically before writing this migration: it raises and the person
-- DELETE fails — which would silently break Person erasure. There is no
-- fact-table precedent for this case; they never receive a cascade
-- delete at all (bare UUID, no FK).
--
-- `reject_direct_mutation()` (a new function; `reject_mutation()` is left
-- untouched and still serves the four fact tables and this table's own
-- TRUNCATE trigger below) tells a cascade-originated `DELETE` apart from
-- a hand-run one via `pg_trigger_depth()`: this trigger's own execution is
-- already depth 1 even when a plain top-level `DELETE FROM inquiry ...`
-- fires it directly, so the test is `> 1`, not `> 0` — `person`'s `ON
-- DELETE CASCADE` reaches `inquiry` from inside the foreign key's own
-- referential-integrity trigger (depth 1), so this trigger's execution of
-- the cascaded row delete runs one level deeper, at depth 2. A direct
-- `DELETE FROM inquiry ...` (crm_migrator, a script, a future runbook)
-- never has that extra level and is rejected exactly like `UPDATE`, which
-- has no legitimate cascade path and is always rejected regardless of
-- depth.
--
-- TRUNCATE has no such nuance — Postgres never cascades a TRUNCATE
-- through a plain (non-CASCADE-clause) `TRUNCATE inquiry` statement's own
-- FK behavior the way DELETE cascades do, and a `TRUNCATE person CASCADE`
-- is not part of this application's erasure path (D-015 §5 erases one
-- Person's row, never truncates the table) — so `inquiry`'s TRUNCATE
-- trigger reuses the existing, always-reject `reject_mutation()`, exactly
-- like the fact tables.

-- `pg_trigger_depth()` is already >= 1 the moment ANY trigger starts
-- executing, including this one firing directly off a top-level `DELETE
-- FROM inquiry ...` (that DELETE is depth 0; this trigger's own execution
-- is depth 1) — so `> 0` would accept every DELETE, direct or cascaded,
-- silently defeating the guard (caught by
-- direct_delete_of_inquiry_is_rejected in db_inquiry_append_only.rs
-- before this migration was finalized). A cascade from `person`'s `ON
-- DELETE CASCADE` runs one level deeper: `person`'s DELETE (depth 0)
-- invokes the FK's own referential-integrity trigger (depth 1), which
-- issues the internal `DELETE FROM inquiry ...` whose row trigger then
-- runs at depth 2. `> 1` is therefore the correct cascade test.
CREATE FUNCTION reject_direct_mutation() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' AND pg_trigger_depth() > 1 THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'inquiry is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER inquiry_append_only
    BEFORE UPDATE OR DELETE ON inquiry
    FOR EACH ROW EXECUTE FUNCTION reject_direct_mutation();

CREATE TRIGGER inquiry_no_truncate
    BEFORE TRUNCATE ON inquiry
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();
