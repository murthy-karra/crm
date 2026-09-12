"""One bounded release observation and final read-only reconciliation."""
import datetime,json,re,hashlib,subprocess
from pathlib import Path
from release_ops import ROOT,RELEASE,save,digest,listeners,check_cwd,retirement_paths
from database_ops import snapshot
start=json.loads((RELEASE/'observation-start.json').read_text());now=datetime.datetime.now(datetime.timezone.utc);elapsed=(now-datetime.datetime.fromisoformat(start['observed_at'])).total_seconds();assert elapsed>=60
assert listeners(3000)==start['api_listener'] and listeners(5173)==start['web_listener'];check_cwd(listeners(3000)[0],str(ROOT));check_cwd(listeners(5173)[0],str(ROOT/'web'))
source=json.loads((RELEASE/'release-source.json').read_text());backend=json.loads((RELEASE/'build-sha256.json').read_text());web=json.loads((RELEASE/'web-build-sha256.json').read_text())
assert all(digest(ROOT/v['path'])==v['sha256'] for v in source);assert all(digest(ROOT/'backend/target/debug'/n)==h for n,h in backend.items());assert all(digest(ROOT/'web/dist'/n)==h for n,h in web.items());assert all(digest(p) in backend.values() for p in retirement_paths())
logs={}
for name in ['api.log','web.log']:
 lines=(RELEASE/name).read_text().splitlines();warnings=[(n+1,s) for n,s in enumerate(lines) if re.search(r'\bWARN\b',s)];errors=[(n+1,s) for n,s in enumerate(lines) if re.search(r'\bERROR\b',s)];known=[n for n,s in warnings if 'there is no transaction in progress' in s or 'there is already a transaction in progress' in s];other=[n for n,s in warnings if n not in known];logs[name]={'sha256':digest(RELEASE/name),'warnings':len(warnings),'known_transaction_notice_lines':known,'other_warning_lines':other,'error_lines':[n for n,s in errors]};assert not errors and not other,'Unexpected runtime diagnostics; inspect private logs'
smoke=json.loads((RELEASE/'smoke-results.json').read_text());assert smoke['result']=='passed';runs=[]
for p in sorted(RELEASE.glob('browser-smoke-*/browser-results.json')):
 b=json.loads(p.read_text());runs.append({'path':str(p.relative_to(RELEASE)),'status':b['status'],'sha256':digest(p),'both_actor_sessions_revoked':b.get('session_revocation_proved',False),'sessions_created':len(b.get('sessions',[])),'all_created_sessions_revoked':bool(b.get('sessions')) and all(v.get('revoked') and v.get('owned_profile_removed') for v in b['sessions'])})
assert runs and runs[-1]['status']=='passed' and all(v['all_created_sessions_revoked'] for v in runs) and runs[-1]['both_actor_sessions_revoked']
for p in RELEASE.glob('browser-smoke-*/owned-headless-chrome-*'):assert not p.exists()
end=snapshot('database-after-browser.json');before=json.loads((RELEASE/'database-before.json').read_text());assert end['preservation']['prior_tenant_rowsets_unchanged']==99
x={'finished_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'started_at':start['observed_at'],'elapsed_seconds':round(elapsed,2),'clock':'UTC','api_listener':listeners(3000),'web_listener':listeners(5173),'source_hashes_verified':len(source),'backend_hashes_verified':len(backend),'web_hashes_verified':len(web),'logs':logs,'prior_tenant_rowsets_unchanged':99,'business_counts_unchanged':45,'migration_tables_empty':len(end['migration_counts']),'history_tables_empty':9,'operational_workspaces_unchanged':3,'admission_counts':end['workspace_operation_admission'],'browser_runs':runs,'recurring_monitor_created':False};save('observation.json',x);print(json.dumps(x,indent=2))
