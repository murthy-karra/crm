from pathlib import Path
import datetime as dt
import hashlib
import json
import os
import re
import shlex
import signal
import subprocess
import time
from urllib.parse import urlparse, unquote

ROOT = Path('/Users/karrad/projects/crm')
RELEASE = Path(__file__).parent
REVISION = json.loads((RELEASE / 'git-integration.json').read_text())['merge']

def save(name, value):
    path = RELEASE / name
    path.write_text(json.dumps(value, indent=2) + '\n')
    path.chmod(0o600)

def values():
    result = {}
    for line in (ROOT / '.env').read_text().splitlines():
        if not line.strip() or line.lstrip().startswith('#') or '=' not in line:
            continue
        key, value = line.split('=', 1)
        parts = shlex.split(value, comments=True)
        result[key.strip().removeprefix('export ')] = parts[0] if parts else ''
    return result

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def listeners(port):
    result = subprocess.run(['lsof', '-nP', '-t', f'-iTCP:{port}', '-sTCP:LISTEN'], capture_output=True, text=True)
    assert result.returncode in (0, 1)
    return [int(x) for x in result.stdout.split()]

def check_cwd(pid, expected):
    result = subprocess.check_output(['lsof', '-a', '-p', str(pid), '-d', 'cwd', '-Fn']).decode()
    assert '\nn' + expected + '\n' in result, 'Unexpected process working directory'

def retirement_paths():
    paths = []
    for profile in ('debug', 'release'):
        base = ROOT / 'backend/target' / profile
        paths += [base / name for name in ('crm-api', 'crm-admin', 'migrate')]
        for pattern in ('crm_api-*', 'crm_admin-*', 'migrate-*'):
            paths += list((base / 'deps').glob(pattern))
    for directory in Path('/private/tmp').glob('crm-*-release-*'):
        paths += [directory / name for name in ('crm-api-before', 'crm-admin-before', 'migrate-before')]
    for profile in ('debug', 'release'):
        examples = ROOT / 'backend/target' / profile / 'examples'
        for pattern in ('snapshot_qa*', 'import_qa*', 'metadata_import_qa*', 'activity_import_qa*', 'history_capture_qa*'):
            paths += list(examples.glob(pattern))
    return sorted(set(p for p in paths if p.is_file() and os.access(p, os.X_OK)))

def retire():
    known = set(json.loads((RELEASE / 'build-sha256.json').read_text()).values())
    retired = []
    for path in retirement_paths():
        fingerprint = digest(path)
        if fingerprint in known:
            continue
        before = path.stat().st_mode & 0o777
        path.chmod(before & ~0o111)
        assert not os.access(path, os.X_OK)
        retired.append({'path': str(path), 'sha256': fingerprint, 'mode_before': oct(before), 'mode_after': oct(path.stat().st_mode & 0o777)})
    save('retired-artifacts.json', retired)
    return retired

