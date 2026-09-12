import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { HistoryCapture, HistoryStream } from '../../api/historyCaptures'
import { resetWorkspace } from '../../workspaceLifecycle'
import HistoryCapturePanel from './HistoryCapturePanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/history-captures'
const cleanup: Array<() => void> = []
const identity = (role = 'admin', actor = 'actor', org = 'org') => ({ user: { id: actor, email: 'synthetic@test.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Org', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false })
const connection = { id: 'connection', revision: 2, status: 'connected', source_account_id: 17, source_display_name: null, source_access_scope: 'unknown', created_at: '2026-09-12T00:00:00Z', updated_at: '2026-09-12T00:00:00Z' }
function run(state: HistoryCapture['state'] = 'proposed'): HistoryCapture {
  return { id: 'capture', parent_import_id: 'parent', parent_plan_id: 'plan', snapshot_id: 'snapshot', parent_capture_sequence: '9', connection_id: 'connection', connection_revision: '2', source_account_id: '9007199254740993123', source_user_id: '9', parent_source_user_id: '3', source_user_difference: true, source_user_evidence_revision: '9007199254740993', profile_version: 'fub-history-v1', schema_version: 'sha256:synthetic', parser_version: '1', initiated_by_user_id: 'actor', state, revision: '9007199254740995', pause_reason: state === 'paused' ? 'storage_limit' : null, created_at: '2026-09-12T01:00:00Z', proposal_expires_at: '2099-01-01T00:00:00Z', started_at: null, completed_at: null, confirmed_at: state === 'proposed' ? null : '2026-09-12T02:00:00Z', parent_source_started_at: '2026-09-01T00:00:00Z', parent_source_completed_at: '2026-09-02T00:00:00Z', capture_sequence: '2', raw_bytes: '256', retained_bytes: '1024', reserved_bytes: '8192', run_byte_limit: '1073741824', org_byte_limit: '2147483648', org_retained_bytes: '4096', org_reserved_bytes: '8192', run_budget_revision: '9007199254740996', org_budget_revision: '9007199254740997', policy_revision: 'policy', run_ceiling_bytes: '4294967296', org_ceiling_bytes: '8589934592', required_reservation_bytes: '16777216', release_ready: true, coverage_reasons: ['api_restricted_records_unknown', 'retained_not_imported'], actions: { confirm: state === 'proposed', retry: state === 'paused', cancel: !['cancelled', 'completed_with_gaps'].includes(state), increase_budget: !['cancelled', 'completed_with_gaps'].includes(state) }, streams: ['events', 'calls', 'text_messages'].map(family => ({ family, state: 'pending', checkpoint: '0', reported_total: null, occurrences: '0', valid_occurrences: '0', invalid_occurrences: '0', unique_ids: '0', equal_repeats: '0', conflicting_variants: '0', linked: '0', parent_excluded: '0', no_parent_identity: '0', invalid_person_reference: '0', conflicting_reference: '0', attempts: '0', api_inaccessible_count: null, count_basis: 'advancing_pages', content_scope: 'exact_returned_json_only', enumeration_is_complete_account_history: false }) as HistoryStream) }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { resolve, promise } }
async function setup(options: { run?: HistoryCapture | null; role?: string; sourceBusy?: boolean; parentState?: string; realRecords?: boolean; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { run: options.run === undefined ? run() : options.run, identity: identity(options.role) }
  const parent = { id: 'parent', state: options.parentState ?? 'completed', confirmed_plan_id: 'plan', snapshot_id: 'snapshot', created_at: '2026-09-01T00:00:00Z' }
  api.mockImplementation(async (url, init) => {
    const value = options.handle?.(url, init); if (value !== undefined) return await value as never
    if (url === '/me') return state.identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [parent], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return parent as never
    if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { captures: state.run ? [state.run] : [], next_cursor: null } as never
    if (url === `${ROOT}/capture` && !init?.method) return state.run as never
    if (url.includes('/records?')) return { records: [], next_cursor: null, capture_sequence: '2', counter_basis: 'current_run', counts: [] } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, state.identity) })
  const wrapper = mount(HistoryCapturePanel, { props: { connection, refreshWorkspace, sourceBusy: options.sourceBusy }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { HistoryCaptureRecords: !options.realRecords } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, state, refreshWorkspace }
}
function button(label: string): HTMLButtonElement { const value = [...document.body.querySelectorAll('button')].find(element => element.textContent?.trim() === label); expect(value, label).toBeDefined(); return value! }
const writes = (suffix = '') => api.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix))
async function ack(wrapper: Awaited<ReturnType<typeof setup>>['wrapper']) { for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true); button('Review historical capture confirmation').click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace(); vi.stubGlobal('crypto', { randomUUID: () => 'exact-request' }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('historical capture workflow', () => {
  it('prepares DB-only from a completed parent during other source work and supports another attempt', async () => {
    const { state } = await setup({ run: run('cancelled'), sourceBusy: true, handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run(); return { capture_id: 'capture', revision: '1', state: 'proposed' } } } })
    expect(button('Prepare historical capture').disabled).toBe(false)
    button('Prepare historical capture').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: 'exact-request', parent_import_id: 'parent', connection_id: 'connection', expected_revision: '2' })
    expect(api.mock.calls.some(([url]) => /snapshots|metadata-imports|activity-imports|assessments|connections/.test(url))).toBe(false)
  })
  it('requires the named scope and changed-user acknowledgements before freezing confirmation', async () => {
    const { wrapper } = await setup({ handle: url => url.endsWith('/confirm') ? { capture_id: 'capture', state: 'queued', revision: '2' } : undefined })
    const checkboxes = wrapper.findAll('input[type=checkbox]'); expect(checkboxes).toHaveLength(4)
    for (const checkbox of checkboxes.slice(0, 3)) await checkbox.setValue(true)
    expect(button('Review historical capture confirmation').disabled).toBe(true)
    await checkboxes[3]!.setValue(true); button('Review historical capture confirmation').click(); await flushPromises()
    expect(writes()).toHaveLength(0); expect(document.body.querySelector('[role=dialog]')?.textContent).toContain('9007199254740993123')
    button('Start historical capture').click(); await flushPromises()
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: 'exact-request', expected_run_revision: '9007199254740995', acknowledgements: { api_visible_account_scope: true, coverage_gaps: true, retained_not_imported: true, source_user_evidence_revision: '9007199254740993', source_user_difference: true } })
  })
  it('recovers a lost response with the exact original body while fresh GET keeps completed state', async () => {
    let attempts = 0
    const { wrapper, state } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('completed_with_gaps'); return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { capture_id: 'capture', state: 'queued', revision: '2' } } } })
    await ack(wrapper); button('Start historical capture').click(); await flushPromises()
    expect(button('Prepare historical capture').disabled).toBe(true)
    button('Retry the same historical capture request').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(2); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
    expect(wrapper.text()).toContain('Historical capture · Completed with gaps'); expect(wrapper.text()).not.toContain('Retry the same historical capture request')
  })
  it('keeps retained review and cancellation after disconnect but fences source confirmation', async () => {
    const { wrapper } = await setup({ handle: url => url.endsWith('/cancel') ? { capture_id: 'capture', state: 'cancelled', revision: '2' } : undefined })
    await ack(wrapper); await wrapper.setProps({ connection: { ...connection, status: 'disconnected' } }); await flushPromises()
    expect(document.body.querySelector('[role=dialog]')).toBeNull(); expect(button('Prepare historical capture').disabled).toBe(true)
    expect(button('Review historical capture confirmation').disabled).toBe(true); expect(wrapper.text()).toContain('9007199254740993123')
    button('Cancel historical capture').click(); await flushPromises(); expect(writes()).toHaveLength(0)
    button('Permanently cancel historical capture').click(); await flushPromises(); expect(writes('/cancel')).toHaveLength(1)
  })
  it('freezes every budget revision, enforces ceilings and requires a separate explicit Resume', async () => {
    const { wrapper, state } = await setup({ run: run('paused'), handle: url => { if (url.endsWith('/budget')) { state.run = { ...run('paused'), revision: '9007199254740996', run_byte_limit: '2147483648' }; return { capture_id: 'capture', state: 'paused', revision: state.run.revision } } if (url.endsWith('/retry')) return { capture_id: 'capture', state: 'queued', revision: '3' } } })
    button('Review historical storage allowances').click(); await flushPromises()
    await wrapper.get('[data-testid=history-run-budget]').setValue('5'); button('Review historical allowance increase').click(); await flushPromises(); expect(wrapper.text()).toContain('exceed the deployment ceilings'); expect(writes()).toHaveLength(0)
    await wrapper.get('[data-testid=history-run-budget]').setValue('2'); button('Review historical allowance increase').click(); await flushPromises(); expect(writes()).toHaveLength(0)
    button('Increase allowances').click(); await flushPromises()
    expect(JSON.parse(String(writes('/budget')[0]![1]?.body))).toEqual({ request_id: 'exact-request', expected_run_revision: '9007199254740995', expected_run_budget_revision: '9007199254740996', expected_org_budget_revision: '9007199254740997', expected_policy_revision: 'policy', run_byte_limit: '2147483648', org_byte_limit: '2147483648' })
    expect(writes('/retry')).toHaveLength(0); button('Resume historical capture').click(); await flushPromises(); expect(writes('/retry')).toHaveLength(1)
  })
  it('discards late receipts and scoped cached content after administrator demotion', async () => {
    const delayed = deferred<unknown>()
    const { wrapper, state, client } = await setup({ handle: url => url.endsWith('/confirm') ? delayed.promise : undefined })
    await ack(wrapper); button('Start historical capture').click(); await flushPromises()
    state.identity = identity('member'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    delayed.resolve({ capture_id: 'capture', state: 'queued', revision: '2' }); await flushPromises()
    expect(wrapper.text()).not.toContain('9007199254740993123'); expect(wrapper.text()).not.toContain('Retry the same historical capture request')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'history-captures'] }).some(query => query.state.data !== undefined)).toBe(false)
  })
  it('requires exact recovery if workspace authority changes and returns while a mutation is pending', async () => {
    const delayed = deferred<unknown>(); let attempts = 0
    const { wrapper, state, client } = await setup({ handle: url => url.endsWith('/confirm') ? ++attempts === 1 ? delayed.promise : { capture_id: 'capture', state: 'queued', revision: '2' } : undefined })
    await ack(wrapper); button('Start historical capture').click(); await flushPromises()
    client.setQueryData(queryKeys.me, { ...state.identity, organization: { ...state.identity.organization, workspace_revision: '3' } }); await flushPromises()
    client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    delayed.resolve({ capture_id: 'capture', state: 'queued', revision: '2' }); await flushPromises()
    expect(wrapper.text()).toContain('Access changed while this request was pending')
    button('Retry the same historical capture request').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(2); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
  })
  it('polls active detail without replacing the frozen observation page series', async () => {
    vi.useFakeTimers()
    const { state } = await setup({ run: run('running'), realRecords: true })
    const before = api.mock.calls.filter(([url]) => url.includes('/records?')).length
    state.run = { ...run('running'), revision: '9007199254740996', capture_sequence: '3' }
    await vi.advanceTimersByTimeAsync(2100); await flushPromises()
    expect(api.mock.calls.filter(([url]) => url.includes('/records?'))).toHaveLength(before)
    expect(document.body.textContent).toContain('fixed capture boundary 2. Current capture progress: 3')
  })
  it('clears source busy when an unselected active run settles without refreshing observation pages', async () => {
    vi.useFakeTimers()
    let active = run('running')
    const terminal = { ...run('completed_with_gaps'), id: 'other-capture' }
    const { wrapper } = await setup({ run: active, realRecords: true, handle: url => {
      if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { captures: [active, terminal], next_cursor: null }
      if (url === `${ROOT}/other-capture`) return terminal
    } })
    expect(wrapper.emitted('sourceBusy')?.at(-1)).toEqual([true])
    await wrapper.findAll('select')[1]!.setValue('other-capture'); await flushPromises()
    expect(wrapper.text()).toContain('Historical capture · Completed with gaps')
    expect(wrapper.emitted('sourceBusy')?.at(-1)).toEqual([true])
    const recordReads = api.mock.calls.filter(([url]) => url.includes('/records?')).length
    const listReads = () => api.mock.calls.filter(([url]) => url === `${ROOT}?parent_import_id=parent&limit=20`).length
    expect(listReads()).toBe(1)
    active = run('completed_with_gaps')
    await vi.advanceTimersByTimeAsync(2100); await flushPromises()
    expect(listReads()).toBe(2); expect(wrapper.emitted('sourceBusy')?.at(-1)).toEqual([false])
    expect(api.mock.calls.filter(([url]) => url.includes('/records?'))).toHaveLength(recordReads)
    const settledReads = api.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000); await flushPromises()
    expect(api.mock.calls).toHaveLength(settledReads)
  })
  it.each(['proposed', 'paused', 'completed_with_gaps', 'cancelled'] as const)('stops automatic polling in %s state', async state => { vi.useFakeTimers(); await setup({ run: run(state) }); const before = api.mock.calls.length; await vi.advanceTimersByTimeAsync(60_000); await flushPromises(); expect(api.mock.calls).toHaveLength(before) })
  it.each(['member', 'inactive'])('does not read historical data for a nonadmin %s', async role => { await setup({ role }); expect(api.mock.calls.filter(([url]) => url.startsWith('/migrations'))).toHaveLength(0) })
  it('honors the server expiry/action gate and current initiator even with checked acknowledgements', async () => { const value = run(); value.actions.confirm = false; value.initiated_by_user_id = 'other-admin'; const { wrapper } = await setup({ run: value }); await ack(wrapper); expect(button('Review historical capture confirmation').disabled).toBe(true); expect(writes()).toHaveLength(0) })
})
