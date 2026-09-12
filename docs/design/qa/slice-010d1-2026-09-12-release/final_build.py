"""Build the committed release correction; preserve the initial deployed attempt."""
import os,json,subprocess,hashlib,time,datetime
from release_ops import ROOT,RELEASE,save,digest
rev=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()
assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT).strip()
prior=json.loads((ROOT/'docs/design/qa/slice-010d1-2026-09-12/gates/final-check-post-visual-source.json').read_text())
source=[{**v,'sha256':digest(ROOT/v['path'])} for v in prior]
delta=[{'path':a['path'],'before':a['sha256'],'after':b['sha256']} for a,b in zip(prior,source) if a!=b]
assert {x['path'] for x in delta}=={'backend/crates/crm-api/src/auth/workspace_http.rs','backend/crates/crm-api/tests/db_history_capture_authority.rs'}
save('release-source.json',source);save('release-source-delta.json',{'revision':rev,'original_merge':json.loads((RELEASE/'git-integration.json').read_text())['merge'],'source_manifest_sha256':digest(RELEASE/'release-source.json'),'changed_files':delta})
cmd=['cargo','build','--manifest-path','backend/Cargo.toml','-p','crm-api','--bin','crm-api','--bin','migrate','--bin','crm-admin','--locked']
log=RELEASE/'build-api.log';start=time.monotonic();stamp=datetime.datetime.now(datetime.timezone.utc).isoformat()
env={**os.environ,'SQLX_OFFLINE':'true'};env.pop('DATABASE_URL',None)
with log.open('wb') as out:rc=subprocess.run(cmd,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT).returncode
result={'command':cmd,'exit_code':rc,'elapsed_seconds':round(time.monotonic()-start,3),'started_at':stamp,'revision':rev,'log_sha256':digest(log),'source_manifest_sha256':digest(RELEASE/'release-source.json'),'replaces_initial_build':True};save('build-api-result.json',result);print(json.dumps(result),flush=True);assert rc==0
hashes={n:digest(ROOT/'backend/target/debug'/n) for n in ['crm-api','crm-admin','migrate']};save('build-sha256.json',hashes)
known=json.loads((RELEASE/'known-builds.json').read_text());names={'api':'crm-api','worker':'crm-api','cli':'crm-admin','migrator':'migrate'}
for v in known['artifacts']:v['sha256']=hashes[names[v['role']]];v['revision']=rev
save('known-builds.json',known)
assert all(digest(ROOT/'web/dist'/n)==h for n,h in json.loads((RELEASE/'web-build-sha256.json').read_text()).items())
