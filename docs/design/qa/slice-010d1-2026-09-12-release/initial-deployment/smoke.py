"""Bounded 010d1 shared-development release smoke. Execute only after deployment.
Uses fresh ordinary seed sessions, read-only CRM/migration queries and asset GETs.
No source connection, import proposal, customer-data mutation or seed/reset.
"""
import hashlib
import json
import re
import subprocess
import uuid

from release_ops import ROOT, RELEASE, values, save
from database_ops import snapshot

PUBLIC_API = 'https://api.tarams.org'
BASES = ('http://127.0.0.1:3000', 'http://127.0.0.1:5173', PUBLIC_API, 'https://app.tarams.org')
MIGRATION_READS = (
    '/api/migrations/fub/', '/api/migrations/fub/snapshots',
    '/api/migrations/fub/imports', '/api/migrations/fub/metadata-imports',
    '/api/migrations/fub/activity-imports', '/api/migrations/fub/history-captures',
)
checks = []
cleanup = []
phase = 'start'


def require(condition, code):
    if not condition:
        raise AssertionError(code)


def req(base, path, method='GET', body=None, cookies=None):
    require(base in BASES, 'unexpected_request_origin')
    require(method == 'GET' or (base == PUBLIC_API and path == '/api/session' and method in ('POST', 'DELETE')), 'unexpected_mutation')
    headers_path = RELEASE / ('smoke-headers-' + uuid.uuid4().hex + '.private')
    headers_path.touch(mode=0o600, exist_ok=False)
    args = ['curl', '-sS', '--max-time', '25', '-X', method, '-D', str(headers_path), '-w', '\n%{http_code}', base + path]
    if cookies is not None:
        args += ['-b', str(cookies), '-c', str(cookies)]
    if body is not None:
        args += ['-H', 'Content-Type: application/json', '--data-binary', '@-']
    try:
        response = subprocess.run(args, input=json.dumps(body).encode() if body is not None else None, capture_output=True)
        require(response.returncode == 0, 'http_transport_failed')
        data, status = response.stdout.rsplit(b'\n', 1)
        headers = {}
        for line in headers_path.read_text().splitlines():
            if ':' in line:
                key, value = line.split(':', 1)
                headers[key.lower()] = value.strip()
        return int(status), data, headers
    finally:
        headers_path.unlink(missing_ok=True)


def revoke(cookies, role):
    status = None
    for _ in range(2):
        try:
            status, _, _ = req(PUBLIC_API, '/api/session', 'DELETE', cookies=cookies)
            if status == 204:
                break
        except Exception:
            status = None
    cleanup.append({'actor_role': role, 'status': status})
    require(status == 204, 'temporary_session_revocation_failed')


