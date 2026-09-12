"""Read-only recomputation of all010f2 logical stores and native result costs."""
from pathlib import Path
import json,os,subprocess,uuid
QA=Path('/private/tmp/crm-010f2-qa-694szdwe')
ctx=json.loads((QA/'people-context.json').read_text())
def query(sql):
 env=os.environ.copy();env.update(PGHOST='127.0.0.1',PGPORT='55432',PGDATABASE='crm_010f2_qa',PGUSER='crm_migrator')
 r=subprocess.run([str(QA/'bin/psql'),'-X','-q','-t','-A','-v','ON_ERROR_STOP=1','-c',sql],env=env,text=True,capture_output=True)
 assert r.returncode==0,r.stderr
 return json.loads(r.stdout)
tables=['migration_activity_'+n for n in ['import','plan','choice','source','mapping','manifest','manifest_issue','identity','result','result_issue','receipt','issue']]
report={'fixture':'synthetic010f2','scope':'Three disposable Organizations. Logical retention policy costs are separate from physical PostgreSQL relation sizes; these are not disk capacity estimates.','cases':[]}
for org in ctx['organizations']:
 oid="'"+str(uuid.UUID(org['id']))+"'::uuid"
 inventory=[]
 for table in tables:
  row=query(f"SELECT jsonb_build_object('table','{table}','rows',count(*),'logical_bytes',coalesce(sum(crm_activity_retained_size('{table}',to_jsonb(t))),0)) FROM {table} t WHERE organization_id={oid}")
  inventory.append(row)
 ledger=query(f"SELECT jsonb_build_object('state',i.state,'retained',i.retained_bytes,'measured',i.measured_bytes,'reserved',i.reserved_bytes,'native',i.native_bytes,'snapshot_retained',s.retained_bytes,'snapshot_reserved',s.reserved_bytes,'organization_retained',o.retained_bytes,'organization_reserved',o.reserved_bytes,'result_native_sum',(SELECT coalesce(sum(native_bytes),0) FROM migration_activity_result WHERE organization_id={oid}),'reservations',(SELECT count(*) FROM migration_activity_reservation WHERE organization_id={oid})) FROM migration_activity_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id JOIN migration_snapshot_storage o ON o.organization_id=i.organization_id WHERE i.organization_id={oid}")
 assert sum(r['logical_bytes'] for r in inventory)==ledger['measured']==ledger['retained']
 assert ledger['native']==ledger['result_native_sum']
 assert ledger['reserved']==ledger['reservations']==ledger['snapshot_reserved']==ledger['organization_reserved']==0
 assert ledger['organization_retained']==ledger['snapshot_retained']>=ledger['retained']
 assert ledger['state'] in ['completed','cancelled']
 report['cases'].append({'case':org['case'],'inventory':inventory,'ledger':ledger})
report['physical_relations']=query("SELECT jsonb_agg(jsonb_build_object('relation',relname,'heap_bytes',pg_table_size(oid),'index_bytes',pg_indexes_size(oid),'total_bytes',pg_total_relation_size(oid)) ORDER BY relname) FROM pg_class WHERE relnamespace='public'::regnamespace AND relkind='r' AND (relname LIKE 'migration_activity_%' OR relname IN ('note','task'))")
p=QA/'storage-reconciliation.json';p.write_text(json.dumps(report,indent=2)+'\n');p.chmod(0o600)
print('All12 durable activity stores recomputed; child/snapshot/Organization reservations and native result costs reconcile.')
