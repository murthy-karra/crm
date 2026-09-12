import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import type { HistoryManifestItem, ImportedMetadata } from '../../api/historyImports'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import HistoryImportRecords from './HistoryImportRecords.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const me = { user: { id: 'actor', email: 'synthetic@example.invalid', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
const metadata: ImportedMetadata = { source_id: '9007199254740993123', source_person_id: '22', source_access_user_id: '3', source_attributed_user_id: '4', source_creator_user_id: '5', source_editor_user_id: null, source_created: null, source_updated: null, source_sent: null, source_event_type: 'Inquiry', source_note_id: null, source_is_incoming: null, source_duration: null, source_duration_unit: null, source_outcome: null, source_status: null, content_availability: 'returned_in_raw', preview_truncated: false }
const row = (id: string): HistoryManifestItem => ({ id, position: '1', family: 'events', disposition: 'eligible', reason: null, person_id: 'person', metadata, capture_id: 'capture', ordinal: 0, observation_id: 'observation' })
const page = (id: string, revision = '1', next_cursor: string | null = null) => ({ records: [row(id)], next_cursor, plan_id: 'plan', revision })
async function setup(handle: (url: string) => unknown) {
  api.mockImplementation(async url => url === '/me' ? me as never : await handle(url) as never)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }] }); await router.push('/')
  const wrapper = mount(HistoryImportRecords, { props: { importId: 'run', planId: 'plan', currentRevision: '1', mode: 'records' }, attachTo: document.body, global: { plugins: [router, [VueQueryPlugin, { queryClient: client }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
async function click(label: string) { const target = [...document.querySelectorAll('button')].find(button => button.textContent?.trim() === label); if (!target) throw new Error(`Missing ${label}`); target.click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; resetWorkspace() })
describe('historical import record pages', () => {
  it.each(['first', 'continuation'] as const)('rejects a delayed %s page when current status advances before it settles', async kind => {
    let resolve!: (value: ReturnType<typeof page>) => void
    const delayed = new Promise<ReturnType<typeof page>>(yes => { resolve = yes })
    const { wrapper } = await setup(url => kind === 'first' || url.includes('cursor=next') ? delayed : page('first', '1', 'next'))
    if (kind === 'continuation') await click('More records')
    await wrapper.setProps({ currentRevision: '2' }); await flushPromises()
    resolve(page('stale', '1')); await flushPromises()
    expect(wrapper.text()).toContain('Import progress changed')
    expect(wrapper.text()).not.toContain('9007199254740993123')
    const reads = api.mock.calls.length
    api.mockImplementation(async url => url === '/me' ? me as never : page('fresh', '2') as never)
    await click('Refresh records')
    expect(api.mock.calls.length).toBeGreaterThan(reads)
    expect(wrapper.text()).toContain('revision 2')
  })
  it('keeps a bounded first page for Previous and clears metadata when progress needs Refresh', async () => {
    const { wrapper } = await setup(url => page(url.includes('cursor=next') ? 'second' : 'first', '1', url.includes('cursor=next') ? null : 'next'))
    const initialReads = api.mock.calls.length
    await click('More records'); await click('Previous records')
    expect(api.mock.calls).toHaveLength(initialReads + 1)
    await click('Inspect metadata'); expect(wrapper.text()).toContain('Source access user ID')
    await wrapper.setProps({ currentRevision: '2' }); await flushPromises()
    expect(wrapper.text()).toContain('Import progress changed')
    expect(wrapper.text()).not.toContain('9007199254740993123')
    expect(wrapper.text()).not.toContain('Source access user ID')
    expect(api.mock.calls).toHaveLength(initialReads + 1)
    api.mockImplementation(async url => url === '/me' ? me as never : page('fresh', '2') as never)
    await click('Refresh records'); expect(wrapper.text()).toContain('revision 2')
  })
  it('rejects a changed revision on continuation and scopes new filters without silently appending', async () => {
    const { wrapper } = await setup(url => { if (url.includes('cursor=')) throw new ApiError(409, 'history_refresh_required'); return page('first', '1', 'next') })
    await click('More records'); expect(wrapper.text()).toContain('Import progress changed')
    await wrapper.findAll('select')[0]!.setValue('calls'); await flushPromises()
    expect(api.mock.calls.at(-1)![0]).toBe('/migrations/fub/history-imports/run/records?family=calls&limit=25')
    expect(wrapper.text()).toContain('1 occurrences on this page')
  })
  it('removes retained row metadata immediately after current-role loss', async () => {
    const { wrapper, client } = await setup(() => page('first'))
    await click('Inspect metadata')
    client.setQueryData(queryKeys.me, { ...me, organization: { ...me.organization, role: 'member' } }); await flushPromises()
    expect(wrapper.text()).not.toContain('9007199254740993123')
    expect(wrapper.find('a[href="/people/person"]').exists()).toBe(false)
  })
})
