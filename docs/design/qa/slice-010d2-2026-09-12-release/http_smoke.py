#!/usr/bin/env python3
"""010d2 read-only release smoke; requires explicit post-rollout authorization.

Only ordinary seed session login/logout mutate state. No database access, source
connection, proposal, capture, import, outbound action or business write occurs.
Cookies/passwords exist only in memory. Root owns database/binary reconciliation.
"""
import base64
import datetime
import email.parser
import hashlib
import http.cookies
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import urllib.parse
import uuid

ROOT = Path('/Users/karrad/projects/crm')
PRIVATE = Path('/private/tmp/crm-010d2-release-q9q7lzwf')
API = 'https://api.tarams.org'
APP = 'https://app.tarams.org'
BASES = ('http://127.0.0.1:3000', 'http://127.0.0.1:5173', API, APP)
HISTORY = '/api/migrations/fub/history-imports'
MIGRATION_READS = (
    '/api/migrations/fub/', '/api/migrations/fub/snapshots',
    '/api/migrations/fub/imports', '/api/migrations/fub/metadata-imports',
    '/api/migrations/fub/activity-imports', '/api/migrations/fub/history-captures', HISTORY,
)
STAMP = datetime.datetime.now(datetime.timezone.utc).isoformat().replace(':', '-')
OUT = PRIVATE / ('http-smoke-' + STAMP)
PROOF = dict(kind='read-only 010d2 release HTTP smoke', status='not_run',
             started_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
             checks=[], requests=[], sessions=[], source_or_business_mutations=0,
             database_reconciliation='owned separately by release coordinator',
             scope='Empty shared development only; populated synthetic walkthrough is separate evidence.')
SECRETS = set()
PHASE = 'initialization'


