-- Keep the applied Mobile005 migration immutable. A contact write at the
-- maximum revision must fail atomically rather than keep a reusable old token.
CREATE OR REPLACE FUNCTION crm_mobile_contact_details_revision()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
  IF TG_OP='UPDATE' AND NEW IS NOT DISTINCT FROM OLD THEN RETURN NULL; END IF;
  IF TG_OP<>'INSERT' THEN
    UPDATE public.person SET details_revision=details_revision+1
      WHERE id=OLD.person_id AND organization_id=OLD.organization_id
        AND details_revision<9223372036854775807;
    IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person
      WHERE id=OLD.person_id AND organization_id=OLD.organization_id) THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
  END IF;
  IF TG_OP='INSERT' OR (TG_OP='UPDATE' AND
    ROW(NEW.person_id,NEW.organization_id) IS DISTINCT FROM
    ROW(OLD.person_id,OLD.organization_id)) THEN
    UPDATE public.person SET details_revision=details_revision+1
      WHERE id=NEW.person_id AND organization_id=NEW.organization_id
        AND details_revision<9223372036854775807;
    IF NOT FOUND AND EXISTS(SELECT 1 FROM public.person
      WHERE id=NEW.person_id AND organization_id=NEW.organization_id) THEN
      RAISE EXCEPTION USING ERRCODE='22003', MESSAGE='details revision overflow';
    END IF;
  END IF;
  -- During a Person erasure cascade its row is already absent. There is no
  -- surviving profile token to invalidate, so contact deletion still succeeds.
  RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION crm_mobile_contact_details_revision() FROM PUBLIC;
