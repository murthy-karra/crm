"""Replace only the API after the no-store correction; preserve the Web build."""
import os,subprocess,time,signal,datetime,json,shutil
from release_ops import ROOT,RELEASE,save,digest,listeners,check_cwd,retire,preflight,values
old=json.loads((RELEASE/'initial-deployment/deployed.json').read_text());build=json.loads((RELEASE/'build-api-result.json').read_text());rev=build['revision']
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()==rev
assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT).strip()
assert listeners(3000)==old['api_listener'] and listeners(5173)==old['web_listener']
pid=listeners(3000)[0];check_cwd(pid,str(ROOT));check_cwd(listeners(5173)[0],str(ROOT/'web'))
command=subprocess.check_output(['ps','-p',str(pid),'-o','command=']).decode().strip();assert command.endswith('backend/target/debug/crm-api')
assert values()['CRM_MIGRATION_RELEASE_REPORT']==str(RELEASE/'release-report.json')
save('replacement-before.json',{'api_pid':pid,'api_command':command,'web_listener':listeners(5173),'revision':old['revision']})
assert subprocess.check_output(['ps','-p',str(pid),'-o','command=']).decode().strip()==command
os.kill(pid,signal.SIGTERM);until=time.monotonic()+15
while listeners(3000):
 assert time.monotonic()<until
 time.sleep(.2)
shutil.copy2(RELEASE/'api.log',RELEASE/'initial-deployment/api.log')
retired=retire();preflight('launch');preflight('confirm')
env={**os.environ,'SQLX_OFFLINE':'true'}
assert not env.get('CRM_FUB_SYSTEM_KEY') and not env.get('CRM_FUB_SYSTEM_NAME')
with (RELEASE/'api.log').open('wb') as log:api=subprocess.Popen(['./scripts/dev-api'],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
until=time.monotonic()+30
while not listeners(3000):
 assert api.poll() is None and time.monotonic()<until
 time.sleep(.3)
assert listeners(5173)==old['web_listener'];report=preflight('confirm')
save('launchers.json',{'api':api.pid,'web':old['web_launcher']})
save('observation-start.json',{'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'api_listener':listeners(3000),'web_listener':listeners(5173)})
x={**old,'revision':rev,'web_revision':old['revision'],'api_listener':listeners(3000),'api_launcher':api.pid,'artifacts':json.loads((RELEASE/'build-sha256.json').read_text()),'confirmation_evidence_expires_at':report['evidence_expires_at'],'source_manifest_sha256':digest(RELEASE/'release-source.json'),'replacement_reason':'History no-store header on outer authorization denials','retired_artifact_paths':len(retired)}
save('deployed.json',x);print(json.dumps({'replaced':True,'api_listener':listeners(3000),'web_listener':listeners(5173),'revision':rev,'confirmation_evidence_expires_at':report['evidence_expires_at']}))
