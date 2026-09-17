//! D-065 workspace readiness. Guards are acquired before domain locks, and
//! held only for database work. No authority is inferred from Origin or role.
use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{Connection, PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::ids::{OrganizationId, UserId};

pub const GATE_VERSION: &str = "crm-workspace-v1";
pub const HISTORY_TIMELINE_CAPABILITY: &str = "fub-history-timeline-v1";
pub const CORE_CHANGE_CAPABILITY: &str = "fub-core-change-v1";
pub const PEOPLE_REFRESH_CAPABILITY: &str = "fub-people-refresh-v1";
pub const ADMITTED_PEOPLE_REFRESH_CAPABILITY: &str = "fub-admitted-people-refresh-v1";
pub const ADMITTED_METADATA_CAPABILITY: &str = "fub-admitted-metadata-v1";
pub const ADMITTED_HISTORY_CAPABILITY: &str = "fub-admitted-history-v1";
pub const ADMITTED_ACTIVITY_CAPABILITY: &str = "fub-admitted-activity-v1";
const ADMITTED_ACTIVITY_PRESENT: &str = "SELECT to_regclass('public.migration_admitted_activity_import') IS NOT NULL OR to_regprocedure('public.crm_admitted_activity_insert_allowed(uuid,text,jsonb)') IS NOT NULL OR EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_activity_identity') AND attname IN ('admitted_import_id','admitted_plan_id','admitted_manifest_id') AND NOT attisdropped)";
const PEOPLE_RECOVERY_SCHEMA: &str = include_str!("people_recovery_schema.sql");
const MAPPING_REPAIR_SCHEMA: &str = include_str!("mapping_repair_schema.sql");
const ADMITTED_HISTORY_SCHEMA: &str = include_str!("admitted_history_schema.sql");
const ADMITTED_HISTORY_PRESENT: &str = "SELECT to_regclass('public.migration_admitted_history_root') IS NOT NULL OR to_regprocedure('public.crm_admitted_history_fact_allowed(jsonb,text)') IS NOT NULL OR EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_history_import_identity') AND attname='admitted_root_id' AND NOT attisdropped)";
const ADMITTED_ACTIVITY_SCHEMA: &str = include_str!("admitted_activity_schema.sql");
const ADMITTED_METADATA_SCHEMA: &str = "SELECT (SELECT bool_and(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_metadata_catalog_readiness'),('migration_metadata_catalog_claim'),('migration_admitted_metadata_import'),('migration_admitted_metadata_plan'),('migration_admitted_metadata_manifest'),('migration_admitted_metadata_source'),('migration_admitted_metadata_reservation'),('migration_admitted_metadata_issue'),('migration_admitted_metadata_mapping'),('migration_admitted_metadata_operation'),('migration_admitted_metadata_result'),('migration_admitted_metadata_receipt'),('migration_admitted_metadata_observation'),('migration_admitted_metadata_alias')) required(name)) AND to_regprocedure('public.crm_admitted_metadata_insert_allowed(uuid,text,jsonb)') IS NOT NULL AND to_regprocedure('public.crm_metadata_identity_guard()') IS NOT NULL AND to_regprocedure('public.crm_metadata_identity_key_matches(bytea,jsonb)') IS NOT NULL AND EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.migration_metadata_identity') AND tgname='migration_metadata_identity_claim_guard' AND tgenabled IN ('O','A') AND tgfoid=to_regprocedure('public.crm_metadata_identity_guard()')) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_metadata_result') AND attname='unit_id' AND atttypid='uuid'::regtype AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_metadata_import') AND attname='checkpoint_id' AND atttypid='uuid'::regtype AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_metadata_import') AND attname='pause_reason' AND atttypid='text'::regtype AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_result_unit') AND indrelid=to_regclass('public.migration_admitted_metadata_result') AND indisunique AND indisvalid) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND attnotnull=r.required AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_plan','snapshot_id','uuid',true),('migration_admitted_metadata_plan','source_report_id','uuid',true),('migration_admitted_metadata_plan','source_output_revision','uuid',true),('migration_admitted_metadata_plan','capture_sequence','bigint',true),('migration_admitted_metadata_plan','previous_plan_id','uuid',false),('migration_admitted_metadata_plan','preparation_kind','text',true),('migration_admitted_metadata_receipt','snapshot_id','uuid',true)) r(table_name,column_name,type_name,required)) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_selected_target') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisunique AND indisvalid AND indisready) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_mapping_prepare') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisvalid AND indisready) AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_admitted_metadata_plan') AND conname='migration_admitted_metadata_plan_preparation_phase_check' AND contype='c' AND convalidated AND pg_get_constraintdef(oid) LIKE '%choices%' AND pg_get_constraintdef(oid) LIKE '%extra_values%' AND pg_get_constraintdef(oid) LIKE '%remainder_mappings%' AND pg_get_constraintdef(oid) LIKE '%remainder_people%' AND pg_get_constraintdef(oid) LIKE '%remainder_operations%')  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND attnotnull=r.required AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_import','admission_plan_id','uuid',true),('migration_admitted_metadata_import','settled_people','bigint',true),('migration_admitted_metadata_import','counts','jsonb',false),('migration_admitted_metadata_import','settled_eligible_people','bigint',true),('migration_admitted_metadata_import','confirmed_at','timestamp with time zone',false),('migration_admitted_metadata_import','completed_at','timestamp with time zone',false),('migration_admitted_metadata_mapping','dependent_count','bigint',true),('migration_admitted_metadata_mapping','alias_count','bigint',true)) r(table_name,column_name,type_name,required)) AND to_regprocedure('public.crm_admitted_metadata_mapping_counts()') IS NOT NULL AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.'||r.table_name) AND tgname='admitted_metadata_mapping_counts' AND tgenabled IN ('O','A') AND tgfoid=to_regprocedure('public.crm_admitted_metadata_mapping_counts()'))) FROM (VALUES ('migration_admitted_metadata_alias'),('migration_admitted_metadata_operation')) r(table_name)) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.'||r.index_name) AND indrelid=to_regclass('public.'||r.table_name) AND indisvalid AND indisready)) FROM (VALUES ('migration_admitted_metadata_alias_page','migration_admitted_metadata_alias'),('migration_admitted_metadata_result_page','migration_admitted_metadata_result'),('migration_admitted_metadata_person_result_page','migration_admitted_metadata_result'),('migration_admitted_metadata_mapping_page','migration_admitted_metadata_mapping'),('migration_admitted_metadata_manifest_page','migration_admitted_metadata_manifest'),('migration_admitted_metadata_operation_read','migration_admitted_metadata_operation'),('migration_admitted_metadata_manifest_disposition_page','migration_admitted_metadata_manifest'),('migration_admitted_metadata_result_kind_page','migration_admitted_metadata_result'),('migration_admitted_metadata_result_disposition_page','migration_admitted_metadata_result'),('migration_admitted_metadata_result_filtered_page','migration_admitted_metadata_result'),('migration_admitted_metadata_root_seek','migration_admitted_metadata_import'),('migration_admitted_metadata_cohort_seek','migration_admitted_metadata_import')) r(index_name,table_name)) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_one_root') AND indrelid=to_regclass('public.migration_admitted_metadata_import') AND indisunique AND indisvalid AND indisready AND pg_get_expr(indpred,indrelid) = '((predecessor_import_id IS NULL) AND ((state <> ''cancelled''::text) OR (confirmed_plan_id IS NOT NULL)))')  AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND attnotnull=r.required AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_plan','remainder_plan_id','uuid',false),('migration_admitted_metadata_plan','evidence_plan_id','uuid',false),('migration_admitted_metadata_mapping','predecessor_mapping_id','uuid',false),('migration_admitted_metadata_mapping','source_mapping_id','uuid',false),('migration_admitted_metadata_mapping','dependency_result_id','uuid',false),('migration_admitted_metadata_mapping','execute_unit','boolean',true),('migration_admitted_metadata_manifest','predecessor_manifest_id','uuid',false),('migration_admitted_metadata_import','held_settled_people','bigint',true)) r(table_name,column_name,type_name,required)) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_mapping_predecessor') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisvalid AND indisready AND indisunique) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_mapping_execute') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisvalid AND indisready) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_manifest_predecessor') AND indrelid=to_regclass('public.migration_admitted_metadata_manifest') AND indisvalid AND indisready AND indisunique)  AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_mapping_parent') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisvalid AND indisready) AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_plan_building') AND indrelid=to_regclass('public.migration_admitted_metadata_plan') AND indisvalid AND indisready) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND attnotnull=r.required AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_manifest','oversized','boolean',true),('migration_admitted_metadata_mapping','field_name_key','bytea',false)) r(table_name,column_name,type_name,required)) AND EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid=to_regclass('public.migration_admitted_metadata_mapping') AND conname='migration_admitted_metadata_mapping_field_name_key_check' AND contype='c' AND convalidated AND pg_get_constraintdef(oid) LIKE '%octet_length(field_name_key) = 32%') AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_admitted_metadata_field_name') AND indrelid=to_regclass('public.migration_admitted_metadata_mapping') AND indisvalid AND indisready AND pg_get_expr(indpred,indrelid) = '(field_name_key IS NOT NULL)') AND position('COALESCE(octet_length(NEW.field_name_key),0)' in pg_get_functiondef(to_regprocedure('public.crm_admitted_metadata_preparation_bytes()'))) > 0 AND position('COALESCE(octet_length(OLD.field_name_key),0)' in pg_get_functiondef(to_regprocedure('public.crm_admitted_metadata_preparation_bytes()'))) > 0 AND to_regprocedure('public.crm_admitted_metadata_preparation_bytes()') IS NOT NULL AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_plan','preparation_phase','text'),('migration_admitted_metadata_plan','preparation_sequence','bigint'),('migration_admitted_metadata_plan','preparation_ordinal','integer'),('migration_admitted_metadata_plan','preparation_key','uuid'),('migration_admitted_metadata_plan','preparation_parent','uuid'),('migration_admitted_metadata_plan','preparation_manifest','uuid'),('migration_admitted_metadata_plan','preparation_bytes','bigint'),('migration_admitted_metadata_plan','fields_processed','bigint'),('migration_admitted_metadata_plan','people_processed','bigint'),('migration_admitted_metadata_source','qualified','boolean'),('migration_admitted_metadata_source','conflict','boolean'),('migration_admitted_metadata_source','observations','bigint'),('migration_admitted_metadata_source','capture_id','uuid'),('migration_admitted_metadata_source','capture_sequence','bigint'),('migration_admitted_metadata_source','ordinal','integer'),('migration_admitted_metadata_manifest','expected_person_id','uuid')) r(table_name,column_name,type_name)) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_metadata_manifest') AND attname='person_id' AND NOT attnotnull AND NOT attisdropped) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.'||r.table_name) AND tgname='admitted_metadata_preparation_bytes' AND tgenabled IN ('O','A') AND tgfoid=to_regprocedure('public.crm_admitted_metadata_preparation_bytes()'))) FROM (VALUES ('migration_admitted_metadata_source'),('migration_admitted_metadata_mapping'),('migration_admitted_metadata_manifest'),('migration_admitted_metadata_operation'),('migration_admitted_metadata_observation')) r(table_name)) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.'||r.index_name) AND indrelid=to_regclass('public.'||r.table_name) AND indisvalid AND indisready)) FROM (VALUES ('migration_admitted_metadata_source_work','migration_admitted_metadata_source'),('migration_admitted_metadata_source_order','migration_admitted_metadata_source'),('migration_admitted_metadata_observation_source','migration_admitted_metadata_observation'),('migration_admitted_metadata_preparation_work','migration_admitted_metadata_import'),('migration_admitted_metadata_admission_work','migration_people_admission_result')) r(index_name,table_name)) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conrelid=to_regclass('public.'||r.table_name) AND c.conname=r.constraint_name AND c.convalidated AND pg_get_constraintdef(c.oid)=r.definition)) FROM (VALUES ('migration_people_admission_result','am_admission_result_owner_key','UNIQUE (id, admission_id, item_id, organization_id, source_id)'),('migration_people_admission_result','am_admission_result_person_key','UNIQUE (id, admission_id, item_id, organization_id, source_id, person_id)'),('migration_people_admission','am_admission_root_key','UNIQUE (id, organization_id, parent_import_id, parent_plan_id, source_account_id, confirmed_admission_plan_id)'),('migration_admitted_metadata_alias','am_alias_source_fk','FOREIGN KEY (source_row_id, plan_id, import_id, organization_id) REFERENCES migration_admitted_metadata_source(id, plan_id, import_id, organization_id)'),('migration_metadata_catalog_claim','am_claim_admitted_account_fk','FOREIGN KEY (admitted_import_id, organization_id, source_account_id) REFERENCES migration_admitted_metadata_import(id, organization_id, source_account_id)'),('migration_metadata_catalog_claim','am_claim_admitted_mapping_fk','FOREIGN KEY (admitted_mapping_id, admitted_plan_id, admitted_import_id, organization_id, kind, source_key, target_id) REFERENCES migration_admitted_metadata_mapping(id, plan_id, import_id, organization_id, kind, source_key, target_id)'),('migration_metadata_catalog_claim','am_claim_original_account_fk','FOREIGN KEY (original_import_id, organization_id, source_account_id) REFERENCES migration_metadata_import(id, organization_id, source_account_id)'),('migration_metadata_catalog_claim','am_claim_original_mapping_fk','FOREIGN KEY (original_mapping_id, original_plan_id, original_import_id, organization_id, kind, source_key, target_id) REFERENCES migration_metadata_mapping(id, plan_id, import_id, organization_id, kind, source_key, target_id)'),('migration_metadata_catalog_claim','am_claim_owner_check','CHECK ((((num_nonnulls(original_import_id, original_plan_id, original_mapping_id) = 3) AND (num_nonnulls(admitted_import_id, admitted_plan_id, admitted_mapping_id) = 0)) OR ((num_nonnulls(original_import_id, original_plan_id, original_mapping_id) = 0) AND (num_nonnulls(admitted_import_id, admitted_plan_id, admitted_mapping_id) = 3))))'),('migration_admitted_metadata_import','am_executor_org_fk','FOREIGN KEY (organization_id, executor_user_id) REFERENCES organization_membership(organization_id, user_id)'),('migration_admitted_metadata_manifest','am_manifest_admission_fk','FOREIGN KEY (admission_result_id, admission_id, admission_item_id, organization_id, source_person_id) REFERENCES migration_people_admission_result(id, admission_id, item_id, organization_id, source_id)'),('migration_admitted_metadata_manifest','am_manifest_base_plan_fk','FOREIGN KEY (plan_id, import_id, organization_id, predecessor_plan_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, remainder_plan_id)'),('migration_admitted_metadata_manifest','am_manifest_expected_person_fk','FOREIGN KEY (admission_result_id, admission_id, admission_item_id, organization_id, source_person_id, expected_person_id) REFERENCES migration_people_admission_result(id, admission_id, item_id, organization_id, source_id, person_id)'),('migration_admitted_metadata_manifest','am_manifest_live_person_check','CHECK (((person_id IS NULL) OR ((expected_person_id IS NOT NULL) AND (person_id = expected_person_id))))'),('migration_admitted_metadata_manifest','am_manifest_owner_key','UNIQUE (id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_manifest','am_manifest_person_key','UNIQUE (id, plan_id, import_id, organization_id, expected_person_id)'),('migration_admitted_metadata_manifest','am_manifest_predecessor_check','CHECK (((predecessor_manifest_id IS NULL) = (predecessor_plan_id IS NULL)))'),('migration_admitted_metadata_manifest','am_manifest_predecessor_fk','FOREIGN KEY (predecessor_manifest_id, predecessor_plan_id, organization_id, admission_result_id, source_person_id) REFERENCES migration_admitted_metadata_manifest(id, plan_id, organization_id, admission_result_id, source_person_id)'),('migration_admitted_metadata_manifest','am_manifest_predecessor_key','UNIQUE (id, plan_id, organization_id, admission_result_id, source_person_id)'),('migration_admitted_metadata_manifest','am_manifest_root_admission_fk','FOREIGN KEY (import_id, organization_id, admission_id) REFERENCES migration_admitted_metadata_import(id, organization_id, admission_id)'),('migration_admitted_metadata_mapping','am_mapping_base_plan_fk','FOREIGN KEY (plan_id, import_id, organization_id, predecessor_plan_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, remainder_plan_id)'),('migration_admitted_metadata_mapping','am_mapping_claim_key','UNIQUE (id, plan_id, import_id, organization_id, kind, source_key, target_id)'),('migration_admitted_metadata_mapping','am_mapping_dependency_check','CHECK ((((dependency_result_id IS NULL) AND (dependency_plan_id IS NULL) AND (dependency_import_id IS NULL)) OR ((dependency_result_id IS NOT NULL) AND (dependency_plan_id IS NOT NULL) AND (dependency_import_id IS NOT NULL))))'),('migration_admitted_metadata_mapping','am_mapping_dependency_fk','FOREIGN KEY (dependency_result_id, dependency_plan_id, dependency_import_id, organization_id, kind) REFERENCES migration_admitted_metadata_result(id, plan_id, import_id, organization_id, kind)'),('migration_admitted_metadata_mapping','am_mapping_evidence_check','CHECK (((source_mapping_id IS NULL) = (evidence_plan_id IS NULL)))'),('migration_admitted_metadata_mapping','am_mapping_evidence_plan_fk','FOREIGN KEY (plan_id, import_id, organization_id, evidence_plan_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, evidence_plan_id)'),('migration_admitted_metadata_mapping','am_mapping_kind_key','UNIQUE (id, plan_id, import_id, organization_id, kind)'),('migration_admitted_metadata_mapping','am_mapping_owner_key','UNIQUE (id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_mapping','am_mapping_predecessor_check','CHECK (((predecessor_mapping_id IS NULL) = (predecessor_plan_id IS NULL)))'),('migration_admitted_metadata_mapping','am_mapping_predecessor_fk','FOREIGN KEY (predecessor_mapping_id, predecessor_plan_id, organization_id, kind, source_key) REFERENCES migration_admitted_metadata_mapping(id, plan_id, organization_id, kind, source_key)'),('migration_admitted_metadata_mapping','am_mapping_source_fk','FOREIGN KEY (source_mapping_id, evidence_plan_id, organization_id, kind, source_key) REFERENCES migration_admitted_metadata_mapping(id, plan_id, organization_id, kind, source_key)'),('migration_admitted_metadata_mapping','am_mapping_source_key','UNIQUE (id, plan_id, organization_id, kind, source_key)'),('migration_admitted_metadata_observation','am_observation_record_fk','FOREIGN KEY (snapshot_record_id, snapshot_id, organization_id) REFERENCES migration_snapshot_record(id, snapshot_id, organization_id)'),('migration_admitted_metadata_observation','am_observation_snapshot_owner_fk','FOREIGN KEY (plan_id, import_id, organization_id, snapshot_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, snapshot_id)'),('migration_admitted_metadata_observation','am_observation_source_fk','FOREIGN KEY (source_row_id, plan_id, import_id, organization_id) REFERENCES migration_admitted_metadata_source(id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_operation','am_operation_manifest_fk','FOREIGN KEY (manifest_id, plan_id, import_id, organization_id) REFERENCES migration_admitted_metadata_manifest(id, plan_id, import_id, organization_id)'),('migration_metadata_import','am_original_account_key','UNIQUE (id, organization_id, source_account_id)'),('migration_metadata_mapping','am_original_claim_key','UNIQUE (id, plan_id, import_id, organization_id, kind, source_key, target_id)'),('migration_admitted_metadata_plan','am_plan_cohort_source_key','UNIQUE (id, organization_id, admission_id, snapshot_id, source_report_id, source_output_revision, capture_sequence)'),('migration_admitted_metadata_plan','am_plan_evidence_key','UNIQUE (id, import_id, organization_id, evidence_plan_id)'),('migration_admitted_metadata_plan','am_plan_evidence_plan_id_fk','FOREIGN KEY (evidence_plan_id, organization_id, admission_id, snapshot_id, source_report_id, source_output_revision, capture_sequence) REFERENCES migration_admitted_metadata_plan(id, organization_id, admission_id, snapshot_id, source_report_id, source_output_revision, capture_sequence)'),('migration_admitted_metadata_plan','am_plan_remainder_key','UNIQUE (id, import_id, organization_id, remainder_plan_id)'),('migration_admitted_metadata_plan','am_plan_remainder_plan_id_fk','FOREIGN KEY (remainder_plan_id, organization_id, admission_id, snapshot_id, source_report_id, source_output_revision, capture_sequence) REFERENCES migration_admitted_metadata_plan(id, organization_id, admission_id, snapshot_id, source_report_id, source_output_revision, capture_sequence)'),('migration_admitted_metadata_plan','am_plan_report_fk','FOREIGN KEY (source_report_id, organization_id, parent_import_id, parent_plan_id, source_account_id, snapshot_id, source_output_revision, capture_sequence) REFERENCES migration_core_change_report(id, organization_id, parent_import_id, parent_plan_id, source_account_id, newer_snapshot_id, output_revision, newer_sequence)'),('migration_admitted_metadata_plan','am_plan_root_fk','FOREIGN KEY (import_id, organization_id, admission_id, parent_import_id, parent_plan_id, source_account_id) REFERENCES migration_admitted_metadata_import(id, organization_id, admission_id, parent_import_id, parent_plan_id, source_account_id)'),('migration_admitted_metadata_plan','am_plan_source_key','UNIQUE (id, import_id, organization_id, snapshot_id)'),('migration_metadata_catalog_readiness','am_readiness_actor_fk','FOREIGN KEY (organization_id, activated_by_user_id) REFERENCES organization_membership(organization_id, user_id)'),('migration_admitted_metadata_receipt','am_receipt_actor_fk','FOREIGN KEY (organization_id, actor_user_id) REFERENCES organization_membership(organization_id, user_id)'),('migration_admitted_metadata_receipt','am_receipt_snapshot_owner_fk','FOREIGN KEY (plan_id, import_id, organization_id, snapshot_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, snapshot_id)'),('migration_core_change_report','am_report_plan_key','UNIQUE (id, organization_id, parent_import_id, parent_plan_id, source_account_id, newer_snapshot_id, output_revision, newer_sequence)'),('migration_core_change_report','am_report_root_key','UNIQUE (id, organization_id, parent_import_id, parent_plan_id, source_account_id, newer_snapshot_id, newer_sequence)'),('migration_admitted_metadata_reservation','am_reservation_snapshot_owner_fk','FOREIGN KEY (plan_id, import_id, organization_id, snapshot_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, snapshot_id)'),('migration_admitted_metadata_result','am_result_kind_owner_key','UNIQUE (id, plan_id, import_id, organization_id, kind)'),('migration_admitted_metadata_result','am_result_manifest_fk','FOREIGN KEY (manifest_id, plan_id, import_id, organization_id) REFERENCES migration_admitted_metadata_manifest(id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_result','am_result_mapping_fk','FOREIGN KEY (mapping_id, plan_id, import_id, organization_id, kind) REFERENCES migration_admitted_metadata_mapping(id, plan_id, import_id, organization_id, kind)'),('migration_admitted_metadata_result','am_result_owner_key','UNIQUE (id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_result','am_result_person_fk','FOREIGN KEY (manifest_id, plan_id, import_id, organization_id, person_id) REFERENCES migration_admitted_metadata_manifest(id, plan_id, import_id, organization_id, expected_person_id)'),('migration_admitted_metadata_result','am_result_unit_check','CHECK ((((kind = ''people''::text) AND (manifest_id IS NOT NULL) AND (unit_id = manifest_id)) OR ((kind <> ''people''::text) AND (manifest_id IS NULL) AND (person_id IS NULL))))'),('migration_admitted_metadata_import','am_root_account_key','UNIQUE (id, organization_id, source_account_id)'),('migration_admitted_metadata_import','am_root_admission_fk','FOREIGN KEY (admission_id, organization_id, parent_import_id, parent_plan_id, source_account_id, admission_plan_id) REFERENCES migration_people_admission(id, organization_id, parent_import_id, parent_plan_id, source_account_id, confirmed_admission_plan_id)'),('migration_admitted_metadata_import','am_root_admission_key','UNIQUE (id, organization_id, admission_id)'),('migration_admitted_metadata_import','am_root_chain_check','CHECK (((predecessor_import_id IS DISTINCT FROM id) AND (successor_import_id IS DISTINCT FROM id)))'),('migration_admitted_metadata_import','am_root_chain_key','UNIQUE (id, organization_id, admission_id, admission_plan_id, parent_import_id, parent_plan_id, source_account_id, source_report_id, snapshot_id, capture_sequence)'),('migration_admitted_metadata_import','am_root_cohort_key','UNIQUE (id, organization_id, admission_id, parent_import_id, parent_plan_id, source_account_id)'),('migration_admitted_metadata_import','am_root_predecessor_fk','FOREIGN KEY (predecessor_import_id, organization_id, admission_id, admission_plan_id, parent_import_id, parent_plan_id, source_account_id, source_report_id, snapshot_id, capture_sequence) REFERENCES migration_admitted_metadata_import(id, organization_id, admission_id, admission_plan_id, parent_import_id, parent_plan_id, source_account_id, source_report_id, snapshot_id, capture_sequence)'),('migration_admitted_metadata_import','am_root_report_fk','FOREIGN KEY (source_report_id, organization_id, parent_import_id, parent_plan_id, source_account_id, snapshot_id, capture_sequence) REFERENCES migration_core_change_report(id, organization_id, parent_import_id, parent_plan_id, source_account_id, newer_snapshot_id, newer_sequence)'),('migration_admitted_metadata_import','am_root_snapshot_fk','FOREIGN KEY (snapshot_id, organization_id, source_account_id) REFERENCES migration_snapshot(id, organization_id, source_account_id)'),('migration_admitted_metadata_import','am_root_successor_fk','FOREIGN KEY (successor_import_id, id, organization_id) REFERENCES migration_admitted_metadata_import(id, predecessor_import_id, organization_id)'),('migration_admitted_metadata_import','am_root_successor_key','UNIQUE (id, predecessor_import_id, organization_id)'),('migration_snapshot','am_snapshot_account_key','UNIQUE (id, organization_id, source_account_id)'),('migration_admitted_metadata_source','am_source_capture_fk','FOREIGN KEY (capture_id, snapshot_id, organization_id) REFERENCES migration_snapshot_capture(id, snapshot_id, organization_id)'),('migration_admitted_metadata_source','am_source_owner_key','UNIQUE (id, plan_id, import_id, organization_id)'),('migration_admitted_metadata_source','am_source_snapshot_owner_fk','FOREIGN KEY (plan_id, import_id, organization_id, snapshot_id) REFERENCES migration_admitted_metadata_plan(id, import_id, organization_id, snapshot_id)')) r(table_name,constraint_name,definition)) AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.'||r.table_name) AND attname=r.column_name AND atttypid=r.type_name::regtype AND attnotnull=r.required::boolean AND NOT attisdropped)) FROM (VALUES ('migration_admitted_metadata_plan','admission_id','uuid','true'),('migration_admitted_metadata_plan','parent_import_id','uuid','true'),('migration_admitted_metadata_plan','parent_plan_id','uuid','true'),('migration_admitted_metadata_plan','source_account_id','bigint','true'),('migration_admitted_metadata_source','snapshot_id','uuid','true'),('migration_admitted_metadata_observation','snapshot_id','uuid','true'),('migration_admitted_metadata_manifest','admission_id','uuid','true'),('migration_admitted_metadata_manifest','admission_item_id','uuid','true'),('migration_admitted_metadata_manifest','predecessor_plan_id','uuid','false'),('migration_admitted_metadata_mapping','predecessor_plan_id','uuid','false'),('migration_admitted_metadata_mapping','evidence_plan_id','uuid','false'),('migration_admitted_metadata_mapping','dependency_plan_id','uuid','false'),('migration_admitted_metadata_mapping','dependency_import_id','uuid','false'),('migration_admitted_metadata_receipt','plan_id','uuid','true'),('migration_admitted_metadata_result','mapping_id','uuid','false')) r(table_name,column_name,type_name,required)) AND EXISTS(SELECT 1 FROM pg_proc WHERE oid=to_regprocedure('public.crm_admitted_metadata_owner_keys()') AND md5(prosrc)='895daa97b355d16596b6ee1a16de19dd') AND (SELECT bool_and(EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass('public.'||r.table_name) AND tgname='admitted_metadata_owner_keys' AND tgenabled IN ('O','A') AND tgfoid=to_regprocedure('public.crm_admitted_metadata_owner_keys()') AND pg_get_triggerdef(oid)=r.definition)) FROM (VALUES ('migration_admitted_metadata_manifest','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id, import_id, organization_id, admission_id, admission_item_id, admission_result_id, predecessor_manifest_id, predecessor_plan_id ON public.migration_admitted_metadata_manifest FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()'),('migration_admitted_metadata_mapping','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id, import_id, organization_id, predecessor_mapping_id, predecessor_plan_id, source_mapping_id, evidence_plan_id, dependency_result_id, dependency_plan_id, dependency_import_id ON public.migration_admitted_metadata_mapping FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()'),('migration_admitted_metadata_observation','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id, import_id, organization_id, snapshot_id ON public.migration_admitted_metadata_observation FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()'),('migration_admitted_metadata_plan','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF import_id, organization_id, admission_id, parent_import_id, parent_plan_id, source_account_id, remainder_plan_id, evidence_plan_id ON public.migration_admitted_metadata_plan FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()'),('migration_admitted_metadata_receipt','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF import_id, organization_id, snapshot_id, plan_id ON public.migration_admitted_metadata_receipt FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()'),('migration_admitted_metadata_source','CREATE TRIGGER admitted_metadata_owner_keys BEFORE INSERT OR UPDATE OF plan_id, import_id, organization_id, snapshot_id ON public.migration_admitted_metadata_source FOR EACH ROW EXECUTE FUNCTION crm_admitted_metadata_owner_keys()')) r(table_name,definition)) AND EXISTS(SELECT 1 FROM pg_attribute a JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE a.attrelid=to_regclass('public.migration_admitted_metadata_result') AND a.attname='mapping_id' AND a.attgenerated='s' AND pg_get_expr(d.adbin,d.adrelid)='\nCASE\n    WHEN (kind <> ''people''::text) THEN unit_id\n    ELSE NULL::uuid\nEND') AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid=to_regclass('public.migration_people_admission_result_settled_cohort_source_page') AND indrelid=to_regclass('public.migration_people_admission_result') AND indisvalid AND indisready AND pg_get_indexdef(indexrelid)='CREATE INDEX migration_people_admission_result_settled_cohort_source_page ON public.migration_people_admission_result USING btree (admission_id, organization_id, source_id) WHERE (disposition = ''settled''::text)') AND to_regclass('public.am_admission_tag_source') IS NULL";
const ADMITTED_METADATA_PRESENT: &str = "SELECT (SELECT bool_or(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_metadata_catalog_readiness'),('migration_metadata_catalog_claim'),('migration_admitted_metadata_import'),('migration_admitted_metadata_plan'),('migration_admitted_metadata_manifest'),('migration_admitted_metadata_source'),('migration_admitted_metadata_reservation'),('migration_admitted_metadata_issue'),('migration_admitted_metadata_mapping'),('migration_admitted_metadata_operation'),('migration_admitted_metadata_result'),('migration_admitted_metadata_receipt'),('migration_admitted_metadata_observation'),('migration_admitted_metadata_alias')) required(name)) OR to_regprocedure('public.crm_admitted_metadata_insert_allowed(uuid,text,jsonb)') IS NOT NULL OR to_regprocedure('public.crm_metadata_identity_guard()') IS NOT NULL OR to_regprocedure('public.crm_metadata_identity_key_matches(bytea,jsonb)') IS NOT NULL OR to_regprocedure('public.crm_admitted_metadata_preparation_bytes()') IS NOT NULL OR to_regprocedure('public.crm_admitted_metadata_mapping_counts()') IS NOT NULL";
const ADMITTED_PEOPLE_REFRESH_SCHEMA: &str = "SELECT (SELECT bool_and(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_admitted_people_refresh'),('migration_admitted_people_refresh_plan'),('migration_admitted_people_refresh_item'),('migration_admitted_people_refresh_contact'),('migration_admitted_people_refresh_result'),('migration_admitted_people_refresh_baseline'),('migration_admitted_people_refresh_receipt'),('migration_admitted_people_refresh_reservation'),('person_admitted_refresh_provenance')) required(name)) AND to_regprocedure('public.crm_admitted_people_refresh_mutation_allowed(uuid,text,text,text,jsonb,jsonb)') IS NOT NULL AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_people_refresh_item') AND attname='baseline_version' AND atttypid='bigint'::regtype AND attnotnull AND NOT attisdropped) AND (SELECT count(*)=3 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_people_refresh_item') AND attname IN ('stage_mapping_id','assignee_mapping_id','baseline_result_id') AND atttypid='uuid'::regtype AND NOT attnotnull AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_people_refresh_item') AND attname='source_account_id' AND atttypid='bigint'::regtype AND NOT attnotnull AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_admitted_people_refresh_item') AND attname='source_semantic_hmac' AND atttypid='bytea'::regtype AND NOT attnotnull AND NOT attisdropped)";
const ADMITTED_PEOPLE_REFRESH_PRESENT: &str = "SELECT (SELECT bool_or(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_admitted_people_refresh'),('migration_admitted_people_refresh_plan'),('migration_admitted_people_refresh_item'),('migration_admitted_people_refresh_contact'),('migration_admitted_people_refresh_result'),('migration_admitted_people_refresh_baseline'),('migration_admitted_people_refresh_receipt'),('migration_admitted_people_refresh_reservation'),('person_admitted_refresh_provenance')) required(name)) OR to_regprocedure('public.crm_admitted_people_refresh_mutation_allowed(uuid,text,text,text,jsonb,jsonb)') IS NOT NULL";
pub const PEOPLE_ADMISSION_CAPABILITY: &str = "fub-people-admission-v1";
const PEOPLE_ADMISSION_SCHEMA: &str = "SELECT (SELECT bool_and(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_people_admission'),('migration_people_admission_plan'),('migration_people_admission_item'),('migration_people_admission_contact'),('migration_people_admission_result'),('migration_people_admission_receipt'),('migration_people_admission_reservation'),('person_admission_provenance'),('person_admitted')) required(name)) AND to_regprocedure('public.crm_people_admission_mutation_allowed(uuid,text,text,text,jsonb)') IS NOT NULL AND to_regprocedure('public.crm_people_admission_lock_stage(uuid,uuid)') IS NOT NULL AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_people_admission') AND attname='confirmed_admission_plan_id' AND atttypid='uuid'::regtype AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_people_admission_plan') AND attname='prepared_bytes' AND atttypid='bigint'::regtype AND attnotnull AND NOT attisdropped) AND (SELECT count(*)=3 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_import_identity') AND attname IN ('admission_id','admission_item_id','admission_result_id') AND atttypid='uuid'::regtype AND NOT attisdropped)";
const PEOPLE_ADMISSION_PRESENT: &str = "SELECT (SELECT bool_or(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_people_admission'),('migration_people_admission_plan'),('migration_people_admission_item'),('migration_people_admission_contact'),('migration_people_admission_result'),('migration_people_admission_receipt'),('migration_people_admission_reservation'),('person_admission_provenance'),('person_admitted')) required(name)) OR to_regprocedure('public.crm_people_admission_mutation_allowed(uuid,text,text,text,jsonb)') IS NOT NULL OR to_regprocedure('public.crm_people_admission_lock_stage(uuid,uuid)') IS NOT NULL OR EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_import_identity') AND attname IN ('admission_id','admission_item_id','admission_result_id') AND NOT attisdropped)";
pub const WAIT: Duration = Duration::from_secs(2);

tokio::task_local! {
    // Set only by a server-owned AuthContext. SQL revalidates active membership.
    static READER: (OrganizationId, UserId);
}

pub async fn with_reader<F: Future>(auth: &AuthContext, future: F) -> F::Output {
    READER
        .scope((auth.active_organization_id, auth.actor_user_id), future)
        .await
}

pub fn is_review_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010C")
}
pub fn is_forbidden_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010A")
}

