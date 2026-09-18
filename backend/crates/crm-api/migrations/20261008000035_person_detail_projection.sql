-- Current-state Person detail read model. Canonical domain tables remain the
-- source of truth; immutable history remains in its existing fact tables.

CREATE TABLE person_detail_projection (
    organization_id UUID NOT NULL,
    person_id UUID NOT NULL,
    snapshot JSONB NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    rebuilt_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (organization_id, person_id),
    FOREIGN KEY (person_id, organization_id)
        REFERENCES person (id, organization_id) ON DELETE CASCADE
);

-- Representation assembly belongs to the read model. Commands remain the only
-- business-mutation path and call this function before committing.
CREATE FUNCTION crm_rebuild_person_detail_projection(p_organization_id UUID, p_person_id UUID)
RETURNS VOID
LANGUAGE SQL
AS $$
INSERT INTO person_detail_projection (
    organization_id, person_id, snapshot, revision, rebuilt_at
)
SELECT p.organization_id, p.id,
       jsonb_build_object(
         'person', jsonb_build_object(
           'id',p.id,'first_name',p.first_name,'last_name',p.last_name,
           'display_name',COALESCE(NULLIF(concat_ws(' ',p.first_name,p.last_name),''),email.value,phone.value,''),
           'stage',jsonb_build_object('id',s.id,'name',s.name),
           'assigned_user',CASE WHEN u.id IS NULL THEN NULL ELSE jsonb_build_object('id',u.id,'display_name',u.display_name) END,
           'primary_email',email.value,'primary_phone',phone.value,
           'inquiry_count',(SELECT count(*) FROM inquiry i WHERE i.organization_id=p.organization_id AND i.person_id=p.id),
           'last_inquiry_at',(SELECT max(i.received_at) FROM inquiry i WHERE i.organization_id=p.organization_id AND i.person_id=p.id),
           'created_at',p.created_at),
         'contact_methods',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',cm.id,'kind',cm.kind,'value',cm.value) ORDER BY cm.import_order NULLS LAST,cm.created_at,cm.id) FROM contact_method cm WHERE cm.organization_id=p.organization_id AND cm.person_id=p.id),'[]'::jsonb),
         'inquiries',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',i.id,'source',i.source,'source_external_id',i.source_external_id,'message',i.message,'received_at',i.received_at) ORDER BY i.received_at DESC) FROM inquiry i WHERE i.organization_id=p.organization_id AND i.person_id=p.id),'[]'::jsonb),
         'tags',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',t.id,'name',t.name) ORDER BY lower(t.name),t.id) FROM person_tag pt JOIN tag t ON t.id=pt.tag_id AND t.organization_id=pt.organization_id WHERE pt.organization_id=p.organization_id AND pt.person_id=p.id),'[]'::jsonb),
         'custom_fields',COALESCE((SELECT jsonb_agg(jsonb_build_object('field_id',v.field_id,'label',cf.label,'field_type',cf.field_type,'value',CASE cf.field_type WHEN 'text' THEN jsonb_build_object('text',v.text_value) WHEN 'number' THEN jsonb_build_object('number',trim_scale(v.number_value)::text) WHEN 'date' THEN jsonb_build_object('date',v.date_value) WHEN 'choice' THEN jsonb_build_object('option_id',v.option_id) END,'option_label',o.label,'updated_at',v.updated_at) ORDER BY cf.position,cf.id) FROM person_custom_field_value v JOIN custom_field cf ON cf.id=v.field_id AND cf.organization_id=v.organization_id LEFT JOIN custom_field_option o ON o.id=v.option_id WHERE v.organization_id=p.organization_id AND v.person_id=p.id AND cf.archived_at IS NULL),'[]'::jsonb),
         'tasks',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',t.id,'person_id',t.person_id,'title',t.title,'kind',t.kind,'due_at',t.due_at,'assignee',CASE WHEN t.assignee_user_id IS NULL THEN NULL ELSE jsonb_build_object('id',t.assignee_user_id,'display_name',au.display_name) END,'created_by',CASE WHEN t.created_by_user_id IS NULL THEN NULL ELSE jsonb_build_object('id',t.created_by_user_id,'display_name',cu.display_name) END,'completed_at',t.completed_at,'completed_by',CASE WHEN t.completed_by_user_id IS NULL THEN NULL ELSE jsonb_build_object('id',t.completed_by_user_id,'display_name',ku.display_name) END,'created_at',t.created_at,'updated_at',t.updated_at) ORDER BY t.due_at NULLS LAST,t.created_at,t.id) FROM task t LEFT JOIN app_user au ON au.id=t.assignee_user_id LEFT JOIN app_user cu ON cu.id=t.created_by_user_id LEFT JOIN app_user ku ON ku.id=t.completed_by_user_id WHERE t.organization_id=p.organization_id AND t.person_id=p.id AND t.completed_at IS NULL AND t.deleted_at IS NULL),'[]'::jsonb)
       ),
       1,
       statement_timestamp()
FROM person p
JOIN stage s ON s.id=p.stage_id AND s.organization_id=p.organization_id
LEFT JOIN app_user u ON u.id=p.assigned_user_id
LEFT JOIN LATERAL (SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='email' ORDER BY import_order NULLS LAST,created_at,id LIMIT 1) email ON true
LEFT JOIN LATERAL (SELECT value FROM contact_method WHERE organization_id=p.organization_id AND person_id=p.id AND kind='phone' ORDER BY import_order NULLS LAST,created_at,id LIMIT 1) phone ON true
WHERE p.organization_id=p_organization_id AND p.id=p_person_id
ON CONFLICT (organization_id,person_id) DO UPDATE
SET snapshot=EXCLUDED.snapshot,
    revision=person_detail_projection.revision+1,
    rebuilt_at=EXCLUDED.rebuilt_at
$$;

GRANT SELECT, INSERT, UPDATE, DELETE ON person_detail_projection TO crm_app;
GRANT EXECUTE ON FUNCTION crm_rebuild_person_detail_projection(UUID, UUID) TO crm_app;

-- Development databases may already contain synthetic People when this
-- migration is applied. Production rollout compatibility is intentionally not
-- required, but making the migration self-contained keeps local state usable.
SELECT crm_rebuild_person_detail_projection(organization_id,id) FROM person;
