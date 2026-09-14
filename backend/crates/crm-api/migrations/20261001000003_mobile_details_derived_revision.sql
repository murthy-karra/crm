-- Preserve applied migrations. Only a name change or a nested contact trigger
-- may advance the aggregate; unrelated direct Person writes cannot invent +1.
CREATE OR REPLACE FUNCTION crm_mobile_details_revision()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF ROW(NEW.first_name, NEW.last_name) IS DISTINCT FROM ROW(OLD.first_name, OLD.last_name) THEN
    IF OLD.details_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
    NEW.details_revision := OLD.details_revision + 1;
  ELSIF NEW.details_revision IS DISTINCT FROM OLD.details_revision THEN
    -- The contact AFTER trigger issues its parent UPDATE, so this BEFORE
    -- trigger is nested. App roles cannot create triggers or invoke a trigger
    -- function directly. No caller-controlled session setting grants this path.
    IF pg_trigger_depth() < 2 OR OLD.details_revision = 9223372036854775807 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision is derived';
    END IF;
    IF NEW.details_revision <> OLD.details_revision + 1 THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision regression';
    END IF;
  END IF;
  RETURN NEW;
END $$;
