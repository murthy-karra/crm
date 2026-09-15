import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import AdmittedHistoryImportPanel from './AdmittedHistoryImportPanel.vue'

vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))

const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/admitted-history-imports'
const cleanup: Array<() => void> = []
const identity = { user: { id: 'admin', email: 'admin@synthetic.test', display_name: 'Admin' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
const parent = { id: 'parent', state: 'completed', confirmed_plan_id: 'parent-plan' }
const admission = { id: 'admission', parent_import_id: 'parent', state: 'completed', progress: { settled_items: '2' } }
const streams = ['events', 'calls', 'text_messages'].map(family => ({ family, state: 'enumerated' }))
const capture = { id: 'capture', parent_import_id: 'parent', state: 'completed_with_gaps', source_account_id: '900719925474099312345', source_user_id: '17', started_at: '2026-09-11T10:00:00Z', completed_at: '2026-09-11T11:00:00Z', streams }
const root = { id: 'history-root', state: 'ready', phase: 'review', revision: '9', admission_id: 'admission', history_capture_id: 'capture', workspace_revision: '2', source_binding: { source_account_id: '900719925474099312345' }, coverage: { streams: streams.map(stream => ({ ...stream, reported_total: '1' })) }, latest_plan: { id: 'plan', state: 'ready', expires_at: null, counts: { eligible: '1' } }, results: {}, current_attempt_id: null, pause_reason: null, run_byte_limit: '100', retained_bytes: '10', reserved_bytes: '20', actions: { confirm: true, resume: false, cancel: true, remainder: false } }

async function setup(options: { handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  api.mockImplementation(async (url, init) => {
    const custom = options.handle?.(url, init)
    if (custom !== undefined) return await custom as never
    if (url === '/me') return identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [parent], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return parent as never
    if (url === '/migrations/fub/people-admissions?parent_import_id=parent&limit=20') return { items: [admission], next_cursor: null } as never
    if (url === '/migrations/fub/history-captures?parent_import_id=parent&limit=20') return { captures: [capture], next_cursor: null } as never
    if (url === `${ROOT}?admission_id=admission&limit=25`) return { imports: [root], next_cursor: null } as never
    if (url === `${ROOT}/history-root`) return root as never
    if (url === `${ROOT}/history-root/plans/plan/manifests?limit=25`) return { manifests: [], next_cursor: null } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, identity) })
  const wrapper = mount(AdmittedHistoryImportPanel, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, client, refreshWorkspace }
}

function button(label: string): HTMLButtonElement {
  const element = [...document.body.querySelectorAll('button')].find(value => value.textContent?.trim() === label)
  expect(element, label).toBeDefined()
  return element!
}
function writes(suffix = '') { return api.mock.calls.filter(([url, init]) => init?.method === 'POST' && url.endsWith(suffix)) }

beforeEach(() => {
  api.mockReset()
  resetWorkspace()
  vi.stubGlobal('crypto', { randomUUID: () => 'request-uuid' })
})
afterEach(() => {
  cleanup.splice(0).forEach(remove => remove())
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
  resetWorkspace()
})

describe('admitted history workflow', () => {
  it('uses one selected parent for terminal admission and fully enumerated retained capture', async () => {
    const { wrapper } = await setup({ handle: (url, init) => {
      if (url === ROOT && init?.method === 'POST') return { import: root }
    } })
    expect(api.mock.calls.some(([url]) => url === '/migrations/fub/history-captures?parent_import_id=parent&limit=20')).toBe(true)
    expect(button('Prepare retained history').disabled).toBe(false)
    button('Prepare retained history').click()
    await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: 'request-uuid', admission_id: 'admission', history_capture_id: 'capture' })
    expect(wrapper.text()).toContain('Coverage:')
  })

  it('replays the same lost confirmation request after a terminal readback', async () => {
    let attempts = 0
    const { wrapper } = await setup({ handle: (url, init) => {
      if (url.endsWith('/confirm') && init?.method === 'POST') return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { import: root }
    } })
    for (const checkbox of wrapper.findAll('input[type=checkbox]')) await checkbox.setValue(true)
    button('Confirm metadata import').click()
    await flushPromises()
    button('Confirm import').click()
    await flushPromises()
    button('Retry the same request').click()
    await flushPromises()
    expect(writes('/confirm')).toHaveLength(2)
    expect(writes('/confirm')[0]![1]?.body).toBe(writes('/confirm')[1]![1]?.body)
  })
})
