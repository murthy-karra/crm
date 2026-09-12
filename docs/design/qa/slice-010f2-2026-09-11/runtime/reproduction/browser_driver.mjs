// Private synthetic QA only. Syntax/inspection is safe; runtime requires both
// explicit CLI authorization and root's R2/gate approval outside this program.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { chromium, request } from '/Users/karrad/projects/crm-010f2/web/node_modules/.pnpm/playwright-core@1.63.0/node_modules/playwright-core/index.mjs'

const QA = '/private/tmp/crm-010f2-qa-694szdwe'
const WEB = 'http://127.0.0.1:5187'
const MIGRATION = '/manage/migration'
const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const BASE = '/migrations/fub/activity-imports'
const args = Object.fromEntries(process.argv.slice(2).map(arg => { const [k, ...rest] = arg.replace(/^--/, '').split('='); return [k, rest.join('=') || true] }))
const phases = ['pre-child', 'plan', 'complete-confirm-lost', 'complete-drain', 'native', 'cancel-partial', 'cancel-revision-and-stop', 'budget-queue', 'budget-probe-lowered', 'budget-restore-and-resume', 'access', 'disconnect']
if (args.describe) {
  console.log(JSON.stringify({ phases, cases: ['complete', 'cancel', 'budget'], runtime_requires: '--authorize-runtime plus root approval', budget_handoffs: ['after budget-queue: lower deployment ceiling and restart', 'after budget-probe-lowered: restore ceiling and restart'], cancel_probe: 'Actual API limit25 cursor409; independent unchanged default50 UI revision refresh' }, null, 2))
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
const PASSWORD = execFileSync('/usr/bin/python3', ['-c', 'import pathlib,shlex; p=pathlib.Path("/private/tmp/crm-010f2-qa-694szdwe/qa.env"); print(next(shlex.split(s.partition("=")[2])[0] for s in p.read_text().splitlines() if s.startswith("CRM_DEV_SEED_PASSWORD=")))'], { encoding: 'utf8' }).trim()
assert.ok(PASSWORD, 'Missing private seed credential')
const statePath = path.join(QA, 'browser-checkpoints.json')
const state = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')) : { fixture: 'synthetic-010f2', cases: {}, attempts: [] }
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
function units() { return Number(readJson('control.activity-stats.json').completed_activity_units) }
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
let context, page
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
  page.on('console', message => { if (message.type() === 'error') attempt.browser_errors.push({ kind: 'console', message_sha256: hash(message.text()) }) })
  page.on('response', captureResponse)
  const files = []
  function walk(dir) { for (const item of fs.readdirSync(dir, { withFileTypes: true })) { const next = path.join(dir, item.name); if (item.isDirectory()) walk(next); else files.push({ file: path.relative('/Users/karrad/projects/crm-010f2/web/dist', next), sha256: hash(fs.readFileSync(next)) }) } }
  walk('/Users/karrad/projects/crm-010f2/web/dist')
  attempt.build = { manifest_sha256: hash(JSON.stringify(files.sort((a, b) => a.file.localeCompare(b.file)))), files }
  attempt.browser = { version: context.browser()?.version() ?? 'persistent-chromium', executable_sha256: hash(fs.readFileSync(CHROME)) }
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
  await page.goto(`${WEB}/login?redirect=${encodeURIComponent(MIGRATION)}`)
  await page.waitForFunction(() => !location.pathname.startsWith('/login') || [...document.querySelectorAll('button')].some(b => b.textContent.trim() === 'Sign in'))
  const form = page.getByRole('button', { name: 'Sign in', exact: true })
  if (await form.isVisible().catch(() => false)) {
    await page.getByLabel('Email', { exact: true }).fill(email)
    await page.getByLabel('Password', { exact: true }).fill(PASSWORD)
    const reply = page.waitForResponse(r => new URL(r.url()).pathname === '/api/session' && r.request().method() === 'POST')
    await form.click(); assert.equal((await reply).status(), 200)
  }
  const me = await eventually(() => read('/me'), value => value.user?.email === email, 'correct browser actor', 20_000)
  assert.equal(me.organization.id, org.id)
  return me
}
async function screenshot(label, target) {
  const safe = `${caseName}-${label}`.replace(/[^a-zA-Z0-9_-]/g, '_')
  if (target) await target.scrollIntoViewIfNeeded()
  const metric = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth }))
  assert.ok(metric.scroll <= metric.width + 1, `Horizontal overflow at ${label}`)
  const file = path.join(screens, `${attemptId}-${safe}.png`)
  await page.screenshot({ path: file, fullPage: false })
  attempt.screenshots.push({ file, width: metric.width, horizontal_overflow: false }); save()
}
async function narrowShot(label, target) { await page.setViewportSize({ width: 390, height: 844 }); await screenshot(`${label}-390`, target); await page.setViewportSize({ width: 1440, height: 1000 }) }
async function noOperationalActions() {
  for (const id of ['log-contact', 'call-button', 'add-tag-button', 'remove-person-tag', 'task-add-form', 'note-composer', 'edit-note', 'edit-task', 'complete-task']) assert.equal(await page.getByTestId(id).count(), 0)
  assert.equal(await page.locator('a[href^="mailto:"]').count(), 0)
  assert.equal(attempt.external_requests.length, 0)
}
async function migration() {
  await page.goto(WEB + MIGRATION)
  const panel = page.getByTestId('activity-import-panel')
  await panel.getByRole('heading', { name: 'Import notes and tasks', exact: true }).waitFor()
  const select = panel.getByLabel('People import for notes and tasks', { exact: true })
  await eventually(() => select.locator('option').evaluateAll(options => options.map(o => o.value)), values => values.includes(org.parent_id), 'original parent option')
  await select.selectOption(org.parent_id)
  return panel
}
async function child() {
  const list = await read(`${BASE}?parent_import_id=${encodeURIComponent(org.parent_id)}&limit=20`)
  assert.ok(list.imports.length <= 1, 'A second child must not exist')
  if (!list.imports.length) return null
  const value = await read(`${BASE}/${list.imports[0].id}`)
  assert.equal(value.parent_import_id, org.parent_id); assert.equal(value.snapshot_id, org.snapshot_id); assert.equal(value.source_account_id, '101')
  checkpoint.child_id = value.id; checkpoint.plan_id = value.latest_plan.id; save()
  return value
}
async function ready() {
  return eventually(child, value => {
    if (value?.state === 'paused') throw new Error(`Preparation paused with code ${value.pause_reason}`)
    return value?.state === 'ready' && value.latest_plan.state === 'ready'
  }, 'ready activity plan')
}
async function freeze() { control({ activity_units: 0 }); await wait(2400); const before = units(); await wait(2100); assert.equal(units(), before, 'Worker did not freeze'); return before }
async function grantOne() {
  const target = units() + 1
  control({ activity_units: target })
  await eventually(async () => units(), value => value >= target, 'one worker unit', 15_000)
  control({ activity_units: 0 }); return target
}
async function uiRefresh(panel) { await panel.getByRole('button', { name: 'Refresh activity import status', exact: true }).click(); await wait(250) }
async function nativeCore(person = org.people['101']) { return read(`/people/${person}/migration-review`) }
async function preChild() {
  assert.equal(await child(), null, 'Pre-child phase requires no child')
  await page.goto(`${WEB}/people/${org.people['101']}`)
  await page.getByTestId('person-activity-review').waitFor()
  const core = await nativeCore()
  assert.equal(core.activity.notes_count, '0'); assert.equal(core.activity.open_tasks_count, '0'); assert.equal(core.activity.completed_tasks_count, '0')
  assert.equal('history' in core, false); assert.equal('tasks' in core, false)
  await noOperationalActions(); await screenshot('pre-child-core'); await narrowShot('pre-child-core')
  check('pre_child_bounded_core', { notes: '0', open: '0', completed: '0' })
}
async function mappingData(id, plan, role) {
  const all = []; let cursor
  do { const q = new URLSearchParams({ plan_id: plan, kind: role, limit: '50' }); if (cursor) q.set('cursor', cursor); const p = await read(`${BASE}/${id}/mappings?${q}`); assert.ok(p.items.length <= 50); all.push(...p.items); cursor = p.next_cursor } while (cursor)
  return all
}
const roleLabels = { note_author: 'Note author', task_creator: 'Task creator', task_assignee: 'Task assignee', task_kind: 'Task kind' }
const kinds = { Call: 'call', Email: 'email', Text: 'text', 'Follow Up': 'follow_up', Appointment: 'other' }
function expectedChoice(row) {
  if (row.role === 'task_kind') { assert.ok(kinds[row.source_value], 'Unexpected fixture task kind'); return { kind: 'map_kind', native_kind: kinds[row.source_value] } }
  if (row.source_value === '999' && row.role === 'note_author') return { kind: 'leave_unmapped' }
  assert.ok(['7', '8'].includes(row.source_value), 'Unexpected fixture source actor')
  assert.ok(!(row.role === 'task_assignee' && row.source_value === '8'))
  return { kind: 'map_existing', target_id: row.source_value === '7' ? org.admin_id : org.former_id }
}
async function applyMappings(panel, value) {
  const mappings = panel.getByTestId('activity-mappings')
  await mappings.waitFor()
  if (!checkpoint.dirty_discard_verified) {
    const choice = mappings.getByRole('combobox', { name: 'Note author choice for 7', exact: true })
    const previous = await choice.inputValue()
    await choice.selectOption(previous === 'leave_unmapped' ? 'hold' : 'leave_unmapped')
    await screenshot('mapping-draft', mappings)
    await panel.getByRole('button', { name: 'Discard activity choices', exact: true }).click()
    assert.equal(await choice.inputValue(), previous)
    await panel.getByLabel('IANA source timezone', { exact: true }).fill('America/New_York')
    assert.equal(await panel.getByRole('button', { name: 'Apply activity choices', exact: true }).isEnabled(), false)
    await panel.getByRole('button', { name: 'Discard activity choices', exact: true }).click()
    checkpoint.dirty_discard_verified = true; check('dirty_choices_discarded_without_apply')
  }
  for (const role of Object.keys(roleLabels)) {
    await mappings.getByRole('button', { name: roleLabels[role], exact: true }).click()
    const rows = await mappingData(value.id, value.latest_plan.id, role)
    for (const row of rows) {
      const expected = expectedChoice(row)
      const select = mappings.getByRole('combobox', { name: `${roleLabels[role]} choice for ${row.source_value}`, exact: true })
      await select.waitFor()
      const entry = mappings.locator('li').filter({ has: page.getByRole('combobox', { name: `${roleLabels[role]} choice for ${row.source_value}`, exact: true }) }).first()
      if (expected.kind === 'map_existing') {
        await entry.getByRole('button', { name: 'Choose CRM user', exact: true }).click()
        const dialog = page.getByRole('dialog', { name: 'Choose CRM user', exact: true })
        await dialog.waitFor()
        const targets = await read(`${BASE}/${value.id}/targets?plan_id=${value.latest_plan.id}&limit=50`)
        const target = targets.items.find(item => item.id === expected.target_id); assert.ok(target)
        if (role === 'task_assignee') {
          const former = targets.items.find(item => item.id === org.former_id)
          assert.equal(await dialog.getByRole('button', { name: `Use ${former.display_name}`, exact: true }).isEnabled(), false)
        }
        if (role === 'note_author' && row.source_value === '8') await screenshot('inactive-historical-author', dialog)
        await dialog.getByRole('button', { name: `Use ${target.display_name}`, exact: true }).click()
      } else await select.selectOption(expected.kind === 'map_kind' ? `map_kind:${expected.native_kind}` : expected.kind)
      if (role === 'note_author' && row.source_value === '999') {
        await entry.getByRole('button', { name: 'Inspect source mapping', exact: true }).click()
        await entry.getByLabel('Activity source field text', { exact: true }).waitFor()
        await screenshot('unknown-author-source', entry)
        await entry.getByRole('button', { name: 'Inspect source mapping', exact: true }).click()
      }
    }
  }
  await panel.getByLabel('IANA source timezone', { exact: true }).fill('America/Los_Angeles')
  const zoneAck = panel.getByLabel('I confirm this source timezone applies to the selected date-only records.', { exact: true })
  if (await zoneAck.isVisible()) await zoneAck.check()
  await screenshot('explicit-mappings-zone', mappings); await narrowShot('explicit-mappings-zone', mappings)
  const apply = panel.getByRole('button', { name: 'Apply activity choices', exact: true })
  if (await apply.isEnabled()) {
    control({ activity_units: null })
    const response = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname === `/api${BASE}/${value.id}/plans`)
    await apply.click(); assert.ok([200, 202].includes((await response).status()))
    value = await ready(); await freeze(); await uiRefresh(panel)
  }
  value = await child()
  for (const role of Object.keys(roleLabels)) for (const row of await mappingData(value.id, value.latest_plan.id, role)) assert.deepEqual(row.choice, expectedChoice(row))
  assert.equal(value.latest_plan.source_timezone, 'America/Los_Angeles')
  assert.equal(value.latest_plan.counts.notes.eligible, '54'); assert.equal(value.latest_plan.counts.tasks.eligible, '7'); assert.equal(value.latest_plan.counts.held_count, '9')
  checkpoint.mappings_verified_plan_id = value.latest_plan.id
  check('explicit_mapping_oracle', { eligible_notes: '54', eligible_tasks: '7', held: '9', source_only: value.latest_plan.counts.source_only_count })
  return value
}
async function recordResponse(action, id) {
  const pending = page.waitForResponse(r => r.request().method() === 'GET' && new URL(r.url()).pathname === `/api${BASE}/${id}/records` && r.status() === 200)
  await action(); const response = await pending; const bytes = await response.body(); assert.ok(bytes.length <= 512 * 1024); return JSON.parse(bytes.toString('utf8'))
}
async function inspectPlan(panel, value) {
  await panel.getByRole('button', { name: 'Planned records', exact: true }).click()
  const records = panel.getByTestId('activity-records')
  let batch = await recordResponse(() => records.getByRole('button', { name: 'Refresh records', exact: true }).click(), value.id)
  const seen = new Map(); let pages = 0
  do {
    pages++; assert.ok(pages <= 100); assert.ok(batch.items.length <= 50)
    for (const row of batch.items) {
      assert.ok(!seen.has(row.id), 'Repeated record during stable plan traversal'); seen.set(row.id, row)
      if (row.source_id === '301') {
        const item = records.locator('li').filter({ has: page.getByRole('heading', { name: /^Note · FUB 301 ·/ }) }).first()
        assert.equal(row.preview.native.body, 'Subject: Readable HTML with replies\n\nSYNTHETIC_ACTIVITY_BODY_SENTINEL Hello José 🏡.\n• First item\n• Second item\nSource link (https://synthetic.invalid/never-fetch)')
        await screenshot('readable-html-native-preview', item)
        await item.getByRole('button', { name: 'Inspect full source and conversion', exact: true }).click()
        await item.getByLabel('Activity source field text', { exact: true }).waitFor()
        assert.ok((await item.getByLabel('Activity source field text', { exact: true }).textContent()).includes('SYNTHETIC_SOURCE_ONLY_REPLY'))
        await screenshot('source-html-replies', item); await narrowShot('source-html-replies', item)
        await item.getByRole('button', { name: 'Inspect full source and conversion', exact: true }).click()
        await item.getByRole('button', { name: 'Review source observations', exact: true }).click()
        const observations = item.getByRole('region', { name: 'Retained source observations', exact: true })
        await observations.getByRole('button', { name: 'Inspect exact capture text', exact: true }).first().click()
        await observations.getByLabel('Activity source field text', { exact: true }).waitFor()
        await screenshot('exact-capture-observation', observations)
        await item.getByRole('button', { name: 'Review source observations', exact: true }).click()
      }
      if (['501', '504'].includes(row.source_id)) {
        const expected = row.source_id === '501' ? '2026-03-09T06:59:59Z' : '2026-11-02T07:59:59Z'
        assert.equal(row.preview.native.due_at.replace(/\+00:00$/, 'Z'), expected)
        await screenshot(`dst-normalized-${row.source_id}`, records.getByText(row.preview.native.due_at, { exact: true }))
      }
      if (row.source_id === '503') assert.equal(row.preview.native.completed_at.replace(/\+00:00$/, 'Z'), '2026-09-01T16:00:00.123456Z')
    }
    if (!batch.next_cursor) break
    batch = await recordResponse(() => records.getByRole('button', { name: 'Next records', exact: true }).click(), value.id)
  } while (true)
  for (const id of ['301', '302', '303', '304', '305', '306', '307', '308', '501', '502', '503', '504', '505', '506', '507', '508', '509', '510', '511', '512']) assert.ok([...seen.values()].some(row => row.source_id === id), `Missing known fixture record ${id}`)
  const noteRows = [...seen.values()].filter(r => r.kind === 'note'); const taskRows = [...seen.values()].filter(r => r.kind === 'task')
  assert.equal(noteRows.filter(r => r.disposition === 'eligible').length, 54); assert.equal(taskRows.filter(r => r.disposition === 'eligible').length, 7)
  checkpoint.plan_preview_verified = value.latest_plan.id
  check('stable_source_preview_traversal', { pages, rows: seen.size, unique: true, exact_dst_and_microseconds: true, source_replies_retained: true })
}
async function preparePlan() {
  const panel = await migration(); let value = await child()
  if (!value) {
    control({ activity_units: null })
    const response = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname === `/api${BASE}`)
    await panel.getByRole('button', { name: 'Prepare notes and tasks plan', exact: true }).click()
    assert.ok([200, 201, 202].includes((await response).status()))
    value = await ready(); await freeze(); await uiRefresh(panel)
  } else if (value.state === 'preparing') { control({ activity_units: null }); value = await ready(); await freeze(); await uiRefresh(panel) }
  assert.equal(value.state, 'ready', 'Plan phase cannot alter a confirmed child')
  value = await applyMappings(panel, value)
  await inspectPlan(panel, value)
  return value
}
async function acknowledge(panel, value) {
  assert.equal(value.latest_plan.id, checkpoint.mappings_verified_plan_id)
  assert.equal(value.latest_plan.counts.notes.eligible, '54'); assert.equal(value.latest_plan.counts.tasks.eligible, '7')
  await panel.getByLabel(new RegExp(`^I reviewed ${value.latest_plan.counts.held_count} held records`)).check()
  await panel.getByLabel(new RegExp(`^I reviewed ${value.latest_plan.counts.source_only_count} source-only components`)).check()
  await panel.getByLabel('This keeps the Organization in administrator review. Cancellation is final and retains settled work.', { exact: true }).check()
  const review = panel.getByRole('button', { name: 'Review notes and tasks confirmation', exact: true })
  assert.equal(await review.isEnabled(), true, 'Fresh release report and reviewed ready plan are required')
  await review.click()
  const dialog = page.getByRole('dialog', { name: 'Confirm notes and tasks import', exact: true })
  await dialog.waitFor(); await screenshot('named-confirmation', dialog); await narrowShot('named-confirmation', dialog)
  return dialog
}
async function confirm({ lost = false } = {}) {
  await freeze(); const panel = await migration(); const value = await child()
  assert.ok(value)
  if (value.confirmed_plan_id) {
    if (lost && !checkpoint.lost_response_replay_verified) {
      // Root-authorized recovery of the already executed retry: route.fetch's
      // APIResponse does not emit a page response. The retained successful
      // page POST therefore comes from the retry branch, which asserted byte
      // equality before route.continue. No new POST or reviewed intent here.
      const prior = [...state.attempts].reverse().find(a => a.id !== attemptId && a.case === caseName && a.phase === phase && a.outcome === 'failed')
      const expectedRoute = hash(`/api${BASE}/${value.id}/confirm`)
      const successfulRetry = prior?.network.find(n => n.method === 'POST' && n.route_sha256 === expectedRoute && n.status === 202)
      const reviewed = readJson(`${caseName}-reviewed-confirm.json`)
      const command = JSON.parse(reviewed.utf8_body)
      assert.ok(successfulRetry && checkpoint.lost_confirm_committed?.status === 202)
      assert.equal(reviewed.sha256, hash(reviewed.utf8_body))
      assert.equal(reviewed.sha256, checkpoint.lost_confirm_committed.body_sha256)
      assert.equal(reviewed.child_id, value.id); assert.equal(command.plan_id, value.confirmed_plan_id)
      assert.equal(value.state, 'queued'); assert.equal(value.native_row_bytes, '0')
      checkpoint.lost_response_replay_verified = true
      checkpoint.lost_response_recovery = { prior_failed_attempt: prior.id, evidence: 'Existing two-request UI trace; byte assertion preceded successful202 retry; no new replay', original_body_sha256: reviewed.sha256, retry_status: successfulRetry.status, unsettled_retry_screenshot: true }
      check('recovered_existing_UI_retry_trace_without_new_request', checkpoint.lost_response_recovery)
    }
    return value
  }
  const dialog = await acknowledge(panel, value)
  let original, firstStatus, replayed = false
  const pattern = `${WEB}/api${BASE}/${value.id}/confirm`
  if (lost) await page.route(pattern, async route => {
    const body = route.request().postDataBuffer(); assert.ok(body)
    if (!original) {
      original = body
      // Save only the reviewed strict command (IDs/counts); never credentials.
      atomic(path.join(QA, `${caseName}-reviewed-confirm.json`), { child_id: value.id, utf8_body: body.toString('utf8'), sha256: hash(body), byte_length: body.length })
      const response = await route.fetch(); firstStatus = response.status()
      assert.ok([200, 202].includes(firstStatus), 'Real confirmation must commit before dropping its response')
      checkpoint.lost_confirm_committed = { status: firstStatus, body_sha256: hash(body) }; save()
      await route.abort('failed')
    } else { assert.ok(body.equals(original), 'Same reviewed retry must be byte-identical'); replayed = true; checkpoint.lost_retry_body_sha256 = hash(body); checkpoint.lost_retry_exact_bytes_asserted = true; save(); await route.continue() }
  })
  const normalConfirmation = lost ? null : page.waitForResponse(r => r.url() === pattern && r.request().method() === 'POST')
  await dialog.getByRole('button', { name: 'Confirm notes and tasks import', exact: true }).click()
  if (normalConfirmation) assert.ok([200, 202].includes((await normalConfirmation).status()))
  if (lost) {
    const retry = panel.getByRole('button', { name: 'Retry the same activity request', exact: true })
    await retry.waitFor(); await page.getByRole('dialog', { name: 'Confirm notes and tasks import', exact: true }).waitFor({ state: 'hidden' }); await wait(200); await screenshot('uncertain-confirmation-retry', panel)
    const response = page.waitForResponse(r => r.url() === pattern && [200, 202].includes(r.status()))
    await retry.click(); await response
    assert.ok(replayed); await page.unroute(pattern)
    checkpoint.lost_response_replay_verified = true
    check('real_lost_confirm_response_recovered_by_same_UI_request', { first_status: firstStatus, identical_bytes: true, command_bytes: original.length })
  }
  const confirmed = await eventually(child, item => !!item.confirmed_plan_id, 'confirmed child')
  assert.equal(confirmed.id, value.id); assert.equal(confirmed.native_row_bytes, '0')
  checkpoint.confirmed_plan_id = confirmed.confirmed_plan_id
  check('queued_without_native_work', { revision: confirmed.revision, child_id: confirmed.id })
  return confirmed
}
async function drain() {
  control({ activity_units: null })
  const done = await eventually(child, value => { if (value?.state === 'paused') throw new Error(`Execution paused with code ${value.pause_reason}`); return value?.state === 'completed' }, 'completed child')
  await freeze(); const panel = await migration(); await uiRefresh(panel)
  await panel.getByRole('heading', { name: 'Activity import · Completed', exact: true }).waitFor()
  await screenshot('completed-activity-summary', panel)
  assert.equal(done.counts.notes.applied, '54'); assert.equal(done.counts.tasks.applied, '7')
  check('completed_61_native_rows', { notes: done.counts.notes.applied, tasks: done.counts.tasks.applied })
}
async function nativePageResponse(action, person, kind, stateName) {
  const pending = page.waitForResponse(r => { const u = new URL(r.url()); return r.request().method() === 'GET' && u.pathname === `/api/people/${person}/migration-review/${kind}` && (!stateName || u.searchParams.get('state') === stateName) && r.status() === 200 })
  await action(); const response = await pending; const bytes = await response.body(); assert.ok(bytes.length <= 512 * 1024); const value = JSON.parse(bytes); assert.ok(value.items.length <= 50); return value
}
async function nativeStart(person) {
  const notes = page.waitForResponse(r => new URL(r.url()).pathname === `/api/people/${person}/migration-review/notes` && r.status() === 200)
  const open = page.waitForResponse(r => { const u = new URL(r.url()); return u.pathname === `/api/people/${person}/migration-review/tasks` && u.searchParams.get('state') === 'open' && r.status() === 200 })
  const done = page.waitForResponse(r => { const u = new URL(r.url()); return u.pathname === `/api/people/${person}/migration-review/tasks` && u.searchParams.get('state') === 'completed' && r.status() === 200 })
  await page.goto(`${WEB}/people/${person}`)
  return { notes: await (await notes).json(), open: await (await open).json(), completed: await (await done).json() }
}
async function openNote(index = 0) {
  await page.getByRole('button', { name: /^Read full note from / }).nth(index).click()
  const dialog = page.getByRole('dialog', { name: 'Full imported note', exact: true })
  await dialog.getByLabel('Full native note body', { exact: true }).waitFor()
  return dialog
}
async function nativeReview() {
  const totals = { notes: 0, open: 0, completed: 0 }; const ids = { notes: new Set(), open: new Set(), completed: new Set() }
  for (const source of ['101', '102']) {
    const person = org.people[source]; const first = await nativeStart(person); const core = await nativeCore(person)
    await noOperationalActions(); await screenshot(`person-${source}-native-core`)
    for (const key of ['notes', 'open', 'completed']) {
      let batch = first[key]; let pages = 0
      do {
        assert.equal(batch.activity_revision, core.activity.activity_revision); pages++; assert.ok(pages < 100); assert.ok(batch.items.length <= 50)
        for (const row of batch.items) { assert.ok(!ids[key].has(row.id), 'Duplicate native row'); ids[key].add(row.id); totals[key]++ }
        if (key === 'notes' && source === '101' && pages === 1 && batch.items.length) {
          const index = batch.items.findIndex(row => Number(row.provenance?.source_id) >= 1000)
          const dialog = await openNote(Math.max(0, index)); const body = await dialog.getByLabel('Full native note body', { exact: true }).textContent()
          if (index >= 0) { assert.equal([...body].length, 10000); assert.equal(body, '🏡'.repeat(10000)) }
          await screenshot('full-native-note', dialog); await narrowShot('full-native-note', dialog)
          await dialog.getByRole('button', { name: 'Inspect original note source', exact: true }).click()
          const original = page.getByRole('dialog', { name: 'Imported activity source', exact: true })
          await original.getByLabel('Activity source field text', { exact: true }).waitFor()
          await screenshot('native-source-provenance', original); await narrowShot('native-source-provenance', original)
          await original.getByRole('button', { name: 'Close source', exact: true }).click()
        }
        if (!batch.next_cursor) break
        const label = key === 'notes' ? 'Next notes page' : `Next ${key === 'open' ? 'open' : 'completed'} tasks page`
        batch = await nativePageResponse(() => page.getByRole('button', { name: label, exact: true }).click(), person, key === 'notes' ? 'notes' : 'tasks', key === 'notes' ? undefined : key)
      } while (true)
      check(`native_${source}_${key}_traversal`, { pages, activity_revision: core.activity.activity_revision })
    }
    await nativeStart(person)
    await eventually(() => page.getByTestId('person-activity-review').textContent(), text => text.includes('activity revision') && !/Loading Person|Loading notes|Loading open tasks|Loading completed tasks/.test(text), 'rendered native cards')
    await noOperationalActions(); await narrowShot(`person-${source}-native-core-loaded`)
    await narrowShot(`person-${source}-notes-loaded`, page.getByRole('heading', { name: /^Notes / }))
    await narrowShot(`person-${source}-notes-paging-controls`, page.getByRole('button', { name: 'Next notes page', exact: true }))
  }
  const value = await child()
  if (value.state === 'completed') { assert.equal(totals.notes, 54); assert.equal(totals.open + totals.completed, 7) }
  checkpoint.native_traversal = { counts: totals, unique: true, ids_sha256: hash(JSON.stringify(Object.fromEntries(Object.entries(ids).map(([k, set]) => [k, [...set].sort()])))) }
  check('exact_native_default50_UI_traversal', checkpoint.native_traversal)
}
async function partial() {
  assert.equal(caseName, 'cancel'); await confirm()
  let count
  for (let index = 0; index < 70; index++) {
    const value = await child(); const c = await nativeCore()
    count = Number(value.counts.notes.applied) + Number(value.counts.tasks.applied)
    assert.ok(count < 60, 'Reserve room for one more settlement while retaining a strict subset')
    if (Number(c.activity.notes_count) > 25) break
    assert.ok(value.state !== 'completed'); await grantOne()
  }
  await freeze(); const c = await nativeCore(); assert.ok(Number(c.activity.notes_count) > 25)
  const value = await child(); assert.ok(count > 0 && count < 60)
  checkpoint.partial = { activity_revision: c.activity.activity_revision, native_count: count, stats_units: units() }
  check('positive_partial_subset_frozen', checkpoint.partial)
}
async function revisionAndCancel() {
  assert.equal(caseName, 'cancel'); await freeze()
  const person = org.people['101']; const before = await child(); assert.ok(['queued', 'running'].includes(before.state))
  const cursorPage = await read(`/people/${person}/migration-review/notes?limit=25`)
  assert.equal(cursorPage.items.length, 25); assert.ok(cursorPage.next_cursor)
  await nativeStart(person); const dialog = await openNote(); const bodyHash = hash(await dialog.getByLabel('Full native note body', { exact: true }).textContent())
  await screenshot('partial-open-note-before-revision', dialog)
  await grantOne(); const after = await nativeCore(); assert.notEqual(after.activity.activity_revision, cursorPage.activity_revision)
  const stale = await api('GET', `/people/${person}/migration-review/notes?limit=25&cursor=${encodeURIComponent(cursorPage.next_cursor)}`, undefined, [409])
  assert.equal(stale.data.error, 'activity_refresh_required')
  await page.evaluate(() => window.dispatchEvent(new Event('focus')))
  await dialog.waitFor({ state: 'hidden' })
  await eventually(() => page.getByTestId('person-activity-review').textContent(), text => text.includes(`revision ${after.activity.activity_revision}`), 'new UI core revision')
  check('actual_API_limit25_old_cursor_409', { before_revision: cursorPage.activity_revision, after_revision: after.activity.activity_revision })
  check('separate_default50_UI_focus_refresh_cleared_body_and_pages', { prior_body_sha256: bodyHash })
  await freeze(); const panel = await migration(); const value = await child()
  const nativeCount = Number(value.counts.notes.applied) + Number(value.counts.tasks.applied); assert.ok(nativeCount > 0 && nativeCount < 61)
  await panel.getByRole('button', { name: 'Cancel activity import', exact: true }).click()
  const cancelDialog = page.getByRole('dialog', { name: 'Permanently cancel activity import', exact: true })
  await cancelDialog.waitFor(); await screenshot('permanent-cancel-dialog', cancelDialog); await narrowShot('permanent-cancel-dialog', cancelDialog)
  const cancellation = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname === `/api${BASE}/${value.id}/cancel`)
  await cancelDialog.getByRole('button', { name: 'Permanently cancel activity import', exact: true }).click()
  assert.ok([200, 202].includes((await cancellation).status()))
  const cancelled = await eventually(child, item => item.state === 'cancelled', 'cancelled child')
  assert.equal(Number(cancelled.counts.notes.applied) + Number(cancelled.counts.tasks.applied), nativeCount)
  await uiRefresh(panel); await screenshot('cancelled-retained-subset', panel)
  check('cancelled_strict_native_subset', { native_count: nativeCount, terminal: true })
}
async function budgetQueue() {
  assert.equal(caseName, 'budget'); const value = await confirm()
  check('ROOT_HANDOFF_lower_deployment_ceiling_then_restart', { child_id: value.id, snapshot_id: org.snapshot_id, worker_frozen: true })
}
async function budgetProbe() {
  assert.equal(caseName, 'budget'); const before = await child(); assert.ok(before.confirmed_plan_id); assert.equal(before.native_row_bytes, '0')
  await grantOne(); const paused = await eventually(child, value => value.state === 'paused', 'storage pause')
  assert.ok(/storage|budget|allowance/.test(paused.pause_reason), 'Expected storage admission pause')
  assert.equal(paused.native_row_bytes, '0'); const panel = await migration(); await uiRefresh(panel)
  await screenshot('deployment-headroom-storage-pause', panel)
  check('ROOT_HANDOFF_restore_deployment_ceiling_then_restart', { child_id: paused.id, snapshot_id: org.snapshot_id, pause_code: paused.pause_reason, native_rows: 0 })
}
async function budgetResume() {
  assert.equal(caseName, 'budget'); await freeze(); const before = await child(); assert.equal(before.state, 'paused')
  const panel = await migration(); await uiRefresh(panel)
  const budget = panel.getByRole('region', { name: 'Snapshot storage allowances', exact: true })
  await budget.getByRole('button', { name: 'Review allowances', exact: true }).click()
  await budget.getByLabel('New run allowance (GiB)', { exact: true }).fill('0.5')
  await budget.getByLabel('New Organization allowance (GiB)', { exact: true }).fill('0.5')
  await budget.getByRole('button', { name: 'Review increase', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Increase snapshot allowances', exact: true })
  await screenshot('explicit-allowance-increase', dialog)
  const increaseResponse = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname === `/api/migrations/fub/snapshots/${org.snapshot_id}/budget`)
  await dialog.getByRole('button', { name: 'Confirm increase', exact: true }).click()
  assert.ok([200, 202].includes((await increaseResponse).status()))
  await eventually(async () => (await read(`/migrations/fub/snapshots/${org.snapshot_id}`)).snapshot, s => s.run_byte_limit === '536870912', 'explicit run allowance increase')
  assert.equal((await child()).state, 'paused', 'Allowance increase must not auto-resume')
  await uiRefresh(panel)
  const resumeResponse = page.waitForResponse(r => r.request().method() === 'POST' && new URL(r.url()).pathname === `/api${BASE}/${before.id}/retry`)
  await panel.getByRole('button', { name: 'Resume activity import', exact: true }).click()
  assert.ok([200, 202].includes((await resumeResponse).status()))
  await eventually(child, value => value.state === 'queued', 'explicit retry queued')
  check('explicit_allowance_increase_then_retry', { old_run_bytes: '268435456', new_run_bytes: '536870912', org_bytes: '536870912', auto_resume: false })
  await drain()
}
async function helperSession(email) {
  const session = await request.newContext({ baseURL: `${WEB}/api`, extraHTTPHeaders: { Origin: WEB } })
  const logged = await session.post('/api/session', { data: { email, password: PASSWORD } }); assert.equal(logged.status(), 200)
  return session
}
async function accessChecks() {
  assert.equal(caseName, 'complete')
  const interrupted = [...state.attempts].reverse().find(a => a.id !== attemptId && a.case === caseName && a.phase === 'access' && a.outcome === 'interrupted_private_teardown')
  if (interrupted) {
    assert.ok(interrupted.checks.some(c => c.name === 'demotion_cleared_native_review_and_API_denied'))
    assert.ok(interrupted.checks.some(c => c.name === 'ordinary_member_hold_and_API_denial'))
    const me = await read('/me'); assert.equal(me.user.id, org.admin_id); assert.equal(me.organization.role, 'admin')
    const core = await nativeCore(); assert.equal(core.person.id, org.people['101'])
    const member = await helperSession(org.member_email)
    try {
      const memberMe = await member.get('/api/me'); assert.equal(memberMe.status(), 200); assert.equal((await memberMe.json()).organization.role, 'member')
      const denied = await member.get(`/api/people/${org.people['101']}/migration-review`); assert.equal(denied.status(), 403)
    } finally { await Promise.race([member.dispose(), wait(5000)]) }
    check('recovered_executed_access_checks_after_interrupted_teardown', { prior_attempt: interrupted.id, current_admin_restored: true, current_member_denied: true, repeated_authority_mutations: false })
    return
  }
  const first = await nativeStart(org.people['101']); assert.ok(first.notes.items.length)
  const dialog = await openNote(); await dialog.waitFor()
  const helper = await helperSession(org.helper_email)
  let demoted = false
  try {
    const response = await helper.put(`/api/organization/members/${org.admin_id}/role`, { data: { role: 'member' } }); assert.equal(response.status(), 200); demoted = true
    await page.evaluate(() => window.dispatchEvent(new Event('focus')))
    await page.reload(); await page.getByTestId('workspace-waiting').waitFor()
    assert.equal(await page.getByLabel('Full native note body', { exact: true }).count(), 0)
    assert.equal(await page.getByTestId('person-activity-review').count(), 0)
    await api('GET', `/people/${org.people['101']}/migration-review`, undefined, [403])
    await screenshot('demoted-admin-hold'); await narrowShot('demoted-admin-hold')
    check('demotion_cleared_native_review_and_API_denied')
  } finally {
    if (demoted) { const response = await helper.put(`/api/organization/members/${org.admin_id}/role`, { data: { role: 'admin' } }); assert.equal(response.status(), 200) }
    await helper.dispose()
  }
  // Separate browser storage/cookie/session for the ordinary member.
  const memberContext = await chromium.launchPersistentContext(path.join(QA, `browser-profile-${caseName}-member`), { executablePath: CHROME, headless: true, viewport: { width: 390, height: 844 }, serviceWorkers: 'block' })
  const adminPage = page; const member = memberContext.pages()[0] ?? await memberContext.newPage()
  member.on('pageerror', error => { attempt.browser_errors.push({ kind: 'pageerror', message_sha256: hash(error.message) }) })
  member.on('console', message => { if (message.type() === 'error') attempt.browser_errors.push({ kind: 'console', message_sha256: hash(message.text()) }) })
  member.on('response', captureResponse)
  await memberContext.route('**/*', async route => {
    const url = new URL(route.request().url())
    if (url.origin === WEB || ['data:', 'about:', 'blob:'].includes(url.protocol)) return route.continue()
    attempt.external_requests.push({ ...safeEndpoint(url.href), method: route.request().method() }); save()
    await route.abort('blockedbyclient')
  })
  try {
    page = member
    await page.goto(`${WEB}/login?redirect=${encodeURIComponent(`/people/${org.people['101']}`)}`)
    await page.waitForFunction(() => !location.pathname.startsWith('/login') || [...document.querySelectorAll('button')].some(b => b.textContent.trim() === 'Sign in'))
    if (await page.getByRole('button', { name: 'Sign in', exact: true }).isVisible().catch(() => false)) { await page.getByLabel('Email', { exact: true }).fill(org.member_email); await page.getByLabel('Password', { exact: true }).fill(PASSWORD); await page.getByRole('button', { name: 'Sign in', exact: true }).click() }
    await page.getByTestId('workspace-waiting').waitFor()
    const me = await read('/me'); assert.equal(me.user.email, org.member_email)
    await api('GET', `/people/${org.people['101']}/migration-review`, undefined, [403])
    await screenshot('ordinary-member-hold-390'); check('ordinary_member_hold_and_API_denial')
  } finally { page = adminPage; await memberContext.close() }
  await page.reload(); await page.goto(`${WEB}/people/${org.people['101']}`); await page.getByTestId('person-activity-review').waitFor()
}
async function disconnect() {
  const panel = await migration()
  const controlBefore = readJson('control.stats.json').source_reader_calls
  const button = page.getByTestId('disconnect-fub')
  if (await button.isVisible()) { const reply = page.waitForResponse(r => r.request().method() === 'DELETE' && new URL(r.url()).pathname === `/api/migrations/fub/connections/${org.connection_id}`); await button.click(); assert.equal((await reply).status(), 204) }
  control({ mode: 'unavailable' }); await uiRefresh(panel); const value = await child(); assert.ok(value)
  assert.equal(readJson('control.stats.json').source_reader_calls, controlBefore)
  check('source_disconnected_retained_child_read_without_source_calls')
}

try {
  await start(); await login()
  if (phase === 'pre-child') await preChild()
  else if (phase === 'plan') await preparePlan()
  else if (phase === 'complete-confirm-lost') { assert.equal(caseName, 'complete'); await confirm({ lost: true }) }
  else if (phase === 'complete-drain') { assert.equal(caseName, 'complete'); await drain() }
  else if (phase === 'native') await nativeReview()
  else if (phase === 'cancel-partial') await partial()
  else if (phase === 'cancel-revision-and-stop') await revisionAndCancel()
  else if (phase === 'budget-queue') await budgetQueue()
  else if (phase === 'budget-probe-lowered') await budgetProbe()
  else if (phase === 'budget-restore-and-resume') await budgetResume()
  else if (phase === 'access') await accessChecks()
  else if (phase === 'disconnect') await disconnect()
  await Promise.race([Promise.allSettled([...responseWork]), wait(5000)])
  assert.equal(attempt.external_requests.length, 0, 'Source content triggered an external request')
  assert.equal(attempt.browser_errors.filter(item => item.kind === 'pageerror').length, 0, 'Unexpected browser pageerror')
  attempt.console_summary = {
    count: attempt.browser_errors.filter(item => item.kind === 'console').length,
    message_hashes: attempt.browser_errors.filter(item => item.kind === 'console').map(item => item.message_sha256),
    observed_error_statuses: attempt.network.filter(item => item.status >= 400).map(item => ({ status: item.status, method: item.method, route_sha256: item.route_sha256 })),
    deliberate_confirm_response_abort: phase === 'complete-confirm-lost',
    deliberate_authority_denials: phase === 'access',
    deliberate_old_cursor_409: phase === 'cancel-revision-and-stop',
  }
  attempt.source_counter_after = readJson('control.stats.json').source_reader_calls
  attempt.publication_counter_after = readJson('control.delivery-stats.json').publications
  assert.equal(attempt.source_counter_after, attempt.source_counter_before, 'Source reader called during retained activity QA')
  assert.equal(attempt.publication_counter_after, attempt.publication_counter_before, 'Unexpected publication during activity QA')
  attempt.outcome = 'passed'; checkpoint.phases[phase] = { attempt: attemptId, completed_at: new Date().toISOString() }
  check('phase_complete')
} catch (error) {
  attempt.outcome = 'failed'
  attempt.failure = { class: error?.name ?? 'Error', message_sha256: hash(String(error?.message ?? error)), stack_sha256: hash(String(error?.stack ?? error)) }
  // Error details are private and may contain DOM snippets. Root receives only
  // safe checkpoint names; inspect this file locally if an assertion fails.
  fs.writeFileSync(path.join(QA, `${attemptId}-failure.txt`), String(error?.stack ?? error).replaceAll(PASSWORD, '[credential redacted]'), { mode: 0o600 })
  if (page) await page.screenshot({ path: path.join(screens, `${attemptId}-failure.png`), fullPage: false }).catch(() => {})
  console.error(JSON.stringify({ phase, case: caseName, outcome: 'failed', failure_file: path.join(QA, `${attemptId}-failure.txt`) }))
  process.exitCode = 1
} finally {
  await Promise.race([Promise.allSettled([...responseWork]), wait(5000)]); await Promise.race([context?.close(), wait(10000)]); attempt.finished_at = new Date().toISOString(); save()
  process.exit(process.exitCode ?? 0)
}
