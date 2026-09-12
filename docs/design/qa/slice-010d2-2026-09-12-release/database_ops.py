"""Private release evidence helper; CLI records a baseline and backup only.

Snapshot queries share one READ ONLY, REPEATABLE READ transaction. Row payloads
never leave PostgreSQL: hash every complete JSONB row, sort the fixed-width row
digests (retaining duplicates), then hash that complete sequence. Sessions are
not tenant tables: their scope column is active_organization_id.
"""

from pathlib import Path
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import time
from urllib.parse import unquote, urlparse

from release_ops import ROOT, RELEASE, values, save


BEFORE_MIGRATION = 20260921000001
AFTER_MIGRATION = 20260922000001
# Exact 45-table count inventory from the prior 010f2 release baseline.
BUSINESS_TABLES = (
    "app_user", "assignment_changed", "call", "call_completed", "capture_address",
    "capture_message", "capture_token_rotated", "contact_attempted", "contact_method",
    "correspondence_captured", "correspondence_raw", "custom_field", "custom_field_option",
    "inquiry", "inquiry_received", "intake_extraction", "intake_rotation",
    "intake_token_rotated", "invitation", "invitation_issued", "invitation_resolved",
    "local_credential", "membership_changed", "note", "operator_proposal",
    "operator_task_proposal", "operator_tool_call", "operator_turn", "organization",
    "organization_created", "organization_membership", "person",
    "person_custom_field_value", "person_tag", "platform_admin", "raw_payload",
    "routing_decision", "saved_list", "stage", "stage_changed", "tag", "task",
    "today_feed_changed", "today_system_feed", "today_work_source",
)
IMPORT_TABLES = {
    "migration_history_import_run", "migration_history_import_plan",
    "migration_history_import_anchor", "migration_history_import_stream",
    "migration_history_import_manifest", "migration_history_import_candidate",
    "migration_history_import_display", "migration_history_import_identity",
    "migration_history_import_result", "migration_history_import_reservation",
    "migration_history_import_receipt",
}
DERIVED_TABLE = "migration_history_review_state"
FACT_TABLES = {"fub_event_record_imported", "fub_call_record_imported", "fub_text_record_imported"}
NEW_MIGRATION_TABLES = IMPORT_TABLES | {DERIVED_TABLE}
NEW_TENANT_TABLES = NEW_MIGRATION_TABLES | FACT_TABLES
REVIEW_COUNT_KEYS = (
    "inquiries", "notes", "open_tasks", "completed_tasks", "person_imported",
    "inquiry_received", "routing_decision", "assignment_changed", "stage_changed",
    "contact_attempted", "call_completed", "correspondence",
    "fub_event_record_imported_known", "fub_event_record_imported_unknown",
    "fub_call_record_imported_known", "fub_call_record_imported_unknown",
    "fub_text_record_imported_known", "fub_text_record_imported_unknown",
)
HASH_METHOD = (
    "sha256(UTF8(newline-joined sorted lowercase hex sha256(UTF8(to_jsonb(row)::text))))"
)


class ReleaseDatabaseError(RuntimeError):
    """Closed diagnostics: never forward SQL, process stderr or configuration."""


def require(condition, code):
    if not condition:
        raise ReleaseDatabaseError(code)


def command_env():
    try:
        url = urlparse(values()["MIGRATION_DATABASE_URL"])
        require(
            url.scheme in ("postgres", "postgresql")
            and url.hostname in ("localhost", "127.0.0.1")
            and url.port in (None, 5432)
            and url.path == "/crm_dev"
            and unquote(url.username or "") == "crm_migrator"
            and bool(url.password)
            and not url.query and not url.fragment,
            "release_database_target_invalid",
        )
        env = {key: value for key, value in os.environ.items() if not key.startswith("PG")}
        env.update(
            PGHOST="127.0.0.1", PGPORT="5432", PGUSER="crm_migrator",
            PGPASSWORD=unquote(url.password), PGDATABASE="crm_dev", PGCONNECT_TIMEOUT="5",
            PGOPTIONS="-c default_transaction_read_only=on -c statement_timeout=120000",
        )
        return env
    except ReleaseDatabaseError:
        raise
    except Exception:
        raise ReleaseDatabaseError("release_database_configuration_unavailable") from None


