#!/usr/bin/env python3
"""Manage only this task's isolated synthetic API and private policy inputs."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import time

QA = pathlib.Path('__PRIVATE_QA_ROOT__')
REPO = pathlib.Path('__CRM_WORKTREE__')
EXE = REPO / 'backend/target/debug/examples/history_capture_qa'
STATE = QA / 'api-process.json'


def private_json(path, value):
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(value, indent=2) + '\n')
    temp.chmod(0o600)
    temp.replace(path)


parser = argparse.ArgumentParser()
parser.add_argument('action', choices=['start','stop','policy'])
parser.add_argument('--epoch')
parser.add_argument('--run-ceiling', type=int)
parser.add_argument('--org-ceiling', type=int)
args = parser.parse_args()
assert QA.is_dir() and QA.stat().st_mode & 0o077 == 0
if args.action == 'policy':
    assert args.run_ceiling and args.org_ceiling
    assert 0 < args.run_ceiling <= args.org_ceiling <= 16 * 1024**3
    values = {'CRM_FUB_SNAPSHOT_RUN_CEILING_BYTES': str(args.run_ceiling),
              'CRM_FUB_SNAPSHOT_ORG_CEILING_BYTES': str(args.org_ceiling)}
    env = QA / 'qa.env'
    lines = [line for line in env.read_text().splitlines() if line.partition('=')[0] not in values]
    lines.extend(k + '=' + value for k, value in values.items())
    env.write_text('\n'.join(lines) + '\n'); env.chmod(0o600)
    print(json.dumps({'private_policy':values,'effective_after_restart':True}))
elif args.action == 'start':
    assert args.epoch and all(ch.isalnum() or ch in '-_' for ch in args.epoch)
    assert not STATE.exists(), 'Stop or reconcile the recorded owned API before starting another'
    listeners = subprocess.run(['/usr/sbin/lsof','-nP','-iTCP:13010','-sTCP:LISTEN','-t'], text=True, capture_output=True)
    assert listeners.returncode in (0,1) and not listeners.stdout.strip(), 'API13010 is already occupied; private controls are unchanged'
    assert not (QA / ('source-' + args.epoch + '.json')).exists(), 'Use a new recorder epoch'
    control_path = QA / 'control.json'
    control = json.loads(control_path.read_text()) if control_path.exists() else {
        'scenario':'happy','mode':'normal','delay_ms':0,'note_detail_gap':True,
        'failure_stream':'events','retry_failures':2,'retry_after_seconds':2}
    control.update(epoch=args.epoch, history_units=0)
    private_json(control_path, control)
    logfile = QA / ('api-' + args.epoch + '.log')
    descriptor = os.open(logfile, os.O_CREAT | os.O_WRONLY | os.O_EXCL, 0o600)
    environment = dict(os.environ, RUST_LOG='info,sqlx=warn')
    with os.fdopen(descriptor, 'wb') as output:
        process = subprocess.Popen([str(EXE), str(QA / 'qa.env'), str(control_path)],
            cwd=REPO, env=environment, stdout=output, stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL, start_new_session=True)
    value = {'pid':process.pid,'executable':str(EXE),'epoch':args.epoch,'log':str(logfile),
             'sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),
             'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
    private_json(STATE, value)
    time.sleep(0.5)
    assert process.poll() is None, 'Synthetic API exited; inspect its private log'
    print(json.dumps({'started_owned_api':process.pid,'epoch':args.epoch}))
else:
    value = json.loads(STATE.read_text())
    assert value['executable'] == str(EXE)
    status = subprocess.run(['/bin/ps','-p',str(value['pid']),'-o','command='],text=True,capture_output=True)
    if status.returncode == 0 and status.stdout.strip():
        assert status.stdout.strip() == str(EXE) + ' ' + str(QA / 'qa.env') + ' ' + str(QA / 'control.json'), 'Owned process identity changed'
        os.kill(value['pid'], signal.SIGINT)
        for _ in range(100):
            status = subprocess.run(['/bin/ps','-p',str(value['pid']),'-o','stat='],text=True,capture_output=True)
            if status.returncode or status.stdout.strip().startswith('Z'): break
            time.sleep(0.1)
        else: raise RuntimeError('Owned API did not stop gracefully')
    value['stopped_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    private_json(QA / ('api-process-' + value['epoch'] + '.json'), value)
    STATE.unlink()
    print(json.dumps({'stopped_owned_api':value['pid'],'epoch':value['epoch']}))
