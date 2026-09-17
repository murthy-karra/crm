import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import FamilyRefreshFields from './FamilyRefreshFields.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: (() => void)[] = []
const me = (role = 'admin') => ({ user: { id: 'actor' }, organization: { id: 'org', role, workspace_mode: 'migration_review', workspace_revision: '2' } })
const field = { id: 'field', section: 'source', label: 'Exact value', total_bytes: '9007199254740993123' }
async function setup(handler: (url: string) => unknown) {
  api.mockImplementation(async url => url === '/me' ? me() as never : await handler(url) as never)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(FamilyRefreshFields, { props: { bundleId: 'bundle', itemId: 'item', revision: '3' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace() })
it('pages exact text without interpreting HTML or converting decimal strings', async () => {
  const { wrapper } = await setup(url => url.includes('/fields/field') ? { item_id: 'item', field_id: 'field', total_bytes: field.total_bytes, offset: '0', text: url.includes('cursor=next') ? 'tail' : '<img src="https://never-fetch.invalid">9007199254740993123', next_cursor: url.includes('cursor=next') ? null : 'next' } : { item_id: 'item', items: [field], next_cursor: null })
  const click = async (label: string) => { await wrapper.findAll('button').find(b => b.text() === label)!.trigger('click'); await flushPromises() }
  await click('Inspect Exact value')
  expect(wrapper.text()).toContain(field.total_bytes)
  expect(wrapper.find('pre').text()).toContain('<img')
  expect(wrapper.find('img').exists()).toBe(false)
  await click('More value pages')
  expect(wrapper.find('pre').text()).toBe('tail')
  await click('Previous value page')
  expect(wrapper.find('pre').text()).toContain('9007199254740993123')
  expect(api.mock.calls.every(([url, init]) => url === '/me' || init?.cache === 'no-store')).toBe(true)
})
it('discards a late private value after authority changes', async () => {
  let resolve!: (value: unknown) => void
  const pending = new Promise(yes => { resolve = yes })
  const { wrapper, client } = await setup(url => url.includes('/fields/field') ? pending : { item_id: 'item', items: [field], next_cursor: null })
  await wrapper.findAll('button').find(b => b.text() === 'Inspect Exact value')!.trigger('click'); await flushPromises()
  client.setQueryData(queryKeys.me, me('member')); await flushPromises()
  resolve({ item_id: 'item', field_id: 'field', total_bytes: '6', offset: '0', text: 'SECRET', next_cursor: null }); await flushPromises()
  expect(wrapper.text()).not.toContain('SECRET')
  expect(wrapper.find('section').exists()).toBe(false)
})
it('clears selected values when the item or bundle revision changes', async () => {
  const { wrapper } = await setup(url => url.includes('/fields/field') ? { item_id: 'item', field_id: 'field', total_bytes: '6', offset: '0', text: 'SECRET', next_cursor: null } : { item_id: 'item', items: [field], next_cursor: null })
  await wrapper.findAll('button').find(b => b.text() === 'Inspect Exact value')!.trigger('click'); await flushPromises()
  expect(wrapper.text()).toContain('SECRET')
  await wrapper.setProps({ revision: '4' }); await flushPromises()
  expect(wrapper.text()).not.toContain('SECRET')
})
