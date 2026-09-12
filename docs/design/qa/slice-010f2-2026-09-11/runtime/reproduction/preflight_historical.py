"""Actual DB/hash probe for historical010f1 candidate, while owned QA is stopped."""
from pathlib import Path
import datetime,hashlib,json,os,subprocess,sys
QA=Path('/private/tmp/crm-010f2-qa-694szdwe');ROOT=Path('/Users/karrad/projects/crm-010f2')
phase=sys.argv[1]; assert phase in ['before_activity','after_activity']
ps=subprocess.check_output(['ps','-axo','pid=,command='],text=True)
for line in ps.splitlines():
    parts=line.strip().split(None,1)
    if len(parts)==2:
        assert not parts[1].startswith(('./backend/target/debug/examples/activity_import_qa ',str(ROOT/'backend/target/debug/examples/activity_import_qa')+' ')), 'Stop the owned API before quiesced candidate probe'
old=QA/'activity-incapable-api'
source=json.loads(Path('/private/tmp/crm-010f1-release-ozyr7lke/known-builds.json').read_text())
sha=hashlib.sha256(old.read_bytes()).hexdigest()
known={'version':1,'artifacts':[a for a in source['artifacts'] if a['role'] in ['api','worker'] and a['sha256']==sha]}
assert len(known['artifacts'])==2
candidate={'version':1,'artifacts':[{'role':r,'path':str(old)} for r in ['api','worker']]}
# This is an explicit quiesced, disposable compatibility probe, not an inventory
# of shared development or a certification of the production fleet. Both API
# and its in-process worker launch roles are selected above; no QA process runs.
inventory={'version':1,'target':'crm-010f2-quiesced-candidate-probe','observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z'),'complete':True,'pre_010c_retired':True,'processes':[]}
for name,value in [('known',known),('candidates',candidate),('inventory',inventory)]:
    p=QA/('historical-'+name+'.json');p.write_text(json.dumps(value,indent=2)+'\n');p.chmod(0o600)
env=os.environ.copy();env.update(PGHOST='127.0.0.1',PGPORT='55432',PGDATABASE='crm_010f2_qa',PGUSER='crm_migrator',PATH=str(QA/'bin')+os.pathsep+env['PATH'])
command=[str(ROOT/'scripts/migration-release-preflight'),'--artifacts',str(QA/'historical-known.json'),'--candidates',str(QA/'historical-candidates.json'),'--inventory',str(QA/'historical-inventory.json'),'--target',inventory['target'],'--purpose','launch']
r=subprocess.run(command,env=env,text=True,capture_output=True)
report=json.loads(r.stdout)
path=QA/('historical-'+phase+'.json');path.write_text(json.dumps({'exit_code':r.returncode,'candidate_never_launched':True,'scope':'quiesced disposable QA only','report':report},indent=2)+'\n');path.chmod(0o600)
assert r.returncode==(0 if phase=='before_activity' else 1),(r.returncode,report)
print(phase+': actual historical artifact/hash + read-only database preflight returned '+str(r.returncode))
