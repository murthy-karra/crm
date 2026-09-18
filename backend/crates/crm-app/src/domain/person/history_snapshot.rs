use serde_json::Value;
use sqlx::PgConnection;

use crate::ids::{OrganizationId, PersonId, UserId};

const HISTORY_HEAD: &str = r#"
WITH history(kind,kind_rank,id,occurred_at,recorded_at,actor,origin,correlation_id,detail) AS (
 SELECT 'person_imported',0,pi.id,pi.occurred_at,pi.recorded_at,NULL::jsonb,pi.origin,pi.correlation_id,
        jsonb_build_object('import_id',pi.import_id,'plan_id',pi.plan_id,'source_record_id',pi.source_record_id,'capture_id',pi.capture_id,'on_behalf_of_user_id',pi.on_behalf_of_user_id)
 FROM person_imported pi WHERE pi.organization_id=$1 AND pi.person_id=$2
 UNION ALL
 SELECT 'inquiry_received',0,ir.id,ir.occurred_at,ir.recorded_at,
        CASE WHEN au.id IS NULL THEN NULL ELSE jsonb_build_object('id',au.id,'display_name',au.display_name) END,
        ir.origin,ir.correlation_id,jsonb_build_object('inquiry_id',ir.inquiry_id,'source',ir.source,'person_created',ir.person_created,'matched_by',ir.matched_by)
 FROM inquiry_received ir LEFT JOIN app_user au ON au.id=ir.actor_user_id WHERE ir.organization_id=$1 AND ir.person_id=$2
 UNION ALL
 SELECT 'routing_decision',1,rd.id,rd.occurred_at,rd.recorded_at,
        CASE WHEN actor.id IS NULL THEN NULL ELSE jsonb_build_object('id',actor.id,'display_name',actor.display_name) END,
        rd.origin,rd.correlation_id,jsonb_build_object('inquiry_id',rd.inquiry_id,'strategy',rd.strategy,'assignee',CASE WHEN assignee.id IS NULL THEN NULL ELSE jsonb_build_object('id',assignee.id,'display_name',assignee.display_name) END)
 FROM routing_decision rd LEFT JOIN app_user actor ON actor.id=rd.actor_user_id LEFT JOIN app_user assignee ON assignee.id=rd.assignee_user_id WHERE rd.organization_id=$1 AND rd.person_id=$2
 UNION ALL
 SELECT 'assignment_changed',2,ac.id,ac.occurred_at,ac.recorded_at,
        CASE WHEN actor.id IS NULL THEN NULL ELSE jsonb_build_object('id',actor.id,'display_name',actor.display_name) END,
        ac.origin,ac.correlation_id,jsonb_build_object('from',CASE WHEN fu.id IS NULL THEN NULL ELSE jsonb_build_object('id',fu.id,'display_name',fu.display_name) END,'to',CASE WHEN tu.id IS NULL THEN NULL ELSE jsonb_build_object('id',tu.id,'display_name',tu.display_name) END,'reason',ac.reason)
 FROM assignment_changed ac LEFT JOIN app_user actor ON actor.id=ac.actor_user_id LEFT JOIN app_user fu ON fu.id=ac.from_user_id LEFT JOIN app_user tu ON tu.id=ac.to_user_id WHERE ac.organization_id=$1 AND ac.person_id=$2
 UNION ALL
 SELECT 'stage_changed',3,sc.id,sc.occurred_at,sc.recorded_at,
        CASE WHEN actor.id IS NULL THEN NULL ELSE jsonb_build_object('id',actor.id,'display_name',actor.display_name) END,
        sc.origin,sc.correlation_id,jsonb_build_object('from_stage',CASE WHEN fs.id IS NULL THEN NULL ELSE jsonb_build_object('id',fs.id,'name',fs.name) END,'to_stage',jsonb_build_object('id',ts.id,'name',ts.name),'reason',sc.reason)
 FROM stage_changed sc LEFT JOIN app_user actor ON actor.id=sc.actor_user_id LEFT JOIN stage fs ON fs.id=sc.from_stage_id JOIN stage ts ON ts.id=sc.to_stage_id WHERE sc.organization_id=$1 AND sc.person_id=$2
 UNION ALL
 SELECT 'contact_attempted',4,ca.id,CASE WHEN ca.corrects_id IS NULL THEN ca.occurred_at ELSE ca.recorded_at END,ca.recorded_at,
        CASE WHEN actor.id IS NULL THEN NULL ELSE jsonb_build_object('id',actor.id,'display_name',actor.display_name) END,
        ca.origin,ca.correlation_id,jsonb_build_object('channel',ca.channel,'outcome',ca.outcome,'call_id',cl.id,'corrects_id',ca.corrects_id,'superseded',EXISTS(SELECT 1 FROM contact_attempted c WHERE c.corrects_id=ca.id))
 FROM contact_attempted ca LEFT JOIN app_user actor ON actor.id=ca.actor_user_id LEFT JOIN call cl ON cl.id=ca.causation_id AND cl.organization_id=ca.organization_id WHERE ca.organization_id=$1 AND ca.person_id=$2
 UNION ALL
 SELECT 'call_completed',5,cc.id,cc.occurred_at,cc.recorded_at,
        CASE WHEN actor.id IS NULL THEN NULL ELSE jsonb_build_object('id',actor.id,'display_name',actor.display_name) END,
        cc.origin,cc.correlation_id,jsonb_build_object('call_id',cc.call_id,'outcome',cc.outcome,'talk_seconds',cc.talk_seconds,'answered_at',cc.answered_at)
 FROM call_completed cc LEFT JOIN app_user actor ON actor.id=cc.actor_user_id WHERE cc.organization_id=$1 AND cc.person_id=$2
 UNION ALL
 SELECT 'correspondence',6,cc.id,cc.occurred_at,cc.recorded_at,NULL::jsonb,cc.origin,cc.correlation_id,
        jsonb_build_object('direction',cc.direction,'agent',jsonb_build_object('id',agent.id,'display_name',agent.display_name),'captured_at',cc.recorded_at,'via',cc.via,'backdated',cc.backdated)
 FROM correspondence_captured cc LEFT JOIN app_user agent ON agent.id=cc.on_behalf_of_user_id WHERE cc.organization_id=$1 AND cc.person_id=$2
 UNION ALL
 SELECT 'note',7,n.id,n.created_at,n.created_at,
        CASE WHEN author.id IS NULL THEN NULL ELSE jsonb_build_object('id',author.id,'display_name',author.display_name) END,
        n.origin,n.correlation_id,jsonb_build_object('body',n.body,'updated_at',n.updated_at,'edited',n.updated_at>n.created_at,'can_manage',($4 OR n.author_user_id=$3))
 FROM note n LEFT JOIN app_user author ON author.id=n.author_user_id WHERE n.organization_id=$1 AND n.person_id=$2 AND n.deleted_at IS NULL
 UNION ALL
 SELECT 'task_completed',8,t.id,t.completed_at,t.completed_at,
        CASE WHEN completed.id IS NULL THEN NULL ELSE jsonb_build_object('id',completed.id,'display_name',completed.display_name) END,
        t.origin,t.correlation_id,jsonb_build_object('title',t.title,'kind',t.kind,'due_at',t.due_at,'assignee',CASE WHEN assignee.id IS NULL THEN NULL ELSE jsonb_build_object('id',assignee.id,'display_name',assignee.display_name) END,'created_by',CASE WHEN creator.id IS NULL THEN NULL ELSE jsonb_build_object('id',creator.id,'display_name',creator.display_name) END,'can_manage',($4 OR t.assignee_user_id=$3 OR t.created_by_user_id=$3))
 FROM task t LEFT JOIN app_user completed ON completed.id=t.completed_by_user_id LEFT JOIN app_user assignee ON assignee.id=t.assignee_user_id LEFT JOIN app_user creator ON creator.id=t.created_by_user_id WHERE t.organization_id=$1 AND t.person_id=$2 AND t.completed_at IS NOT NULL AND t.deleted_at IS NULL
"#;

const ADMITTED: &str = r#"
 UNION ALL SELECT 'person_admitted',12,pa.id,pa.occurred_at,pa.recorded_at,NULL::jsonb,pa.origin,pa.correlation_id,
 jsonb_build_object('admission_id',pa.admission_id,'plan_id',pa.plan_id,'item_id',pa.item_id,'result_id',pa.result_id,'on_behalf_of_user_id',pa.on_behalf_of_user_id)
 FROM person_admitted pa WHERE pa.organization_id=$1 AND pa.person_id=$2
"#;

const RECOVERED: &str = r#"
 UNION ALL SELECT 'person_recovered',13,pr.id,pr.occurred_at,pr.recorded_at,NULL::jsonb,pr.origin,pr.correlation_id,
 jsonb_build_object('admission_id',pr.admission_id,'plan_id',pr.plan_id,'item_id',pr.item_id,'result_id',pr.result_id,'on_behalf_of_user_id',pr.on_behalf_of_user_id)
 FROM person_recovered pr WHERE pr.organization_id=$1 AND pr.person_id=$2
"#;

const HISTORY_TAIL: &str = r#"
)
SELECT jsonb_build_object('kind',kind,'id',id,'occurred_at',occurred_at,'recorded_at',recorded_at,'actor',actor,'origin',origin,'correlation_id',correlation_id,'detail',detail)
FROM history ORDER BY occurred_at,recorded_at,kind_rank,id
"#;

