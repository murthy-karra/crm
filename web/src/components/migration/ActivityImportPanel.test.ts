import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { ActivityCounts, ActivityImport } from '../../api/activityImports'
import { resetWorkspace } from '../../workspaceLifecycle'
import ActivityImportPanel from './ActivityImportPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/activity-imports'
const cleanup: Array<() => void> = []
const family = { planned: '3', eligible: '2', applied: '0', already_present: '0', held: '1', pending: '3' }
const counts = (): ActivityCounts => ({ notes: { ...family }, tasks: { ...family }, held_count: '2', source_only_count: '9007199254740993', invalid_occurrences: '1', unavailable_bodies: '1' })
function identity(role = 'admin', actor = 'actor', org = 'org') { return { user: { id: actor, email: 'synthetic@test.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Org', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false } }
function run(state: ActivityImport['state'] = 'ready'): ActivityImport {
  const confirmed = ['queued', 'running', 'completed'].includes(state)
  return { id: 'child', parent_import_id: 'parent', parent_plan_id: 'parent-plan', snapshot_id: 'snapshot', source_account_id: '900719925474099312345', capture_sequence: '9', workspace_revision: '2', revision: '9007199254740995', activity_revision: '1', engine_version: 'fub-activity-import-v1', state, phase: confirmed ? 'records' : 'preparation', pause_reason: state === 'paused' ? 'storage_limit' : null, created_at: '2026-09-11T12:00:00Z', updated_at: '2026-09-11T12:00:00Z', confirmed_at: null, confirmed_plan_id: confirmed ? 'plan' : null, completed_at: null, retained_bytes: '1024', reserved_bytes: '65536', native_row_bytes: '0', cancellation_reserved_bytes: '65536', release_ready: true, counts: counts(), latest_plan: { id: 'plan', revision: '9007199254740993', state: state === 'preparing' ? 'building' : 'ready', phase: 'ready', expires_at: '2099-01-01T12:00:00Z', counts: counts(), source_timezone: null, source_engine: 'source', html_profile: 'html', time_profile: 'time', tzdb_version: '2025b', confirmation_digest: 'digest', max_added_byte_bound: '10000' }, actions: { replan: ['ready', 'preparing'].includes(state), confirm: state === 'ready', retry: state === 'paused', cancel: !['completed', 'cancelled'].includes(state) }, policy: { run_byte_limit: '268435456', org_byte_limit: '1073741824', run_ceiling_bytes: '536870912', org_ceiling_bytes: '1073741824', run_retained_bytes: '1024', run_reserved_bytes: '65536', org_retained_bytes: '1024', org_reserved_bytes: '65536', unit_byte_limit: '67108864', policy_revision: 'policy' }, coverage: { remaining_data: ['note_replies', 'recurrence', 'activation'], native_review_only: true } }
}
const summary = { fields: [], total_fields: '0', abbreviated: false, field_key: 'source.all' }
function mapping(role: string) { return { id: `mapping-${role}`, role, source_value: role === 'task_kind' ? 'Appointment' : '42', source_value_abbreviated: false, source_value_full_utf8_bytes: '2', choice: { kind: 'hold' }, suggestions: [], suggested_kind: role === 'task_kind' ? 'other' : null, dependent_count: '2', source_summary: summary, field_url: 'https://untrusted.invalid' } }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { resolve, promise } }
async function setup(options: { run?: ActivityImport | null; role?: string; missingStream?: string; snapshotState?: string; parentState?: string; real?: boolean; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { run: options.run === undefined ? run() : options.run, identity: identity(options.role) }
  const parent = { id: 'parent', state: options.parentState ?? 'completed', confirmed_plan_id: 'parent-plan', snapshot_id: 'snapshot', created_at: '2026-09-11T10:00:00Z' }
  api.mockImplementation(async (url, init) => {
    const custom = options.handle?.(url, init); if (custom !== undefined) return await custom as never
    if (url === '/me') return state.identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [parent], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return parent as never
    if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { imports: state.run ? [state.run] : [], next_cursor: null } as never
    if (url === `${ROOT}/child` && !init?.method) return state.run as never
    if (url === '/migrations/fub/snapshots/snapshot') return { snapshot: { id: 'snapshot', state: options.snapshotState ?? 'completed' }, streams: ['people', 'users', 'notes', 'note_detail', 'tasks_open', 'tasks_completed'].map(stream => ({ stream, state: stream === options.missingStream ? 'paused' : 'completed' })) } as never
    if (url.includes('/mappings?')) return { items: [mapping(new URL(url, 'https://local.invalid').searchParams.get('kind')!)], next_cursor: null } as never
    if (url.includes('/records?') || url.includes('/results?')) return { items: [], next_cursor: null } as never
    if (url.includes('/targets?')) return { items: [{ id: 'inactive', display_name: 'Historical author', status: 'inactive', role: 'member' }, { id: 'active', display_name: 'Current member', status: 'active', role: 'member' }], next_cursor: null } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, state.identity) })
  const wrapper = mount(ActivityImportPanel, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { ActivityMappingPanel: !options.real, ActivityRecordPanel: !options.real, SnapshotBudgetPanel: true, RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, state, refreshWorkspace }
}
function button(label: string): HTMLButtonElement { const element = [...document.body.querySelectorAll('button')].find(el => el.textContent?.trim() === label); expect(element, label).toBeDefined(); return element! }
const writes = (suffix = '') => api.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix))
async function ack(wrapper: Awaited<ReturnType<typeof setup>>['wrapper']) { for (const el of wrapper.findAll('input[type=checkbox]')) await el.setValue(true); button('Review notes and tasks confirmation').click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace(); vi.stubGlobal('crypto', { randomUUID: () => 'request-uuid' }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('activity import workflow', () => {
  it('prepares only from exhausted retained streams without a metadata prerequisite or source call', async () => {
    const { state } = await setup({ run: null, handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run('preparing'); return { import: state.run } } } })
    expect(button('Prepare notes and tasks plan').disabled).toBe(false); expect(writes()).toHaveLength(0)
    button('Prepare notes and tasks plan').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: 'request-uuid', parent_import_id: 'parent' })
    expect(api.mock.calls.some(([url]) => /connections|assessments|metadata-imports/.test(url))).toBe(false)
  })
  it.each(['people', 'users', 'notes', 'note_detail', 'tasks_open', 'tasks_completed'])('requires exhausted %s alongside the other five streams', async missingStream => { await setup({ run: null, missingStream }); expect(button('Prepare notes and tasks plan').disabled).toBe(true) })
  it.each([{ parentState: 'paused' }, { parentState: 'cancelled' }, { snapshotState: 'running' }])('rejects an ineligible parent/snapshot %o', async options => { await setup({ run: null, ...options }); expect(button('Prepare notes and tasks plan').disabled).toBe(true); expect(writes()).toHaveLength(0) })
  it('freezes exact child revision and count acknowledgments in the named final dialog', async () => {
    const { wrapper, state, refreshWorkspace } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('queued'); return { import: state.run } } } })
    expect(button('Review notes and tasks confirmation').disabled).toBe(true)
    await ack(wrapper); expect(writes()).toHaveLength(0)
    expect(document.body.querySelector('[role=dialog]')?.textContent).toContain('9007199254740993 source-only components')
    button('Confirm notes and tasks import').click(); await flushPromises()
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', plan_id: 'plan', expected_revision: '9007199254740995', acknowledge_held: '2', acknowledge_source_only: '9007199254740993' })
    expect(refreshWorkspace).not.toHaveBeenCalled()
  })
  it('requires explicit timezone agreement, applies mappings with the zone and closes a dirty confirmation', async () => {
    const { wrapper, state } = await setup({ real: true, handle: (url, init) => { if (url.endsWith('/plans') && init?.method === 'POST') { state.run = run(); state.run.latest_plan = { ...state.run.latest_plan, id: 'plan2', source_timezone: 'America/Los_Angeles' }; return { import: state.run } } } })
    await ack(wrapper); expect(document.body.querySelector('[role=dialog]')).not.toBeNull()
    await wrapper.get('select[aria-label="Note author choice for 42"]').setValue('leave_unmapped')
    expect(document.body.querySelector('[role=dialog]')).toBeNull()
    await wrapper.get('input[placeholder="America/Los_Angeles"]').setValue('America/Los_Angeles')
    expect(button('Apply activity choices').disabled).toBe(true); expect(button('Prepare fresh activity plan').disabled).toBe(true)
    const zoneAck = wrapper.findAll('label').find(el => el.text().includes('I confirm this source timezone'))!
    await zoneAck.get('input').setValue(true)
    button('Apply activity choices').click(); await flushPromises()
    expect(JSON.parse(String(writes('/plans')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', expected_plan_id: 'plan', choices: [{ mapping_id: 'mapping-note_author', choice: { kind: 'leave_unmapped' } }], source_timezone: 'America/Los_Angeles' })
    expect(wrapper.text()).toContain('Confirmed zone: America/Los_Angeles'); expect(button('Review notes and tasks confirmation').disabled).toBe(true)
  })
  it('clears an applied zone explicitly and discards a changed draft without silently applying it', async () => {
    const initial = run(); initial.latest_plan.source_timezone = 'America/Los_Angeles'
    const { wrapper } = await setup({ run: initial, handle: url => url.endsWith('/plans') ? { import: initial } : undefined })
    await wrapper.get('input[placeholder="America/Los_Angeles"]').setValue('America/New_York')
    button('Discard activity choices').click(); await flushPromises(); expect(writes()).toHaveLength(0)
    expect((wrapper.get('input[placeholder="America/Los_Angeles"]').element as HTMLInputElement).value).toBe('America/Los_Angeles')
    await wrapper.get('input[placeholder="America/Los_Angeles"]').setValue('')
    button('Apply activity choices').click(); await flushPromises()
    expect(JSON.parse(String(writes('/plans')[0]![1]?.body))).toMatchObject({ source_timezone: null, choices: [] })
  })
  it('allows inactive historical authors but forbids inactive task assignees', async () => {
    const { wrapper } = await setup({ real: true })
    button('Choose CRM user').click(); await flushPromises()
    expect(document.body.querySelector<HTMLButtonElement>('[aria-label="Use Historical author"]')!.disabled).toBe(false)
    document.body.querySelector<HTMLButtonElement>('[aria-label="Use Historical author"]')!.click(); await flushPromises()
    expect(wrapper.text()).toContain('Use Historical author')
    button('Task assignee').click(); await flushPromises(); button('Choose CRM user').click(); await flushPromises()
    expect(document.body.querySelector<HTMLButtonElement>('[aria-label="Use Historical author"]')!.disabled).toBe(true)
    expect(document.body.querySelector<HTMLButtonElement>('[aria-label="Use Current member"]')!.disabled).toBe(false)
  })
  it('treats kind suggestions as drafts until explicitly selected and applied', async () => {
    const { wrapper } = await setup({ real: true })
    button('Task kind').click(); await flushPromises(); expect(writes()).toHaveLength(0)
    expect(wrapper.text()).toContain('Suggested kind: Other')
    await wrapper.get('select[aria-label="Task kind choice for Appointment"]').setValue('map_kind:other')
    expect(button('Apply activity choices').disabled).toBe(false); expect(writes()).toHaveLength(0)
  })
  it('replays the exact lost confirmation while fresh reads keep completed state', async () => {
    let attempts = 0
    const { wrapper, state } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('completed'); return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { import: run('queued') } } } })
    await ack(wrapper); button('Confirm notes and tasks import').click(); await flushPromises()
    button('Retry the same activity request').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(2); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
    expect(wrapper.text()).toContain('Activity import · Completed'); expect(wrapper.text()).not.toContain('Retry the same activity request')
  })
  it('discards late receipts and private state after the actor changes', async () => {
    const pending = deferred<{ import: ActivityImport }>()
    const { wrapper, client, state } = await setup({ handle: url => url.endsWith('/confirm') ? pending.promise : undefined })
    await ack(wrapper); button('Confirm notes and tasks import').click(); await flushPromises()
    state.identity = identity('member', 'other'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ import: run('queued') }); await flushPromises()
    expect(wrapper.text()).not.toContain('900719925474099312345'); expect(wrapper.text()).not.toContain('Retry the same activity request')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'activity-imports'] }).some(q => q.state.data !== undefined)).toBe(false)
  })
  it('requires separate allowance refresh, explicit retry and named terminal cancellation', async () => {
    const { wrapper } = await setup({ run: run('paused'), handle: url => url.endsWith('/retry') || url.endsWith('/cancel') ? { import: run('paused') } : undefined })
    wrapper.findComponent({ name: 'SnapshotBudgetPanel' }).vm.$emit('refresh'); await flushPromises(); expect(writes()).toHaveLength(0)
    button('Resume activity import').click(); await flushPromises(); expect(JSON.parse(String(writes('/retry')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', expected_revision: '9007199254740995' })
    button('Cancel activity import').click(); await flushPromises(); expect(writes('/cancel')).toHaveLength(0)
    button('Permanently cancel activity import').click(); await flushPromises(); expect(writes('/cancel')).toHaveLength(1)
  })
  it.each(['ready', 'paused', 'completed', 'cancelled'] as const)('does not automatically poll %s', async state => { vi.useFakeTimers(); await setup({ run: run(state) }); const before = api.mock.calls.length; await vi.advanceTimersByTimeAsync(60_000); await flushPromises(); expect(api.mock.calls).toHaveLength(before) })
  it('refreshes same-plan mappings when preparation completes and stops after ready', async () => {
    vi.useFakeTimers(); let prepared = false
    const { wrapper, state } = await setup({ real: true, run: run('preparing'), handle: url => !prepared && url.includes('/mappings?') ? { items: [], next_cursor: null } : undefined })
    expect(wrapper.text()).toContain('No mappings for this role')
    prepared = true; state.run = run(); await vi.advanceTimersByTimeAsync(2000); await flushPromises()
    expect(wrapper.find('select[aria-label="Note author choice for 42"]').exists()).toBe(true)
    const before = api.mock.calls.length; await vi.advanceTimersByTimeAsync(60_000); await flushPromises(); expect(api.mock.calls).toHaveLength(before)
  })
  it('never reads activity data for a member or creates a replacement after cancellation', async () => {
    const member = await setup({ role: 'member' }); expect(api.mock.calls.filter(([url]) => url.startsWith('/migrations/'))).toHaveLength(0); member.wrapper.unmount()
    const { wrapper } = await setup({ run: run('cancelled') }); expect(wrapper.text()).toContain('permanently cancelled'); expect(wrapper.text()).not.toContain('Prepare notes and tasks plan'); expect(wrapper.text()).not.toContain('Resume activity import')
  })
})
