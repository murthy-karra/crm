import { flushPromises, mount, RouterLinkStub } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import type { AdmittedPeopleRefresh, RefreshItemDetail } from '../../api/admittedPeopleRefreshes'
import AdmittedPeopleRefreshPanel from './AdmittedPeopleRefreshPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch); const ROOT = '/migrations/fub/admitted-people-refreshes'; const REPORTS = '/migrations/fub/core-change-reports'
const cleanup: Array<() => void> = []
const identity = (role = 'admin', actor = 'actor', org = 'org', revision = '2') => ({ user: { id: actor, email: 'synthetic@test.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Synthetic', role, workspace_mode: 'migration_review', workspace_revision: revision }, platform_admin: false })
function refresh(state: AdmittedPeopleRefresh['state'] = 'ready'): AdmittedPeopleRefresh {
  return { id: 'refresh', admission_id: 'admission', parent_import_id: 'parent', report_id: 'report', state, lifecycle_revision: '4', created_at: '2026-09-12T00:00:00Z', updated_at: '2026-09-12T00:00:00Z', pause_reason: state === 'paused' ? 'storage_limit' : null, progress: { settled_items: '0' }, plan: { id: 'plan', revision: '2', digest: 'digest', expires_at: new Date(Date.now() + 600_000).toISOString(), counts: { eligible: '1', already_current: '2', held: '3', excluded: '4', name_clears: '1', assignment_clears: '1', contact_removals: '2', no_instruction: '5' } }, actions: { confirm: state === 'ready', repreview: state === 'ready', retry: state === 'paused', cancel: !['completed', 'cancelled'].includes(state) } }
}
const boundary = { snapshot_id: 'capture', capture_sequence: '123', profile_version: 'fub-core-v1', schema_version: 'schema', started_at: '2026-09-10T00:00:00Z', completed_at: '2026-09-10T01:00:00Z', streams: [] }
const source = { id: 'report', parent_import_id: 'parent', state: 'completed', output_revision: 'output', created_at: '2026-09-12T00:00:00Z', inputs: { baseline: boundary, newer: { ...boundary, snapshot_id: 'newer' }, source_scope: 'unknown_identity', warnings: ['effective_access_not_proven'] } }
function item(): RefreshItemDetail {
  const baseline = { first_name: 'Synthetic <script>unsafe()</script>', last_name: 'Original', stage_id: 'stage', assigned_user_id: 'member', contact_counts: { email: '2', phone: '0' } }
  return { id: 'item', source_id: '9007199254740993123', person_id: 'person', disposition: 'eligible', settled_at: null, clear_counts: { names: '1', assignments: '1', contacts: '2' }, no_instruction: ['phones'], plan_id: 'plan', plan_revision: '2', baseline, current: baseline, proposed: { ...baseline, last_name: null, assigned_user_id: null, contact_counts: { email: '0', phone: '0' } } }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { promise, resolve } }