pub fn is_activity_review_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010F")
}

pub fn is_history_review_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|c| c == "P010H")
}

/// Reject complete history representations under the caller's shared workspace
/// transaction before querying any arrays. A confirmed anchor is permanent.
pub async fn history_complete_read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_history_complete_read($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}

/// Call inside the existing shared workspace transaction, before any complete
/// activity fetch. First activity confirmation takes the exclusive barrier.
pub async fn activity_complete_read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_activity_complete_read($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn shared(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT set_config('crm.mapping_repair_reader','fub-people-mapping-repair-v1',true),set_config('crm.people_recovery_reader','fub-people-recovery-v1',true)",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("SELECT set_config('crm.history_reader',$1,true),set_config('crm.admitted_history_reader',$2,true)").bind(HISTORY_TIMELINE_CAPABILITY).bind(ADMITTED_HISTORY_CAPABILITY).execute(&mut *conn).await?;
    sqlx::query("SELECT set_config('crm.admitted_activity_reader',$1,true)")
        .bind(ADMITTED_ACTIVITY_CAPABILITY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT crm_workspace_shared($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn ordinary(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT set_config('crm.mapping_repair_reader','fub-people-mapping-repair-v1',true),set_config('crm.people_recovery_reader','fub-people-recovery-v1',true)",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("SELECT set_config('crm.history_reader',$1,true),set_config('crm.admitted_history_reader',$2,true)").bind(HISTORY_TIMELINE_CAPABILITY).bind(ADMITTED_HISTORY_CAPABILITY).execute(&mut *conn).await?;
    sqlx::query("SELECT set_config('crm.admitted_activity_reader',$1,true)")
        .bind(ADMITTED_ACTIVITY_CAPABILITY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT crm_workspace_operational($1)")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn begin(
    pool: &PgPool,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = tokio::time::timeout(WAIT, pool.begin())
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)??;
    ordinary(&mut tx, org).await?;
    Ok(tx)
}

/// Protects direct-domain queries too. Without explicit server read authority,
/// only operational workspaces are readable. Nested queries use SQLx savepoints.
pub async fn read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    match READER.try_with(|a| *a).ok().filter(|a| a.0 == org) {
        Some((_, actor)) => read_check(&mut tx, org, actor, false).await?,
        None => ordinary(&mut tx, org).await?,
    }
    Ok(tx)
}
pub async fn read_check(
    conn: &mut PgConnection,
    org: OrganizationId,
    actor: UserId,
    operational_only: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT set_config('crm.mapping_repair_reader','fub-people-mapping-repair-v1',true),set_config('crm.people_recovery_reader','fub-people-recovery-v1',true)",
    )
    .execute(&mut *conn)
    .await?;
    // Every caller holds an explicit read transaction. Set on this actual
    // connection, including nested SQLx readers; middleware and handlers do not
    // share a pooled connection. Transaction-local state cannot survive reuse.
    sqlx::query("SELECT set_config('crm.history_reader',$1,true),set_config('crm.admitted_activity_reader',$2,true),set_config('crm.admitted_history_reader','fub-admitted-history-v1',true)")
        .bind(HISTORY_TIMELINE_CAPABILITY)
        .bind(ADMITTED_ACTIVITY_CAPABILITY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT crm_workspace_read($1,$2,$3)")
        .bind(org.0)
        .bind(actor.0)
        .bind(operational_only)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn operational_read(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    ordinary(&mut tx, org).await?;
    Ok(tx)
}
pub async fn exclusive(conn: &mut PgConnection, org: OrganizationId) -> Result<(), sqlx::Error> {
    bounded_lock_wait(conn).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-workspace-v1:'||$1::text,0))")
        .bind(org.0)
        .execute(conn)
        .await?;
    Ok(())
}
/// Transaction-local bound for subsequent membership, Organization and domain
/// row locks. Shared advisory guards alone do not bound those row waits.
pub(crate) async fn bounded_lock_wait(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('lock_timeout','2000ms',true)")
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn admit_operator(
    pool: &PgPool,
    auth: &AuthContext,
    id: Uuid,
    deadline: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let mut tx = begin(pool, auth.active_organization_id).await?;
    read_check(
        &mut tx,
        auth.active_organization_id,
        auth.actor_user_id,
        true,
    )
    .await?;
    sqlx::query("INSERT INTO workspace_operation_admission(id,organization_id,actor_user_id,kind,deadline) VALUES($1,$2,$3,'operator',$4)")
        .bind(id).bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(deadline).execute(&mut *tx).await?;
    tx.commit().await
}
pub async fn release_operator(
    pool: &PgPool,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM workspace_operation_admission WHERE id=$1 AND organization_id=$2")
        .bind(id)
        .bind(org.0)
        .execute(pool)
        .await?;
    Ok(())
}

/// Only terminal settlement calls this after applying the closed transition
/// function. Database triggers independently verify call/person/terminal shape.
pub(crate) async fn terminal_call(
    conn: &mut PgConnection,
    org: OrganizationId,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    shared(conn, org).await?;
    sqlx::query("SELECT set_config('crm.terminal_call',$1,true)")
        .bind(id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn mode(
    conn: &mut PgConnection,
    org: OrganizationId,
) -> Result<(String, i64), sqlx::Error> {
    let r = sqlx::query("SELECT workspace_mode,workspace_revision FROM organization WHERE id=$1")
        .bind(org.0)
        .fetch_one(conn)
        .await?;
    Ok((r.get("workspace_mode"), r.get("workspace_revision")))
}

/// Server/operator-owned readiness, never deserialized from a tenant request.
#[derive(Clone)]
pub struct ReleaseReadiness {
    database: String,
    checked_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    synthetic: bool,
    metadata: bool,
    activity: bool,
    history_capture: bool,
    history_timeline: bool,
    core_change: bool,
    people_refresh: bool,
    people_admission: bool,
    admitted_people_refresh: bool,
    admitted_metadata: bool,
    admitted_activity: bool,
    admitted_history: bool,
    mapping_repair: bool,
    people_recovery: bool,
    family_refresh: bool,
}
impl ReleaseReadiness {
    pub async fn load_report(pool: &PgPool, path: &std::path::Path) -> Result<Self, sqlx::Error> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|_| sqlx::Error::Protocol("release evidence unavailable".into()))?;
        if bytes.len() > 1024 * 1024 {
            return Err(sqlx::Error::Protocol("release evidence invalid".into()));
        }
        let report: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| sqlx::Error::Protocol("release evidence invalid".into()))?;
        let hash = artifact_fingerprint().await?;
        let matching = report["candidates"].as_array().is_some_and(|a| {
            a.iter()
                .any(|v| v["sha256"] == hash && v["gate_version"] == GATE_VERSION)
        });
        if report["confirmation_ready"] != true || !matching {
            return Err(sqlx::Error::Protocol("release not ready".into()));
        }
        let database = report["database_name"]
            .as_str()
            .ok_or_else(|| sqlx::Error::Protocol("release database unavailable".into()))?
            .to_owned();
        let checked_at = serde_json::from_value(report["checked_at"].clone())
            .map_err(|_| sqlx::Error::Protocol("release timestamp invalid".into()))?;
        let expires_at = serde_json::from_value(report["evidence_expires_at"].clone())
            .map_err(|_| sqlx::Error::Protocol("release deadline invalid".into()))?;
        let ready = Self {
            database,
            checked_at,
            expires_at,
            synthetic: false,
            history_capture: history_capture_report_ready(&report, &hash),
            history_timeline: history_timeline_report_ready(&report, &hash),
            core_change: core_change_report_ready(&report, &hash),
            people_refresh: people_refresh_report_ready(&report, &hash),
            people_admission: people_admission_report_ready(&report, &hash),
            admitted_people_refresh: admitted_people_refresh_report_ready(&report, &hash),
            admitted_metadata: admitted_metadata_report_ready(&report, &hash),
            admitted_history: admitted_history_report_ready(&report, &hash),
            mapping_repair: mapping_repair_report_ready(&report, &hash),
            people_recovery: people_recovery_report_ready(&report, &hash),
            family_refresh: family_refresh_report_ready(&report, &hash),
            admitted_activity: report["admitted_activity_confirmation_ready"] == true
                && report["candidates"].as_array().is_some_and(|items| {
                    items.iter().any(|v| {
                        v["sha256"] == hash
                            && v["gate_version"] == GATE_VERSION
                            && v["capabilities"].as_array().is_some_and(|c| {
                                c.iter().any(|v| v == ADMITTED_ACTIVITY_CAPABILITY)
                            })
                    })
                }),
            activity: report["activity_confirmation_ready"] == true
                && report["candidates"].as_array().is_some_and(|items| {
                    items.iter().any(|v| {
                        v["sha256"] == hash
                            && v["gate_version"] == GATE_VERSION
                            && v["capabilities"]
                                .as_array()
                                .is_some_and(|c| c.iter().any(|v| v == "fub-activity-import-v1"))
                    })
                }),
            metadata: report["metadata_confirmation_ready"] == true
                && report["candidates"].as_array().is_some_and(|items| {
                    items.iter().any(|v| {
                        v["sha256"] == hash
                            && v["gate_version"] == GATE_VERSION
                            && v["capabilities"]
                                .as_array()
                                .is_some_and(|c| c.iter().any(|v| v == "fub-metadata-import-v1"))
                    })
                }),
        };
        ready.require_current(&mut *pool.acquire().await?).await?;
        Ok(ready)
    }
    pub async fn require_current(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        startup_compatible(conn).await?;
        if self.synthetic {
            return Ok(());
        }
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(conn)
            .await?;
        if database != self.database
            || self.expires_at <= Utc::now()
            || self.checked_at > Utc::now()
            || Utc::now() - self.checked_at > chrono::Duration::minutes(5)
        {
            return Err(sqlx::Error::Protocol("release evidence expired".into()));
        }
        Ok(())
    }
    pub fn metadata_ready(&self) -> bool {
        self.metadata
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_metadata(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.metadata_ready() {
            return Err(sqlx::Error::Protocol("metadata release not ready".into()));
        }
        Ok(())
    }
    #[cfg(feature = "test-support")]
    pub fn for_tests() -> Self {
        Self {
            database: "synthetic-only".into(),
            checked_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            synthetic: true,
            metadata: true,
            activity: true,
            history_capture: true,
            history_timeline: true,
            core_change: true,
            people_refresh: true,
            people_admission: true,
            admitted_people_refresh: true,
            admitted_metadata: true,
            admitted_activity: true,
            admitted_history: true,
            mapping_repair: true,
            people_recovery: true,
            family_refresh: true,
        }
    }

    pub fn activity_ready(&self) -> bool {
        self.activity
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_activity(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.activity_ready() {
            return Err(sqlx::Error::Protocol("activity release not ready".into()));
        }
        Ok(())
    }

    pub fn admitted_activity_ready(&self) -> bool {
        self.admitted_activity
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_admitted_activity(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.admitted_activity_ready() {
            return Err(sqlx::Error::Protocol(
                "admitted activity release not ready".into(),
            ));
        }
        Ok(())
    }

    pub fn history_capture_ready(&self) -> bool {
        self.history_capture
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_history_capture(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.history_capture_ready() {
            return Err(sqlx::Error::Protocol(
                "history capture release not ready".into(),
            ));
        }
        Ok(())
    }

    pub fn admitted_history_ready(&self) -> bool {
        self.admitted_history
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub fn family_refresh_ready(&self) -> bool {
        self.family_refresh
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_family_refresh(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.family_refresh_ready() {
            return Err(sqlx::Error::Protocol(
                "family refresh release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar("SELECT to_regprocedure('crm_family_refresh_sealing_fence()') IS NOT NULL AND to_regprocedure('crm_family_refresh_recipe_fence()') IS NOT NULL AND EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='migration_family_refresh_plan' AND column_name='seal_counts')")
            .fetch_one(conn).await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "family refresh schema incomplete".into(),
            ));
        }
        Ok(())
    }
    pub fn people_recovery_ready(&self) -> bool {
        self.people_recovery
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_people_recovery(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.people_recovery_ready() {
            return Err(sqlx::Error::Protocol(
                "people recovery release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(PEOPLE_RECOVERY_SCHEMA)
            .fetch_one(conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "people recovery schema incomplete".into(),
            ));
        }
        Ok(())
    }
    pub fn mapping_repair_ready(&self) -> bool {
        self.mapping_repair
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }
    pub async fn require_mapping_repair(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.mapping_repair_ready() {
            return Err(sqlx::Error::Protocol(
                "mapping repair release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(MAPPING_REPAIR_SCHEMA)
            .fetch_one(conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "mapping repair schema incomplete".into(),
            ));
        }
        Ok(())
    }
    pub async fn require_admitted_history(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.admitted_history_ready() {
            return Err(sqlx::Error::Protocol(
                "admitted history release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(ADMITTED_HISTORY_SCHEMA)
            .fetch_one(conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "admitted history schema incomplete".into(),
            ));
        }
        Ok(())
    }
    pub fn history_timeline_ready(&self) -> bool {
        self.history_timeline
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_history_timeline(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.history_timeline_ready() {
            return Err(sqlx::Error::Protocol(
                "history timeline release not ready".into(),
            ));
        }
        Ok(())
    }
    pub fn core_change_ready(&self) -> bool {
        self.core_change
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_core_change(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.core_change_ready() {
            return Err(sqlx::Error::Protocol(
                "core change release not ready".into(),
            ));
        }
        let schema: bool =
            sqlx::query_scalar("SELECT to_regclass('migration_core_change_report') IS NOT NULL")
                .fetch_one(&mut *conn)
                .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "core change schema unavailable".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_core_change_report WHERE engine_version<>$1)",
        )
        .bind(CORE_CHANGE_CAPABILITY)
        .fetch_one(conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "core change engine incompatible".into(),
            ));
        }
        Ok(())
    }
    pub fn people_refresh_ready(&self) -> bool {
        self.people_refresh
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_people_refresh(&self, conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.people_refresh_ready() {
            return Err(sqlx::Error::Protocol(
                "people refresh release not ready".into(),
            ));
        }
        let schema: bool =
            sqlx::query_scalar("SELECT (SELECT bool_and(to_regclass('public.'||name) IS NOT NULL) FROM (VALUES ('migration_people_refresh'),('migration_people_refresh_plan'),('migration_people_refresh_item'),('migration_people_refresh_contact'),('migration_people_refresh_result'),('migration_people_refresh_baseline'),('migration_people_refresh_receipt'),('migration_people_refresh_reservation')) required(name)) AND to_regprocedure('public.crm_people_refresh_mutation_allowed(uuid,text,text,text,jsonb)') IS NOT NULL AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_people_refresh') AND attname='confirmed_refresh_plan_id' AND atttypid='uuid'::regtype AND NOT attisdropped) AND EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('public.migration_people_refresh_plan') AND attname='prepared_bytes' AND atttypid='bigint'::regtype AND attnotnull AND NOT attisdropped)")
                .fetch_one(&mut *conn)
                .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "people refresh schema unavailable".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_people_refresh WHERE engine_version<>$1)",
        )
        .bind(PEOPLE_REFRESH_CAPABILITY)
        .fetch_one(conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "people refresh engine incompatible".into(),
            ));
        }
        Ok(())
    }
    pub fn people_admission_ready(&self) -> bool {
        self.people_admission
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_people_admission(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.people_admission_ready() {
            return Err(sqlx::Error::Protocol(
                "people admission release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(PEOPLE_ADMISSION_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "people admission schema unavailable".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_people_admission WHERE engine_version<>$1 AND engine_version<>'fub-people-recovery-v1')",
        )
        .bind(PEOPLE_ADMISSION_CAPABILITY)
        .fetch_one(conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "people admission engine incompatible".into(),
            ));
        }
        Ok(())
    }
    pub fn admitted_people_refresh_ready(&self) -> bool {
        self.admitted_people_refresh
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_admitted_people_refresh(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.admitted_people_refresh_ready() {
            return Err(sqlx::Error::Protocol(
                "admitted people refresh release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(ADMITTED_PEOPLE_REFRESH_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "admitted people refresh schema unavailable".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_admitted_people_refresh WHERE engine_version<>$1)",
        )
        .bind(ADMITTED_PEOPLE_REFRESH_CAPABILITY)
        .fetch_one(conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted people refresh engine incompatible".into(),
            ));
        }
        Ok(())
    }
    pub fn admitted_metadata_ready(&self) -> bool {
        self.admitted_metadata
            && (self.synthetic
                || (self.expires_at > Utc::now()
                    && self.checked_at <= Utc::now()
                    && Utc::now() - self.checked_at <= chrono::Duration::minutes(5)))
    }

    pub async fn require_admitted_metadata(
        &self,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        self.require_current(conn).await?;
        if !self.admitted_metadata_ready() {
            return Err(sqlx::Error::Protocol(
                "admitted metadata release not ready".into(),
            ));
        }
        let schema: bool = sqlx::query_scalar(ADMITTED_METADATA_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !schema {
            return Err(sqlx::Error::Protocol(
                "admitted metadata schema unavailable".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_import WHERE engine_version<>$1) OR EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE state='ready' AND engine_version IS DISTINCT FROM $1)",
        )
        .bind(ADMITTED_METADATA_CAPABILITY)
        .fetch_one(conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted metadata engine incompatible".into(),
            ));
        }
        Ok(())
    }
}

fn family_refresh_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["family_refresh_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"]
                        .as_array()
                        .is_some_and(|caps| caps.iter().any(|cap| cap == "fub-family-refresh-v1"))
            })
        })
}

fn people_recovery_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["people_recovery_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"]
                        .as_array()
                        .is_some_and(|caps| caps.iter().any(|cap| cap == "fub-people-recovery-v1"))
            })
        })
}

