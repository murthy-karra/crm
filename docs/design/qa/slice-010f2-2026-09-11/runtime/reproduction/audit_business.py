"""Read-only checksums of unrelated tenant business state around activity QA."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,uuid
QA=Path('/private/tmp/crm-010f2-qa-694szdwe')
def query(sql):
    env=os.environ.copy();env.update(PGHOST='127.0.0.1',PGPORT='55432',PGDATABASE='crm_010f2_qa',PGUSER='crm_migrator')
    r=subprocess.run([str(QA/'bin/psql'),'-X','-q','-t','-A','-v','ON_ERROR_STOP=1','-c',sql],env=env,text=True,capture_output=True)
    assert r.returncode==0,r.stderr
    return json.loads(r.stdout)
# Deliberate note/task inserts and authority/session changes are checked by their
# own exact audits. These tables must retain identical rows, not merely counts.
names=['person','contact_method','stage','inquiry','assignment_changed','stage_changed','person_imported','contact_attempted','call','call_completed','operator_turn','operator_tool_call','operator_proposal','operator_task_proposal','today_work_source','today_system_feed','today_feed_changed','saved_list','tag','person_tag','custom_field','custom_field_option','person_custom_field_value','correspondence_raw','correspondence_captured','capture_message','intake_delivery']
scoped=query("SELECT coalesce(jsonb_agg(table_name ORDER BY table_name),'[]'::jsonb) FROM information_schema.columns WHERE table_schema='public' AND column_name='organization_id' AND table_name IN ("+','.join("'"+n+"'" for n in names)+")")
assert len(scoped)>=20
state=json.loads((QA/'people-context.json').read_text())
result={}
for o in state['organizations']:
    oid="'"+str(uuid.UUID(o['id']))+"'::uuid"
    clauses=["SELECT '"+n+"' AS name,count(*) AS count,md5(coalesce(string_agg(md5(to_jsonb(t)::text),'' ORDER BY md5(to_jsonb(t)::text)),'')) AS rows_hash FROM "+n+" t WHERE organization_id="+oid for n in scoped]
    result[o['case']]=query("SELECT jsonb_object_agg(name,jsonb_build_object('rows',count,'rows_hash',rows_hash)) FROM ("+' UNION ALL '.join(clauses)+") inventory")
path=QA/'business-baseline.json'
if '--baseline' in sys.argv:
    assert not path.exists()
    child_count=query('SELECT to_jsonb(count(*)) FROM migration_activity_import')
    assert child_count==0,'Baseline must precede every activity child'
    path.write_text(json.dumps(result,indent=2)+'\n');path.chmod(0o600)
    print('Baseline captured for '+str(len(scoped))+' unrelated business tables per Organization.')
else:
    assert result==json.loads(path.read_text()),'Unrelated business rows changed'
    out=QA/'business-reconciliation.json';out.write_text(json.dumps({'unchanged':True,'tables_per_organization':len(scoped),'cases':result},indent=2)+'\n');out.chmod(0o600)
    print('All unrelated business-table rows remain unchanged.')
