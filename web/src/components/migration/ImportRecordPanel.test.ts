import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import ImportRecordPanel from './ImportRecordPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const fetch = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const me = { user: { id: 'admin' }, organization: { id: 'org', role: 'admin', workspace_mode: 'operational', workspace_revision: '1' } }
const record = { id: 'record', source_id: '900719925474099312345', disposition: 'held', held_reasons: ['trash_person'], transformations: ['duplicate_contacts'], overlap_count: '2', added_byte_bound: '3145728', proposed: { family: 'people', first_name: { value: '<img src=x onerror=alert(1)>', abbreviated: true, full_utf8_bytes: '2097152', field_key: 'first_name' }, contacts: { entries: [{ kind: 'phone', value: '+12025550100', normalized_value: '+12025550100', import_order: 0 }], total_count: '1', abbreviated: false, field_key: 'contacts' }, provenance: { fields: [], total_count: '1', abbreviated: false, field_key: 'provenance' } } }
async function setup(handle?: (url: string) => unknown) {
  fetch.mockImplementation(async url => { const custom = handle?.(url); if (custom !== undefined) return await custom as never
    if (url === '/me') return me as never
    if (url.includes('/results?')) return { results: [{ id: 'record', source_id: record.source_id, disposition: 'imported', planned_disposition: 'eligible', person_id: 'person', contact_count: '1', committed_at: '2026-09-11T12:00:00Z', held_reasons: [] }], next_cursor: null } as never
    if (url.includes('/fields/')) return { text: 'complete <script>source</script>', offset: '0', full_utf8_bytes: '32', next_cursor: null } as never
    return { records: [record], next_cursor: null } as never
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(ImportRecordPanel, { props: { importId: 'import', planId: 'plan', mode: 'plan' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
beforeEach(() => { fetch.mockReset() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = '' })
describe('ImportRecordPanel', () => {
  it('shows exact IDs, held reasons, transformations and escaped abbreviated values', async () => {
    const { wrapper } = await setup()
    expect(wrapper.text()).toContain(record.source_id); expect(wrapper.text()).toContain('Trash person'); expect(wrapper.text()).toContain('Duplicate contacts')
    expect(wrapper.text()).toContain('Name abbreviated'); expect(wrapper.find('img').exists()).toBe(false)
    const inspect = wrapper.findAll('button').find(b => b.text() === `Inspect source ${record.source_id}`)!
    await inspect.trigger('click'); await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url.includes('/records/record/fields/provenance'))).toBe(true)
    expect(wrapper.find('script').exists()).toBe(false)
  })
  it('switches to result pages and links actual imported People', async () => {
    const { wrapper } = await setup()
    await wrapper.setProps({ mode: 'results' }); await flushPromises()
    expect(wrapper.get('a').attributes('href')).toBe('/people/person')
    expect(wrapper.text()).toContain('Imported'); expect(wrapper.text()).not.toContain('Trash person')
    await wrapper.get('select').setValue('pending'); await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url.endsWith('/results?disposition=pending&limit=50'))).toBe(true)
  })
  it('ignores a late source page after an actor change', async () => {
    let resolve!: (value: unknown) => void
    const { wrapper, client } = await setup(url => url.includes('/records?') ? new Promise(yes => { resolve = yes }) : undefined)
    client.setQueryData(queryKeys.me, { ...me, user: { id: 'other' }, organization: { ...me.organization, role: 'member' } }); await flushPromises()
    resolve({ records: [record], next_cursor: null }); await flushPromises()
    expect(wrapper.text()).not.toContain(record.source_id); expect(wrapper.find('table').exists()).toBe(false)
  })
  it('preserves the visible result filter and cursor when parent progress refreshes it', async () => {
    const { wrapper } = await setup(url => url.includes('/results?') ? { results: [], next_cursor: url.includes('cursor=next') ? null : 'next' } : undefined)
    await wrapper.setProps({ mode: 'results', resultVersion: 'running:0' }); await flushPromises()
    await wrapper.get('select').setValue('imported'); await flushPromises()
    await wrapper.findAll('button').find(b => b.text() === 'Next People')!.trigger('click'); await flushPromises()
    const before = fetch.mock.calls.length
    await wrapper.setProps({ resultVersion: 'running:1' }); await flushPromises()
    expect(fetch.mock.calls.slice(before).map(([url]) => url)).toEqual(['/migrations/fub/imports/import/results?disposition=imported&cursor=next&limit=50'])
    expect((wrapper.get('select').element as HTMLSelectElement).value).toBe('imported')
    expect(wrapper.findAll('button').find(b => b.text() === 'Previous People')!.attributes('disabled')).toBeUndefined()
  })
})
