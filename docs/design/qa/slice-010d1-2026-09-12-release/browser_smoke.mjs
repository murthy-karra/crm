#!/usr/bin/env node
// Prepared only. Root must explicitly authorize execution after the 010d1 rollout.
// No source or business mutation is allowed. Credential values are never logged.
// Owned Chrome profiles are private, and removed after session revocation/closure.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir, chmod, rm } from 'node:fs/promises';
import { join, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire('/Users/karrad/projects/crm/web/package.json');
const { chromium } = require('playwright-core');
const PRIVATE = '/private/tmp/crm-010d1-release-6kg5mx3s';
const APP = 'https://app.tarams.org';
const API = 'https://api.tarams.org';
const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const stamp = new Date().toISOString().replaceAll(':', '-');
const OUT = join(PRIVATE, `browser-smoke-${stamp}`);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const proof = {
  kind: 'public read-only 010d1 deployment smoke', started_at: new Date().toISOString(),
  status: 'not_run', app_origin: APP, api_origin: API, checks: [], screenshots: [], frames: [],
  requests: [], responses: [], telemetry: [], blocked: [], request_failures: [], console_errors: [],
  runtime_errors: [], websockets: [], assets: [], sessions: [], source_or_business_mutations: 0,
};
let phase = 'initialization';
let password = '';
const contexts = new Set();
const bodyPromises = [];
const expectedAssets = new Map();
const secrets = new Set();
const requestIds = new WeakMap();
const requestStarts = new WeakMap();
const documentGenerations = new WeakMap();
let nextRequestId = 0;
function requestId(request) {
  if (!requestIds.has(request)) requestIds.set(request, ++nextRequestId);
  return requestIds.get(request);
}
function requestStart(request) {
  if (!requestStarts.has(request)) {
    let generation = null;
    try { generation = documentGenerations.get(request.frame().page()) ?? 0; } catch { /* Non-page requests stay unqualified. */ }
    requestStarts.set(request, { request_id: requestId(request), document_generation: generation, request_started_phase: phase });
  }
  return requestStarts.get(request);
}
function failureEvidence(error, operationPhase) {
  const message = error instanceof Error ? error.message : 'unknown_error';
  const name = error instanceof Error && ['Error', 'TimeoutError', 'TypeError', 'TargetClosedError'].includes(error.name) ? error.name : 'OtherError';
  return { phase: operationPhase, error_class: name, message_sha256: sha(message), code: /^[a-z0-9_]+$/.test(message) ? message : 'browser_assertion_or_operation_failed' };
}
let teardown = false;
function check(condition, code) { if (!condition) throw new Error(code); }
function safeURL(value) { try { const u = new URL(value); return { origin: u.origin, path: u.pathname }; } catch { return { origin: 'invalid', path: 'invalid' }; } }
function telemetryURL(value, method) {
  const u = new URL(value);
  return (u.origin === 'https://static.cloudflareinsights.com' && method === 'GET' && /^\/beacon\.min\.js(?:\/[^/]+)?$/.test(u.pathname)) ||
    ([APP, 'https://cloudflareinsights.com'].includes(u.origin) && u.pathname === '/cdn-cgi/rum' && ['POST', 'OPTIONS'].includes(method));
}
async function bounded(promise, ms, code) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(code)), ms); })]); }
  finally { clearTimeout(timer); }
}
function responseWait(page, predicate, timeout = 20000) {
  const pending = page.waitForResponse(predicate, { timeout });
  // If the paired click/navigation fails, context closure rejects this waiter.
  // Attach a handler immediately so that cleanup cannot cause an unhandled rejection.
  void pending.catch(() => {});
  return pending;
}
async function metadata(name) {
  const bytes = await readFile(join(PRIVATE, name));
  return { file: name, sha256: sha(bytes), value: JSON.parse(bytes.toString('utf8')) };
}
function readPassword() {
  const source = String.raw`import json, shlex, sys
from pathlib import Path
values=[]
for line in Path(sys.argv[1]).read_text().splitlines():
    line=line.strip()
    if line.startswith('export '): line=line[7:]
    if not line or line.startswith('#') or '=' not in line: continue
    key,raw=line.split('=',1)
    if key.strip()!='CRM_DEV_SEED_PASSWORD': continue
    parsed=shlex.split(raw,comments=True)
    if len(parsed)!=1: raise SystemExit('Invalid seed credential format')
    values.append(parsed[0])
if len(values)!=1 or not values[0]: raise SystemExit('Missing or duplicate seed credential')
sys.stdout.write(json.dumps(values[0]))
`;
  // The subprocess stdout is captured in memory; no credential is placed on argv,
  // exported to a child environment, printed, written to a file, or included in an error.
  return JSON.parse(execFileSync('python3', ['-c', source, '/Users/karrad/projects/crm/.env'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 65536 }));
}
async function ownedSessionRequest(actor, method, path, cookie) {
  check(['GET', 'DELETE'].includes(method) && ['/api/me', '/api/session'].includes(path), 'invalid_cleanup_request');
  check((method === 'GET') === (path === '/api/me'), 'invalid_cleanup_method');
  const response = await fetch(API + path, { method, headers: { Cookie: cookie, Origin: APP }, redirect: 'error', signal: AbortSignal.timeout(15000) });
  const status = response.status;
  await response.body?.cancel();
  proof.requests.push({ actor, phase: 'session_cleanup', method, origin: API, path, classification: 'owned_session_revocation', status });
  return status;
}
async function retainCookie(context, actor) {
  const cookies = (await context.cookies(API)).filter(c => c.name === 'crm_session' && c.value);
  for (const cookie of cookies) secrets.add(cookie.value);
  if (!cookies.length) return null;
  check(cookies.length === 1, 'ambiguous_owned_session_cookie');
  actor.cookie = `${cookies[0].name}=${cookies[0].value}`;
  actor.cookieCaptured = true;
  return actor.cookie;
}
async function revoke(context, actor) {
  try { if (!actor.cookie) await retainCookie(context, actor); } catch { actor.cookie_capture_failed = true; }
  const report = { actor: actor.label, login_started: actor.loginStarted === true, login_response_status: actor.loginStatus ?? null, cookie_captured: !!actor.cookie, ui_logout_status: actor.uiLogoutStatus ?? null, ui_logout_destination: actor.uiLogoutDestination ?? null, delete_status: null, original_cookie_me_status: null, revoked: false };
  if (actor.cookie) {
    // Preserve the exact original Cookie in memory and explicitly replay it after
    // DELETE. A 401 proves server-side revocation even if the browser cleared its jar.
    for (let attempt = 0; attempt < 2 && !report.revoked; attempt++) {
      try {
        report.delete_status = await ownedSessionRequest(actor.label, 'DELETE', '/api/session', actor.cookie);
        report.original_cookie_me_status = await ownedSessionRequest(actor.label, 'GET', '/api/me', actor.cookie);
        report.revoked = report.delete_status === 204 && report.original_cookie_me_status === 401;
      } catch { report.transport_failure = true; }
    }
  } else report.no_confirmed_session_to_revoke = !actor.loginStarted;
  proof.sessions.push(report);
  actor.cookie = null;
  return report.revoked || report.no_confirmed_session_to_revoke === true;
}
async function closeContext(context) {
  try { await bounded(context.close(), 20000, 'context_close_timeout'); contexts.delete(context); }
  catch { proof.context_close_failed = true; }
}
async function instrument(context, actor) {
  context.on('request', request => { requestStart(request); });
  await context.route('**/*', async route => {
    const request = route.request(); const method = request.method(); const u = new URL(request.url());
    const location = safeURL(request.url());
    if (telemetryURL(request.url(), method)) {
      proof.telemetry.push({ actor: actor.label, ...location, method, classification: 'known_cloudflare_insights' });
      await route.continue(); return;
    }
    let classification = 'read_only';
    const ownOrigin = [APP, API].includes(u.origin);
    const read = ['GET', 'HEAD', 'OPTIONS'].includes(method);
    const session = u.origin === API && u.pathname === '/api/session' && ['POST', 'DELETE'].includes(method);
    const realtime = u.origin === API && u.pathname === '/api/realtime/token' && method === 'POST';
    if (!ownOrigin || (!read && !session && !realtime)) {
      proof.blocked.push({ actor: actor.label, ...location, method, reason: !ownOrigin ? 'unexpected_origin' : 'business_or_source_mutation' });
      await route.abort('blockedbyclient'); return;
    }
    if (session) classification = 'session';
    if (realtime) classification = 'read_only_realtime_token_issuance';
    proof.requests.push({ ...requestStart(request), actor: actor.label, phase, ...location, method, classification });
    await route.continue();
  });
  await context.routeWebSocket('**/*', ws => {
    const permitted = ws.url() === 'wss://api.tarams.org/connection/websocket';
    proof.websockets.push({ actor: actor.label, ...safeURL(ws.url()), permitted });
    if (permitted) ws.connectToServer();
    else { proof.blocked.push({ actor: actor.label, ...safeURL(ws.url()), method: 'WEBSOCKET', reason: 'unexpected_websocket' }); ws.close(); }
  });
  context.on('response', response => {
    const url = response.url(); const request = response.request(); const method = request.method(); const status = response.status();
    const location = safeURL(url); const observedPhase = phase;
    if (telemetryURL(url, method)) { proof.telemetry.push({ actor: actor.label, ...location, method, status }); return; }
    const expected401 = location.origin === API && location.path === '/api/me' && status === 401 && (observedPhase.includes('login') || observedPhase.includes('logout') || teardown);
    const expected403 = location.origin === API && location.path === '/api/migrations/fub/' && actor.label === 'carol' && status === 403 && observedPhase === 'carol_direct_api_denial';
    proof.responses.push({ ...requestStart(request), actor: actor.label, phase: observedPhase, ...location, method, status, expected_authorization_denial: expected401 || expected403 });
    if (location.origin === APP && response.request().resourceType() === 'document') {
      const record = { actor: actor.label, ...location, status, response_html_sha256: null };
      proof.frames.push(record);
      bodyPromises.push(bounded(response.body(), 20000, 'document_body_timeout').then(bytes => { record.response_html_sha256 = sha(bytes); }).catch(() => { record.body_unavailable = true; }));
    }
    const relative = location.path.replace(/^\//, '');
    if (location.origin === APP && relative !== 'index.html' && (expectedAssets.has(relative) || ['script', 'stylesheet'].includes(request.resourceType()))) {
      const record = { actor: actor.label, path: relative, status, expected_sha256: expectedAssets.get(relative) ?? null, actual_sha256: null, matches: false };
      proof.assets.push(record);
      bodyPromises.push(bounded(response.body(), 20000, 'asset_body_timeout').then(bytes => { record.actual_sha256 = sha(bytes); record.matches = record.actual_sha256 === record.expected_sha256; }).catch(() => { record.body_unavailable = true; }));
    }
  });
  context.on('requestfailed', request => {
    const entry = { ...requestStart(request), actor: actor.label, phase, ...safeURL(request.url()), method: request.method(), reason: request.failure()?.errorText ?? 'unknown' };
    if (telemetryURL(request.url(), request.method())) { proof.telemetry.push({ ...entry, classification: 'known_telemetry_transport_failure' }); return; }
    const cancellableRead = request.method() === 'GET' || (request.method() === 'POST' && entry.origin === API && entry.path === '/api/realtime/token');
    entry.expected_navigation_cancellation = entry.reason === 'net::ERR_ABORTED' && cancellableRead && (phase.includes('navigation') || phase.includes('reload') || phase.includes('logout') || teardown);
    proof.request_failures.push(entry);
  });
  context.on('page', page => watchPage(page, actor));
  for (const page of context.pages()) watchPage(page, actor);
}
function watchPage(page, actor) {
  documentGenerations.set(page, 0);
  page.on('framenavigated', frame => {
    if (frame === page.mainFrame()) documentGenerations.set(page, (documentGenerations.get(page) ?? 0) + 1);
  });
  page.on('pageerror', () => proof.runtime_errors.push({ actor: actor.label, phase, code: 'pageerror' }));
  page.on('console', message => {
    if (message.type() !== 'error') return;
    const text = message.text(); const location = safeURL(message.location().url || page.url());
    const expectedStatus = /Failed to load resource: the server responded with a status of (401|403)/.exec(text);
    proof.console_errors.push({ actor: actor.label, phase, ...location, message_sha256: sha(text), known_telemetry: telemetryURL(message.location().url || page.url(), 'GET'), expected_status: expectedStatus ? Number(expectedStatus[1]) : null });
  });
}
async function emptyHistory(page, label) {
  const panel = page.getByTestId('history-capture-panel');
  await panel.waitFor({ state: 'visible', timeout: 20000 });
  const parent = panel.getByLabel('People import for historical capture', { exact: true });
  await parent.waitFor({ state: 'visible' });
  check((await parent.locator('option').count()) === 1 && (await parent.inputValue()) === '', `${label}_no_parent`);
  check(await panel.getByRole('button', { name: 'Prepare historical capture', exact: true }).isDisabled(), `${label}_prepare_disabled`);
  check(await panel.getByRole('alert').count() === 0, `${label}_no_panel_error`);
  check(await panel.getByLabel('Historical capture run', { exact: true }).count() === 0, `${label}_no_capture_run`);
  proof.checks.push({ label, no_completed_parent: true, prepare_disabled: true, no_capture_run: true, panel_alerts: 0 });
  return panel;
}
function importsResponse(response) { const u = new URL(response.url()); return u.origin === API && u.pathname === '/api/migrations/fub/imports' && response.request().method() === 'GET'; }
async function verifyImports(response, label) {
  check(response.status() === 200, `${label}_parent_status`);
  const body = await bounded(response.json(), 15000, 'imports_body_timeout');
  check(Array.isArray(body.imports) && body.imports.length === 0 && body.next_cursor === null, `${label}_parent_empty`);
  proof.checks.push({ label, status: response.status(), ...requestStart(response.request()), imports: body.imports.length, next_cursor: null });
}
async function screenshot(page, locator, name) {
  const path = join(OUT, name);
  await locator.screenshot({ path, animations: 'disabled', timeout: 20000 });
  proof.screenshots.push({ name, sha256: sha(await readFile(path)) });
}
async function layout(page, label, width, height) {
  await page.setViewportSize({ width, height });
  await bounded(page.evaluate(() => document.fonts.ready), 20000, 'font_settlement_timeout');
  const panel = await emptyHistory(page, label);
  await panel.scrollIntoViewIfNeeded();
  const metrics = await page.evaluate(() => {
    const panel = document.querySelector('[data-testid="history-capture-panel"]');
    return {
      viewport: innerWidth, document: document.documentElement.scrollWidth, body: document.body.scrollWidth,
      buttons: Array.from(panel.querySelectorAll('button')).map(button => {
        const r = button.getBoundingClientRect();
        return { label: button.textContent.trim(), left: r.left, right: r.right, width: r.width, height: r.height, client_width: button.clientWidth, scroll_width: button.scrollWidth, disabled: button.disabled };
      }),
    };
  });
  check(metrics.document <= width + 1 && metrics.body <= width + 1, `${label}_document_overflow`);
  check(metrics.buttons.every(b => b.left >= -1 && b.right <= width + 1 && b.height >= 39 && b.scroll_width <= b.client_width + 1), `${label}_button_bounds`);
  proof.checks.push({ label: `${label}_layout`, width, height, ...metrics, horizontal_overflow: false });
  const content = await page.content();
  proof.frames.push({ actor: 'alice', stage: label, ...safeURL(page.url()), dom_html_sha256: sha(content), child_frames: page.frames().map(f => safeURL(f.url())) });
  await screenshot(page, panel, `history-empty-${label}.png`);
  const path = join(OUT, `migration-${label}.png`);
  await page.screenshot({ path, fullPage: true, animations: 'disabled', timeout: 20000 });
  proof.screenshots.push({ name: basename(path), sha256: sha(await readFile(path)) });
}
async function login(page, context, actor) {
  phase = `${actor.label}_login_navigation`;
  await page.goto(`${APP}/login`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.getByLabel('Email', { exact: true }).fill(actor.email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  phase = `${actor.label}_login`;
  const responsePromise = responseWait(page, r => r.url() === `${API}/api/session` && r.request().method() === 'POST', 30000);
  actor.loginStarted = true;
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  const response = await responsePromise;
  actor.loginStatus = response.status();
  await retainCookie(context, actor);
  check(actor.loginStatus === 200 && actor.cookie, `${actor.label}_login_response`);
  const identity = await bounded(response.json(), 20000, 'login_body_timeout');
  check(identity.user?.email === actor.email && identity.organization?.role === actor.role, `${actor.label}_identity`);
  await page.waitForURL(u => u.origin === APP && u.pathname !== '/login', { timeout: 30000 });
  proof.checks.push({ label: `${actor.label}_signin`, response_status: actor.loginStatus, actual_role: identity.organization.role, workspace_mode: identity.organization.workspace_mode, destination: safeURL(page.url()) });
}
async function logout(page, actor) {
  phase = `${actor.label}_logout`;
  const responsePromise = responseWait(page, r => r.url() === `${API}/api/session` && r.request().method() === 'DELETE');
  await page.getByRole('button', { name: 'Log out', exact: true }).click();
  const response = await responsePromise;
  actor.uiLogoutStatus = response.status();
  check(actor.uiLogoutStatus === 204, `${actor.label}_logout_status`);
  await page.waitForURL(u => u.origin === APP && u.pathname === '/login', { timeout: 20000 });
  actor.uiLogoutDestination = '/login';
}
async function runActor(actor, run) {
  const profile = join(OUT, `owned-headless-chrome-${actor.label}`);
  await mkdir(profile, { recursive: true, mode: 0o700 });
  const context = await chromium.launchPersistentContext(profile, { executablePath: CHROME, headless: true, viewport: { width: 1360, height: 950 }, serviceWorkers: 'block', acceptDownloads: false, args: ['--disable-background-networking', '--disable-component-update', '--no-first-run', '--no-default-browser-check'] });
  contexts.add(context);
  context.setDefaultTimeout(15000);
  context.setDefaultNavigationTimeout(30000);
  let failure;
  try {
    await instrument(context, actor);
    const page = context.pages()[0] ?? await context.newPage();
    await login(page, context, actor);
    await run(page, actor);
    await logout(page, actor);
  } catch (error) { failure = error; proof.failure ??= failureEvidence(error, phase); }
  finally {
    phase = `${actor.label}_logout_cleanup`;
    const clean = await revoke(context, actor);
    await closeContext(context);
    if (!contexts.has(context)) {
      try { await rm(profile, { recursive: true }); proof.sessions.at(-1).owned_profile_removed = true; }
      catch { proof.sessions.at(-1).owned_profile_removed = false; if (!failure) failure = new Error('owned_profile_cleanup_failed'); }
    }
    if (!clean && !failure) failure = new Error(`${actor.label}_revocation_not_proved`);
  }
  if (failure) throw failure;
}
try {
  check(process.argv.length === 3 && process.argv[2] === '--after-rollout-authorized', 'explicit_after_rollout_authorization_required');
  await mkdir(OUT, { recursive: false, mode: 0o700 });
  await chmod(OUT, 0o700);
  phase = 'release_metadata';
  const [binary, web, git, deployed] = await Promise.all(['build-sha256.json', 'web-build-sha256.json', 'git-integration.json', 'deployed.json'].map(metadata));
  check(/^[a-f0-9]{40}$/.test(git.value.merge) && /^[a-f0-9]{40}$/.test(git.value.implementation), 'release_commit_metadata');
  check(/^[a-f0-9]{64}$/.test(git.value.source_manifest_sha256), 'release_source_metadata');
  check(['crm-api', 'crm-admin', 'migrate'].every(key => /^[a-f0-9]{64}$/.test(binary.value[key])), 'release_binary_metadata');
  for (const [path, digest] of Object.entries(web.value)) {
    check(!path.startsWith('/') && !path.split('/').includes('..') && /^[a-f0-9]{64}$/.test(digest), 'release_asset_metadata');
    expectedAssets.set(path, digest);
  }
  check(expectedAssets.has('index.html') && [...expectedAssets.keys()].some(p => /^assets\/MigrationView-.*\.js$/.test(p)), 'release_migration_asset_metadata');
  const sourceHash = deployed.value.source_manifest_sha256 ?? git.value.source_manifest_sha256;
  check(/^[a-f0-9]{64}$/.test(sourceHash), 'deployed_source_metadata');
  check(/^[a-f0-9]{40}$/.test(deployed.value.revision ?? ''), 'deployed_revision_metadata');
  const releaseMetadata = [binary, web, git, deployed];
  if (deployed.value.source_manifest_sha256 !== undefined) {
    const source = await metadata('release-source.json');
    check(Array.isArray(source.value) && source.value.length === 1037 && source.value.every(entry => typeof entry.path === 'string' && /^[a-f0-9]{64}$/.test(entry.sha256)), 'deployed_source_manifest_shape');
    check(source.sha256 === sourceHash, 'deployed_source_manifest_digest');
    releaseMetadata.push(source);
  }
  proof.release = {
    deployed_revision: deployed.value.revision,
    deployed_merge: deployed.value.merge ?? git.value.merge,
    source_manifest_sha256: sourceHash,
    source_manifest_hash_basis: deployed.value.source_manifest_sha256 !== undefined ? 'exact_release_source_file_bytes' : 'historical_compact_json',
    historical_original_merge: git.value.merge,
    historical_original_implementation: git.value.implementation,
    historical_original_source_manifest_sha256: git.value.source_manifest_sha256,
    web_source_merge: git.value.merge,
    binary_sha256: Object.fromEntries(['crm-api', 'crm-admin', 'migrate'].map(key => [key, binary.value[key]])),
    expected_web_assets: expectedAssets.size,
    local_index_sha256: expectedAssets.get('index.html'),
    metadata: releaseMetadata.map(x => ({ file: x.file, sha256: x.sha256 })),
  };
  proof.helper_sha256 = sha(await readFile(fileURLToPath(import.meta.url)));
  password = readPassword(); secrets.add(password);
  proof.status = 'running';
  await runActor({ label: 'alice', email: 'alice@acme.test', role: 'admin' }, async page => {
    phase = 'alice_migration_navigation';
    const loaded = responseWait(page, importsResponse);
    await page.getByRole('link', { name: 'Migration', exact: true }).click();
    await page.waitForURL(`${APP}/manage/migration`);
    await verifyImports(await loaded, 'initial_parent_read');
    const panel = await emptyHistory(page, 'initial');
    phase = 'alice_explicit_refresh';
    const refreshBoundary = nextRequestId;
    const refreshGeneration = documentGenerations.get(page);
    const refreshed = responseWait(page, response => importsResponse(response) && requestStart(response.request()).request_id > refreshBoundary && requestStart(response.request()).document_generation === refreshGeneration);
    await panel.getByRole('button', { name: 'Refresh historical capture status', exact: true }).click();
    await verifyImports(await refreshed, 'explicit_history_refresh');
    await emptyHistory(page, 'after_refresh');
    phase = 'alice_reload';
    const previousGeneration = documentGenerations.get(page) ?? 0;
    const requiredGeneration = previousGeneration + 1;
    const reloaded = responseWait(page, response => importsResponse(response) && requestStart(response.request()).document_generation >= requiredGeneration);
    const reloadDocument = await page.reload({ waitUntil: 'domcontentloaded' });
    check(reloadDocument?.status() === 200 && (documentGenerations.get(page) ?? 0) >= requiredGeneration, 'reload_document_committed');
    await verifyImports(await reloaded, 'history_reload');
    proof.checks.push({ label: 'reload_document_boundary', previous_generation: previousGeneration, required_generation: requiredGeneration, committed_generation: documentGenerations.get(page), document_request_id: requestId(reloadDocument.request()) });
    await emptyHistory(page, 'after_reload');
    phase = 'alice_layout';
    await layout(page, 'desktop', 1360, 950);
    await layout(page, '390px', 390, 844);
  });
  await runActor({ label: 'carol', email: 'carol@acme.test', role: 'member' }, async page => {
    phase = 'carol_navigation_denial';
    check(await page.getByRole('link', { name: 'Migration', exact: true }).count() === 0, 'member_no_migration_navigation');
    await page.goto(`${APP}/manage/migration`, { waitUntil: 'domcontentloaded' });
    await page.waitForURL(u => u.origin === APP && ['/today', '/workspace-review'].includes(u.pathname), { timeout: 20000 });
    check(await page.getByTestId('history-capture-panel').count() === 0, 'member_no_history_panel');
    check(await page.getByRole('link', { name: 'Migration', exact: true }).count() === 0, 'member_no_navigation_after_direct_route');
    proof.checks.push({ label: 'member_route_denial', migration_nav_visible: false, history_panel_visible: false, destination: safeURL(page.url()) });
    phase = 'carol_direct_api_denial';
    const status = await page.evaluate(async api => {
      const response = await fetch(`${api}/api/migrations/fub/`, { credentials: 'include', redirect: 'error', signal: AbortSignal.timeout(15000) });
      await response.body?.cancel(); return response.status;
    }, API);
    check(status === 403, 'member_migration_api_denial');
    proof.checks.push({ label: 'member_migration_api_denial', status });
  });
  proof.status = 'checks_completed';
} catch (error) {
  proof.status = 'failed';
  proof.failure ??= failureEvidence(error, phase);
} finally {
  teardown = true;
  phase = 'final_context_cleanup';
  for (const context of contexts) await closeContext(context);
  // Close all browser contexts before joining response-body work. In particular,
  // never wait for a pending public frame body while its page remains open.
  try { await bounded(Promise.allSettled(bodyPromises), 25000, 'pending_body_timeout'); } catch { proof.pending_body_timeout = true; }
  const expectedConsole = entry => entry.known_telemetry || (entry.expected_status !== null && proof.responses.some(response => response.actor === entry.actor && response.origin === entry.origin && response.path === entry.path && response.status === entry.expected_status && response.expected_authorization_denial));
  for (const entry of proof.console_errors) entry.classified_expected = expectedConsole(entry);
  proof.unexpected_http = proof.responses.filter(r => r.status >= 400 && !r.expected_authorization_denial);
  proof.unexpected_console_errors = proof.console_errors.filter(e => !e.classified_expected);
  for (const entry of proof.request_failures) {
    // HTTP 204 is already a complete body-free application success. Preserve a
    // Chromium abort annotation, but qualify only the same exact browser request
    // after successful UI logout and independent original-cookie revocation proof.
    // A DELETE failure without every one of these facts remains unexpected.
    const sameRequest204 = proof.responses.some(response => response.request_id === entry.request_id && response.actor === entry.actor && response.origin === API && response.path === '/api/session' && response.method === 'DELETE' && response.status === 204);
    const revokedSession = proof.sessions.some(session => session.actor === entry.actor && session.login_response_status === 200 && session.cookie_captured && session.ui_logout_status === 204 && session.ui_logout_destination === '/login' && session.delete_status === 204 && session.original_cookie_me_status === 401 && session.revoked);
    entry.expected_completed_logout_cancellation = entry.reason === 'net::ERR_ABORTED' && entry.origin === API && entry.path === '/api/session' && entry.method === 'DELETE' && entry.phase === `${entry.actor}_logout` && sameRequest204 && revokedSession;
  }
  proof.unexpected_request_failures = proof.request_failures.filter(e => !e.expected_navigation_cancellation && !e.expected_completed_logout_cancellation);
  proof.created_session_count = proof.sessions.filter(s => s.cookie_captured || s.login_response_status === 200).length;
  proof.all_created_sessions_revoked = proof.sessions.every(s => s.revoked || s.no_confirmed_session_to_revoke === true);
  proof.both_actors_completed = proof.sessions.length === 2 && proof.sessions.every(s => s.ui_logout_status === 204 && s.ui_logout_destination === '/login');
  proof.session_revocation_proved = proof.both_actors_completed && proof.all_created_sessions_revoked;
  proof.observed_public_assets_match = proof.assets.length > 0 && proof.assets.every(a => a.status === 200 && a.matches);
  proof.migration_asset_observed = proof.assets.some(a => /^assets\/MigrationView-.*\.js$/.test(a.path) && a.matches);
  proof.frame_hashes_complete = proof.frames.length > 0 && proof.frames.every(f => !!(f.response_html_sha256 || f.dom_html_sha256));
  proof.public_html_qualification = 'Actual public frame HTML and DOM hashes are recorded. Edge-injected HTML is not assumed byte-identical to the local index.html; every observed manifest asset is compared byte-for-byte.';
  if (proof.status === 'checks_completed' && proof.session_revocation_proved && proof.blocked.length === 0 && proof.runtime_errors.length === 0 && proof.unexpected_http.length === 0 && proof.unexpected_console_errors.length === 0 && proof.unexpected_request_failures.length === 0 && proof.observed_public_assets_match && proof.migration_asset_observed && proof.frame_hashes_complete && !proof.context_close_failed && !proof.pending_body_timeout) proof.status = 'passed';
  else if (proof.status !== 'failed') { proof.status = 'failed'; proof.failure = { phase: 'final_assertions', code: 'public_smoke_evidence_incomplete_or_unexpected_activity' }; }
  proof.finished_at = new Date().toISOString();
  password = '';
  // Guard the serialized metadata against every credential/cookie we handled.
  let serialized = JSON.stringify(proof, null, 2) + '\n';
  const variants = new Set();
  for (const secret of secrets) for (const value of [secret, encodeURIComponent(secret), Buffer.from(secret).toString('base64'), JSON.stringify(secret).slice(1, -1)]) if (value) variants.add(value);
  const secretMatch = [...variants].some(value => serialized.includes(value));
  if (secretMatch) {
    serialized = JSON.stringify({ status: 'failed', failure: { phase: 'output_scan', code: 'sensitive_metadata_not_written' }, session_revocation_proved: proof.session_revocation_proved }, null, 2) + '\n';
    proof.status = 'failed';
  }
  try {
    await mkdir(OUT, { recursive: true, mode: 0o700 });
    const resultFile = join(OUT, 'browser-results.json');
    await writeFile(resultFile, serialized, { mode: 0o600 });
    const inventory = [{ file: 'browser-results.json', sha256: sha(serialized) }, ...proof.screenshots.map(s => ({ file: s.name, sha256: s.sha256 }))];
    await writeFile(join(OUT, 'SHA256SUMS'), inventory.map(x => `${x.sha256}  ${x.file}\n`).join(''), { mode: 0o600 });
    process.stdout.write(JSON.stringify({ status: proof.status, output_directory: OUT, result_sha256: sha(serialized), session_revocation_proved: proof.session_revocation_proved, all_created_sessions_revoked: proof.all_created_sessions_revoked, created_session_count: proof.created_session_count, both_actors_completed: proof.both_actors_completed }) + '\n');
  } catch { process.stdout.write('{"status":"failed","code":"evidence_write_failed"}\n'); proof.status = 'failed'; }
  process.exitCode = proof.status === 'passed' ? 0 : 1;
}
