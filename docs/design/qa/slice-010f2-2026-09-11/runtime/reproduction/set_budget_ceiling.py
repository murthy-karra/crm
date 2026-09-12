"""Scoped deployment-headroom exercise for only the disposable QA API."""
from pathlib import Path
import datetime,json,os,shlex,subprocess,sys,uuid
QA=Path('/private/tmp/crm-010f2-qa-694szdwe')
mode=sys.argv[1];assert mode in ['lower','restore']
ctx=json.loads((QA/'people-context.json').read_text());org=next(o for o in ctx['organizations'] if o['case']=='budget')
qid=lambda v:"'"+str(uuid.UUID(v))+"'::uuid"
oid,sid=qid(org['id']),qid(org['snapshot_id'])
env=os.environ.copy();env.update(PGHOST='127.0.0.1',PGPORT='55432',PGDATABASE='crm_010f2_qa',PGUSER='crm_migrator')
sql=f"SELECT jsonb_build_object('snapshot_id',s.id,'retained',s.retained_bytes,'reserved',s.reserved_bytes,'run_byte_limit',s.run_byte_limit,'child_id',i.id,'child_state',i.state,'pause_reason',i.pause_reason,'native_bytes',i.native_bytes) FROM migration_snapshot s JOIN migration_activity_import i ON i.snapshot_id=s.id AND i.organization_id=s.organization_id WHERE s.id={sid} AND s.organization_id={oid}"
r=subprocess.run([str(QA/'bin/psql'),'-X','-q','-t','-A','-v','ON_ERROR_STOP=1','-c',sql],env=env,text=True,capture_output=True);assert r.returncode==0,r.stderr
v=json.loads(r.stdout);assert v['native_bytes']==0 and v['child_state']==('queued' if mode=='lower' else 'paused')
control=json.loads((QA/'control.json').read_text());units=json.loads((QA/'control.activity-stats.json').read_text())['completed_activity_units']
assert isinstance(control['activity_units'],int) and control['activity_units']<=units,'Worker must be frozen'
path=QA/'qa.env';lines=path.read_text().splitlines();key='CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES'
index=next(i for i,line in enumerate(lines) if line.startswith(key+'='));old=int(shlex.split(lines[index].split('=',1)[1])[0])
events_file=QA/'budget-deployment-events.json';events=json.loads(events_file.read_text()) if events_file.exists() else []
if mode=='lower':
 assert old==268435456 and not events
 new=v['retained']+v['reserved'];assert 0<new<old
else:
 assert events and events[-1]['mode']=='lower' and old==events[-1]['new_ceiling_bytes']
 new=536870912
subprocess.run([sys.executable,str(QA/'audit_native.py'),'--close-epoch'],check=True)
subprocess.run([sys.executable,str(QA/'manage_api.py'),'stop'],check=True)
lines[index]=key+'='+str(new);path.write_text('\n'.join(lines)+'\n');path.chmod(0o600)
events.append({'mode':mode,'time':datetime.datetime.now(datetime.timezone.utc).isoformat(),'old_ceiling_bytes':old,'new_ceiling_bytes':new,'snapshot_before':v,'cause':'Synthetic lowered deployment headroom, not exhaustion of the original retained-data allowance. No DB ledger or budget edited.'})
events_file.write_text(json.dumps(events,indent=2)+'\n');events_file.chmod(0o600)
print('Owned QA API stopped; '+mode+' deployment ceiling from '+str(old)+' to '+str(new)+' bytes. Ready for explicit restart.')
