import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys, useAuthSessionLifetime } from '../../api/queries'
import type { HistoryImport } from '../../api/historyImports'
import { resetWorkspace } from '../../workspaceLifecycle'
import HistoryImportPanel from './HistoryImportPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/history-imports'
const cleanup: Array<() => void> = []
const identity = (role = 'admin', actor = 'actor', org = 'org') => ({ user: { id: actor, email: 'synthetic@test.invalid', display_name: actor }, organization: { id: org, name: org, role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false })
function run(state: HistoryImport['state'] = 'ready', id = 'run'): HistoryImport {
  return { id, plan_id: 'frozen-plan', parent_import_id: 'parent', capture_id: 'capture', state, phase: state === 'ready' ? 'classify' : 'apply', revision: '9007199254740993123', plan_revision: '9007199254740993122', workspace_revision: '2', interpretation_version: 'fub-history-interpretation-v1', reader_version: 'fub-history-timeline-v1', capture_revision: '9007199254740993121', capture_sequence: '7', parent_capture_sequence: '9', executor_user_id: 'actor', created_at: '2026-09-12T01:00:00Z', updated_at: '2026-09-12T01:00:00Z', confirmed_at: ['ready', 'preparing'].includes(state) ? null : '2026-09-12T01:00:00Z', completed_at: null, plan_expires_at: '2099-01-01T00:00:00Z', pause_reason: state === 'paused' ? 'storage_limit' : null, preview_complete: true, counts: { occurrences: '4', eligible: '2', equal_repeats: '1', held: '1', processed: '0', inserted: '0', already_imported: '0', application_held: '0' }, coverage: { streams: [], warnings: ['api_restricted_records_unknown', 'external_facts_only'], api_inaccessible_count: null, enumeration_is_complete_account_history: false }, added_byte_bound: '131072', retained_bytes: '1024', reserved_bytes: '8192', run_byte_limit: '1073741824', run_budget_revision: '8', org_byte_limit: '2147483648', org_budget_revision: '9', policy_revision: 'policy', run_byte_ceiling: '4294967296', org_byte_ceiling: '8589934592', release_ready: true, actions: { confirm: state === 'ready', resume: state === 'paused', cancel: !['completed', 'cancelled'].includes(state), increase_budget: !['completed', 'cancelled'].includes(state), prepare_same_plan: state === 'cancelled' } }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { promise, resolve } }
async function setup(options: { run?: HistoryImport | null; handle?: (url: string, init?: RequestInit) => unknown; others?: HistoryImport[] } = {}) {
  const state = { run: options.run === undefined ? run() : options.run, identity: identity(), others: options.others ?? [] }
  api.mockImplementation(async (url, init) => {
    const handled = options.handle?.(url, init); if (handled !== undefined) return await handled as never
    if (url === '/me') return state.identity as never
    if (url.startsWith('/migrations/fub/imports?')) return { imports: [{ id: 'parent', state: 'completed', created_at: '2026-09-01T00:00:00Z' }], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return { id: 'parent', state: 'completed', confirmed_plan_id: 'parent-plan' } as never
    if (url.startsWith('/migrations/fub/history-captures?')) return { captures: [{ id: 'capture', parent_import_id: 'parent', state: 'completed_with_gaps', created_at: '2026-09-12T00:00:00Z' }], next_cursor: null } as never
    if (url === '/migrations/fub/history-captures/capture') return { id: 'capture', parent_import_id: 'parent', state: 'completed_with_gaps', revision: '9007199254740993121', policy_revision: 'policy' } as never
    if (url.startsWith(`${ROOT}?`)) return { imports: [...(state.run ? [state.run] : []), ...state.others], next_cursor: null } as never
    if (url.startsWith(`${ROOT}/`) && !init?.method) return (state.run?.id === url.slice(ROOT.length + 1) ? state.run : state.others.find(run => run.id === url.slice(ROOT.length + 1))) as never
    throw new Error(`Unexpected route ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => {})
  const wrapper = mount(HistoryImportPanel, { props: { refreshWorkspace }, attachTo: document.body, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { HistoryImportRecords: true } } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, state, refreshWorkspace }
}
function button(label: string) { const value = [...document.querySelectorAll('button')].find(button => button.textContent?.trim() === label); if (!value) throw new Error(`Missing ${label}`); return value }
async function click(label: string) { button(label).click(); await flushPromises() }
const writes = (suffix = '') => api.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix))
async function confirmReview(wrapper: Awaited<ReturnType<typeof setup>>['wrapper']) { for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true); await click('Review confirmation') }
beforeEach(() => { api.mockReset(); resetWorkspace(); vi.stubGlobal('crypto', { randomUUID: () => 'exact-request' }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('retained history import workflow', () => {
  it.each(['ready', 'paused'] as const)('explains missing release readiness for %s even when server actions are disabled', async state => {
    const unavailable = run(state)
    unavailable.release_ready = false; unavailable.actions.confirm = false; unavailable.actions.resume = false
    const { wrapper } = await setup({ run: unavailable })
    expect(wrapper.text()).toContain('Compatible deployment readiness is required before confirming or resuming.')
    expect(wrapper.text()).not.toContain('Review confirmation')
    expect(wrapper.text()).not.toContain('Resume history import')
    expect(writes()).toHaveLength(0)
  })
  it('prepares from the completed capture without a source connection or source calls', async () => {
    const { state } = await setup({ run: null, handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run(); return { import_id: 'run', plan_id: 'frozen-plan', revision: '1', state: 'preparing' } } } })
    expect(button('Prepare history import').disabled).toBe(false); await click('Prepare history import')
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: 'exact-request', parent_import_id: 'parent', capture_id: 'capture', expected_capture_revision: '9007199254740993121', expected_workspace_revision: '2', expected_policy_revision: 'policy' })
    expect(api.mock.calls.some(([url]) => /connections|assessments|snapshots|identity/.test(url))).toBe(false)
  })
  it('requires four acknowledgements and freezes the exact plan/revisions before confirmation', async () => {
    const { wrapper } = await setup({ handle: url => url.endsWith('/confirm') ? { import_id: 'run', plan_id: 'frozen-plan', revision: '2', state: 'queued' } : undefined })
    expect(button('Review confirmation').disabled).toBe(true)
    await confirmReview(wrapper); expect(writes()).toHaveLength(0)
    expect(document.querySelector('[role=dialog]')?.textContent).toContain('2 eligible historical records')
    await click('Import records')
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: 'exact-request', plan_id: 'frozen-plan', expected_revision: '9007199254740993123', expected_plan_revision: '9007199254740993122', expected_workspace_revision: '2', expected_policy_revision: 'policy', acknowledgements: { external_facts: true, date_uncertainty: true, coverage_and_holds: true, review_only: true } })
  })
  it('replays a lost response with the exact body despite completed fresh GET and stale queued receipt', async () => {
    let attempts = 0
    const { wrapper, state } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('completed'); return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { import_id: 'run', plan_id: 'frozen-plan', revision: '2', state: 'queued' } } } })
    await confirmReview(wrapper); await click('Import records'); expect(wrapper.text()).toContain('History import · Completed')
    expect(button('Prepare history import').disabled).toBe(true)
    await click('Retry same import request')
    expect(writes('/confirm')).toHaveLength(2); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
    expect(wrapper.text()).toContain('History import · Completed'); expect(wrapper.text()).not.toContain('Retry same import request')
  })
  it('freezes all budget revisions, rejects a ceiling breach and resumes only after a separate reviewed action', async () => {
    const { wrapper, state } = await setup({ run: run('paused'), handle: url => { if (url.endsWith('/budget')) { state.run = { ...run('paused'), revision: '9007199254740993124', run_byte_limit: '2147483648' }; return { import_id: 'run', plan_id: 'frozen-plan', state: 'paused', revision: state.run.revision } } if (url.endsWith('/resume')) return { import_id: 'run', plan_id: 'frozen-plan', revision: '3', state: 'queued' } } })
    await click('Review storage allowances')
    await wrapper.get('[data-testid=history-import-run-budget]').setValue('5'); await click('Review increase'); expect(wrapper.text()).toContain('exceed the deployment ceilings'); expect(writes()).toHaveLength(0)
    await wrapper.get('[data-testid=history-import-run-budget]').setValue('2'); await click('Review increase'); await click('Increase allowances')
    expect(JSON.parse(String(writes('/budget')[0]![1]?.body))).toEqual({ request_id: 'exact-request', expected_revision: '9007199254740993123', expected_run_budget_revision: '8', expected_org_budget_revision: '9', expected_policy_revision: 'policy', run_byte_limit: '2147483648', org_byte_limit: '2147483648' })
    expect(writes('/resume')).toHaveLength(0); await click('Resume history import'); expect(writes('/resume')).toHaveLength(0); await click('Resume import'); expect(writes('/resume')).toHaveLength(1)
  })
  it('offers same-plan preparation after cancellation and preserves the chosen parent/capture', async () => {
    const { state } = await setup({ run: run('cancelled'), handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run('ready', 'continuation'); return { import_id: 'continuation', plan_id: 'frozen-plan', revision: '1', state: 'ready' } } } })
    expect(button('Prepare history import').disabled).toBe(false); await click('Prepare history import')
    expect(JSON.parse(String(writes()[0]![1]?.body)).capture_id).toBe('capture')
    expect(api.mock.calls.some(([url]) => url.endsWith('/continuation'))).toBe(true)
  })
  it.each(['actor', 'org', 'role', 'session'] as const)('discards late mutation receipts on %s changes', async transition => {
    const delayed = deferred<unknown>()
    const { wrapper, client } = await setup({ handle: url => url.endsWith('/confirm') ? delayed.promise : undefined })
    await confirmReview(wrapper); await click('Import records')
    if (transition === 'session') useAuthSessionLifetime().value++
    else client.setQueryData(queryKeys.me, identity(transition === 'role' ? 'member' : 'admin', transition === 'actor' ? 'another' : 'actor', transition === 'org' ? 'another' : 'org'))
    delayed.resolve({ import_id: 'late-private-id', plan_id: 'frozen-plan', revision: '2', state: 'queued' }); await flushPromises()
    expect(wrapper.text()).not.toContain('late-private-id'); expect(wrapper.text()).not.toContain('Retry same import request')
    expect(api.mock.calls.some(([url]) => url.endsWith('/late-private-id'))).toBe(false)
  })
  it('preserves uncertain recovery across a workspace change and return without changing the reviewed body', async () => {
    const delayed = deferred<unknown>(); let attempts = 0
    const { wrapper, client, state } = await setup({ handle: url => url.endsWith('/confirm') ? ++attempts === 1 ? delayed.promise : { import_id: 'run', plan_id: 'frozen-plan', revision: '2', state: 'queued' } : undefined })
    await confirmReview(wrapper); await click('Import records')
    client.setQueryData(queryKeys.me, { ...state.identity, organization: { ...state.identity.organization, workspace_revision: '3' } }); await flushPromises()
    client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    delayed.resolve({ import_id: 'run', plan_id: 'frozen-plan', revision: '2', state: 'queued' }); await flushPromises()
    expect(wrapper.text()).toContain('Access changed while this request was pending')
    await click('Retry same import request'); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
  })
  it('polls an unselected active attempt until terminal without changing the selected completed attempt', async () => {
    vi.useFakeTimers()
    const { state, wrapper } = await setup({ run: run('completed'), others: [run('running', 'other')] })
    const before = api.mock.calls.filter(([url]) => url.startsWith(`${ROOT}?`)).length
    state.others = [run('completed', 'other')]
    await vi.advanceTimersByTimeAsync(2100); await flushPromises()
    expect(api.mock.calls.filter(([url]) => url.startsWith(`${ROOT}?`)).length).toBeGreaterThan(before)
    const after = api.mock.calls.length; await vi.advanceTimersByTimeAsync(60000); await flushPromises()
    expect(api.mock.calls).toHaveLength(after); expect(wrapper.text()).toContain('Attempt run · plan frozen-plan')
  })
})
