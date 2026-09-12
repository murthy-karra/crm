"""Remove only identity-verified disposable010f2 services and browser profiles."""
from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess
QA=Path('/private/tmp/crm-010f2-qa-694szdwe');MAIN=Path('/Users/karrad/projects/crm');ROOT=Path('/Users/karrad/projects/crm-010f2')
assert QA.is_dir() and QA.stat().st_uid==os.getuid() and QA.stat().st_mode&0o077==0
base=json.loads((QA/'resource-baseline.json').read_text())
def inspect(name):
 v=json.loads(subprocess.check_output(['docker','inspect',name]))[0]
 return {'name':name,'id':v['Id'],'image':v['Config']['Image'],'started_at':v['State']['StartedAt'],'status':v['State']['Status'],'ports':v['NetworkSettings']['Ports'],'mounts':[{'type':m['Type'],'name':m.get('Name'),'destination':m['Destination']} for m in v['Mounts']]}
def main_state():
 names=subprocess.check_output(['git','ls-files','-m','-o','--exclude-standard','-z'],cwd=MAIN).decode().split('\0')
 return {'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=MAIN,text=True).strip(),'pending_files':{n:hashlib.sha256((MAIN/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (MAIN/n).is_file()}}
main_before=json.loads((QA/'main-preservation-before-final-runtime.json').read_text());assert main_state()==main_before
shared_before=[inspect(v['name']) for v in base['shared_preserve']];assert shared_before==base['shared_preserve']
owned=[inspect(v['name']) for v in base['owned']];assert owned==base['owned']
for port in [3017,5187]:
 r=subprocess.run(['lsof','-nP',f'-iTCP:{port}','-sTCP:LISTEN','-t'],text=True,capture_output=True);assert not r.stdout.strip(),'Owned application listener remains'
ps=subprocess.check_output(['ps','-axo','command='],text=True).splitlines()
assert not any(line.startswith(str(ROOT/'backend/target/debug/examples/activity_import_qa')+' ') for line in ps)
assert not any(line.startswith('/Users/karrad/.nvm/versions/node/v24.16.0/bin/node '+str(QA/'browser_driver.mjs')+' ') for line in ps)
assert not any(line.startswith('/Applications/Google Chrome.app/') and str(QA/'browser-profile-') in line for line in ps)
for item in owned:
 subprocess.run(['docker','rm','--force',item['id']],check=True,capture_output=True)
subprocess.run(['docker','volume','rm','crm-010f2-postgres-data'],check=True,capture_output=True)
profiles=[]
for name in ['browser-profile-complete','browser-profile-complete-member','browser-profile-cancel','browser-profile-budget']:
 p=QA/name
 if p.exists():
  assert p.resolve().parent==QA and not p.is_symlink();shutil.rmtree(p);profiles.append(name)
shared_after=[inspect(v['name']) for v in base['shared_preserve']];assert shared_after==shared_before
assert main_state()==main_before
report={'finished_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'owned_removed':owned,'owned_volume_removed':'crm-010f2-postgres-data','application_ports_absent':[3017,5187],'owned_browser_profiles_removed':profiles,'shared_before':shared_before,'shared_after':shared_after,'shared_unchanged':True,'main_head':main_before['head'],'main_pending_files_unchanged':main_before['pending_files'],'worktree_preserved':str(ROOT),'committed_merged_pushed_deployed':False,'private_evidence_retained':str(QA)}
(QA/'cleanup-reconciliation.json').write_text(json.dumps(report,indent=2)+'\n')
print('Removed2 verified owned containers, owned volume and4 browser profiles; shared containers/start times and main planning hashes unchanged. Worktree/evidence preserved.')