def pg(args, **kwargs):
    command = ["docker", "exec", "-i"]
    for key in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE", "PGCONNECT_TIMEOUT", "PGOPTIONS"):
        command.extend(("-e", key))
    command.extend(("development-postgres-1", *args))
    require("env" not in kwargs, "release_database_environment_override_forbidden")
    try:
        return subprocess.run(command, env=command_env(), **kwargs)
    except ReleaseDatabaseError:
        raise
    except Exception:
        raise ReleaseDatabaseError("release_database_process_unavailable") from None


def snapshot_sql():
    # This is a fixed source-owned list, not tenant-controlled SQL identifiers.
    business = ",".join("'" + name + "'" for name in BUSINESS_TABLES)
    return r"""
\set ON_ERROR_STOP on
\set ECHO none
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO pg_catalog, public;
SELECT json_build_object(
 'kind','context',
 'transaction_read_only',current_setting('transaction_read_only'),
 'transaction_isolation',current_setting('transaction_isolation'),
 'observed_at',transaction_timestamp(),
 'migration',(SELECT max(version)::text FROM public._sqlx_migrations WHERE success),
 'failed_migrations',(SELECT count(*) FROM public._sqlx_migrations WHERE NOT success),
 'capture_schema_present',to_regclass('public.migration_history_capture_run') IS NOT NULL,
 'timeline_schema_present',to_regclass('public.migration_history_import_anchor') IS NOT NULL);

SELECT format(
 $count$SELECT json_build_object('kind','business','table',%L,'count',count(*)::text) FROM public.%I;$count$,
 table_name,table_name)
FROM unnest(ARRAY[""" + business + r"""]) AS names(table_name)
ORDER BY table_name
\gexec

SELECT format(
 $count$SELECT json_build_object('kind','migration','table',%L,'count',count(*)::text) FROM public.%I;$count$,
 tablename,tablename)
FROM pg_tables
WHERE schemaname='public' AND tablename LIKE 'migration_%'
ORDER BY tablename
\gexec

WITH tenant_tables AS (
 SELECT t.tablename
 FROM pg_tables t
 WHERE t.schemaname='public'
   AND (t.tablename='organization' OR EXISTS (
       SELECT 1 FROM information_schema.columns c
       WHERE c.table_schema='public' AND c.table_name=t.tablename
         AND c.column_name='organization_id'))
)
SELECT format(
 $rows$SELECT json_build_object('kind','rowset','table',%L,'count',count(*)::text,
 'sha256',encode(sha256(convert_to(coalesce(string_agg(row_sha,E'\n' ORDER BY row_sha),''),'UTF8')),'hex'))
 FROM (SELECT encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') AS row_sha
 FROM public.%I t) AS hashed_rows;$rows$,
 tablename,tablename)
FROM tenant_tables ORDER BY tablename
\gexec

SELECT json_build_object('kind','workspaces','rows',coalesce(json_agg(
 json_build_object('id',id,'mode',workspace_mode,'revision',workspace_revision)
 ORDER BY id),'[]'::json)) FROM public.organization;
SELECT json_build_object('kind','admissions','count',count(*)::text,
 'active_count',(count(*) FILTER (WHERE deadline>transaction_timestamp()))::text)
 FROM public.workspace_operation_admission;
""" + expected_backfill_sql() + r"""
COMMIT;
"""


