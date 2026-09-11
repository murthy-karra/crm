import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { ImportMapping, ImportMappingPage, ImportSourceDisplay } from '../../api/imports'
import ImportMappingPanel from './ImportMappingPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const fetch = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const identity = { user: { id: 'admin', email: 'admin@synthetic.test', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'operational', workspace_revision: '1' }, platform_admin: false }
const source: ImportSourceDisplay = { family: 'stage', label: { value: 'Unmapped <script>stage</script>', abbreviated: true, full_utf8_bytes: '2048', field_key: 'source.name' }, can_create: true, provenance: { fields: [], total_count: '1', abbreviated: false, field_key: 'provenance' } }
function row(key = '12'): ImportMapping { return { id: `mapping-${key}`, source_key: key, qualified: true, disposition: 'hold', source, choice: { kind: 'hold' }, suggestions: [{ id: 'lead', name: 'Lead' }], target: null, reasons: ['stage_choice_required'], dependent_count: '9007199254740993' } }
async function setup(handle?: (url: string) => unknown) {
  fetch.mockImplementation(async url => { const value = handle?.(url); if (value !== undefined) return await value as never
    if (url === '/me') return identity as never
    if (url === '/stages') return { stages: [{ id: 'lead', name: 'Lead', position: 0 }] } as never
    if (url === '/organization/members') return { members: [{ user_id: 'member', display_name: 'Member', email: 'member@synthetic.test', status: 'active' }, { user_id: 'inactive', display_name: 'Inactive', email: 'inactive@synthetic.test', status: 'inactive' }] } as never
    if (url.includes('kind=assignee')) return { mappings: [{ ...row('user:8'), source: { ...source, family: 'user', is_pond: false }, reasons: ['assignee_choice_required'] }], next_cursor: null } as never
    if (url.includes('/fields/')) return { text: '<script>source</script>', offset: '0', full_utf8_bytes: '23', next_cursor: null } as never
    if (url.includes('/mappings?')) return { mappings: [row()], next_cursor: null } as never
    throw new Error(`Unexpected ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(ImportMappingPanel, { props: { importId: 'import', planId: 'plan', revision: '1', canReplan: true }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
function button(wrapper: Awaited<ReturnType<typeof setup>>['wrapper'], text: string) { return wrapper.findAll('button').find(b => b.text() === text)! }
beforeEach(() => { fetch.mockReset() })
afterEach(() => { cleanup.splice(0).forEach(f => f()); document.body.innerHTML = '' })
describe('ImportMappingPanel', () => {
  it('requires explicit create/unassigned choices and emits only changed patches', async () => {
    const { wrapper } = await setup()
    expect(wrapper.emitted('apply')).toBeUndefined(); expect(wrapper.find('script').exists()).toBe(false)
    await wrapper.get('select[aria-label="Stage choice for source 12"]').setValue('create')
    await button(wrapper, 'Assignment mappings').trigger('click'); await flushPromises()
    expect(wrapper.text()).not.toContain('Inactive')
    await wrapper.get('select[aria-label="Assignment choice for source user:8"]').setValue('unassigned')
    await button(wrapper, 'Apply 2 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([{ stage_mappings: [{ source_key: '12', choice: { kind: 'create' } }], assignee_mappings: [{ source_key: 'user:8', choice: { kind: 'unassigned' } }] }])
    expect(fetch.mock.calls.every(([, init]) => init?.method !== 'POST')).toBe(true)
  })
  it('leaves inherited choices unchanged and sends an explicit existing-stage choice', async () => {
    const { wrapper } = await setup(url => url.includes('kind=stage') ? { mappings: [{ ...row('4'), choice: { kind: 'existing', stage_id: 'lead' } }, row('12')], next_cursor: null } : undefined)
    await wrapper.get('select[aria-label="Stage choice for source 12"]').setValue('existing:lead')
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([{ stage_mappings: [{ source_key: '12', choice: { kind: 'existing', stage_id: 'lead' } }], assignee_mappings: [] }])
  })
  it('caps combined draft patches at 50 across bounded pages', async () => {
    const { wrapper } = await setup(url => url.includes('/mappings?') ? { mappings: url.includes('cursor=next') ? [row('51')] : Array.from({ length: 50 }, (_, n) => row(String(n + 1))), next_cursor: url.includes('cursor=next') ? null : 'next' } : undefined)
    for (const select of wrapper.findAll('select')) await select.setValue('existing:lead')
    await button(wrapper, 'Next mappings').trigger('click'); await flushPromises()
    await wrapper.get('select').setValue('create')
    expect(wrapper.text()).toContain('Apply these 50 changes before choosing more')
    await button(wrapper, 'Apply 50 mapping changes').trigger('click')
    expect((wrapper.emitted('apply')![0]![0] as { stage_mappings: unknown[] }).stage_mappings).toHaveLength(50)
  })
  it('uses the mapping-field endpoint and clears source details after a plan change', async () => {
    let resolve!: (value: ImportMappingPage) => void
    const { wrapper } = await setup(url => url.includes('/plans/new/') ? new Promise<ImportMappingPage>(yes => { resolve = yes }) : undefined)
    await button(wrapper, 'Inspect source 12').trigger('click'); await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url.includes('/mappings/mapping-12/fields/provenance'))).toBe(true)
    await wrapper.setProps({ planId: 'new', revision: '2' }); await flushPromises()
    expect(wrapper.text()).not.toContain('<script>source</script>')
    resolve({ mappings: [], next_cursor: null }); await flushPromises()
  })
  it('discards a late mapping page from the superseded plan', async () => {
    let resolve!: (value: ImportMappingPage) => void
    const { wrapper } = await setup(url => url.includes('/plans/plan/mappings?') ? new Promise<ImportMappingPage>(yes => { resolve = yes }) : url.includes('/plans/new/mappings?') ? { mappings: [row('99')], next_cursor: null } : undefined)
    await wrapper.setProps({ planId: 'new', revision: '2' }); await flushPromises()
    resolve({ mappings: [row('12')], next_cursor: null }); await flushPromises()
    expect(wrapper.find('select[aria-label="Stage choice for source 99"]').exists()).toBe(true)
    expect(wrapper.find('select[aria-label="Stage choice for source 12"]').exists()).toBe(false)
    expect(wrapper.emitted('apply')).toBeUndefined()
  })
  it('discards old actor data and disables mappings for members', async () => {
    const { wrapper, client } = await setup()
    client.setQueryData(queryKeys.me, { ...identity, organization: { ...identity.organization, role: 'member' } }); await flushPromises()
    expect(wrapper.find('table').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('9007199254740993')
  })
})