fn mapping_repair_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["mapping_repair_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|caps| {
                        caps.iter().any(|cap| cap == "fub-people-mapping-repair-v1")
                    })
            })
        })
}

fn admitted_history_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["admitted_history_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"]
                        .as_array()
                        .is_some_and(|c| c.iter().any(|v| v == ADMITTED_HISTORY_CAPABILITY))
            })
        })
}

fn history_timeline_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["history_timeline_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|v| v == HISTORY_TIMELINE_CAPABILITY)
                    })
            })
        })
}

fn core_change_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["core_change_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities.iter().any(|v| v == CORE_CHANGE_CAPABILITY)
                    })
            })
        })
}
fn people_refresh_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["people_refresh_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities.iter().any(|v| v == PEOPLE_REFRESH_CAPABILITY)
                    })
            })
        })
}

fn people_admission_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["people_admission_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|v| v == PEOPLE_ADMISSION_CAPABILITY)
                    })
            })
        })
}
fn admitted_people_refresh_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["admitted_people_refresh_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|v| v == ADMITTED_PEOPLE_REFRESH_CAPABILITY)
                    })
            })
        })
}
fn admitted_metadata_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["admitted_metadata_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|v| v == ADMITTED_METADATA_CAPABILITY)
                    })
            })
        })
}

