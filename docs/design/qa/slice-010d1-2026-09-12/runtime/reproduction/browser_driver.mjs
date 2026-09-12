// Owned synthetic QA only under D-070. Runtime requires the isolated API/Web;
// invoke a declared phase after the coordinator receives the DB/runtime handoff.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { chromium, request } from '__CRM_WORKTREE__/web/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core/index.mjs'

const QA = '__PRIVATE_QA_ROOT__'
const WEB = 'http://127.0.0.1:15173'
const MIGRATION = '/manage/migration'
const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const BASE = '/migrations/fub/history-captures'
const args = Object.fromEntries(process.argv.slice(2).map(arg => { const [k, ...rest] = arg.replace(/^--/, '').split('='); return [k, rest.join('=') || true] }))
const phases = ['prepare', 'confirm-lost', 'progress', 'complete', 'cancel', 'overlap', 'denied', 'retry', 'replacement', 'budget-queue', 'budget-pause', 'budget-resume', 'access', 'disconnect', 'budget-layout']
if (args.describe) {
  console.log(JSON.stringify({ phases, cases: ['complete', 'cancel', 'budget'], fixture: 'constructed-010d1', runtime_requires: 'isolated API13010 and production Web15173' }, null, 2))
  process.exit(0)
}
assert.equal(args['authorize-runtime'], true, 'Runtime has not been explicitly authorized')
assert.ok(phases.includes(args.phase), 'Select one declared phase')
assert.ok(['complete', 'cancel', 'budget'].includes(args.case), 'Select one synthetic case')
const phase = args.phase
const caseName = args.case
const people = JSON.parse(fs.readFileSync(path.join(QA, 'people-context.json'), 'utf8'))
const org = people.organizations.find(row => row.case === caseName)
assert.ok(org?.id && org.parent_id && org.people['101'] && org.people['102'])
const PASSWORD = execFileSync('/usr/bin/python3', ['-c', 'import pathlib,shlex; p=pathlib.Path("__PRIVATE_QA_ROOT__/qa.env"); print(next(shlex.split(s.partition("=")[2])[0] for s in p.read_text().splitlines() if s.startswith("CRM_DEV_SEED_PASSWORD=")))'], { encoding: 'utf8' }).trim()
assert.ok(PASSWORD, 'Missing private seed credential')
const statePath = path.join(QA, 'browser-checkpoints.json')
const state = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')) : { fixture: 'synthetic-010d1', cases: {}, attempts: [] }
const checkpoint = state.cases[caseName] ??= { parent_id: org.parent_id, snapshot_id: org.snapshot_id, phases: {} }
assert.equal(checkpoint.parent_id, org.parent_id)
if (checkpoint.phases[phase] && !args.repeat) {
  console.log(JSON.stringify({ phase, case: caseName, checkpoint: 'already_completed', attempt: checkpoint.phases[phase].attempt }))
  process.exit(0)
}
const screens = path.join(QA, 'screenshots'); fs.mkdirSync(screens, { recursive: true, mode: 0o700 })
const attemptId = `${Date.now()}-${caseName}-${phase}`
const attempt = { id: attemptId, case: caseName, phase, started_at: new Date().toISOString(), outcome: 'running', checks: [], screenshots: [], network: [], browser_errors: [], external_requests: [] }
state.attempts.push(attempt)
function atomic(file, value) { const tmp = `${file}.${process.pid}.tmp`; fs.writeFileSync(tmp, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 }); fs.renameSync(tmp, file) }
function save() { atomic(statePath, state) }
function check(name, details = {}) { attempt.checks.push({ name, ...details }); save(); console.log(JSON.stringify({ phase, case: caseName, checkpoint: name })) }
function hash(value) { return crypto.createHash('sha256').update(value).digest('hex') }
function readJson(name) { return JSON.parse(fs.readFileSync(path.join(QA, name), 'utf8')) }
function control(patch) { atomic(path.join(QA, 'control.json'), { ...readJson('control.json'), ...patch }) }
function units() { return Number(readJson('control.history-stats.json').completed_history_units) }
function safeEndpoint(url) {
  const u = new URL(url)
  return { origin: u.origin, route_sha256: hash(u.pathname), query_keys: [...u.searchParams.keys()].sort() }
}
const wait = ms => new Promise(resolve => setTimeout(resolve, ms))
async function eventually(read, predicate, label, milliseconds = 240_000) {
  const end = Date.now() + milliseconds
  while (Date.now() < end) { const value = await read(); if (predicate(value)) return value; await wait(450) }
  throw new Error(`Timed out waiting for ${label}`)
}
let context, page, closing = false
const lastResponses = new Map()
const responseWork = new Set()
function captureResponse(response) {
  const task = (async () => {
    if (!response.url().startsWith(`${WEB}/api/`)) return
    const bytes = await response.body().catch(() => Buffer.alloc(0))
    const u = new URL(response.url())
    attempt.network.push({ method: response.request().method(), status: response.status(), ...safeEndpoint(response.url()), bytes: bytes.length })
    // Business payloads stay only in bounded transient memory. Never persist
    // cookies, response bodies, source fields, console text or URLs/cursors.
    if (response.status() === 200 && bytes.length <= 600_000) {
      try { lastResponses.set(u.pathname + u.search, JSON.parse(bytes.toString('utf8'))); if (lastResponses.size > 100) lastResponses.delete(lastResponses.keys().next().value) } catch {}
    }
  })().finally(() => responseWork.delete(task))
  responseWork.add(task)
}
async function start() {
  const profile = path.join(QA, `browser-profile-${caseName}${args['fresh-profile'] ? `-${attemptId}` : ''}`)
  context = await chromium.launchPersistentContext(profile, { executablePath: CHROME, headless: true, viewport: { width: 1440, height: 1000 }, serviceWorkers: 'block', args: ['--disable-background-networking', '--disable-component-update', '--no-default-browser-check', '--disable-sync'] })
  page = context.pages()[0] ?? await context.newPage()
  page.setDefaultTimeout(30_000)
  await context.route('**/*', async route => {
    const url = new URL(route.request().url())
    if (url.origin === WEB || ['data:', 'about:', 'blob:'].includes(url.protocol)) return route.continue()
    attempt.external_requests.push({ ...safeEndpoint(url.href), method: route.request().method() }); save()
    await route.abort('blockedbyclient')
  })
  page.on('pageerror', error => { attempt.browser_errors.push({ kind: 'pageerror', message_sha256: hash(error.message) }) })
  page.on('console', message => {
    if (message.type() !== 'error') return
    const text = message.text()
    const url = message.location().url
    const initialSession401 = /Failed to load resource: the server responded with a status of 401/.test(text) && url === `${WEB}/api/me`
    const cleanupAbort = closing && /net::ERR_ABORTED/.test(text)
    const expected = cleanupAbort || initialSession401 || /Failed to load resource: the server responded with a status of (401|403|404)/.test(text) && ['access','disconnect'].includes(phase)
      || /net::ERR_FAILED/.test(text) && phase === 'confirm-lost'
    attempt.browser_errors.push({ kind: 'console', classification: expected ? 'expected_http_failure' : 'unclassified', message_sha256: hash(text) })
  })
  page.on('response', captureResponse)
  const files = []
  function walk(dir) { for (const item of fs.readdirSync(dir, { withFileTypes: true })) { const next = path.join(dir, item.name); if (item.isDirectory()) walk(next); else files.push({ file: path.relative('__CRM_WORKTREE__/web/dist', next), sha256: hash(fs.readFileSync(next)) }) } }
  walk('__CRM_WORKTREE__/web/dist')
  attempt.build = { manifest_sha256: hash(JSON.stringify(files.sort((a, b) => a.file.localeCompare(b.file)))), files }
  attempt.browser = { version: context.browser()?.version() ?? 'persistent-chromium', executable_sha256: hash(fs.readFileSync(CHROME)) }
  attempt.driver_sha256 = hash(fs.readFileSync(new URL(import.meta.url)))
  attempt.source_epoch = readJson('control.json').epoch
  attempt.history_counter_before = readJson('control.stats.json').history_reader_calls
  attempt.source_counter_before = readJson('control.stats.json').source_reader_calls
  attempt.publication_counter_before = readJson('control.delivery-stats.json').publications
  save()
}
async function api(method, url, body, expected = [200]) {
  assert.ok(url.startsWith('/'), 'Only API relative routes')
  const result = await page.evaluate(async ({ method, url, body }) => {
    const response = await fetch(`/api${url}`, { method, credentials: 'include', cache: 'no-store', headers: body === undefined ? undefined : { 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) })
    const text = await response.text()
    return { status: response.status, data: text ? JSON.parse(text) : null, bytes: new TextEncoder().encode(text).length }
  }, { method, url, body })
  assert.ok(expected.includes(result.status), `Unexpected API status ${result.status} for ${method} scoped route`)
  return result
}
const read = async url => (await api('GET', url)).data
async function login(email = org.admin_email) {
  await page.goto(WEB + MIGRATION)
  const session = await api('GET', '/me', undefined, [200,401])
  if (session.status === 200 && session.data.user?.email !== email) await api('DELETE', '/session', undefined, [200,204])
  await page.goto(`${WEB}/login?redirect=${encodeURIComponent(MIGRATION)}`)
  await page.waitForFunction(() => !location.pathname.startsWith('/login') || [...document.querySelectorAll('button')].some(b => b.textContent.trim() === 'Sign in'))
  const form = page.getByRole('button', { name: 'Sign in', exact: true })
  if (await form.isVisible().catch(() => false)) {
    await page.getByLabel('Email', { exact: true }).fill(email)
    await page.getByLabel('Password', { exact: true }).fill(PASSWORD)
    const reply = page.waitForResponse(r => new URL(r.url()).pathname === '/api/session' && r.request().method() === 'POST')
    await form.click(); assert.equal((await reply).status(), 200)
    await page.waitForURL(url => !url.pathname.startsWith('/login'))
  }
  const me = await eventually(() => read('/me'), value => value.user?.email === email, 'correct browser actor', 20_000)
  assert.equal(me.organization.id, org.id)
  return me
}
async function screenshot(label, target) {
  const safe = `${caseName}-${label}`.replace(/[^a-zA-Z0-9_-]/g, '_')
  if (target) { await target.evaluate(element => element.scrollIntoView({ block: 'start', inline: 'nearest' })); await wait(80) }
  const metric = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth }))
  assert.ok(metric.scroll <= metric.width + 1, `Horizontal overflow at ${label}`)
  const file = path.join(screens, `${attemptId}-${safe}.png`)
  await page.screenshot({ path: file, fullPage: false })
  attempt.screenshots.push({ file, width: metric.width, horizontal_overflow: false }); save()
}
async function narrowShot(label, target) {
  await page.setViewportSize({ width: 390, height: 844 })
  await screenshot(`${label}-390`, target)
  if (target && await target.getAttribute('data-testid') === 'history-capture-records') {
    const scroller = target.locator('div.overflow-x-auto').first()
    if (await scroller.count() && await scroller.evaluate(el => el.scrollWidth > el.clientWidth + 1)) {
      await scroller.evaluate(el => { el.scrollLeft = el.scrollWidth })
      assert.ok(await scroller.evaluate(el => {
        const edge = el.getBoundingClientRect()
        const button = el.querySelector('tbody button')?.getBoundingClientRect()
        return button && button.left >= edge.left - 1 && button.right <= edge.right + 1
      }), 'Narrow table review control must be reachable by its own horizontal scroll')
      await screenshot(`${label}-390-review-controls`, target)
      await scroller.evaluate(el => { el.scrollLeft = 0 })
    }
  }
  await page.setViewportSize({ width: 1440, height: 1000 })
}
async function noOperationalActions() {
  for (const id of ['log-contact', 'call-button', 'add-tag-button', 'remove-person-tag', 'task-add-form', 'note-composer', 'edit-note', 'edit-task', 'complete-task']) assert.equal(await page.getByTestId(id).count(), 0)
  assert.equal(await page.locator('a[href^="mailto:"]').count(), 0)
  assert.equal(attempt.external_requests.length, 0)
}