pub async fn load(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    viewer_id: UserId,
    viewer_is_admin: bool,
) -> Result<Vec<Value>, sqlx::Error> {
    let mut statement = String::with_capacity(
        HISTORY_HEAD.len() + ADMITTED.len() + RECOVERED.len() + HISTORY_TAIL.len(),
    );
    statement.push_str(HISTORY_HEAD);
    statement.push_str(ADMITTED);
    statement.push_str(RECOVERED);
    statement.push_str(HISTORY_TAIL);

    let mut entries = sqlx::query_scalar::<_, Value>(&statement)
        .bind(organization_id.0)
        .bind(person_id.0)
        .bind(viewer_id.0)
        .bind(viewer_is_admin)
        .fetch_all(conn)
        .await?;
    for entry in &mut entries {
        super::normalize_json_timestamps(entry);
    }

    let admitted = entries
        .iter()
        .filter(|entry| entry["kind"] == "person_admitted")
        .count();
    let recovered = entries
        .iter()
        .filter(|entry| entry["kind"] == "person_recovered")
        .count();
    let malformed_correspondence = entries.iter().any(|entry| {
        entry["kind"] == "correspondence"
            && (entry["detail"]["agent"]["id"].as_str().is_none()
                || entry["detail"]["agent"]["display_name"].as_str().is_none())
    });
    if admitted > 1 || recovered > 1 || malformed_correspondence {
        return Err(sqlx::Error::Protocol(
            "inconsistent person history projection".into(),
        ));
    }
    Ok(entries)
}
