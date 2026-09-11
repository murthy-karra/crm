import { VueQueryPlugin } from '@tanstack/vue-query'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { computed, defineComponent, h, ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.vue'
import { queryKeys, useMe, usePeople } from './api/queries'
import type { MeResponse, PeopleResponse } from './api/types'
import { queryClient } from './query-client'
import { refreshWorkspace, resetWorkspace, useWorkspacePending, WorkspaceVerificationPendingError } from './workspaceLifecycle'
import { apiFetch } from './api/client'
import { beginSessionTransition, currentSessionGeneration, isSessionVerified, resetSessionCoordination, settleSessionTransition, useAuthSessionLifetime } from './sessionLifecycle'

// Exercise the real App, shell, transport and query lifecycle without opening
// a realtime connection. The ordinary route uses the production People hook.
vi.mock('./realtime/useRealtime', () => ({ useRealtime: () => ({ status: ref('idle') }) }))

const original = (): MeResponse => ({
  user: { id: 'actor', email: 'actor@example.test', display_name: 'Actor' },
  organization: { id: 'org', name: 'Organization', role: 'admin', workspace_mode: 'operational', workspace_revision: '1' },
  platform_admin: false,
})
const reviewed = (): MeResponse => ({ ...original(), organization: { ...original().organization!, workspace_mode: 'migration_review', workspace_revision: '2' } })
function people(name: string): PeopleResponse { return { people: [{ id: 'person', first_name: name, last_name: null, display_name: name, stage: { id: 'stage', name: 'Lead' }, assigned_user: null, primary_email: null, primary_phone: null, inquiry_count: 0, last_inquiry_at: null, created_at: '2026-09-11T12:00:00Z' }], truncated: false } }
function response(value: unknown): Response { return new Response(JSON.stringify(value), { status: 200, headers: { 'Content-Type': 'application/json' } }) }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done }); return { promise, resolve } }

let wrapper: VueWrapper | undefined
beforeEach(() => { vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] }); queryClient.clear(); resetWorkspace() })
afterEach(() => { wrapper?.unmount(); wrapper = undefined; queryClient.clear(); resetWorkspace(); vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); document.body.innerHTML = '' })

async function mountOrdinary(verify: () => Promise<Response>, migration = false, lateRead?: Promise<Response>) {
  let setups = 0
  let reads = 0
  const transport = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async (input) => {
    const path = String(input)
    if (path.endsWith('/me')) return verify()
    if (path.endsWith('/people/person') && lateRead) return lateRead
    if (path.endsWith('/people')) return response(people(++reads === 1 ? 'Before verification' : 'Fresh authorized read'))
    throw new Error(`Unexpected synthetic request: ${path}`)
  })
  vi.stubGlobal('fetch', transport)
  const Ordinary = defineComponent({
    setup() {
      setups++
      if (migration) {
        const reviewedRequest = ref(crypto.randomUUID())
        return () => h('section', { 'data-testid': 'migration-request' }, reviewedRequest.value)
      }
      const { data: me } = useMe()
      const read = usePeople(computed(() => me.value?.organization?.id ?? ''))
      return () => h('section', { 'data-testid': 'ordinary-route' }, read.error.value
        ? [h('p', { role: 'alert' }, 'Ordinary query failed')]
        : read.data.value?.people.map(person => h('p', person.display_name)) ?? [h('p', 'Loading ordinary records')])
    },
  })
  const path = migration ? '/manage/migration' : '/people'
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path, name: migration ? 'manage-migration' : 'people', component: Ordinary }, { path: '/workspace-review', name: 'workspace-review', component: { render: () => h('h1', 'Workspace under review') } }, { path: '/login', meta: { public: true }, component: { render: () => h('h1', 'Sign in') } }, { path: '/:pathMatch(.*)*', component: { render: () => null } }] })
  await router.push(path); await router.isReady()
  queryClient.setQueryData(queryKeys.me, original())
  wrapper = mount(App, { global: { plugins: [router, [VueQueryPlugin, { queryClient }]] }, attachTo: document.body })
  await flushPromises()
  if (!migration) expect(wrapper.text()).toContain('Before verification')
  return { transport, router, reads: () => reads, setups: () => setups }
}

