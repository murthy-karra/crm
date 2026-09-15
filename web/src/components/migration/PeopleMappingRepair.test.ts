import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { PeopleRefresh } from '../../api/peopleRefreshes'
import type { RepairOwner } from '../../api/peopleMappingRepairs'
import PeopleMappingRepair from './PeopleMappingRepair.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const fetch = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const identity = { user: { id: 'admin', email: 'admin@synthetic.test', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
const current = (): PeopleRefresh => ({ id: 'repair', parent_import_id: 'parent', report_id: 'report', state: 'paused', lifecycle_revision: '3', created_at: '', updated_at: '', pause_reason: 'awaiting_mapping_choices', progress: { settled_items: '0' }, actions: { confirm: false, repreview: false, retry: false, cancel: true }, mode: 'mapping_repair', repair_draft_revision: '1', repair_candidate_count: '2', repair_candidates_complete: true, repair_source_refresh_id: 'source' })
async function setup(owner: RepairOwner = 'original', handle?: (url: string, init?: RequestInit) => unknown) {
  fetch.mockImplementation(async (url, init) => {
    const value = handle?.(url, init); if (value !== undefined) return await value as never
    if (url === '/me') return identity as never
    if (url === '/stages') return { stages: [{ id: 'lead', name: 'Lead', position: 0 }] } as never
    if (url === '/organization/members') return { members: [{ user_id: 'member', display_name: 'Active', email: 'active@synthetic.test', status: 'active' }, { user_id: 'inactive', display_name: 'Inactive', email: 'inactive@synthetic.test', status: 'inactive' }] } as never
    if (url.includes('/repair-mappings?')) return { items: [{ id: 'stage-key', kind: 'stage', source: { kind: 'stage_label', key: '<script>source</script>' }, affected_count: '2', choice: null }, { id: 'agent-key', kind: 'assignee', source: { kind: 'assignee', key: '9' }, affected_count: '1', choice: null }], draft_revision: '1', next_cursor: 'page-two' } as never
    if (init?.method === 'POST') return { refresh_id: 'repair' } as never
    throw new Error(`Unexpected ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(PeopleMappingRepair, { props: { owner, current: current(), disabled: false }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
function button(wrapper: Awaited<ReturnType<typeof setup>>['wrapper'], text: string) { return wrapper.findAll('button').find(b => b.text() === text)! }
beforeEach(() => { fetch.mockReset() })
afterEach(() => { cleanup.splice(0).forEach(f => f()); document.body.innerHTML = '' })
describe('People mapping repair', () => {
  it.each(['original', 'admitted'] as const)('edits a %s draft without a plan and sends exact typed choices', async owner => {
    const { wrapper } = await setup(owner)
    expect(wrapper.text()).toContain('2 People in this repair')
    expect(wrapper.find('script').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('Inactive')
    await wrapper.get('#repair-stage-key').setValue('existing:lead')
    await wrapper.get('#repair-agent-key').setValue('unassigned')
    expect(button(wrapper, 'Prepare full People preview').attributes('disabled')).toBeDefined()
    expect(button(wrapper, 'Next mappings').attributes('disabled')).toBeDefined()
    await button(wrapper, 'Save mapping choices').trigger('click'); await flushPromises()
    const [url, init] = fetch.mock.calls.find(([, init]) => init?.method === 'POST')!
    expect(url).toContain(owner === 'original' ? '/people-refreshes/' : '/admitted-people-refreshes/')
    expect(JSON.parse(init!.body as string)).toMatchObject({ expected_draft_revision: 1, choices: [{ key_id: 'stage-key', disposition: 'existing', target_id: 'lead' }, { key_id: 'agent-key', disposition: 'unassigned', target_id: null }] })
    expect(wrapper.emitted('reload')).toHaveLength(1)
  })
  it('seals choices using the lifecycle revision and never sends Retry', async () => {
    const { wrapper } = await setup()
    await button(wrapper, 'Prepare full People preview').trigger('click'); await flushPromises()
    const [url, init] = fetch.mock.calls.find(([, init]) => init?.method === 'POST')!
    expect(url).toMatch(/\/plans$/)
    expect(JSON.parse(init!.body as string)).toMatchObject({ expected_plan_revision: 3 })
  })
  it('replays the identical request after an uncertain response', async () => {
    let attempts = 0
    const { wrapper } = await setup('original', (_url, init) => { if (init?.method === 'POST' && attempts++ === 0) return Promise.reject(new TypeError('network interrupted')) })
    await wrapper.get('#repair-agent-key').setValue('unassigned')
    await button(wrapper, 'Save mapping choices').trigger('click'); await flushPromises()
    await button(wrapper, 'Retry same mapping request').trigger('click'); await flushPromises()
    const posts = fetch.mock.calls.filter(([, init]) => init?.method === 'POST')
    expect(posts).toHaveLength(2); expect(posts[0]).toEqual(posts[1])
  })
  it('clears choices and sensitive labels on access loss', async () => {
    const { wrapper, client } = await setup()
    await wrapper.get('#repair-agent-key').setValue('unassigned')
    client.setQueryData(queryKeys.me, { ...identity, organization: { ...identity.organization, role: 'member' } }); await flushPromises()
    expect(wrapper.text()).not.toContain('<script>source</script>')
    expect(wrapper.find('select').exists()).toBe(false)
    expect(fetch.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(0)
  })
})
