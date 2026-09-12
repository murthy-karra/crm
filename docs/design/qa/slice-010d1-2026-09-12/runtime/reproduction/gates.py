#!/usr/bin/env python3
"""Run one sequential repository gate with pinned toolchains and private logs."""
import argparse, datetime, hashlib, json, os, pathlib, subprocess, time
QA=pathlib.Path('__PRIVATE_QA_ROOT__')
REPO=pathlib.Path('__CRM_WORKTREE__')
NODE='__USER_HOME__/.nvm/versions/node/v24.16.0/bin'
PNPM='__USER_HOME__/Library/pnpm/store/v11/links/@/pnpm/11.22.0/eeb737e15b4ed7190c895e85812ddc0832a617564aa8721c8139de5d87d3a2b4/bin'
p=argparse.ArgumentParser(); p.add_argument('gate',choices=['sqlx-prepare','check','check-db']); p.add_argument('--attempt',required=True); args=p.parse_args()
assert args.attempt.replace('-','').isalnum()
name='final-'+args.gate+'-'+args.attempt
log=QA/'checks'/(name+'.log'); result=QA/'checks'/(name+'.json')
assert not log.exists() and not result.exists()
env=dict(os.environ,PATH=NODE+':'+PNPM+':'+os.environ['PATH'])
paths=subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'],cwd=REPO).decode().split('\0')
files=[{'path':s,'sha256':hashlib.sha256((REPO/s).read_bytes()).hexdigest()} for s in sorted(set(paths)) if s and not s.startswith('docs/') and (REPO/s).is_file()]
manifest=json.dumps(files,sort_keys=True,separators=(',',':'))
manifest_path=QA/'checks'/(name+'-source.json'); manifest_path.write_text(json.dumps(files,indent=2)+'\n'); manifest_path.chmod(0o600)
start=time.monotonic(); value={'command':'./scripts/'+args.gate,'cwd':str(REPO),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'node':'24.16.0','pnpm':'11.22.0','source_manifest_sha256':hashlib.sha256(manifest.encode()).hexdigest()}
fd=os.open(log,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(fd,'wb') as out:
    rc=subprocess.run(['./scripts/'+args.gate],cwd=REPO,env=env,stdout=out,stderr=subprocess.STDOUT).returncode
value.update(exit_code=rc,elapsed_seconds=round(time.monotonic()-start,3),finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),log_sha256=hashlib.sha256(log.read_bytes()).hexdigest())
result.write_text(json.dumps(value,indent=2)+'\n'); result.chmod(0o600)
print(json.dumps(value)); raise SystemExit(rc)