def expected_backfill_sql():
    # Compute the expected new derived rowset from the unchanged native truth in
    # the same read-only snapshot. This works before the new table exists.
    empty = json.dumps({key: 0 for key in REVIEW_COUNT_KEYS}, separators=(",", ":"))
    return r"""
WITH native AS (
 SELECT organization_id,person_id,'person_imported'::text AS kind FROM public.person_imported
 UNION ALL SELECT organization_id,person_id,'inquiry_received' FROM public.inquiry_received
 UNION ALL SELECT organization_id,person_id,'routing_decision' FROM public.routing_decision
 UNION ALL SELECT organization_id,person_id,'assignment_changed' FROM public.assignment_changed
 UNION ALL SELECT organization_id,person_id,'stage_changed' FROM public.stage_changed
 UNION ALL SELECT organization_id,person_id,'contact_attempted' FROM public.contact_attempted
 UNION ALL SELECT organization_id,person_id,'call_completed' FROM public.call_completed
 UNION ALL SELECT organization_id,person_id,'correspondence' FROM public.correspondence_captured
 UNION ALL SELECT organization_id,person_id,'inquiries' FROM public.inquiry
 UNION ALL SELECT organization_id,person_id,'notes' FROM public.note WHERE deleted_at IS NULL
 UNION ALL SELECT organization_id,person_id,CASE WHEN completed_at IS NULL THEN 'open_tasks' ELSE 'completed_tasks' END
   FROM public.task WHERE deleted_at IS NULL
), grouped AS (
 SELECT organization_id,person_id,kind,count(*) AS n FROM native WHERE person_id IS NOT NULL
 GROUP BY organization_id,person_id,kind
), objects AS (
 SELECT organization_id,person_id,jsonb_object_agg(kind,n) AS counts FROM grouped
 GROUP BY organization_id,person_id
), expected AS (
 SELECT jsonb_build_object('organization_id',p.organization_id,'person_id',p.id,'revision',0,
 'counts','""" + empty + r"""'::jsonb||coalesce(o.counts,'{}'::jsonb)) AS row_value
 FROM public.person p LEFT JOIN objects o ON o.organization_id=p.organization_id AND o.person_id=p.id
), hashes AS (
 SELECT encode(sha256(convert_to(row_value::text,'UTF8')),'hex') AS row_sha FROM expected
)
SELECT json_build_object('kind','review_backfill_expected','count',count(*)::text,
 'sha256',encode(sha256(convert_to(coalesce(string_agg(row_sha,E'\n' ORDER BY row_sha),''),'UTF8')),'hex'))
 FROM hashes;
"""


def parse_snapshot(stdout):
    try:
        records = [json.loads(line) for line in stdout.splitlines() if line.strip()]
        singleton = {}
        groups = {"business": {}, "migration": {}, "rowset": {}}
        for row in records:
            kind = row["kind"]
            if kind in groups:
                table = row["table"]
                require(re.fullmatch(r"[a-z_][a-z0-9_]*", table), "release_table_name_invalid")
                require(table not in groups[kind], "release_snapshot_duplicate_table")
                count = int(row["count"])
                require(count >= 0, "release_snapshot_count_invalid")
                if kind == "rowset":
                    require(re.fullmatch(r"[0-9a-f]{64}", row["sha256"]), "release_rowset_hash_invalid")
                    groups[kind][table] = {"count": count, "sha256": row["sha256"]}
                else:
                    groups[kind][table] = count
            else:
                require(kind in ("context", "workspaces", "admissions", "review_backfill_expected") and kind not in singleton,
                        "release_snapshot_record_invalid")
                singleton[kind] = row
        require(set(singleton) == {"context", "workspaces", "admissions", "review_backfill_expected"}, "release_snapshot_incomplete")
        context = singleton["context"]
        require(context["transaction_read_only"] == "on"
                and context["transaction_isolation"] == "repeatable read", "release_snapshot_isolation_invalid")
        require(context["failed_migrations"] == 0, "release_failed_migration_present")
        proof = {
            "migration": int(context["migration"]),
            "business_counts": groups["business"], "migration_counts": groups["migration"],
            "tenant_rowsets": groups["rowset"], "rowset_sha256_method": HASH_METHOD,
            "tenant_table_inventory": sorted(groups["rowset"]),
            "workspaces": singleton["workspaces"]["rows"],
            "workspace_operation_admission": {
                "count": int(singleton["admissions"]["count"]),
                "active_count": int(singleton["admissions"]["active_count"]),
            },
            "capture_schema_present": context["capture_schema_present"],
            "timeline_schema_present": context["timeline_schema_present"],
            "review_backfill_expected": {
                "count": int(singleton["review_backfill_expected"]["count"]),
                "sha256": singleton["review_backfill_expected"]["sha256"],
            },
            "snapshot_context": {key: value for key, value in context.items() if key != "kind"},
            "scope": "All public base tables with organization_id plus organization; complete rowsets, not samples. Session rows are excluded; business counts retain the prior 45-table inventory.",
        }
        return proof
    except ReleaseDatabaseError:
        raise
    except Exception:
        raise ReleaseDatabaseError("release_snapshot_output_invalid") from None


