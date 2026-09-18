import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import type { ReconciliationSummary } from '../../api/migrationReconciliation'
import MigrationReconciliationPanel from './MigrationReconciliationPanel.vue'

vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const me = (org = 'org', role = 'admin', mode = 'migration_review') => ({ user: { id: 'actor' }, organization: { id: org, role, workspace_mode: mode, workspace_revision: '2' } })
const large = '9007199254740993123'
const initial: ReconciliationSummary = {
  original_import_id: 'parent', workspace_revision: '2', generated_at: '2026-09-17T00:00:00Z', review_hold: true, blockers: [],
  families: [
    { coverage: { family: 'people_contacts', path: 'implemented', source_evidence: 'retained_core_profile', destination_surface: 'typed_import', blocker_codes: [] }, unit: 'people', blockers: [], cohorts: [{ cohort_id: 'parent', cohort_origin: 'original', result_totals: { applied: large, already_current: '0', held: '1', excluded: '0', unprocessed: '0' }, latest_bundle: null, evidence_ids: ['parent'], blockers: [] }] },
    { coverage: { family: 'standalone_tag_catalog', path: 'unqualified_source', source_evidence: 'unqualified_catalog', destination_surface: 'unavailable', blocker_codes: ['standalone_tag_catalog_unqualified'] }, unit: 'not_counted', cohorts: [], blockers: [] },
  ],
}
const cleanup: (() => void)[] = []
async function setup(read: () => Promise<ReconciliationSummary> = async () => structuredClone(initial), identity = me()) {
  api.mockImplementation(async url => {
    if (url === '/me') return identity as never
    if (url.includes('/imports?')) return { imports: [{ id: 'parent', state: 'completed' }], next_cursor: null } as never
    if (url.endsWith('/reconciliation')) return await read() as never
    throw new Error(`Unexpected route ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  client.setQueryData(queryKeys.me, identity)
  const wrapper = mount(MigrationReconciliationPanel, { global: { plugins: [[VueQueryPlugin, { queryClient: client }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace() })

it('shows precise counts separately from unsupported coverage and refreshes the same selection', async () => {
  let reads = 0
  const { wrapper } = await setup(async () => { reads++; return structuredClone(initial) })
  await wrapper.get('select').setValue('parent'); await flushPromises()
  expect(wrapper.text()).toContain(`Applied ${large}`)
  const unsupported = wrapper.get('[data-family="standalone_tag_catalog"]')
  expect(unsupported.text()).toContain('Unqualified source catalog')
  expect(unsupported.text()).not.toContain('Applied 0')
  expect(wrapper.text()).toContain('does not approve cutover')
  expect(wrapper.text()).toContain('Initial import results:')
  expect(wrapper.text()).toContain('Later People core refresh and mapping-repair outcomes remain in their detailed review panels.')
  const evidence = wrapper.get('[data-family="people_contacts"] details')
  expect(evidence.get('summary').text()).toBe('Evidence references')
  expect(evidence.get('li').text()).toBe('parent')
  await wrapper.findAll('button').find(button => button.text() === 'Refresh reconciliation')!.trigger('click')
  await flushPromises()
  expect(reads).toBe(2)
  expect(api.mock.calls.some(([, init]) => init?.method === 'POST')).toBe(false)
})

it('shows refresh and warning evidence without merging it with initial import counts', async () => {
  const report = structuredClone(initial)
  const family = report.families[0]!
  family.coverage.family = 'notes'
  family.blockers = [{ code: 'retained_source_truncated', evidence_id: 'capture-evidence' }]
  family.cohorts[0]!.latest_bundle = {
    bundle_id: 'refresh', plan_id: 'refresh-plan', state: 'completed',
    outcome_totals: { applied: '3', already_current: '2', held: '0', excluded: '0', unprocessed: '0' },
    evidence_ids: ['refresh', 'refresh-plan'],
  }
  const { wrapper } = await setup(async () => report)
  await wrapper.get('select').setValue('parent'); await flushPromises()
  const row = wrapper.get('[data-family="notes"]')
  expect(row.text()).toContain(`Initial import results: Applied ${large}`)
  expect(row.text()).toContain('Latest refresh plan: completed · Applied 3 · Already current 2')
  expect(row.text()).toContain('Evidence: capture-evidence')
  expect(row.get('details').text()).toContain('refresh-plan')
})

it('discards a late response when the Organization changes', async () => {
  let resolve!: (value: ReconciliationSummary) => void
  const pending = new Promise<ReconciliationSummary>(done => { resolve = done })
  const { wrapper, client } = await setup(() => pending)
  await wrapper.get('select').setValue('parent'); await flushPromises()
  client.setQueryData(queryKeys.me, me('other'))
  await flushPromises()
  resolve(structuredClone(initial)); await flushPromises()
  expect(wrapper.text()).not.toContain(large)
  expect(wrapper.get('select').element.value).toBe('')
})

it('clears protected data and stops reads after a forbidden response', async () => {
  const { wrapper } = await setup(async () => { throw new ApiError(403, 'forbidden') })
  await wrapper.get('select').setValue('parent'); await flushPromises()
  expect(wrapper.text()).toContain('unavailable with your current access')
  expect(wrapper.find('select').exists()).toBe(false)
})

it('does not load protected report data for an ordinary member or an active workspace', async () => {
  const { wrapper } = await setup(undefined, me('org', 'member'))
  expect(wrapper.find('select').exists()).toBe(false)
  const active = await setup(undefined, me('org', 'admin', 'operational'))
  expect(active.wrapper.find('select').exists()).toBe(false)
  expect(api.mock.calls.some(([url]) => url.includes('/imports'))).toBe(false)
})

it('keeps failures distinct from an empty successful report', async () => {
  const { wrapper } = await setup(async () => { throw new ApiError(503, 'unavailable') })
  await wrapper.get('select').setValue('parent'); await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toContain('could not be read')
  expect(wrapper.text()).not.toContain('Applied 0')
})