fn history_capture_report_ready(report: &serde_json::Value, hash: &str) -> bool {
    report["history_capture_confirmation_ready"] == true
        && report["candidates"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["sha256"] == hash
                    && item["gate_version"] == GATE_VERSION
                    && matches!(item["role"].as_str(), Some("api" | "worker"))
                    && item["capabilities"].as_array().is_some_and(|capabilities| {
                        capabilities.iter().any(|v| v == "fub-history-capture-v1")
                    })
            })
        })
}
/// Every new API/worker/CLI invokes this before accepting work. Old binaries
/// cannot acquire this capability; deployment additionally retires them using
/// the operator release preflight's complete workload inventory.
pub async fn startup_compatible(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('migration_workspace') IS NOT NULL AND to_regclass('migration_activity_import') IS NOT NULL AND to_regclass('migration_history_capture_run') IS NOT NULL AND to_regclass('migration_history_import_anchor') IS NOT NULL")
        .fetch_one(&mut *conn)
        .await?;
    if !exists {
        return Err(sqlx::Error::Protocol(
            "workspace schema upgrade required".into(),
        ));
    }
    let unsupported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM migration_workspace WHERE gate_version<>$1)",
    )
    .bind(GATE_VERSION)
    .fetch_one(&mut *conn)
    .await?;
    if unsupported {
        return Err(sqlx::Error::Protocol(
            "workspace artifact incompatible".into(),
        ));
    }
    let history_unsupported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM migration_history_capture_run WHERE confirmed_at IS NOT NULL AND (profile_version<>'fub-history-v1' OR parser_version<>'1'))",
    )
    .fetch_one(&mut *conn)
    .await?;
    if history_unsupported {
        return Err(sqlx::Error::Protocol(
            "history capture artifact incompatible".into(),
        ));
    }
    let timeline_unsupported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM migration_history_import_anchor WHERE interpretation_version<>'fub-history-interpretation-v1' OR reader_version<>$1)",
    )
    .bind(HISTORY_TIMELINE_CAPABILITY)
    .fetch_one(&mut *conn)
    .await?;
    if timeline_unsupported {
        return Err(sqlx::Error::Protocol(
            "history timeline artifact incompatible".into(),
        ));
    }
    // An older schema remains usable for its existing capabilities. Once
    // refresh state exists, this writer must understand its retained engine.
    let refresh_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('migration_people_refresh') IS NOT NULL")
            .fetch_one(&mut *conn)
            .await?;
    if refresh_exists {
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_people_refresh WHERE engine_version<>$1)",
        )
        .bind(PEOPLE_REFRESH_CAPABILITY)
        .fetch_one(&mut *conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "people refresh artifact incompatible".into(),
            ));
        }
    }
    // Partial admission schemas cannot hide retained identities or provenance.
    let admission_exists: bool = sqlx::query_scalar(PEOPLE_ADMISSION_PRESENT)
        .fetch_one(&mut *conn)
        .await?;
    if admission_exists {
        let compatible: bool = sqlx::query_scalar(PEOPLE_ADMISSION_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !compatible {
            return Err(sqlx::Error::Protocol(
                "people admission schema incompatible".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_people_admission WHERE engine_version<>$1 AND engine_version<>'fub-people-recovery-v1')",
        )
        .bind(PEOPLE_ADMISSION_CAPABILITY)
        .fetch_one(&mut *conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "people admission artifact incompatible".into(),
            ));
        }
    }
    // Partial admitted-refresh schemas cannot hide retained identities or provenance.
    let admitted_refresh_exists: bool = sqlx::query_scalar(ADMITTED_PEOPLE_REFRESH_PRESENT)
        .fetch_one(&mut *conn)
        .await?;
    if admitted_refresh_exists {
        let compatible: bool = sqlx::query_scalar(ADMITTED_PEOPLE_REFRESH_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !compatible {
            return Err(sqlx::Error::Protocol(
                "admitted people refresh schema incompatible".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_admitted_people_refresh WHERE engine_version<>$1)",
        )
        .bind(ADMITTED_PEOPLE_REFRESH_CAPABILITY)
        .fetch_one(&mut *conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted people refresh artifact incompatible".into(),
            ));
        }
    }
    // Partial admitted-metadata schemas cannot hide retained identities or provenance.
    let admitted_metadata_exists: bool = sqlx::query_scalar(ADMITTED_METADATA_PRESENT)
        .fetch_one(&mut *conn)
        .await?;
    if admitted_metadata_exists {
        let compatible: bool = sqlx::query_scalar(ADMITTED_METADATA_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !compatible {
            return Err(sqlx::Error::Protocol(
                "admitted metadata schema incompatible".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_import WHERE engine_version<>$1) OR EXISTS(SELECT 1 FROM migration_metadata_catalog_readiness WHERE state='ready' AND engine_version IS DISTINCT FROM $1)",
        )
        .bind(ADMITTED_METADATA_CAPABILITY)
        .fetch_one(&mut *conn)
        .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted metadata artifact incompatible".into(),
            ));
        }
    }
    if sqlx::query_scalar::<_, bool>(ADMITTED_HISTORY_PRESENT)
        .fetch_one(&mut *conn)
        .await?
    {
        if !sqlx::query_scalar::<_, bool>(ADMITTED_HISTORY_SCHEMA)
            .fetch_one(&mut *conn)
            .await?
        {
            return Err(sqlx::Error::Protocol(
                "admitted history schema incompatible".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_history_root r JOIN migration_admitted_history_plan p ON p.id=r.confirmed_plan_id AND p.organization_id=r.organization_id WHERE p.source_binding->>'interpretation_version' IS DISTINCT FROM 'fub-history-interpretation-v1' OR p.source_binding->>'identity_version' IS DISTINCT FROM 'timeline-import-identity-v1')").fetch_one(&mut *conn).await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted history artifact incompatible".into(),
            ));
        }
    }
    let repair_present:bool=sqlx::query_scalar("SELECT to_regclass('public.migration_mapping_repair_requirement') IS NOT NULL OR to_regprocedure('public.crm_mapping_repair_owned_write()') IS NOT NULL").fetch_one(&mut *conn).await?;
    if repair_present
        && !sqlx::query_scalar::<_, bool>(MAPPING_REPAIR_SCHEMA)
            .fetch_one(&mut *conn)
            .await?
    {
        return Err(sqlx::Error::Protocol(
            "mapping repair schema incompatible".into(),
        ));
    }
    let recovery_present:bool=sqlx::query_scalar("SELECT to_regclass('public.migration_people_recovery_requirement') IS NOT NULL OR to_regprocedure('public.crm_people_recovery_owned_write()') IS NOT NULL").fetch_one(&mut *conn).await?;
    if recovery_present
        && !sqlx::query_scalar::<_, bool>(PEOPLE_RECOVERY_SCHEMA)
            .fetch_one(&mut *conn)
            .await?
    {
        return Err(sqlx::Error::Protocol(
            "people recovery schema incompatible".into(),
        ));
    }
    // The global activity identity registry changes owner shape in 010f4.
    // Refuse a partial installation before an original or admitted worker can
    // claim/settle a unit with the wrong owner tuple.
    let admitted_activity_exists: bool = sqlx::query_scalar(ADMITTED_ACTIVITY_PRESENT)
        .fetch_one(&mut *conn)
        .await?;
    if admitted_activity_exists {
        let compatible: bool = sqlx::query_scalar(ADMITTED_ACTIVITY_SCHEMA)
            .fetch_one(&mut *conn)
            .await?;
        if !compatible {
            return Err(sqlx::Error::Protocol(
                "admitted activity schema incompatible".into(),
            ));
        }
        let unsupported: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE confirmed_plan_id IS NOT NULL AND (source_output_revision IS NULL OR admission_plan_id IS NULL))")
            .fetch_one(&mut *conn)
            .await?;
        if unsupported {
            return Err(sqlx::Error::Protocol(
                "admitted activity artifact incompatible".into(),
            ));
        }
    }
    Ok(())
}