def validate_snapshot(proof, baseline=None):
    version = proof["migration"]
    require(version in (BEFORE_MIGRATION, AFTER_MIGRATION), "release_schema_version_unexpected")
    require(set(proof["business_counts"]) == set(BUSINESS_TABLES), "release_business_inventory_changed")
    require(len(proof["migration_counts"]) == (65 if version == BEFORE_MIGRATION else 77),
            "release_migration_inventory_changed")
    require(proof["capture_schema_present"], "release_capture_schema_missing")
    require(proof["timeline_schema_present"] == (version == AFTER_MIGRATION), "release_timeline_schema_mismatch")
    expected_new = set() if version == BEFORE_MIGRATION else NEW_MIGRATION_TABLES
    require(set(proof["migration_counts"]) & NEW_MIGRATION_TABLES == expected_new,
            "release_history_import_inventory_mismatch")
    expected_tenant = set() if version == BEFORE_MIGRATION else NEW_TENANT_TABLES
    require(set(proof["tenant_rowsets"]) & NEW_TENANT_TABLES == expected_tenant,
            "release_history_tenant_inventory_mismatch")
    require(proof["workspaces"] and all(row["mode"] == "operational" for row in proof["workspaces"]),
            "release_workspace_state_unexpected")
    require(proof["workspace_operation_admission"] == {"count": 0, "active_count": 0},
            "release_workspace_admission_present")
    require("organization" in proof["tenant_rowsets"] and "user_session" not in proof["tenant_rowsets"],
            "release_tenant_inventory_invalid")
    derived = proof["review_backfill_expected"]
    require(derived["count"] == proof["business_counts"]["person"]
            and re.fullmatch(r"[0-9a-f]{64}", derived["sha256"]), "release_expected_backfill_invalid")
    if baseline is None:
        require(version == BEFORE_MIGRATION, "release_baseline_schema_unexpected")
        return {"baseline": True, "validated": True,
                "expected_new_derived_rows": derived["count"],
                "new_tables_absent": sorted(NEW_TENANT_TABLES)}
    require(baseline["migration"] == BEFORE_MIGRATION and baseline["rowset_sha256_method"] == HASH_METHOD,
            "release_baseline_invalid")
    require(proof["business_counts"] == baseline["business_counts"], "release_business_counts_changed")
    require(proof["workspaces"] == baseline["workspaces"], "release_workspace_state_changed")
    require(proof["workspace_operation_admission"] == baseline["workspace_operation_admission"],
            "release_workspace_admission_changed")
    old_migration = baseline["migration_counts"]
    require(all(proof["migration_counts"].get(table) == count for table, count in old_migration.items()),
            "release_prior_migration_counts_changed")
    require(set(proof["migration_counts"]) - set(old_migration) == expected_new,
            "release_additive_schema_mismatch")
    previous = baseline["tenant_rowsets"]
    require(all(proof["tenant_rowsets"].get(table) == fingerprint for table, fingerprint in previous.items()),
            "release_prior_tenant_rowset_changed")
    new_tenant = set(proof["tenant_rowsets"]) - set(previous)
    require(new_tenant == expected_tenant, "release_new_tenant_inventory_mismatch")
    empty = {"count": 0, "sha256": hashlib.sha256(b"").hexdigest()}
    require(all(proof["tenant_rowsets"][table] == empty for table in new_tenant - {DERIVED_TABLE}),
            "release_new_fact_or_import_rows_present")
    require(derived == baseline["review_backfill_expected"], "release_expected_native_backfill_changed")
    if version == AFTER_MIGRATION:
        require(proof["tenant_rowsets"][DERIVED_TABLE] == derived,
                "release_derived_backfill_mismatch")
        require(proof["migration_counts"][DERIVED_TABLE] == derived["count"],
                "release_derived_backfill_count_mismatch")
    return {"baseline": False, "validated": True, "prior_tenant_rowsets_unchanged": len(previous),
            "prior_business_counts_unchanged": len(BUSINESS_TABLES),
            "new_empty_import_and_fact_tables": sorted(new_tenant - {DERIVED_TABLE}),
            "derived_review_state_exact_backfill": version == AFTER_MIGRATION,
            "derived_review_state_rows": derived["count"] if version == AFTER_MIGRATION else None}