async function setup(options: { state?: AdmittedPeopleRefresh['state']; role?: string; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { refresh: refresh(options.state), identity: identity(options.role) }
  const parent = { id: 'parent', state: 'completed', confirmed_plan_id: 'original-plan', created_at: source.created_at }
  api.mockImplementation(async (url, init) => {
    const value = options.handle?.(url, init); if (value !== undefined) return await value as never
    if (url === '/me') return state.identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [parent], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return parent as never
    if (url === '/migrations/fub/people-admissions?parent_import_id=parent&limit=20') return { items: [{ id: 'admission', state: 'completed', progress: { settled_items: '1' } }], next_cursor: null } as never
    if (url === `${REPORTS}?parent_import_id=parent&limit=20`) return { reports: [source, { ...source, id: 'unsealed', state: 'running', output_revision: null }], next_cursor: null } as never
    if (url === `${REPORTS}/report`) return source as never
    if (url === `${ROOT}?admission_id=admission&limit=20`) return { refreshes: [state.refresh], next_cursor: null } as never
    if (url === `${ROOT}/refresh`) return state.refresh as never
    if (url.startsWith(`${ROOT}/refresh/items?`)) return { items: [item()], next_cursor: null, plan_id: 'plan', plan_revision: '2' } as never
    if (url === `${ROOT}/refresh/items/item`) return item() as never
    if (url.startsWith(`${ROOT}/refresh/items/item/contacts?`)) return { contacts: [{ id: 'contact-row', side: 'baseline', change: 'remove', kind: 'email', contact_id: 'contact', import_order: 0, value: { value: 'synthetic@example.test', normalized_value: 'synthetic@example.test' } }], next_cursor: null, plan_id: 'plan', plan_revision: '2' } as never
    if (url.startsWith(`${ROOT}/refresh/results?`)) return { results: [], next_cursor: null } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, state.identity) })
  const wrapper = mount(AdmittedPeopleRefreshPanel, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { RouterLink: RouterLinkStub } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises()
  return { wrapper, client, state, refreshWorkspace }
}
function button(label: string): HTMLButtonElement { const value = [...document.body.querySelectorAll('button')].find(element => element.textContent?.trim() === label); expect(value, label).toBeDefined(); return value! }
function select(label: string): HTMLSelectElement { const text = [...document.body.querySelectorAll('label')].find(element => element.textContent?.includes(label)); expect(text, label).toBeDefined(); return document.getElementById(text!.htmlFor) as HTMLSelectElement }
async function choose(label: string, value: string) { const element = select(label); element.value = value; element.dispatchEvent(new Event('change', { bubbles: true })); await flushPromises() }
async function acknowledge() { for (const input of document.querySelectorAll<HTMLInputElement>('input[type=checkbox]')) { input.checked = true; input.dispatchEvent(new Event('change', { bubbles: true })) }; await flushPromises() }
const writes = () => api.mock.calls.filter(([, init]) => init?.method === 'POST')
beforeEach(() => { api.mockReset(); resetWorkspace(); let next = 0; vi.stubGlobal('crypto', { randomUUID: () => `request-${++next}` }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('Admitted People refresh workflow', () => {
  it('prepares only a selected sealed report and obtains current state after the receipt', async () => {
    const { wrapper } = await setup({ handle: (url, init) => url === ROOT && init?.method === 'POST' ? { refresh_id: 'refresh', state: 'preparing' } : undefined })
    expect([...select('Completed report for a new preview').options].map(v => v.value)).toEqual(['', 'report'])
    expect(button('Prepare People preview').disabled).toBe(true)
    await choose('Completed report for a new preview', 'report'); button('Prepare People preview').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ admission_id: 'admission', report_id: 'report', request_id: expect.stringMatching(/^request-\d+$/) })
    expect(wrapper.text()).toContain('Ready for review'); expect(wrapper.text()).toContain('Effective source access is not proven')
  })
  it('shows owned values safely and confirms the exact counted plan only after acknowledgment and dialog', async () => {
    await setup({ handle: url => url.endsWith('/confirm') ? { refresh_id: 'refresh', state: 'queued' } : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises()
    expect(document.body.textContent).toContain('Synthetic <script>unsafe()</script>'); expect(document.querySelector('script')).toBeNull()
    expect(document.body.textContent).toContain('Will clear'); expect(document.body.textContent).toContain('synthetic@example.test')
    expect(button('Review confirmation').disabled).toBe(true); await acknowledge(); button('Review confirmation').click(); await flushPromises()
    expect(writes()).toHaveLength(0); button('Confirm exact People plan').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: expect.stringMatching(/^request-\d+$/), plan_id: 'plan', plan_revision: 2, plan_digest: 'digest', acknowledged_eligible_count: 1, acknowledged_coverage: true, acknowledged_exclusions: true, acknowledged_name_clears: 1, acknowledged_assignment_clears: 1, acknowledged_contact_removals: 2 })
  })
  it('replays an uncertain confirmation with identical input and request ID', async () => {
    let attempts = 0
    await setup({ handle: url => url.endsWith('/confirm') ? ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { refresh_id: 'refresh', state: 'queued' } : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); await acknowledge(); button('Review confirmation').click(); await flushPromises(); button('Confirm exact People plan').click(); await flushPromises()
    expect(button('Review confirmation').disabled).toBe(true); button('Retry the same refresh request').click(); await flushPromises()
    expect(writes()).toHaveLength(2); expect(writes()[0]![1]?.body).toBe(writes()[1]![1]?.body)
  })
  it('requires a new preview when expired and never queues a no-change plan', async () => {
    const { state, wrapper } = await setup()
    state.refresh.plan!.expires_at = '2020-01-01T00:00:00Z'; await choose('Saved Admitted People refresh', 'refresh')
    expect(wrapper.text()).toContain('This preview expired'); await acknowledge(); expect(button('Review confirmation').disabled).toBe(true)
    state.refresh.plan!.expires_at = new Date(Date.now() + 60_000).toISOString(); state.refresh.plan!.counts.eligible = '0'
    button('Reload Admitted People refreshes').click(); await flushPromises(); await acknowledge(); expect(button('Review confirmation').disabled).toBe(true)
    expect(wrapper.text()).toContain('no native updates to confirm'); expect(writes()).toHaveLength(0)
  })
  it('uses expected plan/lifecycle revisions for re-preview, retry and explicitly confirmed cancellation', async () => {
    const { state } = await setup({ handle: url => ['/plans', '/retry', '/cancel'].some(suffix => url.endsWith(suffix)) ? { refresh_id: 'refresh', state: 'queued' } : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); button('Prepare a new preview').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: expect.stringMatching(/^request-\d+$/), expected_plan_revision: 2 })
    state.refresh = refresh('paused'); button('Reload Admitted People refreshes').click(); await flushPromises(); button('Resume Admitted People refresh').click(); await flushPromises()
    expect(JSON.parse(String(writes()[1]![1]?.body))).toEqual({ request_id: expect.stringMatching(/^request-\d+$/), expected_lifecycle_revision: 4 })
    button('Cancel Admitted People refresh').click(); await flushPromises(); expect(writes()).toHaveLength(2); button('Stop future refresh writes').click(); await flushPromises()
    expect(JSON.parse(String(writes()[2]![1]?.body))).toEqual({ request_id: expect.stringMatching(/^request-\d+$/), expected_lifecycle_revision: 4 })
  })
  it('never labels held comparison values as executable clears or removals', async () => {
    const held = { ...item(), disposition: 'held_local_change', clear_counts: { names: '0', assignments: '0', contacts: '0' } }
    const { wrapper } = await setup({ handle: url => {
      if (url.includes('/items?')) return { items: [held], next_cursor: null, plan_id: 'plan', plan_revision: '2' }
      if (url === `${ROOT}/refresh/items/item`) return held
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises()
    expect(wrapper.text()).toContain('No fields or contacts will be changed')
    expect(wrapper.text()).not.toContain('Will clear'); expect(wrapper.text()).not.toContain('Proposed removal')
    expect(wrapper.text()).toContain('synthetic@example.test')
  })
  it.each(['member', 'inactive'])('makes no retained reads for %s access', async role => { await setup({ role }); expect(api.mock.calls.filter(([url]) => url.startsWith('/migrations'))).toHaveLength(0) })
  it('drops late plaintext and receipts after an actor/Organization transition', async () => {
    const pending = deferred<unknown>()
    const { client, state, wrapper } = await setup({ handle: url => url === `${ROOT}/refresh/items/item` ? pending.promise : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises()
    state.identity = identity('member', 'other', 'foreign'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ ...item(), proposed: { first_name: 'late-private-value' } }); await flushPromises()
    expect(wrapper.text()).not.toContain('late-private-value'); expect(wrapper.text()).not.toContain('9007199254740993123')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'admitted-people-refreshes'] })).toHaveLength(0)
  })
  it('fences a late detail after a disposition change and preserves opaque page filters', async () => {
    const pending = deferred<unknown>()
    const { wrapper } = await setup({ handle: url => {
      if (url === `${ROOT}/refresh/items/item`) return pending.promise
      if (url === `${ROOT}/refresh/items?disposition=held_local_change&limit=50`) return { items: [], next_cursor: 'opaque +/', plan_id: 'plan', plan_revision: '2' }
      if (url === `${ROOT}/refresh/items?disposition=held_local_change&cursor=opaque+%2B%2F&limit=50`) return { items: [], next_cursor: null, plan_id: 'plan', plan_revision: '2' }
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises(); await choose('People preview disposition', 'held_local_change')
    pending.resolve({ ...item(), proposed: { first_name: 'late-private-value' } }); await flushPromises(); expect(wrapper.text()).not.toContain('late-private-value')
    button('More People previews').click(); await flushPromises(); expect(api.mock.calls.some(([url]) => url.includes('disposition=held_local_change&cursor=opaque+%2B%2F&limit=50'))).toBe(true)
  })
  it('rejects a wrong plan revision instead of displaying its values', async () => {
    const { wrapper } = await setup({ handle: url => url.includes('/items?') ? { items: [{ ...item(), source_id: 'wrong-plan-private' }], next_cursor: null, plan_id: 'other', plan_revision: '2' } : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); expect(wrapper.text()).not.toContain('wrong-plan-private'); expect(wrapper.text()).toContain('preview could not be verified')
  })
  it('removes already displayed values when a current read denies access', async () => {
    let revoked = false
    const { wrapper } = await setup({ handle: url => revoked && url === `${ROOT}/refresh` ? Promise.reject(new ApiError(403, 'forbidden')) : undefined })
    await choose('Saved Admitted People refresh', 'refresh'); expect(wrapper.text()).toContain('9007199254740993123'); revoked = true
    button('Reload Admitted People refreshes').click(); await flushPromises(); expect(wrapper.text()).not.toContain('9007199254740993123'); expect(wrapper.text()).toContain('currently verified Organization administrator')
  })
})

describe('Admitted People refresh traversal and late actions', () => {
  it('labels clipped names and traverses complete retained UTF-8 name pages without reusing another side', async () => {
    const { wrapper } = await setup({ handle: url => {
      if (url === `${ROOT}/refresh/items/item`) return { ...item(), baseline: { ...item().baseline, first_name: 'Retained prefix', truncated_fields: ['first_name'] } }
      if (url === `${ROOT}/refresh/items/item/fields/baseline/first_name?limit=16384`) return { plan_id: 'plan', plan_revision: '2', side: 'baseline', field: 'first_name', offset: '0', total_bytes: '40000', fragment: 'Original 👩🏽‍💼 name segment', next_cursor: 'name +/' }
      if (url === `${ROOT}/refresh/items/item/fields/baseline/first_name?cursor=name+%2B%2F&limit=16384`) return { plan_id: 'plan', plan_revision: '2', side: 'baseline', field: 'first_name', offset: '16381', total_bytes: '40000', fragment: 'Next retained name segment', next_cursor: null }
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises()
    expect(wrapper.text()).toContain('Display prefix'); button('Read complete name in pages').click(); await flushPromises()
    expect(wrapper.text()).toContain('Original 👩🏽‍💼 name segment'); button('More name pages').click(); await flushPromises()
    expect(wrapper.text()).toContain('Next retained name segment'); expect(wrapper.text()).not.toContain('Original 👩🏽‍💼 name segment')
    expect(button('More name pages').disabled).toBe(true); button('Previous name page').click(); await flushPromises()
    expect(wrapper.text()).toContain('Original 👩🏽‍💼 name segment'); button('Close name pages').click(); await flushPromises()
    expect(wrapper.text()).not.toContain('Original 👩🏽‍💼 name segment')
  })
  it('rejects a name fragment for another side and drops a late fragment after authority changes', async () => {
    let late = false; const pending = deferred<unknown>()
    const { wrapper, client, state } = await setup({ handle: url => {
      if (url === `${ROOT}/refresh/items/item`) return { ...item(), baseline: { ...item().baseline, first_name: 'Prefix', truncated_fields: ['first_name'] } }
      if (url.includes('/fields/')) return late ? pending.promise : { plan_id: 'plan', plan_revision: '2', side: 'proposed', field: 'first_name', fragment: 'wrong-side-private', next_cursor: null }
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises(); button('Read complete name in pages').click(); await flushPromises()
    expect(wrapper.text()).not.toContain('wrong-side-private'); expect(wrapper.text()).toContain('name fragment could not be verified')
    button('Close name pages').click(); await flushPromises(); late = true; button('Read complete name in pages').click(); await flushPromises()
    state.identity = identity('member', 'other', 'foreign'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ plan_id: 'plan', plan_revision: '2', side: 'baseline', field: 'first_name', fragment: 'late-name-private', next_cursor: null }); await flushPromises()
    expect(wrapper.text()).not.toContain('late-name-private'); expect(wrapper.text()).not.toContain('Prefix')
  })
  it('discards a late prepare receipt after sign-in identity changes', async () => {
    const pending = deferred<unknown>()
    const { wrapper, client, state } = await setup({ handle: (url, init) => url === ROOT && init?.method === 'POST' ? pending.promise : undefined })
    await choose('Completed report for a new preview', 'report'); button('Prepare People preview').click(); await flushPromises()
    state.identity = identity('member', 'new-actor', 'other-org'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ refresh_id: 'old-actor-private-run', state: 'preparing' }); await flushPromises()
    expect(wrapper.text()).not.toContain('old-actor-private-run'); expect(api.mock.calls.some(([url]) => url.endsWith('/old-actor-private-run'))).toBe(false)
  })
  it('retains an uncertain action across workspace re-verification for exact replay', async () => {
    const pending = deferred<unknown>(); let attempts = 0
    const { wrapper, client, state } = await setup({ handle: (url, init) => url === ROOT && init?.method === 'POST' ? ++attempts === 1 ? pending.promise : { refresh_id: 'refresh', state: 'preparing' } : undefined })
    await choose('Completed report for a new preview', 'report'); button('Prepare People preview').click(); await flushPromises()
    state.identity = identity('admin', 'actor', 'org', '3'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ refresh_id: 'refresh', state: 'preparing' }); await flushPromises()
    expect(wrapper.text()).toContain('Access changed while the request was pending'); button('Retry the same refresh request').click(); await flushPromises()
    expect(writes()).toHaveLength(2); expect(writes()[0]![1]?.body).toBe(writes()[1]![1]?.body)
  })
  it('traverses contact pages separately and visibly identifies proposed removals', async () => {
    const { wrapper } = await setup({ handle: url => {
      if (url === `${ROOT}/refresh/items/item/contacts?limit=50`) return { contacts: [{ id: 'row', side: 'baseline', change: 'remove', contact_id: 'contact', kind: 'email', import_order: 0, value: { value: 'remove@synthetic.test', normalized_value: 'remove@synthetic.test' } }], next_cursor: 'contacts +/', plan_id: 'plan', plan_revision: '2' }
      if (url === `${ROOT}/refresh/items/item/contacts?cursor=contacts+%2B%2F&limit=50`) return { contacts: [], next_cursor: null, plan_id: 'plan', plan_revision: '2' }
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('Inspect Person preview').click(); await flushPromises()
    expect(wrapper.text()).toContain('Proposed removal'); button('More contact comparison').click(); await flushPromises()
    expect(api.mock.calls.some(([url]) => url.includes('contacts?cursor=contacts+%2B%2F&limit=50'))).toBe(true); expect(wrapper.text()).not.toContain('remove@synthetic.test')
    button('Previous contact comparison').click(); await flushPromises(); expect(wrapper.text()).toContain('remove@synthetic.test')
  })
  it('keeps the first results page fixed until an explicit new traversal', async () => {
    let reads = 0
    const { wrapper } = await setup({ state: 'completed', handle: url => {
      if (url === `${ROOT}/refresh/results?limit=50`) return { results: [{ id: 'result', item_id: 'item', source_id: ++reads === 1 ? 'first-boundary' : 'new-boundary', person_id: 'person', disposition: 'settled', committed_at: source.created_at }], next_cursor: 'fixed-results' }
      if (url === `${ROOT}/refresh/results?cursor=fixed-results&limit=50`) return { results: [], next_cursor: null }
    } })
    await choose('Saved Admitted People refresh', 'refresh'); button('More settled results').click(); await flushPromises(); button('Previous settled results').click(); await flushPromises()
    expect(reads).toBe(1); expect(wrapper.text()).toContain('first-boundary'); button('Load latest settled results').click(); await flushPromises()
    expect(reads).toBe(2); expect(wrapper.text()).toContain('new-boundary')
  })
})
