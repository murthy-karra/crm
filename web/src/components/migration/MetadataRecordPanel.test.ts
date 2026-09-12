import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import MetadataRecordPanel from './MetadataRecordPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
async function setup(mode: 'plan' | 'results' = 'plan') {
  api.mockImplementation(async url => {
    if (url === '/me') return { user: { id: 'admin' }, organization: { id: 'org', role: 'admin', workspace_mode: 'migration_review', workspace_revision: '2' } } as never
    return { items: [], next_cursor: url.includes('cursor=') ? null : 'next/+=' } as never
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(MetadataRecordPanel, { props: { importId: 'child', planId: 'plan', mode, progressVersion: 'first' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }]], stubs: { RouterLink: true } } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return wrapper
}
beforeEach(() => { api.mockReset() })
afterEach(() => cleanup.splice(0).forEach(fn => fn()))
describe('MetadataRecordPanel', () => {
  it('clears incompatible filters when switching between planned and committed views', async () => {
    const wrapper = await setup('results')
    await wrapper.findAll('select')[0]!.setValue('created'); await wrapper.findAll('select')[1]!.setValue('tag'); await flushPromises()
    await wrapper.setProps({ mode: 'plan' }); await flushPromises()
    expect(wrapper.get('select').element.value).toBe('')
    const plans = api.mock.calls.filter(([url]) => url.includes('/records?')).map(([url]) => url)
    expect(plans).toEqual(['/migrations/fub/metadata-imports/child/plans/plan/records?limit=50'])
    await wrapper.get('select').setValue('eligible'); await flushPromises()
    const before = api.mock.calls.length
    await wrapper.setProps({ mode: 'results' }); await flushPromises()
    expect(wrapper.findAll('select').map(select => select.element.value)).toEqual(['', ''])
    expect(api.mock.calls.slice(before).map(([url]) => url)).toEqual(['/migrations/fub/metadata-imports/child/results?limit=50'])
  })
  it.each(['plan', 'results'] as const)('refreshes only the visible %s page while preserving valid filters and its cursor', async mode => {
    const wrapper = await setup(mode)
    await wrapper.findAll('select')[0]!.setValue('held'); await flushPromises()
    await wrapper.findAll('button').find(button => button.text() === 'Next metadata records')!.trigger('click'); await flushPromises()
    const before = api.mock.calls.length
    await wrapper.setProps({ progressVersion: 'second' }); await flushPromises()
    expect(api.mock.calls).toHaveLength(before + 1)
    const last = api.mock.calls.at(-1)![0]
    expect(last).toContain('disposition=held&cursor=next%2F%2B%3D&limit=50')
    expect(wrapper.findAll('select')[0]!.element.value).toBe('held')
    expect(wrapper.findAll('button').find(button => button.text() === 'Previous metadata records')!.attributes('disabled')).toBeUndefined()
  })
})
