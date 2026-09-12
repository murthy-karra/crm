import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import PersonMetadataProvenance from './PersonMetadataProvenance.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const source = { fields: [{ key: 'source.h', label: 'Retained field', text: '"<script>synthetic</script>"', abbreviated: true, label_abbreviated: false, full_utf8_bytes: '3000000', label_full_utf8_bytes: '14' }], total_fields: '1', abbreviated: true, field_key: 'source.all' }
const result = { id: 'result', kind: 'people', source_id: '900719925474099312345', person_id: 'person', mapping_id: null, record_id: 'record', disposition: 'applied', committed_at: '2026-09-11T12:00:00Z', counts: {}, reasons: ['text_trimmed'], source, operations: { ...source, fields: [{ ...source.fields[0], key: 'operations.h', label: 'Applied field', text: '"native value"' }], field_key: 'operations.all' } }
function identity(role = 'admin', org = 'org') { return { user: { id: 'admin', email: 'synthetic@test.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Org', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false } }
async function setup(handle: (url: string) => unknown, role = 'admin') {
  api.mockImplementation(async url => url === '/me' ? identity(role) as never : await handle(url) as never)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(PersonMetadataProvenance, { props: { personId: 'person' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace() })
describe('PersonMetadataProvenance', () => {
  it('shows source and applied values separately and reads the exact committed result field', async () => {
    const { wrapper } = await setup(url => url.includes('/fields/') ? { text: 'exact retained source 🦀', offset_bytes: '0', full_utf8_bytes: '25', next_cursor: null, complete: true } : { items: [result], next_cursor: null })
    expect(wrapper.text()).toContain('900719925474099312345'); expect(api.mock.calls).toHaveLength(2)
    await wrapper.findAll('button').find(b => b.text() === 'Inspect imported metadata')!.trigger('click'); await flushPromises()
    expect(wrapper.text()).toContain('<script>synthetic</script>'); expect(wrapper.find('script').exists()).toBe(false); expect(wrapper.text()).toContain('native value')
    await wrapper.findAll('button').find(b => b.text() === 'Inspect Retained field')!.trigger('click'); await flushPromises()
    expect(api.mock.calls.some(([url]) => url === '/people/person/metadata-import-provenance/result/fields/source.h?limit=65536')).toBe(true)
    expect(wrapper.get('pre').text()).toBe('exact retained source 🦀'); expect(api.mock.calls.every(([, init]) => !init?.method)).toBe(true)
  })
  it('discards prior Person evidence when navigating or losing tenant authority', async () => {
    let resolve!: (value: unknown) => void
    const delayed = new Promise(yes => { resolve = yes })
    const { wrapper, client } = await setup(url => url.startsWith('/people/person/') ? delayed : { items: [{ ...result, id: 'new-result', source_id: '202' }], next_cursor: null })
    await wrapper.setProps({ personId: 'another' }); await flushPromises()
    resolve({ items: [result], next_cursor: null }); await flushPromises()
    expect(wrapper.text()).toContain('202'); expect(wrapper.text()).not.toContain('900719925474099312345')
    client.setQueryData(queryKeys.me, identity('member', 'other')); await flushPromises()
    expect(wrapper.text()).toBe('')
    for (const query of client.getQueryCache().findAll({ queryKey: ['org', 'org', 'metadata-imports'] })) expect(query.state.data).toBeUndefined()
  })
  it.each(['empty', 'missing', 'member'] as const)('hides the panel for %s without offering mutations', async kind => {
    const { wrapper } = await setup(() => { if (kind === 'missing') throw new ApiError(404, 'not_found'); return { items: [], next_cursor: null } }, kind === 'member' ? 'member' : 'admin')
    expect(wrapper.text()).toBe(''); expect(api.mock.calls.every(([, init]) => !init?.method)).toBe(true)
    if (kind === 'member') expect(api.mock.calls.map(([url]) => url)).toEqual(['/me'])
  })
})
