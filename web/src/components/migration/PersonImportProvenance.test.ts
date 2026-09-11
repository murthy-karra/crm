import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { PersonProvenance } from '../../api/imports'
import type { MeResponse } from '../../api/types'
import { resetWorkspace } from '../../workspaceLifecycle'
import PersonImportProvenance from './PersonImportProvenance.vue'

vi.mock('../../api/client', async (original) => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
function identity(role: 'admin' | 'member' = 'admin', org = 'org'): MeResponse {
  return { user: { id: 'admin', display_name: 'Synthetic admin', email: 'admin@synthetic.test' }, organization: { id: org, name: 'Synthetic', role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
}
function provenance(person = 'person'): PersonProvenance {
  return { person_id: person, import_id: 'run', plan_id: 'plan', snapshot_id: 'snapshot', source_record_id: 'record', capture_id: 'capture', source_id: '9007199254740993', committed_at: '2026-09-11T12:00:00Z', operational_use_available: false,
    source: { family: 'people', provenance: { total_count: '21', abbreviated: true, field_key: 'provenance', fields: [
      { field_key: 'source.abcdef', label: 'sourceUrl', label_abbreviated: false, label_full_utf8_bytes: '9', value: '<img src="https://source.invalid/x" onerror="unsafe()">', abbreviated: true, full_utf8_bytes: '2818088' },
    ] }, contacts: { total_count: '1', abbreviated: false, field_key: 'contacts', entries: [{ kind: 'email', value: 'source@synthetic.test', normalized_value: 'source@synthetic.test', import_order: 0 }] } } }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((yes) => { resolve = yes }); return { resolve, promise } }
async function setup(handle?: (path: string, init?: RequestInit) => unknown, me = identity()) {
  api.mockImplementation(async (path, init) => {
    if (path === '/me') return me as never
    if (handle) return await handle(path, init) as never
    return provenance() as never
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(PersonImportProvenance, { props: { personId: 'person' }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach((fn) => fn()); resetWorkspace() })

describe('PersonImportProvenance', () => {
  it('shows exact attribution and abbreviation, escapes source text, and explicitly opens a bounded field', async () => {
    const { wrapper } = await setup((path) => path.includes('/fields/')
      ? { text: '"https://source.invalid/retained', offset: '0', full_utf8_bytes: '2818088', next_cursor: 'next' }
      : provenance())
    expect(wrapper.text()).toContain('9007199254740993')
    expect(wrapper.text()).toContain('2818088')
    expect(wrapper.text()).toContain('Showing part of 21 retained source fields')
    expect(wrapper.text()).toContain('separate from Inquiries and activity')
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.find('a[href^="https://source.invalid"]').exists()).toBe(false)
    expect(api.mock.calls.filter(([path]) => path.includes('/fields/'))).toHaveLength(0)
    await wrapper.findAll('button').find((button) => button.text() === 'Inspect sourceUrl')!.trigger('click')
    await flushPromises()
    expect(api.mock.calls.map(([path]) => path)).toContain('/people/person/import-provenance/fields/source.abcdef?limit=65536')
    expect(wrapper.get('pre').text()).toContain('https://source.invalid/retained')
    expect(api.mock.calls.every(([, init]) => !init?.method)).toBe(true)
  })

  it('hides absent provenance and never requests it for a member', async () => {
    const absent = await setup(() => { throw new ApiError(404, 'not_found') })
    expect(absent.wrapper.find('section').exists()).toBe(false)
    const before = api.mock.calls.filter(([path]) => path.includes('import-provenance')).length
    const member = await setup(() => { throw new Error('Member must not read provenance') }, identity('member'))
    expect(member.wrapper.find('section').exists()).toBe(false)
    expect(api.mock.calls.filter(([path]) => path.includes('import-provenance'))).toHaveLength(before)
  })

  it('rejects a late previous-Person response and clears source inspection on Person change', async () => {
    const old = deferred<PersonProvenance>()
    let oldSignal: AbortSignal | undefined
    const { wrapper } = await setup((path, init) => {
      if (path === '/people/person/import-provenance') { oldSignal = init?.signal as AbortSignal; return old.promise }
      return { ...provenance('other'), source_id: '22' }
    })
    await wrapper.setProps({ personId: 'other' }); await flushPromises()
    expect(oldSignal?.aborted).toBe(true)
    old.resolve(provenance()); await flushPromises()
    expect(wrapper.text()).toContain('FUB Person 22')
    expect(wrapper.text()).not.toContain('9007199254740993')
    expect(wrapper.find('pre').exists()).toBe(false)
  })

  it('discards pending private provenance after an Organization or role change', async () => {
    const old = deferred<PersonProvenance>()
    const { wrapper, client } = await setup(() => old.promise)
    client.setQueryData(queryKeys.me, identity('member', 'other-org'))
    await flushPromises()
    old.resolve(provenance()); await flushPromises()
    expect(wrapper.find('section').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('9007199254740993')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'people-imports'] })).toHaveLength(0)
  })
})