def inventory():
    hashes = json.loads((RELEASE / 'build-sha256.json').read_text())
    for name, fingerprint in hashes.items():
        assert digest(ROOT / 'backend/target/debug' / name) == fingerprint
    for path in retirement_paths():
        assert digest(path) in hashes.values(), 'Unretired legacy executable'
    worktrees = subprocess.check_output(['git', 'worktree', 'list', '--porcelain'], cwd=ROOT).decode()
    assert worktrees.count('worktree ') == 1, 'Re-inventory additional worktrees'
    processes = [
        {'id': 'launch.dev-api', 'role': 'api', 'sha256': hashes['crm-api']},
        {'id': 'launch.in-process-workers', 'role': 'worker', 'sha256': hashes['crm-api']},
        {'id': 'launch.crm-admin', 'role': 'cli', 'sha256': hashes['crm-admin']},
        {'id': 'launch.db-migrate', 'role': 'migrator', 'sha256': hashes['migrate']},
    ]
    observed = []
    for line in subprocess.check_output(['ps', 'axo', 'pid=,comm=']).decode().splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) != 2:
            continue
        pid, command = parts
        name = Path(command).name
        if name not in ('crm-api', 'crm-admin', 'migrate'):
            assert not re.match(r'^(crm_api-|crm_admin-|migrate-|snapshot_qa|import_qa|metadata_import_qa|activity_import_qa|history_capture_qa)', name), 'Unexpected alternate CRM runtime'
            continue
        expected = ROOT / 'backend/target/debug' / name
        actual = Path(command) if Path(command).is_absolute() else ROOT / command
        assert actual.resolve() == expected, 'Unknown CRM runtime path'
        assert int(pid) in listeners(3000) if name == 'crm-api' else False, 'Unexpected CRM administration process'
        check_cwd(int(pid), str(ROOT))
        observed.append({'pid': int(pid), 'path': command})
        processes.append({'id': 'pid.' + pid, 'role': 'api', 'sha256': hashes[name]})
        processes.append({'id': 'pid.' + pid + '.workers', 'role': 'worker', 'sha256': hashes[name]})
    containers = subprocess.check_output(['docker', 'ps', '--format', '{{.Names}}']).decode().splitlines()
    assert set(containers) == {'development-postgres-1', 'development-centrifugo-1'}, 'Re-inventory containers'
    matching_launch_files = []
    for directory in (Path('/Users/karrad/Library/LaunchAgents'), Path('/Library/LaunchAgents'), Path('/Library/LaunchDaemons')):
        if directory.exists():
            for path in directory.glob('*.plist'):
                data = path.read_bytes().lower()
                if b'/projects/crm' in data or b'crm-api' in data:
                    matching_launch_files.append(str(path))
    assert not matching_launch_files, 'Re-inventory scheduled launch paths'
    cron = subprocess.run(['crontab', '-l'], capture_output=True)
    assert cron.returncode in (0, 1), 'Could not inspect current-user scheduled launch paths'
    assert not any(token in cron.stdout.lower() for token in [b'/projects/crm', b'crm-api', b'crm-admin', b'activity_import_qa', b'history_capture_qa']), 'Re-inventory scheduled CRM cron launch paths'
    result = {'version': 1, 'target': 'shared-development.crm_dev', 'observed_at': dt.datetime.now(dt.timezone.utc).isoformat(timespec='seconds').replace('+00:00', 'Z'), 'complete': True, 'pre_010c_retired': True, 'processes': processes}
    save('process-inventory.json', result)
    save('inventory-observation.json', {'observed_crm_processes': observed, 'containers': containers, 'crm_launch_configuration_files': matching_launch_files, 'git_worktrees': 1, 'legacy_executable_paths_remaining': 0, 'current_user_cron_crm_launch_paths': 0, 'alternate_crm_runtime_processes': 0})
    return result

def preflight(purpose):
    inventory()
    bindir = RELEASE / 'bin'
    bindir.mkdir(exist_ok=True, mode=0o700)
    psql = bindir / 'psql'
    psql.write_text('#!/bin/sh\nexec docker exec -i -e PGHOST -e PGPORT -e PGUSER -e PGPASSWORD -e PGDATABASE -e PGCONNECT_TIMEOUT -e PGOPTIONS development-postgres-1 psql "$@"\n')
    psql.chmod(0o700)
    url = urlparse(values()['MIGRATION_DATABASE_URL'])
    assert url.hostname in ('localhost', '127.0.0.1') and url.path == '/crm_dev'
    env = {**os.environ, 'PATH': str(bindir) + ':' + os.environ['PATH'], 'PGHOST': '127.0.0.1', 'PGPORT': '5432', 'PGUSER': unquote(url.username), 'PGPASSWORD': unquote(url.password), 'PGDATABASE': 'crm_dev'}
    args = ['./scripts/migration-release-preflight', '--artifacts', str(RELEASE / 'known-builds.json'), '--candidates', str(RELEASE / 'candidates.json'), '--inventory', str(RELEASE / 'process-inventory.json'), '--target', 'shared-development.crm_dev', '--purpose', purpose]
    result = subprocess.run(args, cwd=ROOT, env=env, capture_output=True)
    report = json.loads(result.stdout)
    save('preflight-' + purpose + '.json', report)
    assert result.returncode == 0, 'Compatibility preflight failed: ' + str(report)
    if purpose == 'confirm':
        assert report['confirmation_ready']
        assert report['metadata_confirmation_ready']
        assert report['metadata_confirmation_reasons'] == []
        assert report['activity_confirmation_ready']
        assert report['activity_confirmation_reasons'] == []
        assert report['history_capture_confirmation_ready']
        assert report['history_capture_confirmation_reasons'] == []
        assert report['history_capture_schema_present']
        assert report['history_capture_unsupported_count'] == '0'
        save('release-report.next.json', report)
        os.replace(RELEASE / 'release-report.next.json', RELEASE / 'release-report.json')
    return report

