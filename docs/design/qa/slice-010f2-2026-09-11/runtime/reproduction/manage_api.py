"""Manage only the exact owned synthetic QA API process; no shared services."""
from pathlib import Path
import datetime, hashlib, json, os, re, signal, subprocess, sys, time
QA=Path('/private/tmp/crm-010f2-qa-694szdwe'); ROOT=Path('/Users/karrad/projects/crm-010f2')
EXE=ROOT/'backend/target/debug/examples/activity_import_qa'
ARGS=[str(EXE),str(QA/'qa.env'),str(QA/'control.json')]
INFO=QA/'api-process.json'
def exact(pid):
 r=subprocess.run(['ps','-p',str(pid),'-o','command='],text=True,capture_output=True)
 return r.returncode==0 and r.stdout.strip()==' '.join(ARGS)
mode=sys.argv[1]
if mode=='stop':
 info=json.loads(INFO.read_text());pid=info['pid'];assert exact(pid),'Owned API identity changed'
 # Freeze scheduling before termination so a later startup cannot inherit a
 # cumulative grant from the previous process-local counter epoch.
 control=json.loads((QA/'control.json').read_text());control['activity_units']=0
 temp=QA/'control.root.tmp';temp.write_text(json.dumps(control)+'\n');temp.chmod(0o600);temp.replace(QA/'control.json')
 os.kill(pid,signal.SIGTERM)
 for _ in range(100):
  if not exact(pid):break
  time.sleep(.1)
 else:raise RuntimeError('Owned API did not stop')
 info['stopped_at']=datetime.datetime.now(datetime.timezone.utc).isoformat();INFO.write_text(json.dumps(info,indent=2)+'\n')
 print('Stopped verified owned API process.')
elif mode=='start':
 epoch=sys.argv[2];assert re.fullmatch('[a-z0-9-]+',epoch)
 if INFO.exists():assert not exact(json.loads(INFO.read_text())['pid']),'Owned API already running'
 assert json.loads((QA/'control.json').read_text())['activity_units']==0,'Startup requires zero worker grant'
 os.chdir(ROOT)
 info={'pid':os.getpid(),'epoch':epoch,'command':ARGS,'executable_sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
 INFO.write_text(json.dumps(info,indent=2)+'\n');INFO.chmod(0o600)
 (QA/('api-process-'+epoch+'.json')).write_text(json.dumps(info,indent=2)+'\n')
 os.execv(str(EXE),ARGS)
else:raise RuntimeError('Expected start or stop')
