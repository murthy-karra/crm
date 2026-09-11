import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { ImportFieldRequest, ImportFieldSegment } from '../../api/imports'
import type { MeResponse } from '../../api/types'
import { resetWorkspace } from '../../workspaceLifecycle'
import ImportFieldViewer from './ImportFieldViewer.vue'

vi.mock('../../api/client', async (original) => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const request: ImportFieldRequest = { kind: 'mapping', importId: 'run', planId: 'plan', mappingId: 'mapping', fieldKey: 'label' }
const base = '/migrations/fub/imports/run/plans/plan/mappings/mapping/fields/label'
function identity(role: 'admin' | 'member' = 'admin', org = 'org'): MeResponse {
  return { user: { id: 'admin', display_name: 'Synthetic admin', email: 'admin@synthetic.test' }, organization: { id: org, name: 'Synthetic', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((yes) => { resolve = yes }); return { resolve, promise } }
function button(wrapper: VueWrapper, name: string) {
  const found = wrapper.findAll('button').find((candidate) => candidate.text() === name)
  expect(found, name).toBeDefined()
  return found!
}
async function setup(handle: (path: string, init?: RequestInit) => unknown, me = identity()) {
  api.mockImplementation(async (path, init) => {
    if (path === '/me') return me as never
    return await handle(path, init) as never
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(ImportFieldViewer, { props: { request, title: 'Retained stage label' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach((fn) => fn()); resetWorkspace(); vi.useRealTimers() })

describe('ImportFieldViewer', () => {
  it('escapes retained text and shows one UTF-8 segment with accurate byte positions', async () => {
    const first = '<img src=x onerror="unsafe()">é'
    const offset = new TextEncoder().encode(first).length.toString()
    const second = '🦀last'
    const full = new TextEncoder().encode(first + second).length.toString()
    const { wrapper } = await setup((path, init) => {
      expect(init?.signal).toBeInstanceOf(AbortSignal)
      const cursor = new URL(`http://synthetic.test${path}`).searchParams.get('cursor')
      return cursor ? { text: second, offset, full_utf8_bytes: full, next_cursor: null }
        : { text: first, offset: '0', full_utf8_bytes: full, next_cursor: 'next/+=' }
    })
    expect(wrapper.get('pre').text()).toBe(first)
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.text()).toContain(full)
    expect(button(wrapper, 'Previous segment').attributes('disabled')).toBeDefined()
    await button(wrapper, 'Next segment').trigger('click'); await flushPromises()
    expect(wrapper.get('pre').text()).toBe(second)
    expect(wrapper.get('pre').text()).not.toContain(first)
    expect(wrapper.text()).toContain(`Bytes ${offset}–${full} of ${full}`)
    expect(button(wrapper, 'Next segment').attributes('disabled')).toBeDefined()
    expect(api.mock.calls.some(([path]) => path === `${base}?cursor=next%2F%2B%3D&limit=65536`)).toBe(true)
    await button(wrapper, 'Previous segment').trigger('click'); await flushPromises()
    expect(wrapper.get('pre').text()).toBe(first)
    expect(api.mock.calls.every(([, init]) => !init?.method)).toBe(true)
  })

  it('discards a delayed field when the selected mapping changes and aborts the old read', async () => {
    const old = deferred<ImportFieldSegment>()
    let signal: AbortSignal | undefined
    const { wrapper } = await setup((path, init) => {
      if (path.startsWith(base)) { signal = init?.signal as AbortSignal; return old.promise }
      return { text: 'current stage', offset: '0', full_utf8_bytes: '13', next_cursor: null }
    })
    await wrapper.setProps({ request: { ...request, mappingId: 'new-mapping' } })
    await flushPromises()
    expect(signal?.aborted).toBe(true)
    old.resolve({ text: 'stale private source', offset: '0', full_utf8_bytes: '20', next_cursor: null })
    await flushPromises()
    expect(wrapper.get('pre').text()).toBe('current stage')
    expect(wrapper.text()).not.toContain('stale private source')
  })

  it('removes displayed fields and pending reads immediately on admin role loss', async () => {
    const old = deferred<ImportFieldSegment>()
    const { wrapper, client } = await setup((path) => path.includes('cursor=') ? old.promise
      : { text: 'private stage', offset: '0', full_utf8_bytes: '32', next_cursor: 'next' })
    await button(wrapper, 'Next segment').trigger('click'); await flushPromises()
    const readsBeforeRoleLoss = api.mock.calls.filter(([path]) => path !== '/me').length
    client.setQueryData(queryKeys.me, identity('member'))
    await flushPromises()
    old.resolve({ text: 'private late source', offset: '13', full_utf8_bytes: '32', next_cursor: null })
    await flushPromises()
    expect(wrapper.find('pre').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('private')
    expect(wrapper.text()).toContain('current administrator session')
    // A disabled mounted observer may recreate an empty query shell; it must
    // retain no private data and must issue no further retained-field read.
    for (const query of client.getQueryCache().findAll({ queryKey: ['org', 'org', 'people-imports'] })) {
      expect(query.state.data).toBeUndefined()
      expect(query.state.fetchStatus).toBe('idle')
    }
    expect(api.mock.calls.filter(([path]) => path !== '/me')).toHaveLength(readsBeforeRoleLoss)
  })

  it('requires explicit retry after a failed segment read and makes no member read', async () => {
    let attempts = 0
    const { wrapper } = await setup(() => {
      attempts++
      if (attempts === 1) throw new ApiError(503, 'unavailable')
      return { text: 'recovered', offset: '0', full_utf8_bytes: '9', next_cursor: null }
    })
    expect(attempts).toBe(1)
    await button(wrapper, 'Retry field read').trigger('click'); await flushPromises()
    expect(wrapper.get('pre').text()).toBe('recovered')
    const denied = await setup(() => { throw new Error('Member must not read retained fields') }, identity('member'))
    expect(denied.wrapper.find('pre').exists()).toBe(false)
    expect(attempts).toBe(2)
  })
})
