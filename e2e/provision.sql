-- Schema roles, not business fixtures. Only runs in the owned fresh container.
\getenv app_password CRM_DB_APP_PASSWORD
\getenv migrator_password CRM_DB_MIGRATOR_PASSWORD
\getenv audit_password E2E_AUDIT_PASSWORD
CREATE ROLE crm_migrator LOGIN;
CREATE ROLE crm_app LOGIN;
CREATE ROLE e2e_audit LOGIN;
ALTER ROLE crm_app PASSWORD :'app_password';
ALTER ROLE crm_migrator PASSWORD :'migrator_password';
ALTER ROLE e2e_audit PASSWORD :'audit_password';
ALTER ROLE e2e_audit SET default_transaction_read_only = on;
ALTER DATABASE crm OWNER TO crm_migrator;
ALTER DEFAULT PRIVILEGES FOR ROLE crm_migrator IN SCHEMA public GRANT SELECT ON TABLES TO e2e_audit;
