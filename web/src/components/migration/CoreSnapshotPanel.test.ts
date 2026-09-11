import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { FubConnection } from '../../api/migrations'
import type { CoreSnapshot, SnapshotDetail, SnapshotList, SnapshotProposal, SnapshotState } from '../../api/snapshots'
import type { MeResponse } from '../../api/types'
import ConfirmDialog from '../ConfirmDialog.vue'
import CoreSnapshotPanel from './CoreSnapshotPanel.vue'
import SnapshotPreviewPanel from './SnapshotPreviewPanel.vue'

vi.mock('../../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/snapshots'
const ORG = '11111111-1111-1111-1111-111111111111'
const RUN = '22222222-2222-2222-2222-222222222222'
const CONNECTION = '33333333-3333-3333-3333-333333333333'
const PREVIEW = '44444444-4444-4444-4444-444444444444'
const REQUEST = '55555555-5555-5555-5555-555555555555'
const cleanup: Array<() => void> = []
type Handler = (path: string, init?: RequestInit) => unknown | Promise<unknown>
function me(org = ORG, actor = 'u-alice', role: 'admin' | 'member' = 'admin'): MeResponse {
  return { user: { id: actor, email: 'alice@example.test', display_name: 'Alice' }, organization: { id: org, name: 'Example Realty', role }, platform_admin: false }
}
function connection(): FubConnection {
  return { id: CONNECTION, revision: 7, source_account_id: 42, source_display_name: 'synthetic.fub.example', source_access_scope: 'unknown', status: 'connected', created_at: '2026-09-10T12:00:00Z', updated_at: '2026-09-10T12:00:00Z' }
}
function snapshot(state: SnapshotState = 'completed', overrides: Partial<CoreSnapshot> = {}): CoreSnapshot {
  return {
    id: RUN, connection_id: CONNECTION, connection_revision: 7, source_account_id: '9007199254740993',
    profile_version: 'fub-core-v1', state, pause_reason: state === 'paused' ? 'source_unavailable' : null,
    created_at: '2026-09-10T12:00:00Z', started_at: '2026-09-10T12:00:01Z', completed_at: state === 'completed' ? '2026-09-10T12:01:00Z' : null,
    proposal_expires_at: '2026-09-10T12:15:00Z', raw_bytes: '1024', retained_bytes: '4096', reserved_bytes: '0', accepted_captures: '8', capture_sequence: '8',
    original_run_byte_limit: '2147483648', run_byte_limit: '2147483648', org_byte_limit: '4294967296', org_retained_bytes: '4096', org_reserved_bytes: '0',
    run_budget_revision: '1', org_budget_revision: '1', policy_revision: 'v1-2147483648-4294967296', run_budget_policy_revision: 'v1-2147483648-4294967296', org_budget_policy_revision: 'v1-2147483648-4294967296',
    run_ceiling_bytes: '2147483648', org_ceiling_bytes: '4294967296', required_reservation_bytes: '67108864', actions: ['generate_preview'], preview_ids: [], ...overrides,
  }
}
function listing(snapshots: CoreSnapshot[]): SnapshotList { return { snapshots, next_cursor: null, active_snapshot_id: null, latest_completed_snapshot_id: snapshots[0]?.id ?? null } }
function detail(s: CoreSnapshot): SnapshotDetail { return { snapshot: s, streams: [], coverage: [] } }
function proposal(s: CoreSnapshot): SnapshotProposal { return { snapshot: s, proposal: { families: ['people', 'users', 'stages', 'custom_fields', 'notes', 'tasks'], profile_version: s.profile_version, expires_at: s.proposal_expires_at, run_byte_limit: s.run_byte_limit, org_byte_limit: s.org_byte_limit, source_scope: 'current credential' } } }
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(yes => { resolve = yes })
  return { promise, resolve }
}
function writes(suffix?: string) { return apiFetchMock.mock.calls.filter(([path, init]) => init?.method === 'POST' && (!suffix || path.endsWith(suffix))) }
function reads(path: string) { return apiFetchMock.mock.calls.filter(([url, init]) => url === path && !init?.method).length }
function dialogButton(label: string) {
  const button = [...document.body.querySelectorAll('button')].find(b => b.textContent?.trim() === label)
  expect(button, `dialog button ${label}`).toBeDefined()
  return button!
}
async function mountPanel(options: { snapshot?: CoreSnapshot | null; identity?: MeResponse; connected?: boolean; assessmentBusy?: boolean; handle?: Handler } = {}) {
  const state = { snapshot: options.snapshot === undefined ? snapshot() : options.snapshot, identity: options.identity ?? me() }
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const intercepted = options.handle?.(path, init)
    if (intercepted !== undefined) return await intercepted as never
    if (path === '/me') return state.identity as never
    if (path === `${ROOT}?limit=20`) return listing(state.snapshot ? [state.snapshot] : []) as never
    if (path === `${ROOT}/${state.snapshot?.id}` && !init?.method) return detail(state.snapshot!) as never
    throw new Error(`unexpected ${init?.method ?? 'GET'} ${path}`)
  })
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = mount(CoreSnapshotPanel, { props: { connection: options.connected === false ? null : connection(), assessmentBusy: options.assessmentBusy ?? false }, global: { plugins: [[VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]], stubs: { SnapshotBudgetPanel: true, SnapshotPreviewPanel: true } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); queryClient.clear() })
  await flushPromises()
  return { wrapper, queryClient, state }
}
beforeEach(() => { apiFetchMock.mockReset(); vi.stubGlobal('crypto', { randomUUID: () => REQUEST }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers() })

describe('CoreSnapshotPanel', () => {
  it('prepares a bounded proposal and waits for an explicit capture confirmation', async () => {
    const proposed = snapshot('proposed', { actions: ['confirm', 'cancel'], started_at: null })
    const { wrapper, state } = await mountPanel({ snapshot: null, handle: (path, init) => {
      if (path === ROOT && init?.method === 'POST') return proposal(proposed)
      if (path === `${ROOT}/${RUN}`) return detail(proposed)
      if (path === `${ROOT}/${RUN}/confirm`) { state.snapshot = snapshot('queued', { actions: ['cancel'] }); return { snapshot: state.snapshot } }
    } })
    await wrapper.get('[data-testid="snapshot-prepare"]').trigger('click')
    await flushPromises()
    expect(writes()).toHaveLength(1)
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: REQUEST, connection_id: CONNECTION, expected_revision: 7 })
    expect(writes('/confirm')).toHaveLength(0)
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(true)
    expect(document.body.textContent).toContain('9007199254740993')
    expect(document.body.textContent).toContain('does not import or change CRM records')
    dialogButton('Start source capture').click()
    await flushPromises()
    expect(writes('/confirm')).toHaveLength(1)
    expect(JSON.parse(String(writes('/confirm')[0]![1]?.body))).toEqual({ request_id: REQUEST })
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(false)
  })
  it('reopens a retained proposal for review without confirming it during load', async () => {
    const { wrapper } = await mountPanel({ snapshot: snapshot('proposed', { actions: ['confirm', 'cancel'] }) })
    expect(writes()).toHaveLength(0)
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(false)
    await wrapper.get('[data-testid="snapshot-review-confirm"]').trigger('click')
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(true)
    dialogButton('Cancel').click()
    await flushPromises()
    expect(writes()).toHaveLength(0)
  })
  it.each(['expired', 'completed'] as const)('does not invent unsupported actions for a %s run', async (state) => {
    const { wrapper } = await mountPanel({ snapshot: snapshot(state, { actions: ['unrecognized_future_action'] }) })
    expect(wrapper.get('[data-testid="snapshot-state"]').text().toLowerCase()).toContain(state)
    for (const action of ['review-confirm', 'retry', 'cancel', 'generate-preview']) expect(wrapper.find(`[data-testid="snapshot-${action}"]`).exists()).toBe(false)
    expect(writes()).toHaveLength(0)
  })
  it('disables preparation while a 010a assessment is busy', async () => {
    const { wrapper } = await mountPanel({ assessmentBusy: true })
    expect(wrapper.get('[data-testid="snapshot-prepare"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-testid="snapshot-prepare"]').trigger('click')
    expect(writes()).toHaveLength(0)
    await wrapper.setProps({ assessmentBusy: false })
    expect(wrapper.get('[data-testid="snapshot-prepare"]').attributes('disabled')).toBeUndefined()
  })
  it('generates a retained preview after disconnect without resuming source capture', async () => {
    const { wrapper } = await mountPanel({ snapshot: snapshot('paused', { actions: ['generate_preview'] }), connected: false, handle: path => path.endsWith('/previews') ? { preview_id: PREVIEW, state: 'queued' } : undefined })
    expect(wrapper.get('[data-testid="snapshot-prepare"]').attributes('disabled')).toBeDefined()
    expect(wrapper.find('[data-testid="snapshot-retry"]').exists()).toBe(false)
    expect(wrapper.text()).toContain('Retained evidence is partial')
    await wrapper.get('[data-testid="snapshot-generate-preview"]').trigger('click')
    await flushPromises()
    expect(writes().map(([path]) => path)).toEqual([`${ROOT}/${RUN}/previews`])
    expect(wrapper.getComponent(SnapshotPreviewPanel).props('previewId')).toBe(PREVIEW)
    expect(wrapper.getComponent(SnapshotPreviewPanel).props('snapshotId')).toBe(RUN)
  })
  it('resumes source capture only when its separate retry action is clicked', async () => {
    const { wrapper, state } = await mountPanel({ snapshot: snapshot('paused', { actions: ['retry', 'generate_preview'] }), handle: path => {
      if (path.endsWith('/retry')) { state.snapshot = snapshot('queued', { actions: ['cancel'] }); return { snapshot: state.snapshot } }
    } })
    await wrapper.get('[data-testid="snapshot-retry"]').trigger('click')
    await flushPromises()
    expect(writes().map(([path]) => path)).toEqual([`${ROOT}/${RUN}/retry`])
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Queued')
    expect(wrapper.emitted('sourceBusy')?.at(-1)).toEqual([true])
    expect(wrapper.get('[data-testid="snapshot-prepare"]').attributes('disabled')).toBeDefined()
  })
  it('cancels source capture through its own command and reloads retained state', async () => {
    const { wrapper, state } = await mountPanel({ snapshot: snapshot('running', { actions: ['cancel'] }), handle: path => {
      if (path.endsWith('/cancel')) { state.snapshot = snapshot('cancelled'); return { snapshot: state.snapshot } }
    } })
    await wrapper.get('[data-testid="snapshot-cancel"]').trigger('click')
    await flushPromises()
    expect(writes().map(([path]) => path)).toEqual([`${ROOT}/${RUN}/cancel`])
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({})
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Cancelled')
    expect(wrapper.emitted('sourceBusy')?.at(-1)).toEqual([false])
  })
  it('blocks repeated pending actions and restores durable state after a reload', async () => {
    const pending = deferred<{ preview_id: string; state: 'queued' }>()
    const first = await mountPanel({ snapshot: snapshot('paused', { actions: ['generate_preview', 'retry'] }), handle: path => path.endsWith('/previews') ? pending.promise : undefined })
    await first.wrapper.get('[data-testid="snapshot-generate-preview"]').trigger('click')
    expect(first.wrapper.get('[data-testid="snapshot-generate-preview"]').attributes('disabled')).toBeDefined()
    expect(first.wrapper.get('[data-testid="snapshot-retry"]').attributes('disabled')).toBeDefined()
    await first.wrapper.get('[data-testid="snapshot-generate-preview"]').trigger('click')
    expect(writes('/previews')).toHaveLength(1)
    first.wrapper.unmount()
    const next = await mountPanel({ snapshot: snapshot('paused', { preview_ids: [PREVIEW] }) }); const before = reads(`${ROOT}?limit=20`)
    pending.resolve({ preview_id: PREVIEW, state: 'queued' })
    await flushPromises()
    expect(reads(`${ROOT}?limit=20`)).toBe(before)
    expect(next.wrapper.getComponent(SnapshotPreviewPanel).props('previewId')).toBe(PREVIEW)
    expect(next.wrapper.get('[data-testid="snapshot-generate-preview"]').attributes('disabled')).toBeUndefined()
  })
  it('retains the same idempotency key across an uncertain confirmation retry', async () => {
    const newId = vi.fn().mockReturnValueOnce(REQUEST).mockReturnValue('66666666-6666-6666-6666-666666666666')
    let attempts = 0
    const { wrapper } = await mountPanel({ snapshot: snapshot('proposed', { actions: ['confirm'] }), handle: path => {
      if (path.endsWith('/confirm')) { attempts++; return attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { snapshot: snapshot('queued') } }
    } })
    await wrapper.get('[data-testid="snapshot-review-confirm"]').trigger('click')
    vi.stubGlobal('crypto', { randomUUID: newId })
    dialogButton('Start source capture').click()
    await flushPromises()
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(true)
    expect(wrapper.getComponent(ConfirmDialog).props('isPending')).toBe(false)
    dialogButton('Start source capture').click()
    await flushPromises()
    expect(writes('/confirm').map(([, init]) => JSON.parse(String(init?.body)))).toEqual([{ request_id: REQUEST }, { request_id: REQUEST }])
    expect(newId).toHaveBeenCalledTimes(1)
  })
  it('closes stale confirmation on conflict and refreshes the authoritative status', async () => {
    const { wrapper, state } = await mountPanel({ snapshot: snapshot('proposed', { actions: ['confirm'] }), handle: path => {
      if (path.endsWith('/confirm')) { state.snapshot = snapshot('expired', { actions: [] }); return Promise.reject(new ApiError(409, 'conflict')) }
    } })
    await wrapper.get('[data-testid="snapshot-review-confirm"]').trigger('click')
    dialogButton('Start source capture').click()
    await flushPromises()
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(false)
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Expired')
    expect(wrapper.text()).toContain('Refresh the status before retrying')
    expect(writes('/confirm')).toHaveLength(1)
  })
  it('uses separate Organization and actor cache keys and clears old data on identity changes', async () => {
    const { wrapper, queryClient, state } = await mountPanel({ snapshot: snapshot('completed', { preview_ids: [PREVIEW] }) })
    const prefix = queryKeys.snapshots(ORG, 'u-alice', 0)
    expect(queryClient.getQueryData([...prefix, 'run', RUN])).toEqual(detail(state.snapshot!))
    expect(queryClient.getQueryData([...queryKeys.snapshots('other-org', 'u-alice', 0), 'run', RUN])).toBeUndefined()
    state.snapshot = null; state.identity = me('other-org', 'u-bob'); queryClient.setQueryData(queryKeys.me, state.identity)
    await flushPromises()
    expect(queryClient.getQueriesData({ queryKey: prefix })).toHaveLength(0)
    expect(wrapper.find('snapshot-preview-panel-stub').exists()).toBe(false)
    expect(wrapper.find('[data-testid="snapshot-state"]').exists()).toBe(false)
    expect(queryClient.getQueryData([...queryKeys.snapshots('other-org', 'u-bob', 0), 'list', ''])).toEqual(listing([]))
  })
  it.each(['organization', 'actor'] as const)('ignores late proposal completion after the %s changes', async (changed) => {
    const pending = deferred<SnapshotProposal>()
    const { wrapper, queryClient, state } = await mountPanel({ snapshot: null, handle: (path, init) => path === ROOT && init?.method === 'POST' ? pending.promise : undefined })
    await wrapper.get('[data-testid="snapshot-prepare"]').trigger('click')
    state.identity = changed === 'organization' ? me('other-org') : me(ORG, 'u-bob'); queryClient.setQueryData(queryKeys.me, state.identity)
    await flushPromises()
    const before = reads(`${ROOT}?limit=20`); pending.resolve(proposal(snapshot('proposed', { actions: ['confirm'] })))
    await flushPromises()
    expect(reads(`${ROOT}?limit=20`)).toBe(before)
    expect(wrapper.getComponent(ConfirmDialog).props('visible')).toBe(false)
    expect(wrapper.find('[data-testid="snapshot-state"]').exists()).toBe(false)
    expect(writes('/confirm')).toHaveLength(0)
    expect(queryClient.getQueriesData({ queryKey: queryKeys.snapshots(ORG, 'u-alice', 0) })).toHaveLength(0)
  })
  it.each(['organization', 'actor'] as const)('discards a late private detail read after the %s changes', async (changed) => {
    const pending = deferred<SnapshotDetail>()
    const { wrapper, queryClient, state } = await mountPanel({ handle: path => path === `${ROOT}/${RUN}` ? pending.promise : undefined })
    expect(wrapper.text()).toContain('Loading snapshot')
    state.snapshot = null
    state.identity = changed === 'organization' ? me('other-org') : me(ORG, 'u-bob')
    queryClient.setQueryData(queryKeys.me, state.identity)
    await flushPromises()
    pending.resolve(detail(snapshot('completed', { preview_ids: [PREVIEW] })))
    await flushPromises()
    expect(wrapper.find('[data-testid="snapshot-state"]').exists()).toBe(false)
    expect(wrapper.find('snapshot-preview-panel-stub').exists()).toBe(false)
    expect(queryClient.getQueriesData({ queryKey: queryKeys.snapshots(ORG, 'u-alice', 0) })).toHaveLength(0)
    expect(writes()).toHaveLength(0)
  })
  it('does not fetch private snapshot data for an Organization member', async () => {
    const { wrapper } = await mountPanel({ identity: me(ORG, 'u-alice', 'member') })
    expect(apiFetchMock.mock.calls.filter(([path]) => path.startsWith(ROOT))).toHaveLength(0)
    expect(wrapper.text()).toContain('current Organization administrator session is required')
    expect(wrapper.find('[data-testid="snapshot-prepare"]').exists()).toBe(false)
  })
  it('clears retained data and actions after a forbidden read', async () => {
    let forbidden = false
    const { wrapper, queryClient } = await mountPanel({ snapshot: snapshot('completed', { preview_ids: [PREVIEW] }), handle: path => forbidden && path === `${ROOT}/${RUN}` ? Promise.reject(new ApiError(403, 'forbidden')) : undefined })
    forbidden = true
    await wrapper.get('[data-testid="snapshot-refresh"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="snapshot-state"]').exists()).toBe(false)
    expect(wrapper.find('snapshot-preview-panel-stub').exists()).toBe(false)
    expect(wrapper.find('[data-testid="snapshot-generate-preview"]').exists()).toBe(false)
    expect(queryClient.getQueriesData({ queryKey: queryKeys.snapshots(ORG, 'u-alice', 0) }).every(([, value]) => value === undefined)).toBe(true)
  })
  it.each(['list', 'detail'] as const)('recovers a failed %s read through explicit refresh without a source write', async (failed) => {
    let failing = true; const path = failed === 'list' ? `${ROOT}?limit=20` : `${ROOT}/${RUN}`
    const { wrapper } = await mountPanel({ handle: url => failing && url === path ? Promise.reject(new ApiError(503, 'unavailable')) : undefined })
    expect(wrapper.find('[role="alert"]').exists()).toBe(true); failing = false
    await wrapper.get('[data-testid="snapshot-refresh"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Completed')
    expect(writes()).toHaveLength(0)
  })
  it.each(['paused', 'completed', 'completed_with_gaps', 'cancelled', 'expired'] as const)('stops polling a %s snapshot', async (state) => {
    vi.useFakeTimers()
    await mountPanel({ snapshot: snapshot(state) }); const before = apiFetchMock.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000)
    expect(apiFetchMock.mock.calls).toHaveLength(before)
  })
  it('stops an existing polling loop once the source reaches a terminal state', async () => {
    vi.useFakeTimers()
    const { wrapper, state } = await mountPanel({ snapshot: snapshot('running') })
    state.snapshot = snapshot('completed')
    await vi.advanceTimersByTimeAsync(2_000)
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Completed')
    const before = apiFetchMock.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000)
    expect(apiFetchMock.mock.calls).toHaveLength(before)
  })
  it('increases repeated failure delays to a bounded thirty seconds and resets after recovery', async () => {
    vi.useFakeTimers()
    let failing = false
    await mountPanel({ snapshot: snapshot('running'), handle: path => failing && path.startsWith(ROOT) ? Promise.reject(new ApiError(503, 'unavailable')) : undefined })
    let count = reads(`${ROOT}?limit=20`)
    failing = true
    for (const delay of [2_000, 4_000, 8_000, 16_000, 30_000, 30_000]) {
      await vi.advanceTimersByTimeAsync(delay - 1)
      expect(reads(`${ROOT}?limit=20`)).toBe(count)
      await vi.advanceTimersByTimeAsync(1)
      expect(reads(`${ROOT}?limit=20`)).toBe(++count)
    }
    failing = false
    await vi.advanceTimersByTimeAsync(30_000)
    expect(reads(`${ROOT}?limit=20`)).toBe(++count)
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads(`${ROOT}?limit=20`)).toBe(count + 1)
  })
  it('polls active source work and backs off failed reads before recovering', async () => {
    vi.useFakeTimers(); let failing = false
    const { wrapper } = await mountPanel({ snapshot: snapshot('running', { actions: ['cancel'] }), handle: path => failing && path.startsWith(ROOT) ? Promise.reject(new ApiError(503, 'unavailable')) : undefined })
    const before = reads(`${ROOT}?limit=20`); failing = true
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads(`${ROOT}?limit=20`)).toBe(before + 1)
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads(`${ROOT}?limit=20`)).toBe(before + 1)
    failing = false
    await vi.advanceTimersByTimeAsync(2_000)
    expect(reads(`${ROOT}?limit=20`)).toBe(before + 2)
    expect(wrapper.get('[data-testid="snapshot-state"]').text()).toContain('Running')
    expect(writes()).toHaveLength(0)
  })
})
