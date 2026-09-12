import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { MetadataCounts, MetadataImport } from '../../api/metadataImports'
import { resetWorkspace } from '../../workspaceLifecycle'
import MetadataImportPanel from './MetadataImportPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/metadata-imports'
const cleanup: Array<() => void> = []
const empty = { planned: '0', eligible: '0', created: '0', applied: '0', already_present: '0', held: '0', not_supplied: '0', source_null: '0', pending: '0' }
function counts(): MetadataCounts { return { people: { source: '5', eligible: '4', excluded: '1', settled: '0' }, tags: { ...empty, eligible: '2', planned: '2' }, fields: { ...empty, eligible: '3' }, options: { ...empty, eligible: '4' }, tag_links: { ...empty, eligible: '5' }, values: { ...empty, eligible: '6', held: '2' }, held_count: '2', invalid_source_ids: '0', issues: [] } }
function identity(role = 'admin', actor = 'admin', revision = '2', mode = 'migration_review') { return { user: { id: actor, email: 'admin@synthetic.test', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role, workspace_mode: mode, workspace_revision: revision }, platform_admin: false } }
function run(state: MetadataImport['state'] = 'proposed'): MetadataImport {
  const confirmed = ['queued', 'running', 'completed'].includes(state)
  return { id: 'child', parent_import_id: 'parent', parent_plan_id: 'parent-plan', snapshot_id: 'snapshot', source_account_id: '900719925474099312345', capture_sequence: '8', workspace_revision: '2', engine_version: 'fub-metadata-import-v1', state, phase: confirmed ? 'people' : 'preparation', pause_reason: state === 'paused' ? 'storage_limit' : null, created_at: '2026-09-11T12:00:00Z', updated_at: '2026-09-11T12:00:00Z', confirmed_at: null, completed_at: null, confirmed_plan_id: confirmed ? 'plan' : null, retained_bytes: '1024', reserved_bytes: '65536', cancellation_reserved_bytes: '65536', release_ready: true, counts: counts(), latest_plan: { id: 'plan', revision: '9007199254740993', state: 'ready', phase: 'ready', pause_reason: null, expires_at: '2099-01-01T12:00:00Z', confirmation_digest: 'exact-digest', counts: counts(), max_added_byte_bound: '3145728' }, actions: { confirm: state === 'proposed', replan: state === 'proposed', retry: state === 'paused', cancel: !['completed', 'cancelled'].includes(state) }, policy: { run_byte_limit: '2147483648', org_byte_limit: '4294967296', run_ceiling_bytes: '2147483648', org_ceiling_bytes: '4294967296', run_retained_bytes: '1024', run_reserved_bytes: '65536', org_retained_bytes: '1024', org_reserved_bytes: '65536', unit_byte_limit: '67108864', policy_revision: 'policy' }, coverage: { embedded_tags_only: true, custom_fields_complete: true, metadata_excluded_people: '1', remaining_data: ['notes', 'tasks', 'emails'], source_gaps: ['notes_unavailable'] } }
}
const parent = { id: 'parent', state: 'completed', created_at: '2026-09-11T10:00:00Z', snapshot_id: 'snapshot', confirmed_plan_id: 'parent-plan' }
const summary = { fields: [{ key: 'source.label', label: 'label', label_abbreviated: false, label_full_utf8_bytes: '5', text: '"Buyer"', full_utf8_bytes: '7', abbreviated: false }], total_fields: '1', abbreviated: false, field_key: 'source.all' }
async function setup(options: { run?: MetadataImport | null; role?: string; completeFields?: boolean; parentState?: string; realChildren?: boolean; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { run: options.run === undefined ? run() : options.run, identity: identity(options.role) }
  api.mockImplementation(async (url, init) => {
    const custom = options.handle?.(url, init); if (custom !== undefined) return await custom as never
    if (url === '/me') return state.identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [{ ...parent, state: options.parentState ?? 'completed' }], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return { ...parent, state: options.parentState ?? 'completed' } as never
    if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { imports: state.run ? [state.run] : [], next_cursor: null } as never
    if (url === `${ROOT}/child` && !init?.method) return state.run as never
    if (url === '/migrations/fub/snapshots/snapshot') return { snapshot: { id: 'snapshot' }, streams: [{ stream: 'custom_fields', state: options.completeFields === false ? 'paused' : 'completed' }] } as never
    if (url.includes('/issues?')) return { items: [{ code: 'invalid_value', count: '2' }], next_cursor: null } as never
    if (url.includes('/mappings?')) return { items: url.includes('kind=tag') ? [{ id: 'mapping', kind: 'tag', parent_mapping_id: null, source_id: null, disposition: 'held', qualified: true, create_matching_available: true, choice: { kind: url.includes('/next-plan/') ? 'create_matching' : 'hold' }, target_id: null, field_id: null, reasons: ['mapping_required'], suggestions: [], dependent_count: '2', source: summary, added_byte_bound: '1024', alias_count: '0', target: null }] : [], next_cursor: null } as never
    if (url.includes('/records?') || url.includes('/results?')) return { items: [], next_cursor: null } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, state.identity) })
  const wrapper = mount(MetadataImportPanel, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { MetadataMappingPanel: !options.realChildren, MetadataRecordPanel: !options.realChildren, SnapshotBudgetPanel: true, RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, state, refreshWorkspace }
}
function button(text: string): HTMLButtonElement { const el = [...document.body.querySelectorAll('button')].find(b => b.textContent?.trim() === text); expect(el, text).toBeDefined(); return el! }
function writes(suffix = '') { return api.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix)) }
async function acknowledge(wrapper: Awaited<ReturnType<typeof setup>>['wrapper']) { for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true); button('Review metadata confirmation').click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace(); vi.stubGlobal('crypto', { randomUUID: () => 'request-uuid' }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('MetadataImportPanel', () => {
  it('automatically refreshes same-plan mappings and planned People when preparation becomes ready', async () => {
    vi.useFakeTimers()
    let prepared = false
    const initial = run(); initial.latest_plan = { ...initial.latest_plan!, state: 'building', phase: 'captures' }; initial.actions.confirm = false; initial.actions.replan = false
    const { wrapper, state } = await setup({ run: initial, realChildren: true, handle: url => {
      if (!prepared && url.includes('/mappings?')) return { items: [], next_cursor: null }
      if (url.includes('/records?')) return { items: prepared ? [{ id: 'record', source_id: '101', person_id: 'person', disposition: 'eligible', reasons: [], counts: counts(), added_byte_bound: '2048', source: summary, operations: summary }] : [], next_cursor: null }
    } })
    expect(wrapper.find('select[aria-label="Tag choice for Buyer"]').exists()).toBe(false)
    expect(wrapper.text()).toContain('No metadata records match these filters')
    prepared = true; state.run = run()
    await vi.advanceTimersByTimeAsync(2000); await flushPromises()
    expect(wrapper.find('select[aria-label="Tag choice for Buyer"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('FUB 101')
    expect(writes()).toHaveLength(0)
  })
  it('only prepares from the completed parent and complete field-definition stream', async () => {
    const { state } = await setup({ run: null, handle: (url, init) => { if (url === ROOT && init?.method === 'POST') { state.run = run(); return { import: state.run } } } })
    expect(writes()).toHaveLength(0); expect(button('Prepare tags and fields plan').disabled).toBe(false)
    button('Prepare tags and fields plan').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: 'request-uuid', parent_import_id: 'parent' })
    expect(api.mock.calls.some(([url]) => /connections|assessments|\/snapshots\/[^/]+\/confirm/.test(url))).toBe(false)
  })
  it.each([{ completeFields: false }, { parentState: 'paused' }])('blocks ineligible retained evidence %o', async options => {
    await setup({ run: null, ...options }); expect(button('Prepare tags and fields plan').disabled).toBe(true); expect(writes()).toHaveLength(0)
  })
  it('freezes exact confirmation only after every acknowledgment, without activating the workspace', async () => {
    const { wrapper, state, refreshWorkspace } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('queued'); return { import: state.run } } } })
    expect(button('Review metadata confirmation').disabled).toBe(true)
    await acknowledge(wrapper); expect(writes()).toHaveLength(0)
    expect(document.body.textContent).toContain('plan revision 9007199254740993')
    button('Confirm metadata import').click(); await flushPromises()
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', plan_id: 'plan', plan_revision: '9007199254740993', confirmation_digest: 'exact-digest', workspace_revision: '2', acknowledgments: { held_count: '2', review_only: true, remaining_data: true } })
    expect(refreshWorkspace).not.toHaveBeenCalled(); expect(state.identity.organization.workspace_mode).toBe('migration_review')
  })
  it.each(['apply', 'discard'] as const)('requires %s of real mapping drafts and binds the subsequent revision', async action => {
    const { wrapper, state } = await setup({ realChildren: true, handle: (url, init) => { if (url.endsWith('/plans') && init?.method === 'POST') { state.run = run(); state.run.latest_plan = { ...state.run.latest_plan!, id: 'next-plan', revision: '9007199254740994', confirmation_digest: 'next-digest' }; return { import: state.run } } if (url.endsWith('/confirm')) return { import: state.run } } })
    await acknowledge(wrapper); expect(document.body.querySelector('[role=dialog]')).not.toBeNull()
    await wrapper.get('select[aria-label="Tag choice for Buyer"]').setValue('create_matching'); await flushPromises()
    expect(document.body.querySelector('[role=dialog]')).toBeNull(); expect(button('Review metadata confirmation').disabled).toBe(true); expect(button('Prepare fresh metadata plan').disabled).toBe(true)
    button(action === 'apply' ? 'Apply 1 mapping changes' : 'Discard mapping changes').click(); await flushPromises()
    if (action === 'apply') { expect(JSON.parse(String(writes('/plans')[0]![1]?.body))).toEqual({ request_id: 'request-uuid', expected_plan_revision: '9007199254740993', mappings: [{ mapping_id: 'mapping', choice: { kind: 'create_matching' } }] }); expect(button('Review metadata confirmation').disabled).toBe(true) }
    await acknowledge(wrapper); button('Confirm metadata import').click(); await flushPromises()
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toMatchObject({ plan_id: action === 'apply' ? 'next-plan' : 'plan', confirmation_digest: action === 'apply' ? 'next-digest' : 'exact-digest' })
  })
  it('replays the exact lost confirmation receipt even after completion and readback', async () => {
    let attempts = 0
    const { wrapper, state } = await setup({ handle: url => { if (url.endsWith('/confirm')) { state.run = run('completed'); return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { import: run('queued') } } } })
    await acknowledge(wrapper); button('Confirm metadata import').click(); await flushPromises()
    expect(button('Retry the same metadata request').disabled).toBe(false)
    button('Retry the same metadata request').click(); await flushPromises()
    expect(writes('/confirm')).toHaveLength(2); expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
    expect(wrapper.text()).toContain('Metadata import · Completed'); expect(wrapper.text()).not.toContain('Retry the same metadata request'); expect(writes('/plans')).toHaveLength(0)
  })
  it('clears frozen requests and private summaries when the actor loses admin access', async () => {
    const { wrapper, state, client } = await setup({ handle: url => url.endsWith('/confirm') ? Promise.reject(new ApiError(0, 'network_error')) : undefined })
    await acknowledge(wrapper); button('Confirm metadata import').click(); await flushPromises()
    state.identity = identity('member', 'other'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    expect(wrapper.text()).not.toContain('900719925474099312345'); expect(wrapper.text()).not.toContain('Retry the same metadata request'); expect(wrapper.find('select').exists()).toBe(false)
    for (const query of client.getQueryCache().findAll({ queryKey: ['org', 'org', 'metadata-imports'] })) expect(query.state.data).toBeUndefined()
  })
  it('does not create a replacement after terminal cancellation', async () => {
    const { wrapper } = await setup({ run: run('cancelled') }); expect(wrapper.text()).toContain('permanently cancelled'); expect(wrapper.text()).not.toContain('Prepare tags and fields plan'); expect(wrapper.text()).not.toContain('Resume metadata import'); expect(writes()).toHaveLength(0)
  })
  it('makes resume and terminal cancellation explicit and never starts work on allowance refresh', async () => {
    const { wrapper } = await setup({ run: run('paused'), handle: url => url.endsWith('/retry') || url.endsWith('/cancel') ? { import: run('paused') } : undefined })
    wrapper.findComponent({ name: 'SnapshotBudgetPanel' }).vm.$emit('refresh'); await flushPromises(); expect(writes()).toHaveLength(0)
    button('Resume metadata import').click(); await flushPromises(); expect(writes('/retry')).toHaveLength(1)
    button('Cancel metadata import').click(); await flushPromises(); expect(writes('/cancel')).toHaveLength(0)
    button('Permanently cancel import').click(); await flushPromises(); expect(writes('/cancel')).toHaveLength(1)
  })
  it.each(['completed', 'cancelled', 'paused'] as const)('stops automatic polling for %s', async state => { vi.useFakeTimers(); await setup({ run: run(state) }); const before = api.mock.calls.length; await vi.advanceTimersByTimeAsync(60_000); await flushPromises(); expect(api.mock.calls).toHaveLength(before) })
  it('refreshes visible results as progress changes and stops after completion', async () => {
    vi.useFakeTimers(); let applied = false
    const { wrapper, state } = await setup({ realChildren: true, run: run('running'), handle: url => url.includes('/results?') ? { items: applied ? [{ id: 'result', kind: 'people', source_id: '101', person_id: applied ? 'person' : null, mapping_id: null, record_id: 'record', disposition: applied ? 'applied' : 'pending', committed_at: null, counts: counts(), reasons: [], source: summary, operations: summary }] : [], next_cursor: null } : undefined })
    const resultReads = () => api.mock.calls.filter(([url]) => url.includes('/results?')).length
    const before = resultReads(); expect(wrapper.find('a[href="/people/person"]').exists()).toBe(false)
    applied = true; state.run = { ...state.run!, counts: { ...state.run!.counts, people: { ...state.run!.counts.people, settled: '1' } } }
    await vi.advanceTimersByTimeAsync(2000); await flushPromises(); expect(resultReads()).toBe(before + 1); expect(wrapper.find('a[href="/people/person"]').exists()).toBe(true)
    state.run = run('completed'); await vi.advanceTimersByTimeAsync(2000); await flushPromises(); const total = api.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000); await flushPromises(); expect(api.mock.calls).toHaveLength(total)
  })
  it('does not read import data for ordinary members', async () => { await setup({ role: 'member' }); expect(api.mock.calls.filter(([url]) => url.startsWith('/migrations/'))).toHaveLength(0) })
})
