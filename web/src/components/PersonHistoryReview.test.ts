import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys, useAuthSessionLifetime } from '../api/queries'
import { historyKinds, type HistoryReviewCore, type TimelineEntry } from '../api/historyReview'
import { resetWorkspace } from '../workspaceLifecycle'
import PersonHistoryReview from './PersonHistoryReview.vue'
vi.mock('../api/client', async original => ({ ...await original<typeof import('../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const me = (role = 'admin', actor = 'actor', org = 'org') => ({ user: { id: actor, email: 'test@example.invalid', display_name: actor }, organization: { id: org, name: org, role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false })
function core(revision = '1'): HistoryReviewCore {
  return { person: { id: 'person', first_name: 'Synthetic', last_name: '', display_name: 'Synthetic Person', stage: { id: 'stage', name: 'Lead' }, assigned_user: null, primary_email: null, primary_phone: null, inquiry_count: '1', last_inquiry_at: null, created_at: '2026-09-01T00:00:00Z' }, contact_methods: [], tags: [], custom_fields: [], inquiries: { count: '1', url: 'https://never-fetch.invalid' }, activity: { notes_count: '2', open_tasks_count: '0', completed_tasks_count: '1', activity_revision: '4', notes_url: '/unused', tasks_url: '/unused' }, history: { read_revision: revision, known_count: '9007199254740993', unknown_count: '2', counts: Object.fromEntries(historyKinds.map(kind => [kind, '0'])) as HistoryReviewCore['history']['counts'], timeline_url: 'https://never-fetch.invalid' } }
}
function entry(id = 'first', unknown = false): TimelineEntry {
  return { id, kind: 'fub_text_record_imported', display_at: unknown ? null : '2021-01-01T00:00:00Z', occurred_at: '2026-09-12T00:00:00Z', recorded_at: '2026-09-12T00:00:00.123456Z', actor: { id: 'executor', display_name: 'Import Executor' }, origin: 'migration', detail_url: 'https://never-fetch.invalid/detail', metadata: { source_id: '9007199254740993123', source_person_id: '22', source_attributed_user_id: '9', source_access_user_id: '3', source_creator_user_id: '4', source_editor_user_id: null, source_created: unknown ? null : '2021-01-01T00:00:00Z', source_updated: null, source_sent: '2021-01-02T00:00:00Z', source_status: 'source-only-status', content_availability: 'returned_in_raw', body: 'NEVER DISPLAY BODY', subject: 'NEVER DISPLAY SUBJECT', url: 'https://never-fetch.invalid/private' } }
}
const page = (items: unknown[], next_cursor: string | null = null, read_revision = '1') => ({ items, next_cursor, read_revision })
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { promise, resolve } }
async function setup(handle: (url: string) => unknown, compact = false) {
  api.mockImplementation(async url => url === '/me' ? me() as never : await handle(url) as never)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const refreshCore = vi.fn(async () => {})
  const wrapper = mount(PersonHistoryReview, { props: { personId: 'person', core: core(), refreshCore, compact }, attachTo: document.body, global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] } })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client, refreshCore }
}
function button(label: string) { const value = [...document.querySelectorAll('button')].find(button => button.textContent?.trim() === label); if (!value) throw new Error(`Missing ${label}`); return value }
async function click(label: string) { button(label).click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); document.body.innerHTML = ''; vi.restoreAllMocks(); resetWorkspace() })
describe('bounded historical Person review', () => {
  it('labels source creation separately from import and roles, with metadata-only detail and local routes', async () => {
    const { wrapper } = await setup(url => url.includes('/timeline/fub_') ? { ...entry(), read_revision: '1', correlation_id: 'correlation', provenance: { plan_id: 'plan', attempt_id: 'attempt', manifest_id: 'manifest', identity_id: 'identity', source_time_basis: 'fub_record_created', stable_position: '8' } } : page(url.includes('/timeline?') ? [entry()] : [{ id: 'inquiry', source: 'Native referral', source_external_id: 'source', received_at: '2020-01-01T00:00:00Z', message: 'NEVER INQUIRY BODY' }]))
    expect(wrapper.text()).toContain('9007199254740993 dated facts')
    expect(wrapper.text()).toContain('FUB record created')
    expect(wrapper.text()).toContain('Imported by Import Executor')
    await click('Inspect fact metadata')
    expect(document.body.textContent).toContain('Source access user ID')
    expect(document.body.textContent).toContain('Attributed source user ID')
    expect(document.body.textContent).toContain('FUB sent')
    expect(document.body.textContent).not.toContain('NEVER')
    expect(document.querySelectorAll('img, script, a[href^="http"]')).toHaveLength(0)
    expect(api.mock.calls.every(([url, init]) => url.startsWith('/people/') || url === '/me' && !init?.method)).toBe(true)
    expect(api.mock.calls.some(([url]) => url === '/people/person/migration-review/timeline/fub_text_record_imported/first')).toBe(true)
  })
  it('uses bounded Previous/More and restarts filters without merging pages or folding native calls', async () => {
    const correction: TimelineEntry = { ...entry('correction'), kind: 'contact_attempted', display_at: '2026-09-13T00:00:00Z', occurred_at: '2020-01-01T00:00:00Z', metadata: { channel: 'call', outcome: 'connected', corrects_id: 'prior', superseded: true } }
    const { wrapper } = await setup(url => page(url.includes('/timeline?') ? url.includes('dated=unknown') ? [entry('undated', true)] : url.includes('cursor=next') ? [correction] : [entry()] : [], url.includes('/timeline?') && !url.includes('cursor=next') ? 'next' : null))
    await click('More history'); expect(wrapper.text()).toContain('Corrects a previous contact attempt'); expect(wrapper.text()).not.toContain('FUB text record')
    expect(wrapper.findAll('button').some(item => /outcome|call person/i.test(item.text()))).toBe(false)
    await click('Previous history'); expect(wrapper.text()).toContain('FUB text record')
    await wrapper.findAll('select')[1]!.setValue('unknown'); await flushPromises()
    expect(wrapper.text()).toContain('Date unknown'); expect(button('Previous history').disabled).toBe(true)
    expect(api.mock.calls.filter(([url]) => url.includes('/timeline?')).every(([url]) => url.includes('limit=25'))).toBe(true)
  })
  it('clears pages and open detail on revision changes, requiring explicit Refresh', async () => {
    const { wrapper, refreshCore } = await setup(url => url.includes('/timeline/fub_') ? { ...entry(), read_revision: '1', correlation_id: 'correlation', provenance: null } : page(url.includes('/timeline?') ? [entry()] : [], 'next'))
    await click('Inspect fact metadata')
    await wrapper.setProps({ core: core('2') }); await flushPromises()
    expect(wrapper.text()).toContain('History changed. Refresh')
    expect(wrapper.text()).not.toContain('source-only-status')
    expect(document.querySelector('[role=dialog]')).toBeNull()
    const before = api.mock.calls.length
    expect(button('More history').disabled).toBe(true)
    api.mockImplementation(async url => url === '/me' ? me() as never : page(url.includes('/timeline?') ? [entry('fresh')] : [], null, '2') as never)
    await click('Refresh history'); expect(refreshCore).toHaveBeenCalledOnce(); expect(api.mock.calls.length).toBeGreaterThan(before); expect(wrapper.text()).toContain('FUB text record')
  })
  it('rejects a server revision mismatch and does not silently accept a newer first page', async () => {
    const { wrapper } = await setup(url => { if (url.includes('cursor=next')) throw new ApiError(409, 'history_refresh_required'); return page(url.includes('/timeline?') ? [entry()] : [], 'next') })
    await click('More history'); expect(wrapper.text()).toContain('History changed. Refresh'); expect(wrapper.text()).not.toContain('source-only-status')
  })
  it('rejects newer detail metadata instead of mixing it with the current page revision', async () => {
    const { wrapper } = await setup(url => url.includes('/timeline/fub_') ? { ...entry(), read_revision: '2', correlation_id: 'correlation', provenance: null, metadata: { ...entry().metadata, source_status: 'NEWER PRIVATE METADATA' } } : page(url.includes('/timeline?') ? [entry()] : []))
    await click('Inspect fact metadata')
    expect(wrapper.text()).toContain('History changed. Refresh')
    expect(document.body.textContent).not.toContain('NEWER PRIVATE METADATA')
    expect(document.querySelector('[role=dialog]')).toBeNull()
  })
  it('keeps old pages hidden after a failed core refresh, including filter changes, until a successful retry', async () => {
    const { wrapper, refreshCore } = await setup(url => page(url.includes('/timeline?') ? [entry()] : []))
    refreshCore.mockRejectedValueOnce(new Error('synthetic refresh failure'))
    const reads = api.mock.calls.length
    await click('Refresh history')
    expect(wrapper.text()).toContain('Could not refresh history')
    expect(wrapper.text()).not.toContain('source-only-status')
    await wrapper.findAll('select')[0]!.setValue('calls'); await flushPromises()
    expect(api.mock.calls).toHaveLength(reads)
    expect(button('More history').disabled).toBe(true)
    await click('Refresh history')
    expect(refreshCore).toHaveBeenCalledTimes(2)
    expect(api.mock.calls.length).toBeGreaterThan(reads)
    expect(wrapper.text()).toContain('source-only-status')
    expect(wrapper.text()).not.toContain('Could not refresh history')
  })
  it.each(['person', 'actor', 'org', 'role', 'workspace', 'session'] as const)('discards late detail after %s changes', async transition => {
    const delayed = deferred<unknown>()
    const { wrapper, client } = await setup(url => url.includes('/timeline/fub_') ? delayed.promise : page(url.includes('/timeline?') ? [entry()] : []))
    await click('Inspect fact metadata')
    if (transition === 'person') await wrapper.setProps({ personId: 'another' })
    else if (transition === 'session') useAuthSessionLifetime().value++
    else { const identity = me(transition === 'role' ? 'member' : 'admin', transition === 'actor' ? 'another' : 'actor', transition === 'org' ? 'another' : 'org'); if (transition === 'workspace') identity.organization.workspace_revision = '3'; client.setQueryData(queryKeys.me, identity) }
    delayed.resolve({ ...entry(), metadata: { ...entry().metadata, source_status: 'LATE PRIVATE METADATA' }, read_revision: '1', correlation_id: 'correlation', provenance: null }); await flushPromises()
    expect(document.body.textContent).not.toContain('LATE PRIVATE METADATA'); expect(document.querySelector('[role=dialog]')).toBeNull()
  })
  it('fetches only three history rows and one inquiry for the compact inspector', async () => {
    await setup(url => page(url.includes('/timeline?') ? [entry()] : []), true)
    expect(api.mock.calls.map(([url]) => url)).toEqual(['/me', '/people/person/migration-review/timeline?family=all&dated=known&limit=3', '/people/person/migration-review/inquiries?limit=1'])
    expect(document.querySelectorAll('select')).toHaveLength(0)
  })
})
