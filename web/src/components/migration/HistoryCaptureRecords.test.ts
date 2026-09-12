import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { HistoryRecord } from '../../api/historyCaptures'
import { resetWorkspace } from '../../workspaceLifecycle'
import HistoryCaptureRecords from './HistoryCaptureRecords.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/history-captures/capture/records'
const cleanup: Array<() => void> = []
const identity = (role = 'admin', org = 'org') => ({ user: { id: 'actor', email: 'test@synthetic.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Org', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false })
function record(id = 'observation1'): HistoryRecord {
  return { id, capture_id: 'raw1', capture_sequence: '2', series_capture_sequence: '2', ordinal: '0', family: 'events', source_id: '900719925474099312345', source_person_id: '101', source_user_ids: ['9007199254740993'], source_created: null, source_updated: null, source_kind: 'events', source_event_type: '<img src="https://untrusted.invalid" onerror="window.executed=true">', source_timestamp_uncertain: true, preview_truncated: true, relationship_uncertain: true, content_availability: 'returned_in_raw', disposition: 'conflicting_reference', person_id: null, reference_available: false, variant_count: '2', observation_count: '3', representation: 'fub-history-v1/events/default', profile_version: 'fub-history-v1', parser_version: '1', schema_version: 'sha256:synthetic', captured_at: '2026-09-12T00:00:00Z', integrity_status: 'verified_projection' }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { resolve, promise } }
async function setup(handle?: (url: string, init?: RequestInit) => unknown) {
  api.mockImplementation(async (url, init) => {
    const custom = handle?.(url, init); if (custom !== undefined) return await custom as never
    if (url === '/me') return identity() as never
    if (url.startsWith(`${ROOT}?`)) return { records: [record()], next_cursor: null, capture_sequence: '2', counter_basis: 'current_run', counts: [] } as never
    if (url === `${ROOT}/observation1`) return { ...record(), message: 'PRIVATE_BODY_SENTINEL', subject: 'PRIVATE_SUBJECT_SENTINEL', body_url: 'https://untrusted.invalid/private' } as never
    throw new Error(`Unexpected ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(HistoryCaptureRecords, { props: { captureId: 'capture', currentSequence: '2' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
function button(label: string): HTMLButtonElement { const value = [...document.body.querySelectorAll('button')].find(element => element.textContent?.trim() === label); expect(value, label).toBeDefined(); return value! }
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; resetWorkspace() })
describe('historical observation metadata', () => {
  it('escapes source labels, preserves huge IDs, and exposes no raw body or source URL action', async () => {
    const { wrapper } = await setup()
    expect(wrapper.text()).toContain('900719925474099312345'); expect(wrapper.text()).toContain('<img src=')
    expect(wrapper.find('img').exists()).toBe(false); expect(wrapper.find('a').exists()).toBe(false)
    button('Inspect metadata').click(); await flushPromises()
    expect(wrapper.text()).toContain('A content field was returned in raw evidence; completeness is unknown.')
    expect(wrapper.text()).toContain('Source timestamp is uncertain.'); expect(wrapper.text()).toContain('Metadata label abbreviated.')
    expect(wrapper.text()).not.toContain('PRIVATE_BODY_SENTINEL'); expect(wrapper.text()).not.toContain('PRIVATE_SUBJECT_SENTINEL')
    expect(wrapper.find('img').exists()).toBe(false); expect(api.mock.calls.every(([url, init]) => url.startsWith('/') && !init?.method)).toBe(true)
  })
  it('preserves the first page and boundary across progress and Previous until explicit refresh', async () => {
    let latest = false; let firstReads = 0
    const { wrapper } = await setup(url => {
      if (url === `${ROOT}?limit=50`) { firstReads++; return { records: [record(latest ? 'new-row' : 'old-row')], next_cursor: 'fixed+cursor', capture_sequence: latest ? '3' : '2', counter_basis: 'current_run', counts: [] } }
      if (url === `${ROOT}?cursor=fixed%2Bcursor&limit=50`) return { records: [record('second-page')], next_cursor: null, capture_sequence: '2', counter_basis: 'current_run', counts: [] }
    })
    expect(firstReads).toBe(1)
    button('More captured observations').click(); await flushPromises()
    expect(wrapper.find('[aria-label="Inspect historical observation second-page"]').exists()).toBe(true)
    latest = true; await wrapper.setProps({ currentSequence: '3' }); await flushPromises()
    button('Previous captured observations').click(); await flushPromises()
    expect(firstReads).toBe(1); expect(wrapper.find('[aria-label="Inspect historical observation old-row"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('fixed capture boundary 2. Current capture progress: 3')
    button('Refresh captured observations').click(); await flushPromises()
    expect(firstReads).toBe(2); expect(wrapper.find('[aria-label="Inspect historical observation new-row"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('fixed capture boundary 3')
  })
  it('starts new scoped series for filters and uses local observation UUID for variants', async () => {
    const { wrapper } = await setup()
    await wrapper.findAll('select')[0]!.setValue('calls'); await flushPromises()
    expect(api.mock.lastCall?.[0]).toBe(`${ROOT}?family=calls&limit=50`)
    await wrapper.findAll('select')[1]!.setValue('parent_excluded'); await flushPromises()
    expect(api.mock.lastCall?.[0]).toBe(`${ROOT}?family=calls&disposition=parent_excluded&limit=50`)
    button('Show variants').click(); await flushPromises()
    expect(api.mock.lastCall?.[0]).toBe(`${ROOT}?record_id=observation1&limit=50`)
    expect(wrapper.text()).toContain('one source identity')
    button('Show all captured identities').click(); await flushPromises(); expect(api.mock.lastCall?.[0]).toBe(`${ROOT}?limit=50`)
  })
  it('cannot resurrect a private delayed page after an Organization switch', async () => {
    const delayed = deferred<unknown>()
    const { wrapper, client } = await setup(url => url.startsWith(`${ROOT}?`) ? delayed.promise : undefined)
    client.setQueryData(queryKeys.me, identity('member', 'foreign')); await flushPromises()
    delayed.resolve({ records: [record()], next_cursor: null, capture_sequence: '2', counter_basis: 'current_run', counts: [] }); await flushPromises()
    expect(wrapper.text()).not.toContain('900719925474099312345')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'history-captures'] }).some(query => query.state.data !== undefined)).toBe(false)
  })
})
