import os,subprocess,time,datetime,json,concurrent.futures
from pathlib import Path
from release_ops import ROOT,RELEASE,save,digest,REVISION
assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT).strip()
assert REVISION==json.loads((RELEASE/'git-integration.json').read_text())['merge']
files=subprocess.check_output(['git','ls-files','backend','web','scripts','Cargo.toml','pnpm-lock.yaml'],cwd=ROOT,text=True).splitlines()
source={p:digest(ROOT/p) for p in files if (ROOT/p).is_file()}
save('release-source.json',{'revision':REVISION,'files':source})
def build(name,args,cwd):
 env={**os.environ,'SQLX_OFFLINE':'true'};env.pop('DATABASE_URL',None)
 log=RELEASE/('build-'+name+'.log');start=time.monotonic()
 with log.open('wb') as out: result=subprocess.run(args,cwd=cwd,env=env,stdout=out,stderr=subprocess.STDOUT)
 log.chmod(0o600)
 record={'command':args,'exit_code':result.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'revision':REVISION,'log_sha256':digest(log),'source_manifest_sha256':digest(RELEASE/'release-source.json')}
 save('build-'+name+'-result.json',record);print(json.dumps({'build':name,**record}),flush=True)
 assert result.returncode==0,'Build failed; inspect protected '+name+' log'
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
 futures=[pool.submit(build,'api',['cargo','build','--manifest-path','backend/Cargo.toml','-p','crm-api','--bin','crm-api','--bin','migrate','--bin','crm-admin','--locked'],ROOT),pool.submit(build,'web',['pnpm','exec','vite','build','--outDir',str(RELEASE/'web-dist-staged')],ROOT/'web')]
 for f in futures:f.result()
assert all(digest(ROOT/p)==h for p,h in source.items())
hashes={n:digest(ROOT/'backend/target/debug'/n) for n in ['crm-api','crm-admin','migrate']};save('build-sha256.json',hashes)
assets={str(p.relative_to(RELEASE/'web-dist-staged')):digest(p) for p in sorted((RELEASE/'web-dist-staged').rglob('*')) if p.is_file()};save('web-build-sha256.json',assets)
names={'api':'crm-api','worker':'crm-api','cli':'crm-admin','migrator':'migrate'}
known=[];candidates=[]
for role,name in names.items():
 row={'sha256':hashes[name],'role':role,'gate_version':'crm-workspace-v1','revision':REVISION}
 if role in ['api','worker']:row['capabilities']=['fub-metadata-import-v1','fub-activity-import-v1','fub-history-capture-v1','fub-history-timeline-v1']
 known.append(row);candidates.append({'role':role,'path':str(ROOT/'backend/target/debug'/name)})
save('known-builds.json',{'version':1,'artifacts':known});save('candidates.json',{'version':1,'artifacts':candidates})
print(json.dumps({'builds_passed':True,'source_files':len(source),'web_files':len(assets),'backend_artifacts':hashes}),flush=True)