def snapshot(name):
    require(re.fullmatch(r"database-[a-z0-9-]+\.json", name), "release_snapshot_name_invalid")
    destination = RELEASE / name
    require(not destination.exists(), "release_snapshot_already_exists")
    result = pg(["psql", "-X", "--no-password", "-qAt", "-v", "ON_ERROR_STOP=1"],
                input=snapshot_sql(), text=True, capture_output=True, timeout=600)
    require(result.returncode == 0, "release_database_snapshot_failed")
    proof = parse_snapshot(result.stdout)
    # Preserve metadata-only evidence even when a later comparison rejects it.
    save(name, proof)
    baseline = None
    if name != "database-before.json":
        try:
            baseline = json.loads((RELEASE / "database-before.json").read_text())
        except Exception:
            raise ReleaseDatabaseError("release_baseline_unavailable") from None
    proof["preservation"] = validate_snapshot(proof, baseline)
    save(name, proof)
    return proof


def backup():
    path = RELEASE / "crm-dev-before.dump"
    start = time.monotonic()
    try:
        with path.open("xb") as output:
            path.chmod(0o600)
            result = pg(["pg_dump", "--format=custom", "--no-password"],
                        stdout=output, stderr=subprocess.PIPE, timeout=600)
        require(result.returncode == 0 and path.stat().st_size > 0, "release_backup_failed")
        with path.open("rb") as source:
            catalog = pg(["pg_restore", "--list"], stdin=source, capture_output=True, timeout=120)
        require(catalog.returncode == 0 and b"TABLE DATA" in catalog.stdout, "release_backup_catalog_failed")
        catalog_path = RELEASE / "backup-catalog.private"
        with catalog_path.open("xb") as output:
            catalog_path.chmod(0o600)
            output.write(catalog.stdout)
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        proof = {
            "result": "passed", "database": "crm_dev", "format": "custom",
            "bytes": path.stat().st_size, "sha256": digest.hexdigest(),
            "catalog_entries": sum(bool(line) and not line.startswith(b";") for line in catalog.stdout.splitlines()),
            "catalog_verified": True, "restore_exercised": False,
            "elapsed_seconds": round(time.monotonic() - start, 2),
            "observed_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "consistency": "pg_dump owns a separate consistent backup snapshot; it is not the earlier audit transaction snapshot.",
        }
        save("backup-result.json", proof)
        return proof
    except ReleaseDatabaseError:
        raise
    except Exception:
        raise ReleaseDatabaseError("release_backup_io_failed") from None


def main():
    before = snapshot("database-before.json")
    result = backup()
    print(json.dumps({
        "business_tables_recorded": len(before["business_counts"]),
        "tenant_rowsets_hashed": len(before["tenant_rowsets"]),
        "migration_tables_recorded": len(before["migration_counts"]),
        "migration_tables_empty": sum(count == 0 for count in before["migration_counts"].values()),
        "expected_derived_review_rows": before["review_backfill_expected"]["count"],
        "workspaces_operational": len(before["workspaces"]), "backup": result,
    }))


if __name__ == "__main__":
    try:
        main()
    except ReleaseDatabaseError as error:
        print(json.dumps({"result": "failed", "reason": str(error)}))
        raise SystemExit(1) from None
    except Exception:
        print(json.dumps({"result": "failed", "reason": "release_database_helper_failed"}))
        raise SystemExit(1) from None
