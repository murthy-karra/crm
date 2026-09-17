//! Latest frozen follow-on attempt, never a claim of complete source migration.
use super::*;
pub(crate) async fn coverage(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    run: &PgRow,
) -> Result<Value, MigrationError> {
    let id = run.get::<Uuid, _>("id");
    let org = ctx.organization_id.0;
    let eligible=matches!(run.get::<String,_>("state").as_str(),"completed"|"cancelled") && sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_people_admission_result WHERE admission_id=$1 AND organization_id=$2 AND disposition='settled')").bind(id).bind(org).fetch_one(&mut *conn).await?;
    // Each indexed lookup visits one latest cohort-owned root. Counts are durable
    // plan/result summaries; no browser-supplied Person list authorizes a family.
    let queries=[
      ("core","Later core refresh","SELECT r.id,r.state,COALESCE(p.held_count,0)+CASE WHEN EXISTS(SELECT 1 FROM migration_admitted_people_refresh_result x WHERE x.refresh_id=r.id AND x.organization_id=r.organization_id AND x.disposition LIKE 'held%') THEN 1 ELSE 0 END held,COALESCE(p.excluded_count,0) excluded,false remainder FROM migration_admitted_people_refresh r LEFT JOIN migration_admitted_people_refresh_plan p ON p.id=r.confirmed_refresh_plan_id AND p.organization_id=r.organization_id WHERE r.admission_id=$1 AND r.organization_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT 1"),
      ("metadata","Tags and custom fields","SELECT id,state,COALESCE((counts->>'held_count')::bigint,0)+held_settled_people held,COALESCE((counts->'people'->>'excluded')::bigint,0) excluded,predecessor_import_id IS NOT NULL remainder FROM migration_admitted_metadata_import WHERE admission_id=$1 AND organization_id=$2 ORDER BY created_at DESC,id DESC LIMIT 1"),
      ("activity","Notes and tasks","SELECT id,state,COALESCE((counts->>'held_count')::bigint,0) held,COALESCE((counts->>'excluded_count')::bigint,0)+COALESCE((counts->>'source_only_count')::bigint,0) excluded,predecessor_import_id IS NOT NULL remainder FROM migration_admitted_activity_import WHERE admission_id=$1 AND organization_id=$2 ORDER BY created_at DESC,id DESC LIMIT 1"),
      ("history","Historical events, calls and texts","SELECT r.id,r.state,COALESCE((r.result_counts->>'unique_held')::bigint,p.held,0) held,COALESCE(p.excluded,0) excluded,false remainder FROM migration_admitted_history_root r LEFT JOIN migration_admitted_history_plan p ON p.id=r.latest_plan_id AND p.organization_id=r.organization_id WHERE r.admission_id=$1 AND r.organization_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT 1")
    ];
    let mut items = Vec::new();
    for (family, label, sql) in queries {
        let row = sqlx::query(sql)
            .bind(id)
            .bind(org)
            .fetch_optional(&mut *conn)
            .await?;
        let (root, state, status) = if let Some(r) = row {
            let state = r.get::<String, _>("state");
            let status = if r.get::<i64, _>("held") > 0 || state == "paused" {
                "held"
            } else if state == "completed"
                && r.get::<i64, _>("excluded") == 0
                && !r.get::<bool, _>("remainder")
            {
                "completed"
            } else {
                "partial"
            };
            (Some(r.get::<Uuid, _>("id")), Some(state), status)
        } else {
            (None, None, "not_started")
        };
        items.push(json!({"family":family,"label":label,"status":status,"root_id":root,"run_state":state,"can_review":eligible,"admission_id":id,"parent_import_id":run.get::<Uuid,_>("parent_import_id"),"scope":"latest_retained_attempt","prerequisite":if family=="history"{"A completed history capture starting after this recovery core capture; separate preview and confirmation."}else if family=="core"{"A later qualified core report; separate preview and confirmation."}else{"A qualified retained report; separate preview and confirmation."}}));
    }
    Ok(json!(items))
}
