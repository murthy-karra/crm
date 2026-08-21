-- Idempotent role provisioning for the local dev database (Slice 001).
-- Run by scripts/dev-services on every `up`, as the container superuser,
-- via psql so \getenv and :'var'/:"var" interpolation happen client-side.
-- Passwords never appear on argv or in a printed log (D-013).
\getenv app_password CRM_DB_APP_PASSWORD
\getenv migrator_password CRM_DB_MIGRATOR_PASSWORD
\getenv dbname POSTGRES_DB

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'crm_migrator') THEN
    CREATE ROLE crm_migrator LOGIN CREATEDB;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'crm_app') THEN
    CREATE ROLE crm_app LOGIN;
  END IF;
END
$$;

-- Always reapply so a changed .env takes effect on the next `up`.
ALTER ROLE crm_migrator PASSWORD :'migrator_password';
ALTER ROLE crm_app PASSWORD :'app_password';

-- PostgreSQL 15+ restricts CREATE on schema public to the database
-- owner; crm_migrator needs it to run migrations (and to create the
-- _sqlx_test bookkeeping schema used by check-db).
ALTER DATABASE :"dbname" OWNER TO crm_migrator;