/// Called before listening, so a later filesystem replacement cannot relabel
/// the executable already running in this process.
pub async fn artifact_fingerprint() -> Result<String, sqlx::Error> {
    static ARTIFACT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(hash) = ARTIFACT.get() {
        return Ok(hash.clone());
    }
    let hash = tokio::task::spawn_blocking(|| {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let path = std::env::current_exe()
            .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
        let mut file = std::fs::File::open(path)
            .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok::<_, sqlx::Error>(
            hash.finalize()
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
        )
    })
    .await
    .map_err(|_| sqlx::Error::Protocol("artifact unavailable".into()))??;
    let _ = ARTIFACT.set(hash.clone());
    Ok(hash)
}

#[cfg(test)]
mod history_capture_readiness_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn people_refresh_cannot_borrow_other_readiness_or_artifact_identity() {
        let mut report = json!({
            "core_change_confirmation_ready": true,
            "people_refresh_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":[PEOPLE_REFRESH_CAPABILITY]}]
        });
        assert!(people_refresh_report_ready(&report, "verified"));
        assert!(!people_refresh_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!([CORE_CHANGE_CAPABILITY])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!people_refresh_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("people_refresh_confirmation_ready");
        assert!(!people_refresh_report_ready(&report, "verified"));
    }

    #[test]
    fn people_admission_cannot_borrow_other_readiness_or_artifact_identity() {
        let mut report = json!({
            "people_refresh_confirmation_ready": true,
            "people_admission_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":[PEOPLE_ADMISSION_CAPABILITY]}]
        });
        assert!(people_admission_report_ready(&report, "verified"));
        assert!(!people_admission_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!([PEOPLE_REFRESH_CAPABILITY])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!people_admission_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("people_admission_confirmation_ready");
        assert!(!people_admission_report_ready(&report, "verified"));
    }

    #[test]
    fn admitted_people_refresh_cannot_borrow_other_readiness_or_artifact_identity() {
        let mut report = json!({
            "people_refresh_confirmation_ready": true,
            "admitted_people_refresh_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":[ADMITTED_PEOPLE_REFRESH_CAPABILITY]}]
        });
        assert!(admitted_people_refresh_report_ready(&report, "verified"));
        assert!(!admitted_people_refresh_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!([PEOPLE_REFRESH_CAPABILITY])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!admitted_people_refresh_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("admitted_people_refresh_confirmation_ready");
        assert!(!admitted_people_refresh_report_ready(&report, "verified"));
    }

    #[test]
    fn admitted_metadata_cannot_borrow_other_readiness_or_artifact_identity() {
        let mut report = json!({
            "people_refresh_confirmation_ready": true,
            "admitted_metadata_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":[ADMITTED_METADATA_CAPABILITY]}]
        });
        assert!(admitted_metadata_report_ready(&report, "verified"));
        assert!(!admitted_metadata_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!([PEOPLE_REFRESH_CAPABILITY])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!admitted_metadata_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("admitted_metadata_confirmation_ready");
        assert!(!admitted_metadata_report_ready(&report, "verified"));
    }

    #[test]
    fn core_change_cannot_borrow_other_readiness_or_artifact_identity() {
        let mut report = json!({
            "history_timeline_confirmation_ready": true,
            "core_change_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":[CORE_CHANGE_CAPABILITY]}]
        });
        assert!(core_change_report_ready(&report, "verified"));
        assert!(!core_change_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!([HISTORY_TIMELINE_CAPABILITY])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!core_change_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("core_change_confirmation_ready");
        assert!(!core_change_report_ready(&report, "verified"));
    }

    #[test]
    fn timeline_cannot_borrow_capture_readiness_or_another_artifact_role() {
        let mut report = json!({
            "history_capture_confirmation_ready":true,
            "history_timeline_confirmation_ready":true,
            "candidates":[{"sha256":"verified","gate_version":GATE_VERSION,
                "role":"api","capabilities":[HISTORY_TIMELINE_CAPABILITY]}]
        });
        assert!(history_timeline_report_ready(&report, "verified"));
        assert!(!history_capture_report_ready(&report, "verified"));
        assert!(!history_timeline_report_ready(&report, "other"));
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!(["fub-history-capture-v1"])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!history_timeline_report_ready(&changed, "verified"));
        }
        report
            .as_object_mut()
            .unwrap()
            .remove("history_timeline_confirmation_ready");
        assert!(!history_timeline_report_ready(&report, "verified"));
    }

    #[test]
    fn capability_requires_matching_artifact_role_and_explicit_history_readiness() {
        let report = json!({
            "history_capture_confirmation_ready": true,
            "candidates": [{"sha256":"verified", "gate_version":GATE_VERSION,
                "role":"api", "capabilities":["fub-history-capture-v1"]}]
        });
        assert!(history_capture_report_ready(&report, "verified"));
        assert!(!history_capture_report_ready(&report, "other"));
        for field in ["history_capture_confirmation_ready", "candidates"] {
            let mut changed = report.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(!history_capture_report_ready(&changed, "verified"));
        }
        for (field, value) in [
            ("role", json!("cli")),
            ("gate_version", json!("pre-010c")),
            ("capabilities", json!(["fub-activity-import-v1"])),
        ] {
            let mut changed = report.clone();
            changed["candidates"][0][field] = value;
            assert!(!history_capture_report_ready(&changed, "verified"));
        }
    }

    #[test]
    fn history_readiness_expires_and_rejects_future_evidence() {
        let mut ready = ReleaseReadiness {
            database: "synthetic".into(),
            checked_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            synthetic: false,
            metadata: true,
            activity: true,
            history_capture: true,
            history_timeline: true,
            core_change: true,
            people_refresh: true,
            people_admission: true,
            admitted_people_refresh: true,
            admitted_metadata: true,
            admitted_activity: true,
            admitted_history: true,
            mapping_repair: true,
            people_recovery: true,
        };
        assert!(ready.admitted_activity_ready());
        assert!(ready.history_capture_ready());
        assert!(ready.history_timeline_ready());
        assert!(ready.core_change_ready());
        assert!(ready.people_refresh_ready());
        assert!(ready.people_admission_ready());
        assert!(ready.admitted_people_refresh_ready());
        ready.history_capture = false;
        assert!(!ready.history_capture_ready());
        ready.history_capture = true;
        ready.expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(!ready.history_capture_ready());
        assert!(!ready.history_timeline_ready());
        assert!(!ready.core_change_ready());
        assert!(!ready.people_refresh_ready());
        assert!(!ready.people_admission_ready());
        assert!(!ready.admitted_people_refresh_ready());
        ready.expires_at = Utc::now() + chrono::Duration::minutes(5);
        ready.checked_at = Utc::now() + chrono::Duration::seconds(60);
        assert!(!ready.history_capture_ready());
        assert!(!ready.history_timeline_ready());
        assert!(!ready.core_change_ready());
        assert!(!ready.people_refresh_ready());
        assert!(!ready.people_admission_ready());
        assert!(!ready.admitted_people_refresh_ready());
        ready.checked_at = Utc::now() - chrono::Duration::minutes(6);
        assert!(!ready.history_capture_ready());
        assert!(!ready.history_timeline_ready());
        assert!(!ready.core_change_ready());
        assert!(!ready.people_refresh_ready());
        assert!(!ready.people_admission_ready());
        assert!(!ready.admitted_people_refresh_ready());
    }
}
