import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import FamilyRefreshMappings from './FamilyRefreshMappings.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: (() => void)[] = []
const me = (role = 'admin') => ({ user: { id: 'actor' }, organization: { id: 'org', role, workspace_mode: 'migration_review', workspace_revision: '2' } })
async function setup() {
  api.mockImplementation(async url => {
    if (url === '/me') return me() as never
    if (url.includes('/targets')) return { mapping_id: 'mapping', items: [{ id: 'target', label: 'Review Agent', field_type: null, status: 'active' }], next_cursor: null } as never
    return { items: [{ id: 'mapping', parent_id: null, kind: 'task_assignee', label: 'Source agent', value_qualified: true, creation_allowed: false, choice: { action: 'hold' } }], inventory_complete: true, next_cursor: null } as never
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(FamilyRefreshMappings, { props: { bundleId: 'bundle', planId: 'plan', revision: '3', family: 'activity', disabled: false }, global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace() })
it('uses scoped destination reads and emits explicit reversible choices', async () => {
  const { wrapper } = await setup()
  await wrapper.findAll('button').find(b => b.text() === 'Choose existing destination')!.trigger('click'); await flushPromises()
  expect(api.mock.calls.some(([url]) => url === '/migrations/fub/family-refreshes/bundle/mappings/mapping/targets?limit=25')).toBe(true)
  await wrapper.findAll('button').find(b => b.text() === 'Choose Review Agent')!.trigger('click')
  expect(wrapper.emitted('change')?.at(-1)).toEqual([[{ mapping_id: 'mapping', choice: { action: 'existing', target_id: 'target' } }]])
  await wrapper.find('select').setValue('unassigned')
  expect(wrapper.emitted('change')?.at(-1)).toEqual([[{ mapping_id: 'mapping', choice: { action: 'unassigned' } }]])
  await wrapper.setProps({ revision: '4' }); await flushPromises()
  expect(wrapper.emitted('change')?.at(-1)).toEqual([[]])
})
it('clears private destination labels and drafts after access changes', async () => {
  const { wrapper, client } = await setup()
  await wrapper.findAll('button').find(b => b.text() === 'Choose existing destination')!.trigger('click'); await flushPromises()
  expect(wrapper.text()).toContain('Review Agent')
  client.setQueryData(queryKeys.me, me('member')); await flushPromises()
  expect(wrapper.text()).not.toContain('Review Agent')
  expect(wrapper.emitted('change')?.at(-1)).toEqual([[]])
})
