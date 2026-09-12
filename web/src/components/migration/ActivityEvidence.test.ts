import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { ActivityRecord } from '../../api/activityImports'
import { resetWorkspace } from '../../workspaceLifecycle'
import ActivityRecordPanel from './ActivityRecordPanel.vue'
import ActivityFieldViewer from './ActivityFieldViewer.vue'
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const root = '/migrations/fub/activity-imports/child'
const cleanup: Array<() => void> = []
const identity = { user: { id: 'actor', email: 'a@test.invalid', display_name: 'Admin' }, organization: { id: 'org', role: 'admin', name: 'Org', workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
const html = '<img src="https://untrusted.invalid/beacon"><script>alert(1)</script><p>Source body</p>'
function row(id = 'record'): ActivityRecord { return { id, kind: 'task', source_id: '9007199254740993123456789', person_id: 'person', target_id: null, disposition: 'eligible', source_summary: { fields: [{ key: 'source.body', label: 'body', label_abbreviated: false, label_full_utf8_bytes: '4', text: html, abbreviated: true, full_utf8_bytes: '500000' }], total_fields: '1', abbreviated: true, field_key: 'source.all' }, field_url: 'https://untrusted.invalid/source', observations_url: 'https://untrusted.invalid/observations', preview: { native: { kind: 'task', title: 'Call José\nSource title', source_type: 'Call', source_type_abbreviated: false, source_type_full_utf8_bytes: '4', creator_source_id: '42', assignee_source_id: null, created_at: '2026-09-01T16:00:00.123456Z', updated_at: '2026-09-01T16:00:00.123456Z', due_at: '2026-03-09T06:59:59Z', completed_at: '2026-09-01T16:00:00.123456Z' }, native_kind: 'call', reasons: [], transformations: ['date_only_end_of_day'], source_only: { attachments_source_only: '2' }, source_observations: '3' } } }
function button(label: string): HTMLButtonElement { const el = [...document.body.querySelectorAll('button')].find(b => b.textContent?.trim() === label); expect(el, label).toBeDefined(); return el! }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { resolve, promise } }
async function setup(handle?: (url: string) => unknown, field = false) {
  api.mockImplementation(async url => {
    if (url === '/me') return identity as never
    const custom = handle?.(url); if (custom !== undefined) return await custom as never
    if (url.includes('/records?')) return { items: [row()], next_cursor: null } as never
    if (url.includes('/results?')) return { items: [{ ...row(), preview: undefined, reasons: ['native_target_changed'], disposition: 'held', committed_at: '2026-09-11T12:00:00Z' }], next_cursor: null } as never
    if (url.includes('/observations?')) return { items: [{ id: 'observation', capture_id: 'capture', record_id: 'source', stream: 'note_detail', representation: 'detail', negative: true, source_id: '9007199254740993123456789', field_url: 'https://evil.invalid', raw_url: 'https://evil.invalid/raw' }], next_cursor: null } as never
    if (url.includes('/fields/')) return { text: html, offset: '0', next_offset: '98', total_utf8_bytes: '98', next_cursor: null } as never
    throw new Error(`Unexpected ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const global = { plugins: [[VueQueryPlugin, { queryClient: client }]] as [typeof VueQueryPlugin, { queryClient: QueryClient }][], stubs: { RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } } }
  const wrapper = field ? mount(ActivityFieldViewer, { props: { request: { kind: 'records', importId: 'child', planId: 'plan', rowId: 'record', fieldKey: 'all' }, title: 'Exact source' }, global, attachTo: document.body }) : mount(ActivityRecordPanel, { props: { importId: 'child', planId: 'plan', mode: 'plan', progressVersion: '1', sourceTimezone: 'America/Los_Angeles' }, global, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); client.clear() }); await flushPromises(); return { wrapper, client }
}
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace(); document.body.innerHTML = '' })
describe('activity evidence and page safety', () => {
  it('shows exact native microseconds/source zone and renders source HTML only as text', async () => {
    const { wrapper } = await setup()
    expect(wrapper.text()).toContain('2026-09-01T16:00:00.123456Z'); expect(wrapper.text()).toContain('2026-03-09T06:59:59Z'); expect(wrapper.text()).toContain('America/Los_Angeles')
    expect(wrapper.text()).toContain(html); expect(wrapper.findAll('img,script,iframe')).toHaveLength(0)
    expect(wrapper.text()).toContain('2 source-only components'); expect(wrapper.text()).toContain('3 retained source observations')
    button('Inspect full source and conversion').click(); await flushPromises()
    expect(api.mock.calls.some(([url]) => url === `${root}/records/record/fields/all?plan_id=plan&limit=65536`)).toBe(true)
    expect(wrapper.findAll('a').every(a => !a.attributes('href')?.startsWith('http'))).toBe(true)
  })
  it('inspects linked negative observations and raw source through authenticated typed routes', async () => {
    const { wrapper } = await setup(); button('Review source observations').click(); await flushPromises()
    expect(wrapper.text()).toContain('Negative observation')
    button('Inspect exact capture text').click(); await flushPromises()
    expect(api.mock.calls.some(([url]) => url === `${root}/observations/observation/fields/raw?plan_id=plan&limit=65536`)).toBe(true)
    expect(api.mock.calls.every(([url]) => url.startsWith('/'))).toBe(true); expect(wrapper.findAll('img,script,iframe')).toHaveLength(0)
  })
  it('keeps one UTF-8 segment visible and resets the cursor after a new source owner', async () => {
    const { wrapper } = await setup(url => url.includes('/fields/') ? { text: url.includes('cursor=') ? 'second exact segment' : 'first exact segment 🏡', offset: url.includes('cursor=') ? '9007199254740993' : '0', next_offset: '9007199254740999', total_utf8_bytes: '9007199254741000', next_cursor: url.includes('cursor=') ? null : 'opaque+/' } : undefined, true)
    button('Next segment').click(); await flushPromises()
    expect(wrapper.text()).toContain('second exact segment'); expect(wrapper.text()).not.toContain('first exact segment'); expect(wrapper.text()).toContain('9007199254740993')
    await wrapper.setProps({ request: { kind: 'results', importId: 'child', rowId: 'result', fieldKey: 'all' } } as never); await flushPromises()
    expect(api.mock.lastCall![0]).toBe(`${root}/results/result/fields/all?limit=65536`)
  })
  it('discards a delayed old plan page and starts the new plan at its first page', async () => {
    const late = deferred<{ items: ActivityRecord[]; next_cursor: null }>()
    const { wrapper } = await setup(url => url.includes('/records?') ? url.includes('cursor=') ? late.promise : { items: [row(url.includes('plan2') ? 'new' : 'old')], next_cursor: url.includes('plan2') ? null : 'next' } : undefined)
    button('Next records').click(); await flushPromises()
    await wrapper.setProps({ planId: 'plan2' } as never); await flushPromises(); late.resolve({ items: [row('stale')], next_cursor: null }); await flushPromises()
    button('Inspect full source and conversion').click(); await flushPromises()
    expect(api.mock.calls.some(([url]) => url.includes('/records/new/fields/all?plan_id=plan2'))).toBe(true)
    expect(api.mock.calls.some(([url]) => url.includes('/records/stale/'))).toBe(false)
  })
  it('fences a late source segment when admin authority changes', async () => {
    const late = deferred<{ text: string; offset: string; next_offset: string; total_utf8_bytes: string; next_cursor: null }>()
    const { wrapper, client } = await setup(() => late.promise, true)
    client.setQueryData(queryKeys.me, { ...identity, user: { ...identity.user, id: 'other' }, organization: { ...identity.organization, role: 'member' } }); await flushPromises()
    late.resolve({ text: 'private late source', offset: '0', next_offset: '19', total_utf8_bytes: '19', next_cursor: null }); await flushPromises()
    expect(wrapper.text()).not.toContain('private late source'); expect(wrapper.text()).toContain('current administrator')
  })
  it('sends committed result filters to the result endpoint rather than the planned issue list', async () => {
    const { wrapper } = await setup(); await wrapper.setProps({ mode: 'results' } as never); await flushPromises()
    await wrapper.get('input[placeholder="All issues"]').setValue('native_target_changed'); await wrapper.get('form').trigger('submit'); await flushPromises()
    expect(api.mock.calls.some(([url]) => url === `${root}/results?plan_id=plan&issue=native_target_changed&limit=25`)).toBe(true)
  })
})
