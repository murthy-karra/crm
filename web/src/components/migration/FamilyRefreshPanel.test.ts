import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import type { RefreshCounts, RefreshDetail } from '../../api/familyRefreshes'
import FamilyRefreshPanel from './FamilyRefreshPanel.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const root = '/migrations/fub/family-refreshes'
const me = (role = 'admin') => ({ user: { id: 'actor' }, organization: { id: 'org', role, workspace_mode: 'migration_review', workspace_revision: '2' } })
const counts: RefreshCounts = { units: '9007199254740993123', inserts: '2', updates: '0', already_current: '0', held: '1', excluded: '0', tag_removals: '0', field_clears: '0', task_completions: '0', task_reopens: '0', history_corrections: '7', source_only: '5' }
const initial: RefreshDetail = { bundle: { id: 'bundle', parent_import_id: 'parent', revision: '9007199254740993', state: 'ready', core_report_id: null, history_capture_id: 'capture', predecessor_id: null, digest: 'a'.repeat(64), created_at: '2026-09-16T00:00:00Z', confirmed_at: null }, families: [{ family: 'history', plan_id: 'plan', revision: '7', state: 'ready', phase: 'classify', digest: 'b'.repeat(64), pause_reason: null, expires_at: '2099-01-01T00:00:00Z', counts, results: { ...counts, units: '0' }, completed_units: '0', remainder_eligible: false, retained_bytes: '123', reserved_bytes: '456', run_byte_limit: '9999' }] }
const cleanup: (() => void)[] = []
async function setup(command?: (url: string, body: unknown) => unknown) {
  const detail = structuredClone(initial)
  api.mockImplementation(async (url, init) => {
    if (init?.method === 'POST') return await command?.(url, JSON.parse(String(init.body))) as never
    if (url === '/me') return me() as never
    if (url.includes('/people-imports?') || url.includes('/imports?')) return { imports: [{ id: 'parent', confirmed_plan_id: 'p', created_at: 'Original import', state: 'completed', counts: { imported_people: '10' } }], next_cursor: null } as never
    if (url.startsWith('/migrations/fub/core-change-reports?')) return { reports: [], next_cursor: null } as never
    if (url.startsWith('/migrations/fub/history-captures?')) return { captures: [{ id: 'capture', created_at: 'Retained history', state: 'completed_with_gaps' }], next_cursor: null } as never
    if (url.startsWith(`${root}?`)) return { items: [detail.bundle], next_cursor: null } as never
    if (url === `${root}/bundle`) return structuredClone(detail) as never
    if (url.includes('/items?') || url.includes('/results?')) return { items: [], next_cursor: null, bundle_id: 'bundle', bundle_revision: detail.bundle.revision, plan_id: 'plan', plan_revision: '7', family: 'history' } as never
    throw new Error(`Unexpected synthetic route ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(FamilyRefreshPanel, { props: { refreshWorkspace: async () => {} }, attachTo: document.body, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises()
  await wrapper.findAll('select')[0]!.setValue('parent'); await flushPromises()
  return { wrapper, client, detail }
}
async function click(text: string) { const button = [...document.querySelectorAll('button')].find(b => b.textContent?.trim() === text); expect(button, text).toBeTruthy(); button!.click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; resetWorkspace() })
it('confirms only after reviewing exact counts and replays an uncertain request unchanged', async () => {
  const bodies: unknown[] = []
  const { wrapper, detail } = await setup((url, body) => {
    expect(url).toBe(`${root}/bundle/confirm`); bodies.push(body)
    if (bodies.length === 1) throw new ApiError(503, 'unavailable')
    detail.bundle.state = 'queued'; detail.bundle.confirmed_at = '2026-09-16T01:00:00Z'; detail.bundle.revision = '9007199254740994'; detail.families[0]!.state = 'queued'
    return { bundle_id: 'bundle', revision: detail.bundle.revision, state: 'queued', families: [] }
  })
  await wrapper.findAll('select').find(s => s.text().includes('Choose a refresh'))!.setValue('bundle'); await flushPromises()
  expect(wrapper.text()).toContain(counts.units)
  expect(bodies).toHaveLength(0)
  const ack = wrapper.findAll('label').find(l => l.text().startsWith('I reviewed every selected family'))!
  await ack.find('input').setValue(true); await click('Review confirmation')
  expect(document.querySelector('[role=dialog]')?.textContent).toContain(`Total units: ${counts.units}`)
  expect(bodies).toHaveLength(0)
  await click('Apply reviewed refresh')
  expect(document.body.textContent).toContain('Retry same confirmation')
  await click('Retry same confirmation')
  expect(bodies).toHaveLength(2); expect(bodies[1]).toEqual(bodies[0])
  expect(bodies[0]).toMatchObject({ expected_revision: initial.bundle.revision, bundle_digest: initial.bundle.digest, families: [{ family: 'history', plan_id: 'plan', plan_revision: '7', plan_digest: 'b'.repeat(64), expected_counts: counts }], acknowledged_exclusions: true })
})
it('clears a confirmation after the current admin loses access', async () => {
  const { wrapper, client } = await setup()
  await wrapper.findAll('select').find(s => s.text().includes('Choose a refresh'))!.setValue('bundle'); await flushPromises()
  await wrapper.findAll('label').find(l => l.text().startsWith('I reviewed every selected family'))!.find('input').setValue(true); await click('Review confirmation')
  client.setQueryData(queryKeys.me, me('member')); await flushPromises()
  expect(document.body.textContent).not.toContain('Confirm family refresh')
  expect(api.mock.calls.some(([, init]) => init?.method === 'POST')).toBe(false)
})
it('prepares history-only evidence without sending an unrelated core report', async () => {
  const bodies: unknown[] = []
  const { wrapper } = await setup((url, body) => { expect(url).toBe(root); bodies.push(body); return { bundle_id: 'bundle', revision: '1', state: 'preparing', families: [] } })
  const labels = wrapper.findAll('label')
  for (const name of ['Tags and custom fields', 'Notes and tasks']) await labels.find(l => l.text() === name)!.find('input').setValue(false)
  await wrapper.findAll('select').find(s => s.text().includes('Choose history evidence'))!.setValue('capture')
  await click('Prepare family refresh')
  expect(bodies).toHaveLength(1)
  expect(bodies[0]).toMatchObject({ parent_import_id: 'parent', families: ['history'], core_report_id: null, history_capture_id: 'capture' })
})

it('prepares one exact remainder for all eligible cancelled families', async () => {
  const bodies: unknown[] = []
  const { wrapper, detail } = await setup((url, body) => { expect(url).toBe(`${root}/bundle/remainder`); bodies.push(body); return { bundle_id: 'bundle', revision: '1', state: 'preparing', families: [] } })
  detail.bundle.state = 'cancelled'; detail.bundle.confirmed_at = '2026-09-16T01:00:00Z'; detail.families[0]!.state = 'cancelled'; detail.families[0]!.remainder_eligible = true
  await wrapper.findAll('select').find(s => s.text().includes('Choose a refresh'))!.setValue('bundle'); await flushPromises()
  await click('Prepare exact remainder')
  expect(bodies).toHaveLength(1); expect(bodies[0]).toMatchObject({ expected_revision: initial.bundle.revision, families: ['history'] })
})

it('confirms a ready family while a paused sibling remains unselected', async () => {
  const bodies: unknown[] = []
  const { wrapper, detail } = await setup((url, body) => { expect(url).toBe(`${root}/bundle/confirm`); bodies.push(body); return { bundle_id: 'bundle', revision: '8', state: 'queued', families: [] } })
  detail.bundle.state = 'preparing'
  detail.families.push({ ...structuredClone(detail.families[0]!), family: 'activity', plan_id: 'paused-plan', state: 'paused', digest: null, pause_reason: 'storage_limit' })
  await wrapper.findAll('select').find(s => s.text().includes('Choose a refresh'))!.setValue('bundle'); await flushPromises()
  await wrapper.findAll('label').find(l => l.text().startsWith('I reviewed every selected family'))!.find('input').setValue(true)
  await click('Review confirmation'); await click('Apply reviewed refresh')
  expect(bodies).toHaveLength(1)
  expect(bodies[0]).toMatchObject({ families: [{ family: 'history', plan_id: 'plan' }] })
  expect((bodies[0] as { families: unknown[] }).families).toHaveLength(1)
})