describe('ordinary route workspace verification', () => {
  it('preserves the mounted migration confirmation while hiding it during verification', async () => {
    const authority = deferred<Response>()
    const route = await mountOrdinary(() => authority.promise, true)
    const before = wrapper!.get('[data-testid="migration-request"]').text()
    const refresh = refreshWorkspace(); await flushPromises(); await vi.advanceTimersByTimeAsync(5_000)
    expect(wrapper!.get('[data-testid="migration-request"]').isVisible()).toBe(false)
    expect(route.setups()).toBe(1)
    authority.resolve(response(reviewed())); await refresh; await flushPromises()
    expect(wrapper!.get('[data-testid="migration-request"]').isVisible()).toBe(true)
    expect(wrapper!.get('[data-testid="migration-request"]').text()).toBe(before)
    expect(route.setups()).toBe(1)
  })

  it('waits beyond the query retry window before mounting and fetching with the verified authority', async () => {
    const authority = deferred<Response>()
    const route = await mountOrdinary(() => authority.promise)
    const refresh = refreshWorkspace()
    await flushPromises()
    await vi.advanceTimersByTimeAsync(5_000)
    expect(wrapper!.find('[data-testid="ordinary-route"]').exists()).toBe(false)
    expect(route.setups()).toBe(1)
    expect(route.reads()).toBe(1)
    authority.resolve(response(reviewed()))
    await refresh; await flushPromises()
    expect(route.setups()).toBe(2)
    expect(route.reads()).toBe(2)
    expect(wrapper!.get('[data-testid="ordinary-route"]').text()).toBe('Fresh authorized read')
    expect(wrapper!.find('[data-testid="ordinary-route"] [role="alert"]').exists()).toBe(false)
  })

  it('keeps ordinary routes unmounted through failed verification and reads afresh after explicit retry', async () => {
    const verify = vi.fn<() => Promise<Response>>().mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(response(reviewed()))
    const route = await mountOrdinary(verify)
    await expect(refreshWorkspace()).rejects.toThrow()
    await flushPromises(); await vi.advanceTimersByTimeAsync(5_000)
    expect(wrapper!.find('[data-testid="ordinary-route"]').exists()).toBe(false)
    expect(route.setups()).toBe(1)
    expect(route.reads()).toBe(1)
    await wrapper!.get('[data-testid="workspace-verification"] button').trigger('click')
    await flushPromises()
    expect(verify).toHaveBeenCalledTimes(2)
    expect(route.reads()).toBe(2)
    expect(wrapper!.get('[data-testid="ordinary-route"]').text()).toBe('Fresh authorized read')
    expect(wrapper!.find('[data-testid="workspace-verification"]').exists()).toBe(false)
  })
})


describe('workspace authority when a private tab resumes', () => {
  it('rechecks a settled cookie generation, coalesces visible events and rejects the old Person bytes after demotion', async () => {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
    const authority = deferred<Response>()
    const oldPerson = deferred<Response>()
    let resume = false
    const route = await mountOrdinary(() => resume ? authority.promise : Promise.resolve(response(reviewed())), false, oldPerson.promise)
    // Finish real startup session verification. Its opaque cookie generation
    // deliberately stays unchanged when governance later demotes this actor.
    resetSessionCoordination(); await flushPromises()
    expect(isSessionVerified()).toBe(true)
    const generation = currentSessionGeneration()
    const lifetime = useAuthSessionLifetime().value
    const initialMeReads = route.transport.mock.calls.filter(([path]) => String(path).endsWith('/me')).length
    const stale = apiFetch('/people/person').then(
      value => ({ value, error: undefined }),
      (error: unknown) => ({ value: undefined, error }),
    )
    const personSignal = route.transport.mock.calls.find(([path]) => String(path).endsWith('/people/person'))![1]!.signal!
    queryClient.setQueryData(['org', 'org', 'private-old-person'], { name: 'Old authorized Person' })
    resume = true

    window.dispatchEvent(new Event('focus'))
    expect(useWorkspacePending().value).toBe(true)
    expect(personSignal.aborted).toBe(true)
    expect(queryClient.getQueryData(['org', 'org', 'private-old-person'])).toBeUndefined()
    window.dispatchEvent(new Event('visibilitychange'))
    window.dispatchEvent(new Event('pageshow'))
    await flushPromises()
    expect(route.transport.mock.calls.filter(([path]) => String(path).endsWith('/me'))).toHaveLength(initialMeReads + 1)
    expect(useAuthSessionLifetime().value).toBe(lifetime)
    expect(currentSessionGeneration()).toBe(generation)
    expect(wrapper!.find('[data-testid="ordinary-route"]').exists()).toBe(false)

    authority.resolve(response({ ...reviewed(), organization: { ...reviewed().organization!, role: 'member' } }))
    await flushPromises()
    expect(route.router.currentRoute.value.path).toBe('/workspace-review')
    expect(wrapper!.text()).toContain('Workspace under review')
    oldPerson.resolve(response({ display_name: 'Old authorized Person' }))
    expect((await stale).error).toBeInstanceOf(WorkspaceVerificationPendingError)
    await flushPromises()
    expect(wrapper!.text()).not.toContain('Old authorized Person')
    expect(queryClient.getQueryData(['org', 'org', 'private-old-person'])).toBeUndefined()
    expect(useAuthSessionLifetime().value).toBe(lifetime)
  })

  it('does not start workspace verification while hidden, during an auth attempt, or after entering a public route', async () => {
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    const route = await mountOrdinary(async () => response(original()))
    const meReads = () => route.transport.mock.calls.filter(([path]) => String(path).endsWith('/me')).length
    const initial = meReads()
    for (const type of ['focus', 'visibilitychange', 'pageshow']) window.dispatchEvent(new Event(type))
    await flushPromises(); expect(meReads()).toBe(initial)
    visibility.mockReturnValue('visible')
    const attempt = beginSessionTransition()
    for (const type of ['focus', 'visibilitychange', 'pageshow']) window.dispatchEvent(new Event(type))
    await flushPromises(); expect(meReads()).toBe(initial)
    settleSessionTransition(attempt); await flushPromises()
    await route.router.push('/login'); await flushPromises()
    const publicReads = meReads()
    for (const type of ['focus', 'visibilitychange', 'pageshow']) window.dispatchEvent(new Event(type))
    await flushPromises(); expect(meReads()).toBe(publicReads)
    expect(wrapper!.text()).toBe('Sign in')
  })
})
