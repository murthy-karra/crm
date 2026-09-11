import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { PeopleImportRead } from '../../api/imports'
import PeopleImportPanel from './PeopleImportPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const fetch = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/imports'
const cleanup: Array<() => void> = []
function me(role = 'admin', actor = 'admin', revision = '1', mode = 'operational') { return { user: { id: actor, email: 'admin@synthetic.test', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role, workspace_mode: mode, workspace_revision: revision }, platform_admin: false } }
function run(state: PeopleImportRead['state'] = 'proposed'): PeopleImportRead {
  const confirmed = ['queued', 'running', 'completed'].includes(state)
  const counts = { source_people: '5', eligible_people: '4', held_people: '1', invalid_ids: '0', contacts: '8', overlap_people: '3', stages_to_create: '1', assigned_people: '2', unassigned_people: '2', imported_people: state === 'completed' ? '4' : '0', imported_contacts: state === 'completed' ? '8' : '0', pending_people: state === 'completed' ? '0' : '5', reasons: { trash_person: '1' } }
  return { id: 'import', snapshot_id: 'snapshot', preview_id: 'preview', source_account_id: '900719925474099312345', capture_sequence: '8', state, phase: confirmed ? 'people' : 'preparation', pause_reason: state === 'paused' ? 'storage_limit' : null, latest_plan_id: 'plan', confirmed_plan_id: confirmed ? 'plan' : null, executor_user_id: 'admin', created_at: '2026-09-11T12:00:00Z', updated_at: '2026-09-11T12:00:00Z', counts, retained_bytes: '1024', reserved_bytes: '65536', cancellation_reserved_bytes: '65536', release_ready: true, actions: { confirm: state === 'proposed', replan: !confirmed, retry: state === 'paused', cancel: !['completed', 'cancelled'].includes(state) }, plan: { id: 'plan', revision: '9007199254740993', state: 'ready', phase: 'ready', confirmation_digest: 'exact-digest', expires_at: '2099-09-11T12:10:00Z', expired: false, counts, required_reservation_bytes: '3145728', created_at: '2026-09-11T12:00:00Z', completed_at: '2026-09-11T12:01:00Z' }, workspace: { mode: confirmed ? 'migration_review' : 'operational', revision: confirmed ? '2' : '1', activation_available: false }, coverage: { imported_families: ['people'], remaining_families: ['notes', 'tasks', 'emails'], source_remains_retained: true, cutover_complete: false }, source_window: { first_observed_at: '2026-09-11T12:00:00Z', last_observed_at: '2026-09-11T12:01:00Z' }, policy: { run_byte_limit: '2147483648', org_byte_limit: '4294967296', run_ceiling_bytes: '2147483648', org_ceiling_bytes: '4294967296', unit_ceiling_bytes: '67108864', policy_revision: 'policy' }, engine_version: 'fub-people-import-v1' }
}
const snapshot = { id: 'snapshot', created_at: '2026-09-11T12:00:00Z', state: 'completed', profile_version: 'fub-core-v1', capture_sequence: '8', preview_ids: ['preview'] }
const sourceDetail = { snapshot, streams: ['people', 'users', 'stages'].map(stream => ({ stream, state: 'completed' })), coverage: [] }
async function setup(options: { state?: PeopleImportRead | null; role?: string; identity?: ReturnType<typeof me>; realChildren?: boolean; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { run: options.state === undefined ? run() : options.state, identity: options.identity ?? me(options.role) }
  fetch.mockImplementation(async (url, init) => { const custom = options.handle?.(url, init); if (custom !== undefined) return await custom as never
    if (url === '/me') return state.identity as never
    if (url === `${ROOT}?limit=20`) return { imports: state.run ? [state.run] : [], next_cursor: null } as never
    if (url === `${ROOT}/import` && !init?.method) return state.run as never
    if (url === '/migrations/fub/snapshots?limit=20') return { snapshots: [snapshot], next_cursor: null, latest_completed_snapshot_id: 'snapshot', active_snapshot_id: null } as never
    if (url === '/migrations/fub/snapshots/snapshot') return sourceDetail as never
    if (url === '/migrations/fub/snapshots/snapshot/previews/preview') return { preview: { id: 'preview', state: 'completed', capture_sequence: '8' } } as never
    if (url === '/stages') return { stages: [{ id: 'lead', name: 'Lead', position: 0 }, { id: 'contacted', name: 'Contacted', position: 1 }] } as never
    if (url === '/organization/members') return { members: [] } as never
    if (url.includes('/mappings?')) {
      const stage = url.includes('/next-plan/') ? { id: 'contacted', name: 'Contacted' } : { id: 'lead', name: 'Lead' }
      return { mappings: url.includes('kind=stage') ? [{ id: 'mapping', source_key: '11', qualified: true, disposition: 'existing', source: { family: 'stage', label: { value: 'Lead', abbreviated: false, full_utf8_bytes: '4', field_key: 'source.name' }, can_create: true, provenance: { fields: [], total_count: '0', abbreviated: false, field_key: 'provenance' } }, choice: { kind: 'existing', stage_id: stage.id }, suggestions: [], target: stage, reasons: [], dependent_count: '4' }] : [], next_cursor: null } as never
    }
    if (url.includes('/records?')) return { records: [], next_cursor: null } as never
    if (url.includes('/results?')) return { results: [], next_cursor: null } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { if (state.run) { state.identity = me('admin', state.identity.user.id, state.run.workspace.revision, state.run.workspace.mode); client.setQueryData(queryKeys.me, state.identity) } })
  const wrapper = mount(PeopleImportPanel, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { ImportMappingPanel: !options.realChildren, ImportRecordPanel: !options.realChildren, SnapshotBudgetPanel: true } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, state, refreshWorkspace }
}
function button(text: string): HTMLButtonElement { const el = [...document.body.querySelectorAll('button')].find(b => b.textContent?.trim() === text); expect(el, text).toBeDefined(); return el! }
function writes(suffix = '') { return fetch.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix)) }
async function acknowledge(wrapper: Awaited<ReturnType<typeof setup>>['wrapper']) { for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true); button('Review People import').click(); await flushPromises() }
beforeEach(() => { fetch.mockReset(); vi.stubGlobal('crypto', { randomUUID: () => 'request-uuid' }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers() })
describe('PeopleImportPanel', () => {
  it('plans from explicitly selected retained evidence without starting any source request', async () => {
    const { state } = await setup({ state: null, handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run(); return { import_id: 'import', plan_id: 'plan', state: 'building' } } } })
    expect(writes()).toHaveLength(0); expect(button('Plan People import').disabled).toBe(false)
    button('Plan People import').click(); await flushPromises()
    expect(writes().map(([url, init]) => [url, JSON.parse(String(init?.body))])).toEqual([[ROOT, { request_id: 'request-uuid', snapshot_id: 'snapshot', preview_id: 'preview' }]])
  })
  it('requires all three acknowledgments and confirms the exact immutable plan', async () => {
    const { wrapper, state, refreshWorkspace } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('queued'); return { import: state.run, workspace_mode: 'migration_review', workspace_revision: '2' } } } })
    expect(button('Review People import').disabled).toBe(true)
    await acknowledge(wrapper); expect(writes()).toHaveLength(0)
    expect(document.body.textContent).toContain('plan revision 9007199254740993')
    button('Confirm People import').click(); await flushPromises()
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', plan_id: 'plan', plan_revision: '9007199254740993', confirmation_digest: 'exact-digest', acknowledgments: { held_count: '1', review_only: true, remaining_data: true } })
    expect(refreshWorkspace).toHaveBeenCalled(); expect(state.identity.organization.workspace_mode).toBe('migration_review')
  })
  it.each(['apply', 'discard'] as const)('requires %s of real mapping drafts before confirming the reviewed plan', async action => {
    const { wrapper, state } = await setup({ realChildren: true, handle: (url, init) => {
      if (url.endsWith('/plans') && init?.method === 'POST') {
        const next = run(); next.plan = { ...next.plan, id: 'next-plan', revision: '9007199254740994', confirmation_digest: 'next-digest' }; next.latest_plan_id = 'next-plan'; state.run = next
        return { import_id: 'import', plan_id: 'next-plan', state: 'building' }
      }
      if (url.endsWith('/confirm')) return { import: state.run, workspace_mode: 'migration_review', workspace_revision: '2' }
    } })
    for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true)
    expect(button('Review People import').disabled).toBe(false)
    await wrapper.get('select[aria-label="Stage choice for source 11"]').setValue('existing:contacted')
    expect(button('Review People import').disabled).toBe(true)
    expect(button('Prepare fresh plan revision').disabled).toBe(true)
    button('Review People import').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(0)
    expect(document.body.querySelector('[role=dialog]')).toBeNull()
    expect(wrapper.text()).toContain('Apply or discard your mapping changes')
    button(action === 'apply' ? 'Apply 1 mapping changes' : 'Discard mapping changes').click(); await flushPromises()
    if (action === 'apply') {
      expect(JSON.parse(String(writes('/plans')[0]![1]?.body)).stage_mappings).toEqual([{ source_key: '11', choice: { kind: 'existing', stage_id: 'contacted' } }])
      expect(button('Review People import').disabled).toBe(true)
      expect((wrapper.get('select[aria-label="Stage choice for source 11"]').element as HTMLSelectElement).value).toBe('existing:contacted')
    } else {
      expect(writes('/plans')).toHaveLength(0)
      expect((wrapper.get('select[aria-label="Stage choice for source 11"]').element as HTMLSelectElement).value).toBe('existing:lead')
    }
    await acknowledge(wrapper); button('Confirm People import').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(1)
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toMatchObject({ plan_id: action === 'apply' ? 'next-plan' : 'plan', confirmation_digest: action === 'apply' ? 'next-digest' : 'exact-digest' })
  })
  it('invalidates an already-open confirmation when a real mapping draft changes', async () => {
    const { wrapper } = await setup({ realChildren: true })
    await acknowledge(wrapper)
    expect(document.body.querySelector('[role=dialog]')).not.toBeNull()
    await wrapper.get('select[aria-label="Stage choice for source 11"]').setValue('existing:contacted'); await flushPromises()
    expect(document.body.querySelector('[role=dialog]')).toBeNull()
    expect(button('Review People import').disabled).toBe(true)
    expect(writes('/confirm')).toHaveLength(0)
    button('Discard mapping changes').click(); await flushPromises()
    expect(document.body.querySelector('[role=dialog]')).toBeNull()
    button('Review People import').click(); await flushPromises()
    expect(document.body.querySelector('[role=dialog]')).not.toBeNull()
    expect(writes('/confirm')).toHaveLength(0)
  })
  it('refreshes the real visible result page from parent progress polling and stops at completion', async () => {
    vi.useFakeTimers()
    let imported = false
    const { wrapper, state } = await setup({ realChildren: true, state: run('running'), identity: me('admin', 'admin', '2', 'migration_review'), handle: url => url.includes('/results?') ? { results: [{ id: 'record', source_id: '105', disposition: imported ? 'imported' : 'pending', planned_disposition: 'eligible', person_id: imported ? 'created-person' : null, contact_count: imported ? '4' : null, committed_at: imported ? '2026-09-11T12:01:00Z' : null, held_reasons: [] }], next_cursor: null } : undefined })
    const resultReads = () => fetch.mock.calls.filter(([url]) => url.includes('/results?')).length
    expect(wrapper.text()).toContain('Import results'); expect(wrapper.text()).toContain('Pending')
    expect(wrapper.find('a[href="/people/created-person"]').exists()).toBe(false)
    const before = resultReads()
    await vi.advanceTimersByTimeAsync(2000); await flushPromises()
    expect(resultReads()).toBe(before)
    imported = true
    state.run = { ...state.run!, counts: { ...state.run!.counts, imported_people: '1', imported_contacts: '4', pending_people: '4' } }
    await vi.advanceTimersByTimeAsync(2000); await flushPromises()
    expect(resultReads()).toBe(before + 1)
    expect(wrapper.get('a[href="/people/created-person"]').text()).toBe('View imported Person 105')
    state.run = run('completed')
    await vi.advanceTimersByTimeAsync(2000); await flushPromises()
    expect(resultReads()).toBe(before + 2)
    expect(wrapper.find('a[href="/people/created-person"]').exists()).toBe(true)
    const completedReads = fetch.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000); await flushPromises()
    expect(fetch.mock.calls).toHaveLength(completedReads)
  })
  it('preserves exact confirmation replay after a lost response and workspace-mode refresh', async () => {
    let attempts = 0
    const { wrapper, state, refreshWorkspace } = await setup({ handle: url => { if (url.endsWith('/confirm')) { attempts++; state.run = run('completed'); return attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { import: state.run, workspace_mode: 'migration_review', workspace_revision: '2' } } } })
    await acknowledge(wrapper); button('Confirm People import').click(); await flushPromises(); await flushPromises()
    expect(refreshWorkspace).toHaveBeenCalled(); expect(state.identity.organization.workspace_revision).toBe('2')
    expect(button('Retry confirmation request').disabled).toBe(false)
    button('Retry confirmation request').click(); await flushPromises()
    const bodies = writes('/confirm').map(([, init]) => String(init?.body))
    expect(bodies).toHaveLength(2); expect(bodies[0]).toBe(bodies[1]); expect(writes('/plans')).toHaveLength(0)
    expect(document.body.textContent).not.toContain('Retry confirmation request')
  })
  it('clears uncertain confirmation after actor or role loss', async () => {
    const { wrapper, client, state } = await setup({ handle: url => url.endsWith('/confirm') ? Promise.reject(new ApiError(0, 'network_error')) : undefined })
    await acknowledge(wrapper); button('Confirm People import').click(); await flushPromises()
    expect(button('Retry confirmation request')).toBeDefined()
    state.identity = me('member', 'other'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    expect(document.body.textContent).not.toContain('Retry confirmation request'); expect(document.body.textContent).not.toContain('900719925474099312345')
  })
  it('keeps pause/resume/cancel explicit and keeps budget refresh separate', async () => {
    const { wrapper } = await setup({ state: run('paused'), handle: url => url.endsWith('/retry') || url.endsWith('/cancel') ? { import: run('paused') } : undefined })
    expect(writes()).toHaveLength(0); expect(wrapper.text()).toContain('Paused: Storage limit')
    wrapper.findComponent({ name: 'SnapshotBudgetPanel' }).vm.$emit('refresh'); await flushPromises(); expect(writes()).toHaveLength(0)
    button('Resume preparation').click(); await flushPromises(); expect(writes('/retry')).toHaveLength(1)
    button('Cancel remaining import').click(); await flushPromises()
    expect(document.body.textContent).toContain('Already committed People, stages and retained evidence remain')
    const dialogs = [...document.body.querySelectorAll('button')].filter(b => b.textContent?.trim() === 'Cancel remaining import'); dialogs.at(-1)!.click(); await flushPromises()
    expect(writes('/cancel')).toHaveLength(1)
  })
  it.each(['completed', 'cancelled', 'paused'] as const)('stops polling a %s run', async state => {
    vi.useFakeTimers(); const data = run(state)
    if (state === 'completed') data.workspace = { mode: 'operational', revision: '1', activation_available: false }
    await setup({ state: data }); const before = fetch.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000); expect(fetch.mock.calls).toHaveLength(before)
  })
  it('backs active read failures off and stops after completion', async () => {
    vi.useFakeTimers(); let failing = false
    const data = run('running'); data.workspace = { mode: 'operational', revision: '1', activation_available: false }
    const { state } = await setup({ state: data, handle: url => failing && (url === `${ROOT}/import` || url === `${ROOT}?limit=20`) ? Promise.reject(new ApiError(503, 'unavailable')) : undefined })
    failing = true; let reads = fetch.mock.calls.filter(([url]) => url === `${ROOT}/import`).length
    for (const delay of [2000, 4000, 8000, 16000, 30000]) { await vi.advanceTimersByTimeAsync(delay - 1); expect(fetch.mock.calls.filter(([url]) => url === `${ROOT}/import`)).toHaveLength(reads); await vi.advanceTimersByTimeAsync(1); reads++; expect(fetch.mock.calls.filter(([url]) => url === `${ROOT}/import`)).toHaveLength(reads) }
    failing = false; state.run = { ...data, state: 'completed' }; await vi.advanceTimersByTimeAsync(30_000); const before = fetch.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000); expect(fetch.mock.calls).toHaveLength(before)
  })
  it('discards a late planning mutation after identity changes', async () => {
    let resolve!: (value: unknown) => void
    const { wrapper, client, state } = await setup({ state: null, handle: (url, init) => url === ROOT && init?.method === 'POST' ? new Promise(yes => { resolve = yes }) : undefined })
    button('Plan People import').click(); await flushPromises()
    state.identity = me('member', 'other'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    resolve({ import_id: 'private-late-import', plan_id: 'private-late-plan', state: 'building' }); await flushPromises()
    expect(wrapper.text()).not.toContain('private-late'); expect(wrapper.find('select').exists()).toBe(false)
    expect(writes()).toHaveLength(1); expect(writes('/confirm')).toHaveLength(0)
  })
  it('does not fetch import data for ordinary members', async () => { await setup({ role: 'member' }); expect(fetch.mock.calls.filter(([url]) => url.startsWith(ROOT))).toHaveLength(0) })
})