def run():
    global phase
    config = values()
    require(bool(config.get('CRM_DEV_SEED_PASSWORD')), 'development_seed_password_missing')
    phase = 'health_and_anonymous'
    for base in (BASES[0], PUBLIC_API):
        for path in ('/api/health', '/internal/ready'):
            status, data, headers = req(base, path)
            require(status == 200 and bool(headers.get('x-request-id')), 'health_response_invalid')
            require(json.loads(data)['status'] in ('ok', 'ready'), 'health_status_invalid')
            checks.append({'base': base, 'path': path, 'status': status})
        for path in MIGRATION_READS:
            status, _, _ = req(base, path)
            require(status == 401, 'anonymous_migration_read_not_denied')
            checks.append({'base': base, 'path': path, 'anonymous_status': status})

    for email, role, expected in (
        ('alice@acme.test', 'admin', 200),
        ('carol@acme.test', 'member', 403),
        ('bob@best.test', 'other_org_admin', 200),
    ):
        phase = role + '_reads'
        cookies = RELEASE / ('http-' + role + '-' + uuid.uuid4().hex + '.cookies.private')
        cookies.touch(mode=0o600, exist_ok=False)
        attempted_login = False
        try:
            attempted_login = True
            status, _, _ = req(PUBLIC_API, '/api/session', 'POST', {'email': email, 'password': config['CRM_DEV_SEED_PASSWORD']}, cookies)
            require(status == 200, 'ordinary_seed_login_failed')
            for path in MIGRATION_READS:
                status, data, headers = req(PUBLIC_API, path, cookies=cookies)
                require(status == expected, 'migration_read_wrong_role_status')
                if status == 200:
                    require(headers.get('cache-control') == 'no-store', 'migration_read_cacheable')
                    value = json.loads(data)
                    if path.endswith('history-captures'):
                        require(value == {'captures': [], 'next_cursor': None}, 'unexpected_existing_history_capture')
                    elif path.endswith('imports'):
                        require(value == {'imports': [], 'next_cursor': None}, 'unexpected_existing_import')
                    elif path.endswith('snapshots'):
                        require(value == {'snapshots': [], 'next_cursor': None, 'active_snapshot_id': None, 'latest_completed_snapshot_id': None}, 'unexpected_existing_snapshot')
                    else:
                        require(value == {'connection': None, 'active_assessment': None, 'latest_assessment': None, 'latest_report': None}, 'unexpected_existing_source_connection')
                checks.append({'actor_role': role, 'path': path, 'status': status, 'empty': status == 200})
            for kind in ('metadata-imports', 'activity-imports', 'history-captures'):
                path = '/api/migrations/fub/' + kind + '/' + str(uuid.uuid4())
                status, _, headers = req(PUBLIC_API, path, cookies=cookies)
                require(status == (404 if expected == 200 else 403), 'unknown_import_wrong_status')
                if kind in ('activity-imports', 'history-captures'):
                    require(headers.get('cache-control') == 'no-store', 'unknown_activity_error_cacheable')
                checks.append({'actor_role': role, 'unknown_import_kind': kind, 'status': status, 'no_store': headers.get('cache-control') == 'no-store'})
            status, data, _ = req(PUBLIC_API, '/api/me', cookies=cookies)
            require(status == 200, 'me_read_failed')
            workspace = json.loads(data)['organization']
            require(workspace['workspace_mode'] == 'operational' and workspace['workspace_revision'] == '1', 'workspace_binding_changed')
            checks.append({'actor_role': role, 'workspace_mode': 'operational', 'workspace_revision': '1'})
            if role == 'admin':
                for path in ('/api/people', '/api/today', '/api/custom-fields', '/api/saved-lists'):
                    status, _, _ = req(PUBLIC_API, path, cookies=cookies)
                    require(status == 200, 'existing_crm_read_failed')
                    checks.append({'path': path, 'status': status})
        finally:
            if attempted_login:
                revoke(cookies, role)
            cookies.unlink(missing_ok=True)

    phase = 'published_assets'
    index = (ROOT / 'web/dist/index.html').read_bytes()
    for base in (BASES[1], BASES[3]):
        status, data, _ = req(base, '/')
        require(status == 200 and data == index, 'published_index_mismatch')
        checks.append({'base': base, 'index_matches': True, 'sha256': hashlib.sha256(data).hexdigest()})
    files = re.findall(r'(?:src|href)="(/assets/[^\"]+)"', index.decode())
    migration_assets = list((ROOT / 'web/dist/assets').glob('MigrationView-*.js'))
    require(len(migration_assets) == 1, 'migration_bundle_missing_or_ambiguous')
    files += ['/assets/' + path.name for path in migration_assets]
    for path in sorted(set(files)):
        status, data, _ = req(BASES[3], path)
        require(status == 200 and data == (ROOT / 'web/dist' / path.lstrip('/')).read_bytes(), 'published_asset_mismatch')
        checks.append({'asset': path, 'sha256': hashlib.sha256(data).hexdigest(), 'matches': True})

    phase = 'database_reconciliation'
    before = json.loads((RELEASE / 'database-before.json').read_text())
    after = snapshot('database-after-http.json')
    require(after['business_counts'] == before['business_counts'], 'business_table_counts_changed')
    require(after['workspaces'] == before['workspaces'], 'workspace_rows_changed')
    require(after['migration'] == 20260921000001, 'unexpected_migration_version')
    require(all(value == 0 for value in after['migration_counts'].values()), 'unexpected_migration_data_created')
    require(len([table for table in after['migration_counts'] if table.startswith('migration_activity_')]) == 13, 'activity_table_count_wrong')
    require(len([table for table in after['migration_counts'] if table.startswith('migration_history_')]) == 9, 'history_table_count_wrong')
    result = {
        'result': 'passed', 'checks': checks, 'migration': after['migration'],
        'business_counts_unchanged': True, 'business_table_count': len(after['business_counts']),
        'workspaces_unchanged': True, 'migration_counts': after['migration_counts'],
        'session_cleanup': cleanup, 'temporary_sessions_revoked': all(row['status'] == 204 for row in cleanup),
        'source_connection_created': False, 'activity_import_proposal_created': False, 'history_capture_proposal_created': False,
        'source_or_business_mutations': 0, 'seed_or_reset_performed': False,
        'live_fub_processing_performed': False,
    }
    save('smoke-results.json', result)
    print(json.dumps({'status': 'passed', 'http_auth_asset_checks': len(checks), 'business_tables_unchanged': len(after['business_counts']), 'activity_tables_present_and_empty': 13, 'history_tables_present_and_empty': 9, 'sessions_revoked': len(cleanup)}))


if __name__ == '__main__':
    try:
        run()
    except Exception as error:
        # Never emit HTTP bodies, raw transport errors, cookies or configuration.
        code = str(error) if isinstance(error, AssertionError) and re.fullmatch('[a-z_]+', str(error)) else 'release_smoke_operation_failed'
        save('smoke-results.json', {'result': 'failed', 'phase': phase, 'reason': code, 'checks': checks, 'session_cleanup': cleanup})
        print(json.dumps({'status': 'failed', 'phase': phase, 'reason': code}))
        raise SystemExit(1)
