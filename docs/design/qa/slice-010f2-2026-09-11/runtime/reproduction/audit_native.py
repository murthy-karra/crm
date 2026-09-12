"""Read-only exact synthetic native reconciliation; no credentials in output."""
from pathlib import Path
import datetime, hashlib, json, os, subprocess, sys, uuid
QA=Path('/private/tmp/crm-010f2-qa-694szdwe')
CTX=QA/'people-context.json'
state=json.loads(CTX.read_text())
def digest(value):
    return hashlib.sha256(json.dumps(value,ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def query(sql):
    env=os.environ.copy()
    env.update(PGHOST='127.0.0.1',PGPORT='55432',PGDATABASE='crm_010f2_qa',PGUSER='crm_migrator')
    r=subprocess.run([str(QA/'bin/psql'),'-X','-q','-t','-A','-v','ON_ERROR_STOP=1','-c',sql],env=env,text=True,capture_output=True)
    if r.returncode: raise RuntimeError('Scoped read-only audit query failed: '+r.stderr)
    return json.loads(r.stdout)
def qid(raw): return "'"+str(uuid.UUID(raw))+"'::uuid"
def invariant(org):
    oid,pid=qid(org['id']),qid(org['parent_id'])
    return query(f"SELECT jsonb_build_object('parent',(SELECT to_jsonb(p) FROM migration_import p WHERE id={pid} AND organization_id={oid}),'workspace',(SELECT to_jsonb(w) FROM migration_workspace w WHERE organization_id={oid}),'parent_results',(SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY id),'[]'::jsonb) FROM migration_import_result r WHERE import_id={pid} AND organization_id={oid}),'parent_identities',(SELECT coalesce(jsonb_agg(to_jsonb(i) ORDER BY family,source_id),'[]'::jsonb) FROM migration_import_identity i WHERE import_id={pid} AND organization_id={oid}),'metadata_imports',(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY id),'[]'::jsonb) FROM migration_metadata_import m WHERE organization_id={oid}),'metadata_identities',(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY kind,source_key),'[]'::jsonb) FROM migration_metadata_identity m WHERE organization_id={oid}),'metadata_results',(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY id),'[]'::jsonb) FROM migration_metadata_result m WHERE organization_id={oid}))")
BASELINE=QA/'native-baseline.json'
if '--baseline' in sys.argv:
    assert not BASELINE.exists(),'Do not overwrite pre-activity baseline'
    result={'cases':{o['case']:digest(invariant(o)) for o in state['organizations']},'source':json.loads((QA/'control.stats.json').read_text()),'delivery':json.loads((QA/'control.delivery-stats.json').read_text()),'epochs':[]}
    BASELINE.write_text(json.dumps(result,indent=2)+'\n');BASELINE.chmod(0o600)
    print('Read-only parent/workspace/sibling/source baseline captured.')
    sys.exit(0)
baseline=json.loads(BASELINE.read_text())
if '--close-epoch' in sys.argv:
    after=json.loads((QA/'control.stats.json').read_text()); delivery=json.loads((QA/'control.delivery-stats.json').read_text())
    assert after['source_reader_calls']==baseline['source']['source_reader_calls']
    assert delivery['publications']==baseline['delivery']['publications']
    baseline.setdefault('epochs',[]).append({'source_before':baseline['source'],'source_after':after,'delivery_before':baseline['delivery'],'delivery_after':delivery})
    baseline['awaiting_restart']=True
    BASELINE.write_text(json.dumps(baseline,indent=2)+'\n')
    print('Closed API counter epoch with zero source calls and publications.')
    sys.exit(0)
if '--start-epoch' in sys.argv:
    assert baseline.pop('awaiting_restart',False)
    baseline['source']=json.loads((QA/'control.stats.json').read_text())
    baseline['delivery']=json.loads((QA/'control.delivery-stats.json').read_text())
    assert baseline['source']['source_reader_calls']==0 and baseline['delivery']['publications']==0
    BASELINE.write_text(json.dumps(baseline,indent=2)+'\n')
    print('New API counter epoch started at zero.')
    sys.exit(0)
assert not baseline.get('awaiting_restart')
report={'fixture':'synthetic010f2','cases':[],'previous_counter_epochs':baseline.get('epochs',[])}
def utc(s): return datetime.datetime.fromisoformat(s.replace('Z','+00:00')).astimezone(datetime.timezone.utc)
for org in state['organizations']:
    oid=qid(org['id'])
    assert digest(invariant(org))==baseline['cases'][org['case']],org['case']+' parent/workspace/sibling changed'
    rows=query(f"SELECT jsonb_build_object('child',(SELECT to_jsonb(i) FROM migration_activity_import i WHERE organization_id={oid}),'notes',(SELECT coalesce(jsonb_agg(to_jsonb(n) ORDER BY source_external_id),'[]'::jsonb) FROM note n WHERE organization_id={oid}),'tasks',(SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY source_external_id),'[]'::jsonb) FROM task t WHERE organization_id={oid}),'results',(SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'kind',kind,'source_id',source_id,'target_id',target_id,'disposition',disposition) ORDER BY kind,source_id),'[]'::jsonb) FROM migration_activity_result WHERE organization_id={oid}),'identities',(SELECT count(*) FROM migration_activity_identity WHERE organization_id={oid}),'issues',(SELECT count(*) FROM migration_activity_result_issue WHERE organization_id={oid}))")
    child=rows['child']; assert child
    assert child['retained_bytes']==child['measured_bytes']
    assert child['reserved_bytes']==0
    assert rows['identities']==len(rows['notes'])+len(rows['tasks'])
    by_result={(r['kind'],r['source_id']):r for r in rows['results']}
    assert len(by_result)==len(rows['results']), 'duplicate source result'
    if org['case']!='cancel':
        assert len(rows['results'])==70
        assert sum(r['disposition']=='held' for r in rows['results'])==9
        assert sum(r['disposition']=='applied' for r in rows['results'])==61
        assert {r['source_id'] for r in rows['results'] if r['kind']=='note'}==({str(i) for i in range(301,309)}|{str(i) for i in range(1000,1050)})
        assert {r['source_id'] for r in rows['results'] if r['kind']=='task'}=={str(i) for i in range(501,513)}
    else:
        assert 0<len(rows['results'])<70
    safe=[]
    for kind in ['note','task']:
        for row in rows[kind+'s']:
            assert row['origin']=='migration' and row['source']=='fub'
            assert row['deleted_at'] is None
            source=row['source_external_id'].split(':')
            assert source[:2]==['v1',str(org['parent_baseline']['source_account_id'])]
            result=by_result[(kind,source[2])]
            assert result['disposition']=='applied' and result['target_id']==row['id']
            safe.append({'kind':kind,'source_id':source[2],'target':row['id'],'person':row['person_id'],'text_sha256':hashlib.sha256(row.get('body',row.get('title')).encode()).hexdigest(),'text_codepoints':len(row.get('body',row.get('title'))),'created_at':row['created_at'],'updated_at':row['updated_at'],'author':row.get('author_user_id'),'creator':row.get('created_by_user_id'),'assignee':row.get('assignee_user_id'),'completed_by':row.get('completed_by_user_id'),'due_at':row.get('due_at'),'completed_at':row.get('completed_at')})
    assert child['state']==('cancelled' if org['case']=='cancel' else 'completed')
    notes={r['source_external_id'].split(':')[2]:r for r in rows['notes']}
    tasks={r['source_external_id'].split(':')[2]:r for r in rows['tasks']}
    expected_notes={'301','303','307','308'}|{str(i) for i in range(1000,1050)}
    expected_tasks={'501','503','504','506','508','511','512'}
    assert set(notes)<=expected_notes and set(tasks)<=expected_tasks
    if org['case']!='cancel':
        assert set(notes)==expected_notes and set(tasks)==expected_tasks
    else:
        assert 0<len(notes)+len(tasks)<61 and child['confirmed_plan_id'] is not None
    for source,n in notes.items():
        assert n['person_id']==org['people']['102' if source in {'307','308'} else '101']
        assert utc(n['created_at'])==utc('2026-09-01T12:00:00.123456Z')
        assert utc(n['updated_at'])==utc(n['created_at'])
        assert n['author_user_id']==(org['former_id'] if source=='303' else None if source in {'307','308'} else org['admin_id'])
        assert 'SYNTHETIC_LIST_MUST_NOT_EXECUTE' not in n['body']
        assert 'SYNTHETIC_SOURCE_ONLY_REPLY' not in n['body']
        if int(source)>=1000: assert n['body']=='🏡'*10000
    if '301' in notes: assert notes['301']['body']=='Subject: Readable HTML with replies\n\nSYNTHETIC_ACTIVITY_BODY_SENTINEL Hello José 🏡.\n• First item\n• Second item\nSource link (https://synthetic.invalid/never-fetch)'
    if '303' in notes: assert notes['303']['body']=='Subject: Plain subject\n\nExact plain text\nSecond line — José 🏡'
    if '307' in notes: assert notes['307']['body']=='Subject: Unknown historical source author\n\nKeep the source author evidence and explicitly choose mapping or unmapped.'
    if '308' in notes: assert notes['308']['body']=='Subject: Missing historical author\n\nPreserve the absent source actor explicitly.'
    titles={'501':'Spring DST date-only task','503':'Completed task with exact timestamp','504':'Fall DST date-only task','506':'Deliberately undated task','508':'Explicit unknown task kind','511':'Editor is not completion actor','512':'Unassigned with historical creator'}
    expected={
        '501':('call','2026-03-09T06:59:59Z',None),
        '503':('email','2026-09-01T17:00:00Z','2026-09-01T16:00:00.123456Z'),
        '504':('call','2026-11-02T07:59:59Z',None),
        '506':('follow_up',None,None),'508':('other',None,None),
        '511':('text',None,'2026-09-02T16:00:00Z'),'512':('text',None,None)}
    for source,t in tasks.items():
        kind,due,completed=expected[source]
        assert t['title']==titles[source]
        assert t['kind']==kind
        assert (utc(t['due_at']) if t['due_at'] else None)==(utc(due) if due else None)
        assert (utc(t['completed_at']) if t['completed_at'] else None)==(utc(completed) if completed else None)
        assert t['completed_by_user_id'] is None
        assert t['created_by_user_id']==(org['former_id'] if source in {'511','512'} else org['admin_id'])
        assert t['assignee_user_id']==(None if source=='512' else org['admin_id'])
        assert t['person_id']==org['people']['102' if source in {'508','511','512'} else '101']
        assert utc(t['created_at'])==utc('2026-03-01T12:00:00Z')
        assert utc(t['updated_at'])==utc('2026-09-01T16:00:00.123456Z' if source=='503' else '2026-03-01T12:00:00Z')
    report['cases'].append({'case':org['case'],'state':child['state'],'counts':child['counts'],'native_note_count':len(rows['notes']),'native_task_count':len(rows['tasks']),'native_rows':safe,'retained_bytes':child['retained_bytes'],'native_row_bytes':child['native_bytes'],'committed_issue_count':rows['issues'],'parent_workspace_sibling_unchanged':True})
report['source_after']=json.loads((QA/'control.stats.json').read_text())
report['source_before']=baseline['source']
assert report['source_after']['source_reader_calls']==baseline['source']['source_reader_calls']
report['delivery_after']=json.loads((QA/'control.delivery-stats.json').read_text())
report['delivery_before']=baseline['delivery']
# Membership demotion intentionally disconnects sessions; migration must not publish.
assert report['delivery_after']['publications']==baseline['delivery']['publications']
out=QA/'native-reconciliation.json';out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');out.chmod(0o600)
print('All three synthetic activity outcomes reconciled exactly; source calls and publications unchanged.')
