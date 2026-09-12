import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys, useAuthSessionLifetime } from '../api/queries'
import type { ActivityReviewCore, ReviewFullNote, ReviewNote, ReviewTask } from '../api/activityReview'
import type { MeResponse } from '../api/types'
import { resetWorkspace } from '../workspaceLifecycle'
import PersonActivityReview from './PersonActivityReview.vue'
vi.mock('../api/client', async original => ({ ...await original<typeof import('../api/client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
function identity(role: 'admin' | 'member' = 'admin', actor = 'admin', org = 'org'): MeResponse {
  return { user: { id: actor, email: 'synthetic@example.invalid', display_name: actor }, organization: { id: org, name: org, role, workspace_mode: 'migration_review', workspace_revision: '2' }, platform_admin: false }
}
function core(revision = '0', person = 'person'): ActivityReviewCore {
  return {
    person: { id: person, first_name: 'José', last_name: '', display_name: `José ${person}`, stage: { id: 'stage', name: 'Lead' }, assigned_user: null, primary_email: null, primary_phone: null, inquiry_count: 0, last_inquiry_at: null, created_at: '2026-09-01T12:00:00Z' },
    contact_methods: [{ id: 'email', kind: 'email', value: 'plain@example.invalid' }], inquiries: [], tags: [], custom_fields: [], core_history: [],
    activity: { notes_count: '51', open_tasks_count: '2', completed_tasks_count: '1', activity_revision: revision, notes_url: 'https://never-fetch.invalid/notes', tasks_url: 'https://never-fetch.invalid/tasks' },
  }
}
const provenance = { activity_import_id: 'child', result_id: 'result', source_account_id: '101', source_id: '900719925474099312345', source_url: 'https://never-fetch.invalid/source' }
function note(id = 'note'): ReviewNote { return { id, created_at: '2026-09-01T12:00:00.123456Z', updated_at: '2026-09-01T12:00:00.123456Z', author: null, can_manage: false, excerpt: `Excerpt ${id} <img src="https://never-fetch.invalid">`, has_more: true, provenance } }
function full(id = 'note', body = '🏡'.repeat(10000)): ReviewFullNote {
  const row = note(id)
  return { id: row.id, created_at: row.created_at, updated_at: row.updated_at, author: row.author, can_manage: false, provenance, body }
}
const task: ReviewTask = { id: 'task', title: 'Exact <script>task</script>', kind: 'call', due_at: null, completed_at: null, created_at: '2026-09-01T12:00:00Z', updated_at: '2026-09-01T12:00:00Z', assignee: null, created_by: { id: 'former', display_name: 'Former Agent' }, completed_by: null, can_manage: false, provenance }
function page<T>(items: T[], revision = '0', next: string | null = null) { return { items, next_cursor: next, activity_revision: revision } }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes }); return { promise, resolve } }
async function setup(handle: (url: string) => unknown, me = identity()) {
  api.mockImplementation(async url => url === '/me' ? me as never : await handle(url) as never)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }] })
  await router.push('/people/person')
  const wrapper = mount(PersonActivityReview, { props: { personId: 'person' }, attachTo: document.body, global: { plugins: [router, [VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]], stubs: { PersonImportProvenance: true, PersonMetadataProvenance: true } } })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises(); return { wrapper, client }
}
function button(text: string): HTMLButtonElement { const result = [...document.querySelectorAll('button')].find(b => b.textContent?.trim() === text); if (!result) throw new Error(`Missing button ${text}`); return result }
async function click(text: string) { button(text).click(); await flushPromises() }
beforeEach(() => { api.mockReset(); resetWorkspace() })
afterEach(() => { cleanup.splice(0).forEach(fn => fn()); resetWorkspace(); document.body.innerHTML = ''; vi.restoreAllMocks() })
describe('bounded Person activity review', () => {
  it('reviews a valid pre-child/partial-parent core without requiring a completed activity import', async () => {
    const { wrapper } = await setup(url => url.endsWith('/migration-review') ? core() : page([]))
    expect(wrapper.text()).toContain('José person')
    expect(wrapper.text()).toContain('Notes (51)')
    expect(wrapper.text()).toContain('No notes on this page.')
    expect(api.mock.calls.map(([url]) => url)).toEqual(['/me', '/people/person/migration-review', '/people/person/migration-review/notes?limit=50', '/people/person/migration-review/tasks?limit=50&state=open', '/people/person/migration-review/tasks?limit=50&state=completed'])
    expect(wrapper.find('a[href^="mailto:"]').exists()).toBe(false)
    expect(wrapper.findAll('button').some(b => /edit|add task|call person|operator/i.test(b.text()))).toBe(false)
  })
  it('traverses bounded pages, reads the complete Unicode note, and inspects exact retained source without interpreting URLs or HTML', async () => {
    const { wrapper } = await setup(url => {
      if (url.endsWith('/migration-review')) return core()
      if (url.includes('/fields/all')) return { text: '<script>retained source</script>', offset: '0', next_offset: null, total_utf8_bytes: '32', next_cursor: null }
      if (url.endsWith('/notes/n0')) return full('n0')
      if (url.includes('/notes?')) return url.includes('cursor=') ? page([note('last')]) : page(Array.from({ length: 50 }, (_, i) => note(`n${i}`)), '0', 'notes-next')
      if (url.includes('state=completed')) return page([{ ...task, id: 'done', completed_at: '2026-09-02T16:00:00Z' }])
      return url.includes('cursor=') ? page([{ ...task, id: 'last-task', title: 'Final open task' }]) : page([task], '0', 'open-next')
    })
    expect(wrapper.findAll('img, script')).toHaveLength(0)
    expect(wrapper.text()).toContain('CRM creator: Former Agent · Assignee: Unassigned')
    expect(wrapper.text()).toContain('CRM completer: Not recorded')
    await click('Read full note')
    const body = document.querySelector('[aria-label="Full native note body"]')!
    expect(body.textContent).toBe('🏡'.repeat(10000))
    expect([...body.textContent!]).toHaveLength(10000)
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('Full imported note')
    await click('Inspect original note source')
    expect(document.body.textContent).toContain('900719925474099312345')
    expect(document.body.textContent).toContain('<script>retained source</script>')
    expect(document.querySelectorAll('script, img')).toHaveLength(0)
    expect(api.mock.calls.some(([url]) => url === '/migrations/fub/activity-imports/child/results/result/fields/all?limit=65536')).toBe(true)
    await click('Close source'); await click('Next notes page')
    expect(wrapper.text()).not.toContain('Excerpt n0')
    expect(wrapper.text()).toContain('Excerpt last')
    await click('Next open tasks page')
    expect(wrapper.text()).toContain('Final open task')
    expect(api.mock.calls.every(([url, init]) => url.startsWith('/') && !init?.method)).toBe(true)
  })
  it('clears every page and full body on stale-cursor 409 before refetching core and page one', async () => {
    const fresh = deferred<ActivityReviewCore>(); let coreReads = 0
    const { wrapper } = await setup(url => {
      if (url.endsWith('/migration-review')) return ++coreReads === 1 ? core('1') : fresh.promise
      if (url.endsWith('/notes/note')) return full('note', 'PRIVATE OLD FULL BODY')
      if (url.includes('cursor=old')) throw new ApiError(409, 'activity_refresh_required')
      return page(url.includes('/notes?') ? [note(coreReads === 1 ? 'note' : 'fresh')] : [], coreReads === 1 ? '1' : '2', url.includes('/notes?') && coreReads === 1 ? 'old' : null)
    })
    await click('Read full note'); expect(document.body.textContent).toContain('PRIVATE OLD FULL BODY')
    await click('Next notes page')
    expect(wrapper.text()).not.toContain('Excerpt note')
    expect(document.body.textContent).not.toContain('PRIVATE OLD FULL BODY')
    fresh.resolve(core('2')); await flushPromises()
    expect(wrapper.text()).toContain('Excerpt fresh')
    expect(api.mock.calls.filter(([url]) => url === '/people/person/migration-review/notes?limit=50')).toHaveLength(2)
  })
  it.each(['person', 'actor', 'org', 'role', 'workspace', 'session'] as const)('rejects a late full body after %s changes', async transition => {
    const delayed = deferred<ReviewFullNote>()
    const { wrapper, client } = await setup(url => url.endsWith('/migration-review') ? core('1', url.includes('/another/') ? 'another' : 'person') : url.endsWith('/notes/note') ? delayed.promise : page(url.includes('/notes?') ? [note()] : [], '1'))
    await click('Read full note')
    if (transition === 'person') await wrapper.setProps({ personId: 'another' })
    else if (transition === 'session') useAuthSessionLifetime().value++
    else { const next = identity(transition === 'role' ? 'member' : 'admin', transition === 'actor' ? 'new-actor' : 'admin', transition === 'org' ? 'new-org' : 'org'); if (transition === 'workspace') next.organization!.workspace_revision = '3'; client.setQueryData(queryKeys.me, next) }
    await flushPromises(); delayed.resolve(full('note', 'LATE PRIVATE BODY')); await flushPromises()
    expect(document.body.textContent).not.toContain('LATE PRIVATE BODY')
    expect(document.querySelector('[aria-label="Full native note body"]')).toBeNull()
    if (transition === 'role') { expect(wrapper.text()).toContain('requires a current'); expect(wrapper.text()).not.toContain('Excerpt note') }
  })
  it('does not fetch core, pages or full notes for a member', async () => {
    const { wrapper } = await setup(() => { throw new Error('Unexpected private read') }, identity('member'))
    expect(api.mock.calls.map(([url]) => url)).toEqual(['/me'])
    expect(wrapper.text()).toContain('requires a current')
  })
  it('refreshes core on focus and reconnect and restarts pages when its revision changes', async () => {
    let revision = '1'
    const { wrapper } = await setup(url => url.endsWith('/migration-review') ? core(revision) : page(url.includes('/notes?') ? [note(`revision-${revision}`)] : [], revision))
    expect(wrapper.text()).toContain('Excerpt revision-1')
    revision = '2'; window.dispatchEvent(new Event('focus')); await flushPromises()
    expect(wrapper.text()).toContain('Excerpt revision-2')
    expect(wrapper.text()).not.toContain('Excerpt revision-1')
    revision = '3'; window.dispatchEvent(new Event('online')); await flushPromises()
    expect(wrapper.text()).toContain('Excerpt revision-3')
    expect(api.mock.calls.filter(([url]) => url.endsWith('/migration-review'))).toHaveLength(3)
    expect(api.mock.calls.filter(([url]) => url.includes('/notes?'))).toHaveLength(3)
  })
  it('rejects an earlier response after closing and reopening the same full note', async () => {
    const old = deferred<ReviewFullNote>(); let noteReads = 0
    await setup(url => url.endsWith('/migration-review') ? core() : url.endsWith('/notes/note') ? ++noteReads === 1 ? old.promise : full('note', 'CURRENT FULL BODY') : page(url.includes('/notes?') ? [note()] : []))
    await click('Read full note'); await click('Close note'); await click('Read full note')
    expect(document.body.textContent).toContain('CURRENT FULL BODY')
    old.resolve(full('note', 'OLD CLOSED BODY')); await flushPromises()
    expect(document.body.textContent).not.toContain('OLD CLOSED BODY')
    expect(document.body.textContent).toContain('CURRENT FULL BODY')
  })
})