async function migration() {
  await page.goto(WEB + MIGRATION)
  const panel = page.getByTestId('history-capture-panel')
  await panel.waitFor()
  const select = panel.getByLabel('People import for historical capture', { exact: true })
  await eventually(() => select.locator('option').evaluateAll(options => options.map(o => o.value)), values => values.includes(org.parent_id), 'completed People parent option')
  await select.selectOption(org.parent_id)
  if (checkpoint.capture_id) {
    const captures = panel.getByLabel('Historical capture run', { exact: true })
    await eventually(() => captures.locator('option').evaluateAll(options => options.map(o => o.value)), values => values.includes(checkpoint.capture_id), 'named capture option')
    await captures.selectOption(checkpoint.capture_id)
  }
  return panel
}
async function capture() {
  assert.ok(checkpoint.capture_id, 'A named capture must be prepared')
  const value = await read(`${BASE}/${checkpoint.capture_id}`)
  assert.equal(value.parent_import_id, org.parent_id)
  assert.equal(value.snapshot_id, org.snapshot_id)
  assert.equal(value.source_account_id, '101')
  assert.equal(value.profile_version, 'fub-history-v1')
  return value
}
async function refresh(panel) {
  const button = panel.getByRole('button', { name: 'Refresh historical capture status', exact: true })
  if (await button.count()) { await button.click(); await wait(200) }
}
async function grantOne() {
  const target = units() + 1
  control({ history_units: target })
  await eventually(async () => units(), value => value >= target, 'one history worker unit', 25_000)
  control({ history_units: 0 })
}
async function prepared(panel) {
  const before = readJson('control.stats.json').source_reader_calls
  const response = page.waitForResponse(r => new URL(r.url()).pathname === `/api${BASE}` && r.request().method() === 'POST')
  await panel.getByRole('button', { name: 'Prepare historical capture', exact: true }).click()
  const reply = await response
  assert.equal(reply.status(), 201)
  const receipt = await reply.json()
  assert.deepEqual(Object.keys(receipt).sort(), ['capture_id', 'revision', 'state'])
  checkpoint.capture_id = receipt.capture_id; checkpoint.proposal_receipt = receipt; save()
  const value = await capture()
  assert.equal(value.state, 'proposed'); assert.equal(value.capture_sequence, '0')
  assert.equal(readJson('control.stats.json').source_reader_calls, before)
  check('database-only-proposal', { capture_id: receipt.capture_id, source_calls: 0 })
  await eventually(() => panel.getByRole('checkbox').count(), count => count >= 3, 'prepared proposal controls', 20_000)
  if (phase !== 'prepare') { await screenshot('proposed-header', panel); await narrowShot('proposed-header', panel) }
  return value
}
async function confirmDialog(panel) {
  const boxes = panel.getByRole('checkbox')
  await eventually(() => boxes.count(), count => count >= 3, 'proposal acknowledgements', 20_000)
  assert.ok(await boxes.count() >= 3)
  for (const box of await boxes.all()) await box.check()
  await panel.getByRole('button', { name: 'Review historical capture confirmation', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await dialog.waitFor()
  assert.match(await dialog.innerText(), /retained|capture/i)
  return dialog
}
async function confirmed(panel, loseResponse = false) {
  const url = `${WEB}/api${BASE}/${checkpoint.capture_id}/confirm`
  let committed, submitted
  if (loseResponse) {
    await page.route(url, async route => {
      submitted = route.request().postDataJSON()
      const response = await route.fetch()
      assert.equal(response.status(), 202)
      committed = await response.json()
      await route.abort('failed')
    }, { times: 1 })
  }
  const dialog = await confirmDialog(panel)
  await screenshot('confirmation', dialog); await narrowShot('confirmation', dialog)
  const response = loseResponse ? null : page.waitForResponse(r => r.url() === url && r.request().method() === 'POST')
  await dialog.getByRole('button', { name: 'Start historical capture', exact: true }).click()
  if (loseResponse) {
    await eventually(async () => committed, Boolean, 'committed dropped confirmation')
    const before = await capture()
    const replay = page.waitForResponse(r => r.url() === url && r.request().method() === 'POST')
    await panel.getByRole('button', { name: 'Retry the same historical capture request', exact: true }).click()
    const reply = await replay
    assert.equal(reply.status(), 202)
    assert.deepEqual(reply.request().postDataJSON(), submitted)
    assert.deepEqual(await reply.json(), committed)
    const after = await capture()
    assert.equal(after.retained_bytes, before.retained_bytes)
    assert.equal(after.revision, before.revision)
    check('lost-confirmation-exact-replay', { request_digest: hash(JSON.stringify(submitted)), receipt_digest: hash(JSON.stringify(committed)) })
  } else {
    const reply = await response
    assert.equal(reply.status(), 202)
  }
  const value = await capture()
  assert.equal(value.state, 'queued')
  assert.equal(value.capture_sequence, '0')
  checkpoint.confirmed_source_calls = readJson('control.stats.json').source_reader_calls
  checkpoint.confirmed_history_calls = readJson('control.stats.json').history_reader_calls
  checkpoint.confirmed_publications = readJson('control.delivery-stats.json').publications
  save()
  check('confirmed-before-source', { state: value.state, sequence: value.capture_sequence })
}
async function walkRecords(extra = '') {
  let cursor = null, sequence = null, pages = 0
  const rows = []
  do {
    const value = await read(`${BASE}/${checkpoint.capture_id}/records?limit=50${extra}${cursor ? '&cursor=' + encodeURIComponent(cursor) : ''}`)
    assert.ok(value.records.length <= 50)
    assert.ok(Buffer.byteLength(JSON.stringify(value)) <= 512 * 1024)
    if (sequence === null) sequence = value.capture_sequence
    assert.equal(value.capture_sequence, sequence)
    for (const record of value.records) {
      assert.ok(Buffer.byteLength(JSON.stringify(record)) <= 8192)
      for (const forbidden of ['body', 'message', 'subject', 'html', 'recordingUrl', 'mediaUrl', 'phone', 'email']) assert.equal(forbidden in record, false)
      assert.equal(record.integrity_status, 'verified_projection')
      rows.push(record)
    }
    cursor = value.next_cursor; pages++
    assert.ok(pages <= 30, 'Synthetic 403-row traversal must remain bounded')
  } while (cursor)
  assert.equal(new Set(rows.map(v => v.id)).size, rows.length)
  return { rows, pages, sequence }
}
async function complete(panel) {
  control({ mode: 'normal', history_units: null })
  const value = await eventually(capture, v => v.state === 'completed_with_gaps', 'three enumerated streams')
  control({ history_units: 0 })
  assert.equal(value.reserved_bytes, '0')
  assert.equal(value.streams.length, 3)
  for (const [family, count] of [['events','201'],['calls','101'],['text_messages','101']]) {
    const stream = value.streams.find(v => v.family === family)
    assert.equal(stream.state, 'enumerated')
    assert.equal(stream.occurrences, count); assert.equal(stream.unique_ids, count)
    assert.equal(stream.invalid_occurrences, '0'); assert.equal(stream.api_inaccessible_count, null)
    assert.equal(stream.enumeration_is_complete_account_history, false)
  }
  const all = await walkRecords()
  assert.equal(all.rows.length, 403)
  assert.equal(readJson('control.stats.json').history_reader_calls - checkpoint.confirmed_history_calls, 7)
  assert.equal(readJson('control.delivery-stats.json').publications, checkpoint.confirmed_publications)
  checkpoint.complete_detail = value
  checkpoint.record_ids_sha256 = hash(all.rows.map(v => v.id).join(',')); save()
  await refresh(panel)
  await screenshot('completed-with-gaps', panel); await narrowShot('completed-with-gaps', panel)
  const records = panel.getByTestId('history-capture-records')
  const unlinkedResponse = page.waitForResponse(r => new URL(r.url()).pathname === `/api${BASE}/${checkpoint.capture_id}/records` && new URL(r.url()).searchParams.get('disposition') === 'no_parent_identity' && r.status() === 200)
  await records.getByLabel('Historical Person link status', { exact: true }).selectOption('no_parent_identity')
  const unlinked = await (await unlinkedResponse).json()
  assert.ok(unlinked.records.length > 0 && unlinked.records.every(row => row.disposition === 'no_parent_identity'))
  await eventually(() => records.locator('tbody tr').count(), count => count === unlinked.records.length, 'unlinked source observation rows')
  for (const row of await records.locator('tbody tr').all()) assert.match(await row.innerText(), /No parent identity/)
  await screenshot('unlinked-observations', records); await narrowShot('unlinked-observations', records)
  await page.reload(); await migration()
  assert.equal((await capture()).state, 'completed_with_gaps')
  await page.goto(`${WEB}/people/${org.people['101']}`)
  await page.getByTestId('person-activity-review').waitFor()
  const core = await read(`/people/${org.people['101']}/migration-review`)
  assert.equal('history' in core, false)
  await noOperationalActions(); await screenshot('person-review-remains-bounded')
  await migration()
  check('all-streams-enumerated-with-disclosed-gaps', { rows: 403, pages: all.pages, sequence: all.sequence, collection_calls: 7, business_publications: 0 })
}
async function progress(panel) {
  assert.equal((await capture()).capture_sequence, '0')
  await grantOne(); await grantOne()
  const value = await capture()
  assert.equal(value.capture_sequence, '2'); assert.equal(value.streams[0].occurrences, '100')
  const first = await read(`${BASE}/${checkpoint.capture_id}/records?limit=50`)
  assert.equal(first.capture_sequence, '2'); assert.equal(first.records.length, 50)
  assert.ok(first.next_cursor)
  await refresh(panel)
  const records = panel.getByTestId('history-capture-records')
  await records.getByRole('button', { name: 'Refresh captured observations', exact: true }).click()
  await records.getByRole('button', { name: 'More captured observations', exact: true }).click()
  await wait(500)
  const secondBefore = await read(`${BASE}/${checkpoint.capture_id}/records?limit=50&cursor=${encodeURIComponent(first.next_cursor)}`)
  await grantOne()
  const secondAfter = await read(`${BASE}/${checkpoint.capture_id}/records?limit=50&cursor=${encodeURIComponent(first.next_cursor)}`)
  assert.equal(secondAfter.capture_sequence, '2')
  assert.deepEqual(secondAfter.records, secondBefore.records)
  assert.equal(secondAfter.next_cursor, null)
  await refresh(panel); await wait(500)
  assert.match(await records.innerText(), /fixed capture boundary 2\. Current capture progress: 3/)
  assert.equal(await records.locator('tbody tr').count(), 50)
  await screenshot('active-fixed-record-series', records); await narrowShot('active-fixed-record-series', records)
  checkpoint.fixed_series = { sequence: '2', ids_sha256: hash(secondAfter.records.map(v => v.id).join(',')), current_sequence: (await capture()).capture_sequence }; save()
  check('cursor-series-survives-later-capture', checkpoint.fixed_series)
}
async function cancelCapture(panel) {
  control({ mode: 'normal', history_units: 0, scenario: `cancel-${caseName}-${Date.now()}` })
  await prepared(panel); await confirmed(panel)
  await grantOne(); await grantOne()
  const before = await capture()
  assert.equal(before.streams[0].occurrences, '100')
  control({ mode: 'delayed', delay_ms: 4000, history_units: units() + 1 })
  await eventually(async () => readJson('control.stats.json').requests, requests => requests.some(v => v.operation === 'history' && v.completed_at === null), 'inflight synthetic collection')
  await refresh(panel)
  const response = page.waitForResponse(r => new URL(r.url()).pathname === `/api${BASE}/${checkpoint.capture_id}/cancel` && r.request().method() === 'POST')
  await panel.getByRole('button', { name: 'Cancel historical capture', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await dialog.waitFor()
  await dialog.getByRole('button', { name: 'Permanently cancel historical capture', exact: true }).click()
  assert.equal((await response).status(), 200)
  control({ history_units: 0 })
  await eventually(async () => readJson('control.stats.json').requests, requests => !requests.some(v => v.operation === 'history' && v.completed_at === null), 'cancelled request finishes')
  const after = await capture()
  assert.equal(after.state, 'cancelled'); assert.equal(after.reserved_bytes, '0')
  assert.equal(after.capture_sequence, before.capture_sequence)
  assert.equal(after.streams[0].occurrences, before.streams[0].occurrences)
  assert.equal((await walkRecords()).rows.length, 100)
  await refresh(panel); await screenshot('cancelled-partial', panel); await narrowShot('cancelled-partial', panel)
  check('inflight-cancellation-fences-commit', { rows: 100, sequence: after.capture_sequence, reserved_bytes: '0' })
  control({ mode: 'normal', delay_ms: 0 })
}
async function special(panel, mode) {
  control({ mode, delay_ms: 0, history_units: 0, scenario: `${mode}-${caseName}-${Date.now()}`, failure_stream: 'events' })
  await prepared(panel); await confirmed(panel)
  control({ history_units: null })
  if (mode === 'retry') {
    const waiting = await eventually(capture, v => v.state === 'waiting_retry', 'visible retry backoff', 20_000)
    assert.notEqual(waiting.streams[0].state, 'enumerated')
    await refresh(panel); await screenshot('retry-backoff', panel)
    check('retry-backoff-is-visible', { state: waiting.state, reason: waiting.pause_reason })
  }
  const value = await eventually(capture, v => v.state === (mode === 'retry' ? 'completed_with_gaps' : 'paused'), `${mode} outcome`)
  control({ history_units: 0 })
  if (mode === 'overlap') {
    assert.equal(value.pause_reason, 'enumeration_identity_uncertain')
    assert.equal(value.streams[0].occurrences, '200'); assert.equal(value.streams[0].unique_ids, '199')
    assert.equal(value.streams[0].conflicting_variants, '1')
    const all = await walkRecords()
    const duplicate = all.rows.find(v => v.source_id === '100')
    const variants = await walkRecords(`&record_id=${duplicate.id}`)
    assert.equal(variants.rows.length, 2)
    assert.ok(variants.rows.every(v => v.variant_count === '2'))
    const sourceCalls = readJson('control.stats.json').source_reader_calls
    await refresh(panel)
    const records = panel.getByTestId('history-capture-records')
    await records.getByLabel('Historical Person link status', { exact: true }).selectOption('conflicting_reference')
    await eventually(() => records.locator('tbody tr').count(), count => count === 2, 'two conflicting reference observations')
    await records.getByRole('button', { name: `Show variants for historical observation ${duplicate.id}`, exact: true }).click()
    await records.getByRole('button', { name: 'Show all captured identities', exact: true }).waitFor()
    await eventually(() => records.locator('tbody tr').count(), count => count === 2, 'UI identity variants')
    await records.getByRole('button', { name: `Inspect historical observation ${duplicate.id}`, exact: true }).click()
    const metadata = records.getByRole('region', { name: 'Historical observation metadata', exact: true })
    await metadata.waitFor()
    await eventually(() => metadata.innerText(), text => text.includes('Encrypted projection verified'), 'verified metadata detail')
    assert.equal(await metadata.locator('a,audio,video,iframe,img').count(), 0)
    assert.doesNotMatch(await records.innerText(), /SYNTHETIC_HISTORY_CONFLICT_BODY_SENTINEL/)
    await screenshot('variants-metadata', metadata); await narrowShot('variants-metadata', metadata)
    assert.equal(readJson('control.stats.json').source_reader_calls, sourceCalls)
    check('ui-conflicting-variants-and-metadata-are-source-free', { rows: 2 })
  } else if (mode === 'denied') {
    assert.equal(value.streams[0].occurrences, '0')
    assert.equal(value.streams[0].reported_total, null)
    assert.equal(value.streams[0].api_inaccessible_count, null)
    assert.notEqual(value.streams[0].state, 'enumerated')
  } else {
    assert.equal(value.streams[0].occurrences, '201')
    const requests = readJson('control.stats.json').requests.filter(v => v.scenario.startsWith(`retry-${caseName}-`) && v.operation === 'history')
    const failures = requests.filter(v => v.status === 429)
    assert.equal(failures.length, 6)
    assert.equal(new Set(failures.map(v => v.path)).size, 3)
    for (const path of new Set(failures.map(v => v.path))) assert.equal(failures.filter(v => v.path === path).length, 2)
    for (const failure of failures) {
      const next = requests.find(v => v.sequence > failure.sequence)
      assert.ok(new Date(next.started_at) - new Date(failure.completed_at) >= 1900, 'Retry-After should delay source requests')
    }
  }
  await refresh(panel); await screenshot(mode, panel); await narrowShot(mode, panel)
  check(`${mode}-honest-coverage`, { state: value.state, reason: value.pause_reason, total: value.streams[0].reported_total, inaccessible: null })
  checkpoint[mode + '_capture_id'] = checkpoint.capture_id; save()
  control({ mode: 'normal', history_units: 0 })
}
async function budgetQueue(panel) {
  control({ mode: 'normal', history_units: 0, scenario: `budget-${caseName}-${Date.now()}` })
  await prepared(panel); await confirmed(panel)
  checkpoint.budget_original = await capture(); save()
  check('budget-run-queued-for-lowered-policy', { state: 'queued' })
}
async function replacement(panel) {
  control({ mode: 'normal', history_units: 0, scenario: `replacement-${caseName}-${Date.now()}` })
  await prepared(panel); await confirmed(panel)
  const original = await capture()
  const collectionCalls = readJson('control.stats.json').history_reader_calls
  const oldId = checkpoint.capture_id
  const connection = (await read('/migrations/fub/')).connection
  control({ mode: 'changed_user' })
  try {
    await api('PUT', `/migrations/fub/connections/${org.connection_id}/credential`, { request_id: crypto.randomUUID(), expected_revision: connection.revision, api_key: 'synthetic-snapshot' })
    const fenced = await capture()
    assert.equal(fenced.state, 'paused'); assert.equal(fenced.pause_reason, 'connection_changed')
    assert.equal(fenced.capture_sequence, original.capture_sequence)
    assert.equal(fenced.actions.retry, false)
    panel = await migration()
    await screenshot('old-revision-fenced', panel)
    const proposed = await prepared(panel)
    assert.notEqual(checkpoint.capture_id, oldId)
    assert.equal(proposed.source_user_id, '8'); assert.equal(proposed.parent_source_user_id, '7')
    assert.equal(proposed.source_user_difference, true)
    await confirmed(panel)
    await grantOne()
    const current = await capture()
    assert.equal(current.capture_sequence, '1')
    assert.equal(current.state, 'queued')
    assert.equal(current.streams[0].occurrences, '0')
    await api('POST', `${BASE}/${checkpoint.capture_id}/cancel`, { request_id: crypto.randomUUID(), expected_run_revision: current.revision })
    assert.equal(readJson('control.stats.json').history_reader_calls, collectionCalls)
    checkpoint.replaced_old_capture_id = oldId; save()
    check('credential-replacement-requires-new-named-capture', { previous_state: 'paused', source_user_difference: true, collection_calls: 0 })
  } finally {
    control({ mode: 'normal', history_units: 0 })
    const latest = (await read('/migrations/fub/')).connection
    await api('PUT', `/migrations/fub/connections/${org.connection_id}/credential`, { request_id: crypto.randomUUID(), expected_revision: latest.revision, api_key: 'synthetic-snapshot' })
  }
}
async function budgetPause(panel) {
  control({ history_units: null })
  const value = await eventually(capture, v => v.state === 'paused', 'lowered ceiling pause')
  control({ history_units: 0 })
  assert.equal(value.pause_reason, 'storage_limit')
  assert.equal(value.capture_sequence, '0')
  assert.equal(value.run_byte_limit, checkpoint.budget_original.run_byte_limit)
  assert.equal(readJson('control.stats.json').history_reader_calls, 0)
  assert.equal(readJson('control.stats.json').source_reader_calls, 0)
  checkpoint.budget_paused = value; save()
  await refresh(panel); await screenshot('lowered-policy-pause', panel); await narrowShot('lowered-policy-pause', panel)
  check('lowered-policy-pauses-before-source', { raw_bytes: value.raw_bytes, retained_bytes: value.retained_bytes, source_calls: 0 })
}
async function budgetResume(panel) {
  const value = await capture()
  assert.equal(value.state, 'paused')
  assert.ok(BigInt(value.run_ceiling_bytes) > BigInt(value.run_byte_limit))
  const nextRun = (BigInt(value.run_byte_limit) + 1073741824n).toString()
  const nextOrg = (BigInt(value.org_byte_limit) + 1073741824n).toString()
  // Labels are frozen with the Web contract; verify both separate confirmation
  // and separate Resume, with no implicit worker grant or start after increase.
  await panel.getByRole('button', { name: 'Review historical storage allowances', exact: true }).click()
  await panel.getByLabel('Historical run allowance (GiB)', { exact: true }).fill(String(Number(nextRun) / 1073741824))
  await panel.getByLabel('Historical Organization allowance (GiB)', { exact: true }).fill(String(Number(nextOrg) / 1073741824))
  await panel.getByRole('button', { name: 'Review historical allowance increase', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await screenshot('allowance-increase', dialog); await narrowShot('allowance-increase', dialog)
  const increased = page.waitForResponse(r => new URL(r.url()).pathname === `/api${BASE}/${checkpoint.capture_id}/budget` && r.request().method() === 'POST')
  await dialog.getByRole('button', { name: 'Increase allowances', exact: true }).click()
  assert.equal((await increased).status(), 200)
  const after = await capture()
  assert.equal(after.run_byte_limit, nextRun); assert.equal(after.org_byte_limit, nextOrg)
  assert.equal(after.state, 'paused')
  assert.equal(after.capture_sequence, '0')
  const resumed = page.waitForResponse(r => new URL(r.url()).pathname === `/api${BASE}/${checkpoint.capture_id}/retry` && r.request().method() === 'POST')
  await panel.getByRole('button', { name: 'Resume historical capture', exact: true }).click()
  assert.equal((await resumed).status(), 202)
  assert.equal((await capture()).state, 'queued')
  check('allowance-increase-requires-explicit-resume', { run_byte_limit: nextRun, org_byte_limit: nextOrg })
  checkpoint.confirmed_history_calls = readJson('control.stats.json').history_reader_calls
  checkpoint.confirmed_publications = readJson('control.delivery-stats.json').publications
  await complete(panel)
}
async function budgetLayout(panel) {
  const completedId = checkpoint.capture_id
  assert.equal((await capture()).state, 'completed_with_gaps')
  control({ history_units: 0 })
  await prepared(panel)
  const before = await capture()
  const source = readJson('control.stats.json').source_reader_calls
  assert.equal(before.state, 'proposed')
  await panel.getByRole('button', { name: 'Review historical storage allowances', exact: true }).click()
  await panel.getByLabel('Historical run allowance (GiB)', { exact: true }).fill('3.5')
  await panel.getByLabel('Historical Organization allowance (GiB)', { exact: true }).fill('6')
  await panel.getByRole('button', { name: 'Review historical allowance increase', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await page.setViewportSize({ width: 390, height: 844 })
  const bounds = []
  for (const label of ['Cancel', 'Increase allowances']) {
    const button = dialog.getByRole('button', { name: label, exact: true })
    await button.waitFor()
    assert.equal(await button.isEnabled(), true)
    const rect = await button.evaluate(el => { const r = el.getBoundingClientRect(); return { left:r.left, right:r.right, top:r.top, bottom:r.bottom, width:innerWidth, height:innerHeight } })
    assert.ok(rect.left >= 0 && rect.right <= rect.width && rect.top >= 0 && rect.bottom <= rect.height, 'Both allowance actions must fit within the 390px viewport')
    bounds.push({ label, ...rect })
  }
  await screenshot('allowance-increase-fixed-390', dialog)
  await page.setViewportSize({ width: 1440, height: 1000 })
  await screenshot('allowance-increase-fixed', dialog)
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
  await dialog.waitFor({ state:'hidden' })
  const after = await capture()
  assert.equal(after.revision, before.revision)
  assert.equal(after.run_byte_limit, before.run_byte_limit)
  assert.equal(after.org_byte_limit, before.org_byte_limit)
  assert.equal(readJson('control.stats.json').source_reader_calls, source)
  const proposalId = checkpoint.capture_id
  await api('POST', `${BASE}/${proposalId}/cancel`, { request_id: crypto.randomUUID(), expected_run_revision: after.revision })
  assert.equal((await capture()).state, 'cancelled')
  assert.equal(readJson('control.stats.json').source_reader_calls, source)
  checkpoint.capture_id = completedId; checkpoint.layout_proposal_id = proposalId; save()
  check('allowance-dialog-actions-fit-390-and-cancel-preserves-state', { bounds, source_calls:0, proposal_final_state:'cancelled', confirmed:false })
}

async function helperContext() {
  const helper = await request.newContext({ baseURL: WEB })
  const response = await helper.post('/api/session', { data: { email: org.helper_email, password: PASSWORD } })
  assert.equal(response.status(), 200)
  return helper
}
async function access(panel) {
  const retained = await capture()
  const helper = await helperContext()
  try {
    const helperDetail = await helper.get(`/api${BASE}/${checkpoint.capture_id}`)
    assert.equal(helperDetail.status(), 200)
    const helperValue = await helperDetail.json()
    assert.equal(helperValue.actions.confirm, false); assert.equal(helperValue.actions.retry, false)
    const demote = await helper.put(`/api/organization/members/${org.admin_id}/role`, { data: { role: 'member' } })
    assert.equal(demote.status(), 200)
    const denied = await api('GET', `${BASE}/${checkpoint.capture_id}`, undefined, [401,403])
    assert.ok([401,403].includes(denied.status))
    await page.reload()
    await eventually(() => page.getByTestId('history-capture-panel').count(), v => v === 0, 'admin capture view cleared')
    await screenshot('demoted-admin-cleared'); await narrowShot('demoted-admin-cleared')
    const restore = await helper.put(`/api/organization/members/${org.admin_id}/role`, { data: { role: 'admin' } })
    assert.equal(restore.status(), 200)
    await api('DELETE', '/session', undefined, [204,200,401])
    await login(); await migration()
    assert.equal((await capture()).capture_sequence, retained.capture_sequence)
    check('current-role-revocation-clears-retained-view', { helper_source_actions: false })
  } finally {
    const restored = await helper.put(`/api/organization/members/${org.admin_id}/role`, { data: { role: 'admin' } })
    assert.equal(restored.status(), 200, 'Always restore the synthetic actor after the demotion test')
    await helper.dispose()
  }
  await api('DELETE', '/session', undefined, [204,200])
  const other = people.organizations.find(v => v.id !== org.id)
  await page.goto(WEB + '/login')
  await page.getByLabel('Email', { exact: true }).fill(other.admin_email)
  await page.getByLabel('Password', { exact: true }).fill(PASSWORD)
  const switched = page.waitForResponse(r => new URL(r.url()).pathname === '/api/session' && r.request().method() === 'POST')
  await page.getByRole('button', { name: 'Sign in', exact: true }).click()
  assert.equal((await switched).status(), 200)
  await page.waitForURL(url => !url.pathname.startsWith('/login'))
  await eventually(() => read('/me'), v => v.organization.id === other.id, 'other Organization context')
  await api('GET', `${BASE}/${checkpoint.capture_id}`, undefined, [404])
  await page.goto(WEB + MIGRATION)
  const foreignPanel = page.getByTestId('history-capture-panel')
  await foreignPanel.waitFor()
  const foreignParent = foreignPanel.getByLabel('People import for historical capture', { exact: true })
  await eventually(() => foreignParent.locator('option').evaluateAll(v => v.map(o => o.value)), ids => ids.includes(other.parent_id), 'other Organization completed parent')
  await foreignParent.selectOption(other.parent_id)
  const foreignList = await api('GET', `${BASE}?parent_import_id=${other.parent_id}&limit=50`)
  assert.ok(!JSON.stringify(foreignList.data).includes(checkpoint.capture_id))
  const options = await page.getByLabel('Historical capture run', { exact: true }).locator('option').evaluateAll(v => v.map(o => o.value))
  assert.equal(options.includes(checkpoint.capture_id), false)
  await screenshot('organization-switch-cleared', foreignPanel); await narrowShot('organization-switch-cleared', foreignPanel)
  check('other-organization-cannot-read-or-select-capture')
}
async function disconnect(panel) {
  const before = await capture()
  const connectionResponse = await read('/migrations/fub/')
  assert.equal(connectionResponse.connection.id, org.connection_id)
  await api('DELETE', `/migrations/fub/connections/${org.connection_id}`, undefined, [204])
  const retained = await capture()
  assert.equal(retained.capture_sequence, before.capture_sequence)
  assert.equal(retained.actions.confirm, false); assert.equal(retained.actions.retry, false)
  const calls = readJson('control.stats.json').source_reader_calls
  const records = await walkRecords()
  assert.equal(readJson('control.stats.json').source_reader_calls, calls)
  await page.reload(); const fresh = await migration()
  await screenshot('retained-after-disconnect', fresh); await narrowShot('retained-after-disconnect', fresh)
  check('disconnect-preserves-admin-read-only-evidence', { rows: records.rows.length, source_calls: 0 })
}

try {
  await start(); await login()
  const panel = await migration()
  if (phase === 'prepare') { control({ history_units: 0 }); await prepared(panel); await screenshot('proposed', panel); await narrowShot('proposed', panel) }
  else if (phase === 'confirm-lost') await confirmed(panel, true)
  else if (phase === 'progress') await progress(panel)
  else if (phase === 'complete') await complete(panel)
  else if (phase === 'cancel') await cancelCapture(panel)
  else if (['overlap','denied','retry'].includes(phase)) await special(panel, phase)
  else if (phase === 'replacement') await replacement(panel)
  else if (phase === 'budget-queue') await budgetQueue(panel)
  else if (phase === 'budget-pause') await budgetPause(panel)
  else if (phase === 'budget-resume') await budgetResume(panel)
  else if (phase === 'budget-layout') await budgetLayout(panel)
  else if (phase === 'access') await access(panel)
  else if (phase === 'disconnect') await disconnect(panel)
  assert.equal(attempt.external_requests.length, 0)
  assert.equal(readJson('control.stats.json').real_source_http_requests, 0)
  assert.equal(attempt.browser_errors.filter(v => v.kind === 'pageerror' || v.classification !== 'expected_http_failure').length, 0, 'Unclassified browser errors must be investigated')
  attempt.outcome = 'passed'
  checkpoint.phases[phase] = { attempt: attemptId, checks: attempt.checks.length }
} catch (error) {
  control({ history_units: 0 })
  attempt.outcome = 'failed'
  attempt.failure = { name: error.name, message: String(error.message).slice(0,1500), page_path: page ? new URL(page.url()).pathname : null }
  if (page && !page.isClosed()) await screenshot('failure-state').catch(() => {})
  // Assertion messages contain only constructed counters, closed routes or labels.
  console.error(JSON.stringify({ phase, case: caseName, failure: attempt.failure }))
  process.exitCode = 1
} finally {
  closing = true
  // Closing the context first releases response-body reads whose browser
  // requests were still open after navigation. Never await them with a live page.
  try { await context?.close() } catch (error) {
    attempt.outcome = 'failed'; attempt.cleanup_error = { name: error.name, message_sha256: hash(String(error.message)) }; process.exitCode = 1
    delete checkpoint.phases[phase]
  }
  await Promise.allSettled([...responseWork])
  if (attempt.outcome === 'passed' && attempt.browser_errors.some(v => v.kind === 'pageerror' || v.classification !== 'expected_http_failure')) {
    attempt.outcome = 'failed'; attempt.failure = { name: 'LateBrowserError', message: 'Unclassified browser error observed during final response collection' }; process.exitCode = 1
    delete checkpoint.phases[phase]
  }
  attempt.source_counter_after = readJson('control.stats.json').source_reader_calls
  attempt.history_counter_after = readJson('control.stats.json').history_reader_calls
  attempt.publication_counter_after = readJson('control.delivery-stats.json').publications
  attempt.finished_at = new Date().toISOString(); save()
}

