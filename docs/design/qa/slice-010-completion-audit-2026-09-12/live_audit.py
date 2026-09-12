from pathlib import Path
import datetime, hashlib, json, re, shlex, subprocess

ROOT = Path('/Users/karrad/projects/crm')
OUT = Path(__file__).parent
RELEASE = ROOT / 'docs/design/qa/slice-010f2-2026-09-11-release'
PRIVATE = Path('/private/tmp/crm-010f2-release-guor74mb')

def digest(path, algorithm='sha256'):
    h = hashlib.new(algorithm)
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def run(args, **kw):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, **kw).stdout

result = {'observed_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
result['head'] = run(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip()
result['remote_main'] = run(['git', 'ls-remote', 'origin', 'refs/heads/main'], cwd=ROOT).decode().split()[0]
result['listeners'] = run(['lsof', '-nP', '-iTCP:3000', '-iTCP:5173', '-sTCP:LISTEN']).decode()
result['runtime_paths'] = run(['lsof', '-a', '-p', '25757,25780', '-d', 'cwd,txt', '-Fn']).decode()
for manifest, base in [('build-sha256.json', ROOT/'backend/target/debug'),
                       ('web-build-sha256.json', ROOT/'web/dist')]:
    entries = json.loads((RELEASE/manifest).read_text())
    bad = [p for p, h in entries.items() if not (base/p).is_file() or digest(base/p) != h]
    result[manifest] = {'checked': len(entries), 'mismatches': bad}

web = json.loads((RELEASE/'web-build-sha256.json').read_text())
assets = ['index.html'] + [p for p in web if re.match(r'assets/(index-|MigrationView-).*\.js$', p)]
urls = ['http://127.0.0.1:3000/api/health', 'http://127.0.0.1:3000/internal/ready',
        'https://api.tarams.org/api/health']
urls += ['https://app.tarams.org/'+('' if p == 'index.html' else p) for p in assets]
urls += ['http://127.0.0.1:3000/api/migrations/fub/activity-imports',
         'https://api.tarams.org/api/migrations/fub/activity-imports']
result['http'] = []
for url in urls:
    r = subprocess.run(['curl', '-sS', '--max-time', '20', '-w', '\n%{http_code}', url],
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if r.returncode:
        result['http'].append({'url': url, 'curl_exit': r.returncode})
        continue
    body, code = r.stdout.rsplit(b'\n', 1)
    item = {'url': url, 'status': int(code), 'sha256': hashlib.sha256(body).hexdigest(), 'bytes': len(body)}
    if url.startswith('https://app.tarams.org/'):
        name = url.removeprefix('https://app.tarams.org/') or 'index.html'
        item['matches_release'] = item['sha256'] == web[name]
    result['http'].append(item)

baseline = json.loads((RELEASE/'database-after-browser.json').read_text())
tables = list(baseline['business_counts']) + list(baseline['migration_counts']) + ['workspace_operation_admission']
assert all(re.fullmatch(r'[a-z_]+', t) for t in tables)
counts = ' UNION ALL '.join(f'SELECT \'{t}\' AS name, count(*) AS count FROM "{t}"' for t in tables)
sql = '''BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout='30s';
SELECT json_build_object(
'counts', (SELECT json_object_agg(name,count) FROM (COUNTS) c),
'workspaces', (SELECT json_agg(json_build_object('id',id,'mode',workspace_mode,'revision',workspace_revision) ORDER BY id) FROM organization),
'migrations', (SELECT json_agg(json_build_object('version',version,'success',success,'checksum',encode(checksum,'hex')) ORDER BY version) FROM _sqlx_migrations));
ROLLBACK;
'''.replace('COUNTS', counts)
db = json.loads(run(['docker','exec','-i','development-postgres-1','sh','-c',
                     'exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d crm_dev'],
                    input=sql.encode()))
expected = baseline['business_counts'] | baseline['migration_counts']
changed = {t: {'release': n, 'now': db['counts'][t]} for t, n in expected.items() if db['counts'][t] != n}
mismatches = []
for m in db['migrations']:
    files = list((ROOT/'backend/crates/crm-api/migrations').glob(str(m['version'])+'_*.sql'))
    if len(files) != 1 or not m['success'] or digest(files[0], 'sha384') != m['checksum']:
        mismatches.append(m['version'])
result['database'] = {'read_only': True, 'business_tables_checked': len(baseline['business_counts']),
    'migration_tables_checked': len(baseline['migration_counts']), 'count_changes': changed,
    'workspace_states_match_release': db['workspaces'] == baseline['workspaces'],
    'operation_admissions': db['counts']['workspace_operation_admission'],
    'applied_migrations': len(db['migrations']), 'latest_migration': db['migrations'][-1]['version'],
    'migration_checksum_or_success_mismatches': mismatches}

expected_backup = json.loads((RELEASE/'backup-result.json').read_text())
backup = PRIVATE/'crm-dev-before.dump'
result['backup'] = {'exists': backup.is_file(), 'restore_exercised': False}
if backup.is_file():
    with backup.open('rb') as f:
        catalog = run(['docker','exec','-i','development-postgres-1','pg_restore','--list'],stdin=f)
    entries = sum(bool(re.match(rb'^\d+;', line)) for line in catalog.splitlines())
    result['backup'].update({'bytes': backup.stat().st_size,
        'sha256_matches_release': digest(backup) == expected_backup['sha256'], 'catalog_entries': entries})

selected = {}
for line in (ROOT/'.env').read_text().splitlines():
    match = re.match(r'^(?:export\s+)?(CRM_MIGRATION_RELEASE_REPORT|CRM_FUB_SYSTEM_NAME|CRM_FUB_SYSTEM_KEY)=(.*)$', line)
    if match:
        words = shlex.split(match[2], comments=True)
        selected[match[1]] = words[0] if words else ''
report_path = selected.get('CRM_MIGRATION_RELEASE_REPORT')
if report_path:
    report = json.loads(Path(report_path).read_text())
    expiry = datetime.datetime.fromisoformat(report['evidence_expires_at'].replace('Z','+00:00'))
    result['compatibility'] = {'installed_report_matches_release': digest(Path(report_path)) == digest(PRIVATE/'release-report.json'),
        'evidence_expires_at': report['evidence_expires_at'],
        'expired_now': expiry < datetime.datetime.now(datetime.timezone.utc), 'renewed_by_audit': False}
result['fub_configuration'] = {k+'_set': bool(v) for k,v in selected.items() if k != 'CRM_MIGRATION_RELEASE_REPORT'}
result['runtime_log_counts'] = {}
for name in ['api.log','web.log']:
    text = (PRIVATE/name).read_text()
    result['runtime_log_counts'][name] = {'bytes': (PRIVATE/name).stat().st_size,
        'warn_lines': sum(bool(re.search(r'\bWARN\b',l)) for l in text.splitlines()),
        'error_lines': sum(bool(re.search(r'\bERROR\b',l)) for l in text.splitlines()),
        'transaction_state_notice_lines': sum('already a transaction in progress' in l or 'no transaction in progress' in l for l in text.splitlines())}
(OUT/'live-audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
