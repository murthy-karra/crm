-- Legacy ownership may be reconstructed only for writes after native revision
-- tracking was installed. SQLx records transaction-start installed_on and then
-- monotonic execution_time (nanoseconds), including commit. Their sum is a
-- conservative completion bound; unfinished/unknown/checksum-mismatched ledger
-- entries deliberately cannot qualify. This exposes no migration ledger writes.
CREATE FUNCTION crm_family_refresh_revision_installed(kind TEXT) RETURNS timestamptz
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=pg_catalog,public,pg_temp AS $$
  SELECT m.installed_on + (ceil(m.execution_time::numeric / 1000)::double precision * interval '1 microsecond')
  FROM public._sqlx_migrations m
  JOIN (VALUES
    ('task', 20260923000001::bigint, decode('021732320efa7c94279d8b29fa4a8be66bd9dcc0a9a1f7dbfb8de14542cf9d2c3e5677e2ab1825d653e3baf5795f1c28','hex')),
    ('note', 20260925000001::bigint, decode('76daa367b2303ed75aed0bf6f3c2b36a56201ece7eafc58aadc8211df07a378f3a9f1128ae85dffce1f59d06e1e42d05','hex')),
    ('metadata', 20261003000001::bigint, decode('6881596030626d64ba22819e8810a73b60e689542c7bcf476d0c4d6d3d0f2f73daf56dda068e0f30e79becb31fd9cea6','hex'))
  ) expected(kind,version,checksum) ON m.version=expected.version AND m.checksum=expected.checksum
  WHERE expected.kind=$1 AND m.success AND m.execution_time>=0;
$$;
REVOKE ALL ON FUNCTION crm_family_refresh_revision_installed(TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION crm_family_refresh_revision_installed(TEXT) TO crm_app;
