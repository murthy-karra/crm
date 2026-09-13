import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { coreChangeDispositions, coreChangeFamilies, type CoreChangeReport, type CoreChangeRow } from '../../api/coreChangeReports'
import { resetWorkspace } from '../../workspaceLifecycle'
import CoreChangeReports from './CoreChangeReports.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const ROOT = '/migrations/fub/core-change-reports'
const cleanup: Array<() => void> = []
const identity = (role = 'admin', actor = 'actor', org = 'org', revision = '2') => ({ user: { id: actor, email: 'synthetic@test.invalid', display_name: 'Admin' }, organization: { id: org, name: 'Org', role, workspace_mode: 'migration_review', workspace_revision: revision }, platform_admin: false })
const original = { id: 'baseline', source_account_id: '9007199254740993123', profile_version: 'fub-core-v1', state: 'completed', started_at: '2026-09-01T00:00:00Z', completed_at: '2026-09-01T02:00:00Z', created_at: '2026-09-01T00:00:00Z' }
const newerCapture = { ...original, id: 'newer', state: 'completed_with_gaps', started_at: '2026-09-02T00:00:00Z', completed_at: '2026-09-02T02:00:00Z' }
function report(state: CoreChangeReport['state'] = 'completed', id = 'report'): CoreChangeReport {
  const boundary = { snapshot_id: 'baseline', capture_sequence: '9007199254740993123', profile_version: 'fub-core-v1', schema_version: 'schema', started_at: original.started_at, completed_at: original.completed_at, streams: [{ stream: 'notes', state: 'completed_with_gaps', reported_total: null, returned_items: '20', content_gaps: '1', accepted_captures: '2' }] }
  return { id, parent_import_id: 'parent', engine_version: 'fub-core-change-v1', state, phase: state === 'completed' ? 'sealed' : 'compare', created_at: '2026-09-03T00:00:00Z', updated_at: '2026-09-03T01:00:00Z', completed_at: state === 'completed' ? '2026-09-03T01:00:00Z' : null, output_revision: state === 'completed' ? 'revision-1' : null, pause_reason: state === 'paused' ? 'storage_budget_exhausted' : null, inputs: { baseline: boundary, newer: { ...boundary, snapshot_id: 'newer', started_at: newerCapture.started_at, completed_at: newerCapture.completed_at }, source_account_id: original.source_account_id, workspace_revision: '2', source_scope: 'unknown_identity', warnings: ['effective_access_not_proven'] }, counts: state === 'completed' ? { families: Object.fromEntries(coreChangeFamilies.map(family => [family, Object.fromEntries(coreChangeDispositions.map(disposition => [disposition, disposition === 'unchanged' ? '9007199254740993123' : '0']))])) as CoreChangeReport['counts'] extends infer C ? NonNullable<C> extends { families: infer F } ? F : never : never, source_ids: '9007199254740993123', invalid_observations: '2', observations: '50', equal_repeats: '4', conflicting_groups: '1' } : null, progress: { captures_processed: '4', observations_processed: '50', groups_compared: '20' }, retained_bytes: '1024', reserved_bytes: '8192', actions: { resume: state === 'paused', cancel: !['completed', 'cancelled'].includes(state) } }
}
function row(id = 'row1', disposition: CoreChangeRow['disposition'] = 'not_seen_again'): CoreChangeRow {
  return { id, family: 'notes', source_id: '900719925474099312345', disposition, categories: ['note_content'], reasons: ['absence_is_not_deletion'], baseline_observations: '9', newer_observations: '0', components: [{ representation: 'fub-core-v1/notes/detail/replies,reactions', disposition: 'unresolved', baseline_observations: '9', newer_observations: '0' }], evidence: [{ snapshot_id: 'baseline', capture_id: 'evidence-capture', ordinal: 0, side: 'baseline', stream: 'note_detail' }], evidence_is_exhaustive: false }
}
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { promise, resolve } }
async function setup(options: { state?: CoreChangeReport['state']; noReports?: boolean; role?: string; realRows?: boolean; handle?: (url: string, init?: RequestInit) => unknown } = {}) {
  const state = { report: report(options.state), identity: identity(options.role), reports: options.noReports ? [] as CoreChangeReport[] : [report(options.state)] }
  const parent = { id: 'parent', state: 'completed', confirmed_plan_id: 'plan', snapshot_id: 'baseline', source_account_id: original.source_account_id, created_at: original.created_at }
  api.mockImplementation(async (url, init) => {
    const value = options.handle?.(url, init); if (value !== undefined) return await value as never
    if (url === '/me') return state.identity as never
    if (url === '/migrations/fub/imports?limit=20') return { imports: [parent], next_cursor: null } as never
    if (url === '/migrations/fub/imports/parent') return parent as never
    if (url === '/migrations/fub/snapshots?limit=20') return { snapshots: [original, newerCapture, { ...newerCapture, id: 'foreign', source_account_id: 'other' }, { ...newerCapture, id: 'overlap', started_at: original.started_at }], next_cursor: null } as never
    if (url === '/migrations/fub/snapshots/baseline') return { snapshot: original, streams: [{ first_observed_at: original.started_at }], coverage: [] } as never
    if (url === '/migrations/fub/snapshots/newer') return { snapshot: newerCapture, streams: [{ first_observed_at: newerCapture.started_at }], coverage: [] } as never
    if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { reports: state.reports, next_cursor: null } as never
    if (url === `${ROOT}/report` && !init?.method) return state.report as never
    if (url.startsWith(`${ROOT}/report/rows?`)) return { rows: [row()], next_cursor: null, output_revision: 'revision-1' } as never
    if (url === `${ROOT}/report/rows/row1`) return { row: row(), output_revision: 'revision-1' } as never
    throw new Error(`Unexpected ${init?.method ?? 'GET'} ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const refreshWorkspace = vi.fn(async () => { client.setQueryData(queryKeys.me, state.identity) })
  const wrapper = mount(CoreChangeReports, { props: { refreshWorkspace }, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { CoreChangeReportRows: !options.realRows } }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises()
  return { wrapper, client, state, refreshWorkspace }
}
function button(label: string): HTMLButtonElement { const value = [...document.body.querySelectorAll('button')].find(element => element.textContent?.trim() === label); expect(value, label).toBeDefined(); return value! }
function select(label: string): HTMLSelectElement { const text = [...document.body.querySelectorAll('label')].find(element => element.textContent?.includes(label)); expect(text, label).toBeDefined(); return document.getElementById(text!.htmlFor) as HTMLSelectElement }
async function choose(label: string, value: string) { const element = select(label); element.value = value; element.dispatchEvent(new Event('change', { bubbles: true })); await flushPromises() }
const writes = () => api.mock.calls.filter(([, init]) => init?.method === 'POST')
beforeEach(() => { api.mockReset(); resetWorkspace(); let next = 0; vi.stubGlobal('crypto', { randomUUID: () => `request-${++next}` }) })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.unstubAllGlobals(); vi.useRealTimers(); resetWorkspace() })
describe('core change report workflow', () => {
  it('selects only newer completed same-account core captures and opens the authorized existing workflow', async () => {
    const { wrapper } = await setup({ noReports: true, handle: (url, init) => url === ROOT && init?.method === 'POST' ? { report_id: 'report', state: 'queued', inputs: report().inputs } : undefined })
    const values = [...select('Newer retained core capture').options].map(value => value.value)
    expect(values).toEqual(['', 'newer', 'overlap'])
    expect(button('Assess changes').disabled).toBe(true)
    await choose('Newer retained core capture', 'newer'); expect(button('Assess changes').disabled).toBe(false)
    button('Assess changes').click(); await flushPromises()
    expect(JSON.parse(String(writes()[0]![1]?.body))).toEqual({ request_id: expect.stringMatching(/^request-\d+$/), parent_import_id: 'parent', newer_snapshot_id: 'newer' })
    expect(wrapper.text()).toContain('Completed change report')
    button('Review original capture').click(); expect(wrapper.emitted('reviewSnapshot')?.at(-1)).toEqual(['baseline'])
    button('Open core capture workflow').click(); expect(wrapper.emitted('recapture')).toHaveLength(1)
  })
  it('qualifies the first source observation instead of the time the run was queued', async () => {
    let observedAt = original.started_at
    await setup({ noReports: true, handle: url => url === '/migrations/fub/snapshots/overlap'
      ? { snapshot: { ...newerCapture, id: 'overlap', started_at: original.started_at }, streams: [{ first_observed_at: observedAt }], coverage: [] } : undefined })
    await choose('Newer retained core capture', 'overlap'); expect(button('Assess changes').disabled).toBe(true)
    observedAt = newerCapture.started_at
    button('Refresh change reports').click(); await flushPromises()
    expect(button('Assess changes').disabled).toBe(false)
  })
  it('recovers a lost response with the original ID and GET state instead of stale receipt state', async () => {
    let attempts = 0
    const { state, wrapper } = await setup({ noReports: true, handle: (url, init) => {
      if (url === ROOT && init?.method === 'POST') { state.reports = [state.report]; return ++attempts === 1 ? Promise.reject(new ApiError(0, 'network_error')) : { report_id: 'report', state: 'queued', inputs: report().inputs } }
    } })
    await choose('Newer retained core capture', 'newer'); button('Assess changes').click(); await flushPromises()
    expect(button('Assess changes').disabled).toBe(true); expect(select('Completed People import').disabled).toBe(true)
    button('Retry the same report request').click(); await flushPromises()
    expect(writes()).toHaveLength(2); expect(writes()[0]![1]?.body).toBe(writes()[1]![1]?.body)
    expect(wrapper.text()).toContain('Completed change report'); expect(wrapper.text()).not.toContain('Queued change report')
  })
  it('discards a late creation receipt after an actor transition and removes its old cache', async () => {
    const pending = deferred<unknown>()
    const { wrapper, client, state } = await setup({ noReports: true, handle: (url, init) => url === ROOT && init?.method === 'POST' ? pending.promise : undefined })
    await choose('Newer retained core capture', 'newer'); button('Assess changes').click(); await flushPromises()
    state.identity = identity('member', 'other', 'foreign'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ report_id: 'old-report-secret', state: 'queued', inputs: report().inputs }); await flushPromises()
    expect(wrapper.text()).not.toContain('old-report-secret'); expect(wrapper.text()).not.toContain('9007199254740993123')
    expect(client.getQueryCache().findAll({ queryKey: ['org', 'org', 'core-change-reports'] })).toHaveLength(0)
    expect(api.mock.calls.some(([url]) => url.endsWith('/old-report-secret'))).toBe(false)
  })
  it('retains an uncertain request across workspace re-verification instead of accepting its late receipt', async () => {
    const pending = deferred<unknown>(); let attempts = 0
    const { wrapper, client, state } = await setup({ noReports: true, handle: (url, init) => url === ROOT && init?.method === 'POST' ? ++attempts === 1 ? pending.promise : { report_id: 'report', state: 'queued', inputs: report().inputs } : undefined })
    await choose('Newer retained core capture', 'newer'); button('Assess changes').click(); await flushPromises()
    state.identity = identity('admin', 'actor', 'org', '3'); client.setQueryData(queryKeys.me, state.identity); await flushPromises()
    pending.resolve({ report_id: 'report', state: 'queued', inputs: report().inputs }); await flushPromises()
    expect(wrapper.text()).toContain('Access changed while the request was pending')
    button('Retry the same report request').click(); await flushPromises()
    expect(writes()[0]![1]?.body).toBe(writes()[1]![1]?.body)
    expect(wrapper.text()).toContain('Completed change report')
  })
  it('only publishes sealed completed rows and requires confirmation for cancellation', async () => {
    const { wrapper, state } = await setup({ state: 'paused', realRows: true, handle: url => {
      if (url.endsWith('/resume')) { state.report = report('running'); return { report_id: 'report', state: 'running', inputs: report().inputs } }
      if (url.endsWith('/cancel')) { state.report = report('cancelled'); return { report_id: 'report', state: 'cancelled', inputs: report().inputs } }
    } })
    expect(wrapper.text()).toContain('Storage allowance reached'); expect(api.mock.calls.some(([url]) => url.includes('/rows'))).toBe(false)
    button('Resume change report').click(); await flushPromises(); expect(wrapper.text()).toContain('Running change report')
    button('Cancel change report').click(); await flushPromises(); expect(writes()).toHaveLength(1)
    button('Permanently cancel report').click(); await flushPromises()
    expect(wrapper.text()).toContain('Cancelled change report'); expect(api.mock.calls.some(([url]) => url.includes('/rows'))).toBe(false)
  })
  it.each(['member', 'inactive'])('makes no retained reads for %s access', async role => { await setup({ role }); expect(api.mock.calls.filter(([url]) => url.startsWith('/migrations'))).toHaveLength(0) })
  it('clears report content when a current request reports permission revocation', async () => {
    let revoked = false
    const { wrapper } = await setup({ realRows: true, handle: url => revoked && url === `${ROOT}/report` ? Promise.reject(new ApiError(403, 'forbidden')) : undefined })
    expect(wrapper.text()).toContain('9007199254740993123'); revoked = true
    button('Refresh change reports').click(); await flushPromises()
    expect(wrapper.text()).not.toContain('9007199254740993123'); expect(wrapper.text()).toContain('currently verified Organization administrator')
  })
  it('reviews safe component metadata and labels absence and non-exhaustive references explicitly', async () => {
    const poisoned = row(); poisoned.categories.push('secret.customer.email'); poisoned.reasons.push('secret.note.body'); poisoned.components.push({ representation: 'secret.source.url', disposition: 'unresolved', baseline_observations: '0', newer_observations: '0' })
    const { wrapper } = await setup({ realRows: true, handle: url => url === `${ROOT}/report/rows/row1` ? { row: poisoned, output_revision: 'revision-1' } : undefined })
    button('Inspect comparison').click(); await flushPromises()
    expect(wrapper.text()).toContain('this does not establish deletion'); expect(wrapper.text()).toContain('These references are not exhaustive')
    expect(wrapper.text()).toContain('Enriched note detail observations'); expect(wrapper.text()).not.toContain('secret.')
    button('Open authorized snapshot review').click(); expect(wrapper.emitted('reviewSnapshot')?.at(-1)).toEqual(['baseline'])
  })
  it('fences delayed detail after changing filters and binds subsequent pages to the filter cursor', async () => {
    const pending = deferred<unknown>()
    const { wrapper } = await setup({ realRows: true, handle: url => {
      if (url === `${ROOT}/report/rows/row1`) return pending.promise
      if (url === `${ROOT}/report/rows?family=tasks&limit=50`) return { rows: [row('task-row', 'changed')], next_cursor: 'opaque +/', output_revision: 'revision-1' }
      if (url === `${ROOT}/report/rows?family=tasks&cursor=opaque+%2B%2F&limit=50`) return { rows: [], next_cursor: null, output_revision: 'revision-1' }
    } })
    button('Inspect comparison').click(); await flushPromises(); await choose('Comparison family', 'tasks')
    const secret = row(); secret.source_id = 'late-source-id'; pending.resolve({ row: secret, output_revision: 'revision-1' }); await flushPromises()
    expect(wrapper.text()).not.toContain('late-source-id'); button('More comparisons').click(); await flushPromises()
    expect(wrapper.text()).toContain('No comparisons match these filters')
    expect(api.mock.calls.some(([url]) => url.includes('family=tasks&cursor=opaque+%2B%2F&limit=50'))).toBe(true)
    button('Previous comparisons').click(); await flushPromises(); expect(wrapper.text()).toContain('Changed')
  })
  it('hides a response whose output revision differs from the selected sealed report', async () => {
    const { wrapper } = await setup({ realRows: true, handle: url => url.includes('/rows?') ? { rows: [{ ...row(), source_id: 'mismatched-output-secret' }], next_cursor: null, output_revision: 'wrong-revision' } : undefined })
    expect(wrapper.text()).toContain('result revision did not match'); expect(wrapper.text()).not.toContain('mismatched-output-secret')
  })
  it('keeps the first saved-report page fixed when navigating back and exposes new reports only on refresh', async () => {
    let firstReads = 0
    const { wrapper } = await setup({ handle: url => {
      if (url === `${ROOT}?parent_import_id=parent&limit=20`) return { reports: [report('completed', ++firstReads === 1 ? 'report' : 'new-report')], next_cursor: 'report-cursor' }
      if (url === `${ROOT}?parent_import_id=parent&cursor=report-cursor&limit=20`) return { reports: [report('cancelled', 'old-report')], next_cursor: null }
    } })
    button('More report pages').click(); await flushPromises(); expect(wrapper.text()).toContain('old-report')
    button('Previous report page').click(); await flushPromises(); expect(firstReads).toBe(1); expect(wrapper.text()).not.toContain('new-report')
    button('Refresh change reports').click(); await flushPromises(); expect(firstReads).toBe(2); expect(wrapper.text()).toContain('new-report')
  })
})