def require(condition, code):
    if not condition:
        raise AssertionError(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def request(base, path, *, actor='anonymous', method='GET', body=None, cookie=None):
    require(base in BASES and path.startswith('/') and not path.startswith('//'), 'unexpected_origin')
    require(method == 'GET' or base == API and path == '/api/session' and method in ('POST', 'DELETE'), 'unexpected_mutation')
    require(body is None or method == 'POST' and path == '/api/session', 'unexpected_request_body')
    config = ['url = ' + json.dumps(base + path), 'request = ' + json.dumps(method),
              'header = ' + json.dumps('Origin: ' + APP),
              'header = ' + json.dumps('Accept: application/json' if base in (BASES[0], API) else 'Accept: text/html,*/*')]
    if cookie:
        require(base == API, 'cookie_origin_mismatch')
        config.append('header = ' + json.dumps('Cookie: ' + cookie, ensure_ascii=False))
    if body is not None:
        config.extend(('header = "Content-Type: application/json"',
                       'data-binary = ' + json.dumps(json.dumps(body, ensure_ascii=False), ensure_ascii=False)))
    # Sensitive headers and login JSON pass only through captured stdin, never argv,
    # files, shell interpolation or output. curl uses the host's working TLS stack.
    response = subprocess.run(
        ['curl', '--silent', '--show-error', '--max-time', '25', '--max-filesize', str(8 * 1024 * 1024),
         '--suppress-connect-headers', '--include', '--config', '-'],
        input=('\n'.join(config) + '\n').encode(), capture_output=True, timeout=30)
    require(response.returncode == 0, 'http_transport_failed')
    content = response.stdout
    while True:
        raw_headers, content = content.split(b'\r\n\r\n', 1)
        status_line, remaining_headers = raw_headers.split(b'\r\n', 1)
        status = int(status_line.split()[1])
        if status >= 200:
            break
    require(not 300 <= status < 400, 'unexpected_redirect')
    response_headers = email.parser.Parser().parsestr(remaining_headers.decode('iso-8859-1'))
    session_cookies = []
    for line in response_headers.get_all('Set-Cookie', []):
        parsed = http.cookies.SimpleCookie()
        parsed.load(line)
        for name, morsel in parsed.items():
            if name == 'crm_session' and morsel.value:
                SECRETS.add(morsel.value)
                session_cookies.append(name + '=' + morsel.value)
    if session_cookies and isinstance(actor, dict):
        require(len(set(session_cookies)) == 1, 'ambiguous_owned_cookie')
        actor['cookie'] = session_cookies[0]
    require(len(content) <= 8 * 1024 * 1024, 'response_size_limit')
    PROOF['requests'].append(dict(actor=actor['label'] if isinstance(actor, dict) else actor,
                                 phase=PHASE, method=method, base=base, path=path, status=status))
    return status, content, response_headers


def password():
    found = []
    for line in (ROOT / '.env').read_text().splitlines():
        line = line.strip()
        if line.startswith('export '):
            line = line[7:]
        if not line or line.startswith('#') or '=' not in line:
            continue
        key, raw = line.split('=', 1)
        if key.strip() == 'CRM_DEV_SEED_PASSWORD':
            parsed = shlex.split(raw, comments=True)
            require(len(parsed) == 1 and bool(parsed[0]), 'invalid_seed_credential')
            found.append(parsed[0])
    require(len(found) == 1, 'missing_or_duplicate_seed_credential')
    SECRETS.add(found[0])
    return found[0]


def no_store(headers):
    return headers.get('Cache-Control') == 'no-store'


def closed(data, status):
    return json.loads(data) == {'error': {401: 'unauthenticated', 403: 'forbidden', 404: 'not_found'}[status]}


def revoke(actor):
    record = dict(actor=actor['label'], login_started=actor['login_started'],
                  login_status=actor.get('login_status'), cookie_captured=bool(actor.get('cookie')),
                  delete_status=None, original_cookie_me_status=None, revoked=False)
    if actor.get('cookie'):
        # This is the exact pre-logout cookie, not a cookie jar already cleared by logout.
        for _ in range(2):
            try:
                record['delete_status'], _, _ = request(API, '/api/session', actor=actor, method='DELETE', cookie=actor['cookie'])
                record['original_cookie_me_status'], body, _ = request(API, '/api/me', actor=actor, cookie=actor['cookie'])
                record['original_cookie_denial_closed'] = record['original_cookie_me_status'] == 401 and closed(body, 401)
                record['revoked'] = record['delete_status'] == 204 and record['original_cookie_denial_closed']
                if record['revoked']:
                    break
            except Exception:
                record['transport_failure'] = True
    else:
        record['no_confirmed_session_to_revoke'] = not actor['login_started']
    actor['cookie'] = None
    PROOF['sessions'].append(record)
    require(record['revoked'] or record.get('no_confirmed_session_to_revoke'), 'session_revocation_not_proved')


def metadata():
    records = []
    values = {}
    for name in ('deployed.json', 'web-build-sha256.json', 'build-sha256.json'):
        data = (PRIVATE / name).read_bytes()
        values[name] = json.loads(data)
        records.append(dict(file=name, sha256=sha(data)))
    deployed, web, binary = (values[name] for name in ('deployed.json', 'web-build-sha256.json', 'build-sha256.json'))
    require(bool(re.fullmatch('[a-f0-9]{40}', deployed.get('revision', ''))), 'deployed_revision_invalid')
    require(all(re.fullmatch('[a-f0-9]{64}', binary.get(key, '')) for key in ('crm-api', 'crm-admin', 'migrate')), 'binary_metadata_invalid')
    require('index.html' in web and any(re.fullmatch(r'assets/MigrationView-.*\.js', p) for p in web), 'web_metadata_incomplete')
    for path, digest in web.items():
        require(not path.startswith('/') and '..' not in Path(path).parts and bool(re.fullmatch('[a-f0-9]{64}', digest)), 'web_metadata_invalid')
        file = ROOT / 'web/dist' / path
        require(file.is_file() and sha(file.read_bytes()) == digest, 'local_web_manifest_mismatch')
    PROOF['release'] = dict(deployed_revision=deployed['revision'], metadata=records,
                            binary_sha256={key: binary[key] for key in ('crm-api', 'crm-admin', 'migrate')},
                            binary_provenance='Coordinator-owned deployment record; this smoke does not inspect running processes.',
                            local_web_asset_count=len(web), local_web_assets_match_manifest=True)
    return web


def run():
    global PHASE
    require(sys.argv[1:] == ['--after-rollout-authorized'], 'explicit_after_rollout_authorization_required')
    OUT.mkdir(mode=0o700)
    PHASE = 'release_metadata'
    web = metadata()
    credential = password()
    PROOF['transport'] = 'curl with verified host TLS; config, credentials and bodies stay in captured stdin/memory'
    PROOF['helper_sha256'] = sha(Path(__file__).read_bytes())
    PROOF['status'] = 'running'
    unknown = str(uuid.uuid4())
    unknown_reads = [HISTORY + '/' + unknown + suffix for suffix in ('', '/records', '/results')]
    PHASE = 'health_and_anonymous'
    for base in (BASES[0], API):
        for path in ('/api/health', '/internal/ready'):
            status, data, headers = request(base, path)
            require(status == 200 and headers.get('X-Request-ID') and json.loads(data).get('status') in ('ok', 'ready'), 'health_response_invalid')
            PROOF['checks'].append(dict(base=base, path=path, status=status, request_id_present=True))
        for path in (*MIGRATION_READS, *unknown_reads):
            status, data, headers = request(base, path)
            require(status == 401, 'anonymous_migration_not_denied')
            if path.startswith(HISTORY):
                require(no_store(headers) and closed(data, status), 'anonymous_history_error_not_closed_no_store')
            PROOF['checks'].append(dict(base=base, path=path, anonymous_status=status, no_store=no_store(headers)))
    for email, label, role, expected in (
        ('alice@acme.test', 'admin', 'admin', 200),
        ('carol@acme.test', 'member', 'member', 403),
        ('bob@best.test', 'other_org_admin', 'admin', 200),
    ):
        actor = dict(label=label, login_started=False, cookie=None)
        try:
            PHASE = label + '_login'
            actor['login_started'] = True
            status, data, _ = request(API, '/api/session', actor=actor, method='POST', body={'email': email, 'password': credential})
            actor['login_status'] = status
            require(status == 200 and actor['cookie'], 'ordinary_seed_login_failed')
            identity = json.loads(data)
            require(identity['user']['email'] == email and identity['organization']['role'] == role, 'seed_actor_identity_mismatch')
            PHASE = label + '_reads'
            for path in MIGRATION_READS:
                status, data, headers = request(API, path, actor=actor, cookie=actor['cookie'])
                require(status == expected, 'migration_role_status_mismatch')
                if path.startswith(HISTORY):
                    require(no_store(headers), 'history_response_cacheable')
                    if status != 200:
                        require(closed(data, status), 'history_error_not_closed')
                if status == 200:
                    require(no_store(headers), 'migration_response_cacheable')
                    if path.endswith('history-captures'):
                        wanted = {'captures': [], 'next_cursor': None}
                    elif path.endswith('imports'):
                        wanted = {'imports': [], 'next_cursor': None}
                    elif path.endswith('snapshots'):
                        wanted = {'snapshots': [], 'next_cursor': None, 'active_snapshot_id': None, 'latest_completed_snapshot_id': None}
                    else:
                        wanted = {'connection': None, 'active_assessment': None, 'latest_assessment': None, 'latest_report': None}
                    require(json.loads(data) == wanted, 'migration_seed_is_not_empty')
                PROOF['checks'].append(dict(actor=label, path=path, status=status, no_store=no_store(headers), empty=status == 200))
            for path in unknown_reads:
                status, data, headers = request(API, path, actor=actor, cookie=actor['cookie'])
                require(status == (404 if expected == 200 else 403) and no_store(headers) and closed(data, status), 'history_unknown_read_boundary_failed')
                PROOF['checks'].append(dict(actor=label, path=path, status=status, no_store=True, closed_error=True))
            status, data, _ = request(API, '/api/me', actor=actor, cookie=actor['cookie'])
            require(status == 200, 'me_read_failed')
            org = json.loads(data)['organization']
            require(org['workspace_mode'] == 'operational' and org['workspace_revision'] == '1', 'workspace_binding_changed')
            PROOF['checks'].append(dict(actor=label, workspace_mode=org['workspace_mode'], workspace_revision=org['workspace_revision']))
        finally:
            PHASE = label + '_session_cleanup'
            revoke(actor)
    PHASE = 'published_assets'
    index = (ROOT / 'web/dist/index.html').read_bytes()
    for base in (BASES[1], APP):
        status, data, _ = request(base, '/')
        require(status == 200, 'published_index_status')
        if base == BASES[1]:
            require(data == index, 'local_index_mismatch')
        PROOF['checks'].append(dict(base=base, path='/', status=status, sha256=sha(data), matches_local_index=data == index,
                                    qualification='Public edge HTML may inject telemetry; asset equality is checked separately.'))
    selected = set(path.lstrip('/') for path in re.findall(r'(?:src|href)="(/assets/[^\"]+)"', index.decode()))
    selected.update(p for p in web if re.fullmatch(r'assets/MigrationView-.*\.js', p))
    for path in sorted(selected):
        require(path in web, 'entry_asset_missing_from_manifest')
        status, data, _ = request(APP, '/' + path)
        require(status == 200 and sha(data) == web[path], 'published_asset_mismatch')
        PROOF['checks'].append(dict(asset=path, status=status, sha256=sha(data), matches_manifest=True))
    PROOF['status'] = 'passed'


if __name__ == '__main__':
    try:
        run()
    except Exception as error:
        code = str(error) if isinstance(error, AssertionError) and re.fullmatch('[a-z_]+', str(error)) else 'release_smoke_operation_failed'
        PROOF.update(status='failed', failure=dict(phase=PHASE, code=code))
    PROOF['all_created_sessions_revoked'] = bool(PROOF['sessions']) and all(row['revoked'] for row in PROOF['sessions'])
    PROOF['finished_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    text = json.dumps(PROOF, indent=2) + '\n'
    variants = set()
    for secret in SECRETS:
        variants.update((secret, urllib.parse.quote(secret, safe=''), urllib.parse.quote_plus(secret),
                         base64.b64encode(secret.encode()).decode(), json.dumps(secret)[1:-1]))
    if any(value and value in text for value in variants):
        PROOF['status'] = 'failed'
        text = json.dumps(dict(status='failed', failure={'code': 'sensitive_metadata_not_written'},
                               all_created_sessions_revoked=PROOF['all_created_sessions_revoked']), indent=2) + '\n'
    OUT.mkdir(mode=0o700, exist_ok=True)
    output = OUT / 'http-results.json'
    output.write_text(text)
    output.chmod(0o600)
    sums = OUT / 'SHA256SUMS'
    sums.write_text(sha(text.encode()) + '  http-results.json\n')
    sums.chmod(0o600)
    print(json.dumps(dict(status=PROOF['status'], output_directory=str(OUT), result_sha256=sha(text.encode()),
                         checks=len(PROOF['checks']), sessions=len(PROOF['sessions']),
                         all_created_sessions_revoked=PROOF['all_created_sessions_revoked'])))
    sys.exit(0 if PROOF['status'] == 'passed' else 1)