def deploy():
    before = json.loads((RELEASE / 'release-before.json').read_text())['listeners']
    assert listeners(3000) == [before['3000']['pid']] and listeners(5173) == [before['5173']['pid']]
    for port, entry in before.items():
        current = subprocess.check_output(['ps','-p',str(entry['pid']),'-o','command=']).decode().strip()
        assert current == entry['command'], 'Old listener identity changed'
        check_cwd(entry['pid'], str(ROOT if port == '3000' else ROOT / 'web'))
    for pid in (before['3000']['pid'], before['5173']['pid']):
        os.kill(pid, signal.SIGTERM)
    until = time.monotonic() + 15
    while listeners(3000) or listeners(5173):
        assert time.monotonic() < until, 'Old listener did not stop'
        time.sleep(0.2)
    retired = retire()
    preflight('launch'); preflight('confirm')
    config = ROOT / '.env'
    lines = config.read_text().splitlines()
    indices = [i for i, line in enumerate(lines) if line.strip().startswith(('CRM_MIGRATION_RELEASE_REPORT=', 'export CRM_MIGRATION_RELEASE_REPORT='))]
    assert len(indices) == 1, 'Expected one existing operator report configuration'
    assert values()['CRM_MIGRATION_RELEASE_REPORT'] == '/private/tmp/crm-010f2-release-guor74mb/release-report.json', 'Unexpected current report path'
    lines[indices[0]] = 'CRM_MIGRATION_RELEASE_REPORT=' + str(RELEASE / 'release-report.json')
    lines = [line.replace('# 010f2 operator-owned compatibility evidence;', '# 010d1 operator-owned compatibility evidence;') for line in lines]
    config.write_text('\n'.join(lines) + '\n')
    config.chmod(0o600)
    os.replace(ROOT / 'web/dist', RELEASE / 'web-dist-retired')
    os.replace(RELEASE / 'web-dist-staged', ROOT / 'web/dist')
    env = {**os.environ, 'PATH': '/Users/karrad/.nvm/versions/node/v24.16.0/bin:/Users/karrad/Library/pnpm/store/v11/links/@/pnpm/11.22.0/eeb737e15b4ed7190c895e85812ddc0832a617564aa8721c8139de5d87d3a2b4/bin:' + os.environ['PATH'], 'SQLX_OFFLINE': 'true'}
    api_log = RELEASE / 'api.log'; api_log.touch(mode=0o600)
    web_log = RELEASE / 'web.log'; web_log.touch(mode=0o600)
    with api_log.open('ab') as log:
        api = subprocess.Popen(['./scripts/dev-api'], cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    with web_log.open('ab') as log:
        web = subprocess.Popen(['pnpm', 'exec', 'vite', 'preview'], cwd=ROOT / 'web', env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    save('launchers.json', {'api': api.pid, 'web': web.pid})
    until = time.monotonic() + 30
    while not listeners(3000) or not listeners(5173):
        assert api.poll() is None and web.poll() is None, 'New runtime exited'
        assert time.monotonic() < until, 'New listener did not start'
        time.sleep(0.3)
    report = preflight('confirm')
    save('observation-start.json', {'observed_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'api_listener': listeners(3000), 'web_listener': listeners(5173)})
    save('deployed.json', {'revision': REVISION, 'implementation': json.loads((RELEASE / 'git-integration.json').read_text())['implementation'], 'merge': REVISION, 'api_listener': listeners(3000), 'web_listener': listeners(5173), 'api_launcher': api.pid, 'web_launcher': web.pid, 'artifacts': json.loads((RELEASE / 'build-sha256.json').read_text()), 'index_sha256': digest(ROOT / 'web/dist/index.html'), 'migration': 20260921000001, 'retired_artifact_paths': len(retired), 'confirmation_evidence_expires_at': report['evidence_expires_at']})
    print(json.dumps({'started': True, 'api_listener': listeners(3000), 'web_listener': listeners(5173), 'retired_artifact_paths': len(retired), 'compatibility_preflight': 'passed', 'confirmation_evidence_expires_at': report['evidence_expires_at']}))

if __name__ == '__main__':
    deploy()
