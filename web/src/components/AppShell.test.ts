/* eslint-disable vue/one-component-per-file -- local launcher fixtures exercise injected shell behavior. */
// SLICE_005 §13 item 5: drawer toggle and ⌘K/Esc; Ask hidden off
// Organization routes; the drawer persists across navigation while open.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { defineComponent, h, inject, ref, type Component } from 'vue'
import { OPERATOR_LAUNCHER } from '../lib/operatorLauncher'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MeResponse } from '../api/types'
import {
  beginSessionTransition,
  isSessionVerified,
  setRouteAuthorizationReplayPending,
  settleSessionTransition,
  useSessionBlockedByOutstandingAttempt,
  useSessionVerificationInFlight,
} from '../sessionLifecycle'
import AppShell from './AppShell.vue'
import { configureWorkspaceLifecycle, observeWorkspace, refreshWorkspace, resetWorkspace } from '../workspaceLifecycle'

const meRef = ref<MeResponse | undefined>(undefined)
const authSessionLifetimeRef = ref(0)

vi.mock('../api/queries', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/queries')>()
  return {
    ...actual,
    useMe: () => ({
      data: meRef,
      error: ref(null),
      isError: ref(false),
      isFetching: ref(false),
      refetch: vi.fn(),
    }),
    useAuthSessionLifetime: () => authSessionLifetimeRef,
    useLogoutMutation: () => ({ mutate: vi.fn(), isPending: ref(false) }),
    useOperatorTurn: () => ({ mutate: vi.fn(), isPending: ref(false), reset: vi.fn() }),
  }
})

vi.mock('../realtime/useRealtime', () => ({
  useRealtime: () => ({ status: ref('connected') }),
}))

function orgSession(actorId = 'u1', orgId = 'o1'): MeResponse {
  return {
    user: { id: actorId, email: `${actorId}@acme.test`, display_name: actorId === 'u1' ? 'Alice' : 'Bob' },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: orgId, name: orgId === 'o1' ? 'Acme Realty' : 'Other Realty', role: 'member' },
    platform_admin: false,
  } as unknown as MeResponse
}

function platformSession(): MeResponse {
  return {
    user: { id: 'u2', email: 'root@platform.test', display_name: 'Root' },
    organization: null,
    platform_admin: true,
  } as unknown as MeResponse
}

async function mountShell(path: string, me: MeResponse, content: string | Component = '<p>content</p>'): Promise<{ wrapper: ReturnType<typeof mount>; router: Router }> {
  meRef.value = me
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/today', component: { template: '<div />' } },
      { path: '/people', component: { template: '<div />' } },
      { path: '/people/:id', component: { template: '<div />' } },
      { path: '/platform', component: { template: '<div />' } },
      { path: '/:pathMatch(.*)*', component: { template: '<div />' } },
    ],
  })
  await router.push(path)
  await router.isReady()
  const wrapper = mount(AppShell, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient: new QueryClient() }]] },
    slots: { default: typeof content === 'string' ? content : () => h(content) },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, router }
}

function keydown(init: KeyboardEventInit) {
  window.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
}

function deferred() {
  let resolve!: () => void
  const promise = new Promise<void>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

afterEach(() => {
  resetWorkspace()
  setRouteAuthorizationReplayPending(false)
  authSessionLifetimeRef.value = 0
  meRef.value = undefined
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

describe('AppShell Ask drawer', () => {
  it('does not mount a restricted route slot while authorization replay is pending', async () => {
    let restrictedSetups = 0
    const RestrictedRoute = defineComponent({
      setup() {
        restrictedSetups += 1
        return () => h('p', 'restricted route')
      },
    })
    setRouteAuthorizationReplayPending(true)
    const { wrapper } = await mountShell('/manage/members', orgSession(), RestrictedRoute)
    expect(restrictedSetups).toBe(0)
    expect(wrapper.text()).toContain('Updating access…')

    setRouteAuthorizationReplayPending(false)
    await flushPromises()
    expect(restrictedSetups).toBe(1)
    wrapper.unmount()
  })

  it('renders the approved Elysium CRM lockup', async () => {
    const { wrapper } = await mountShell('/today', orgSession())
    const logo = wrapper.get('img[alt="Elysium CRM"]')
    expect(logo.attributes('width')).toBe('164')
    expect(logo.attributes('height')).toBe('48')
    wrapper.unmount()
  })

  it('toggles with the button and persists across route changes while open', async () => {
    const { wrapper, router } = await mountShell('/today', orgSession())
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    await wrapper.get('[data-testid="ask-toggle"]').trigger('click')
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    // The pill yields to the drawer while it is open.
    expect(wrapper.find('[data-testid="ask-toggle"]').exists()).toBe(false)

    await router.push('/people')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)

    await wrapper.get('[data-testid="operator-close"]').trigger('click')
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    expect(wrapper.get('[data-testid="ask-toggle"]').text()).toContain('Ask AI Operator')
    wrapper.unmount()
  })

  it('⌘K and Ctrl+K toggle, Esc closes', async () => {
    const { wrapper } = await mountShell('/today', orgSession())
    keydown({ key: 'k', metaKey: true })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    keydown({ key: 'Escape' })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    keydown({ key: 'k', ctrlKey: true })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    keydown({ key: 'k', ctrlKey: true })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    wrapper.unmount()
  })

  it('Esc does not close the drawer while a dialog is open', async () => {
    const { wrapper } = await mountShell('/today', orgSession())
    keydown({ key: 'k', metaKey: true })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    const dialog = document.createElement('div')
    dialog.setAttribute('role', 'dialog')
    document.body.appendChild(dialog)
    keydown({ key: 'Escape' })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    dialog.remove()
    keydown({ key: 'Escape' })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    wrapper.unmount()
  })

  it('is hidden on platform routes and for a platform-only session', async () => {
    const a = await mountShell('/platform', orgSession())
    expect(a.wrapper.find('[data-testid="ask-toggle"]').exists()).toBe(false)
    keydown({ key: 'k', metaKey: true })
    await flushPromises()
    expect(a.wrapper.find('[data-testid="operator-panel"]').exists()).toBe(false)
    a.wrapper.unmount()

    const b = await mountShell('/platform', platformSession())
    expect(b.wrapper.find('[data-testid="ask-toggle"]').exists()).toBe(false)
    b.wrapper.unmount()
  })

  it('closes the drawer when either the same-Organization actor or auth-session lifetime changes', async () => {
    const { wrapper } = await mountShell('/today', orgSession())
    await wrapper.get('[data-testid="ask-toggle"]').trigger('click')
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)

    meRef.value = orgSession('u2')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)

    await wrapper.get('[data-testid="ask-toggle"]').trigger('click')
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
    authSessionLifetimeRef.value += 1
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    wrapper.unmount()
  })

  it('closes the drawer on an Organization switch with the same actor (SLICE_011c §9 criterion 10)', async () => {
    const { wrapper } = await mountShell('/today', orgSession('u1', 'o1'))
    await wrapper.get('[data-testid="ask-toggle"]').trigger('click')
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)

    // A different Organization id, same actor — `operatorIdentityKey`
    // includes `orgId`, so this remounts OperatorPanel via its `:key`
    // binding just as an actor or auth-session-lifetime change does.
    meRef.value = orgSession('u1', 'o2')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    wrapper.unmount()
  })

  it('does not open the drawer when a deferred person launcher resolves after auth-session replacement', async () => {
    const personId = '55555555-5555-5555-5555-555555555555'
    const navigation = deferred()
    const Launcher = defineComponent({
      setup() { return { launch: inject(OPERATOR_LAUNCHER)!, personId } },
      template: '<button data-testid="deferred-preview-ask" @click="launch(personId)">Ask about person</button>',
    })
    const { wrapper, router } = await mountShell('/people', orgSession(), Launcher)
    const removeGuard = router.beforeEach(async (to) => {
      if (to.path === `/people/${personId}`) await navigation.promise
      return true
    })

    await wrapper.get('[data-testid="deferred-preview-ask"]').trigger('click')
    await Promise.resolve()
    authSessionLifetimeRef.value += 1
    await flushPromises()
    navigation.resolve()
    await flushPromises()

    expect(router.currentRoute.value.path).toBe(`/people/${personId}`)
    expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
    removeGuard()
    wrapper.unmount()
  })
})

describe('AppShell session recovery copy (SLICE_011c §6, F1/F3)', () => {
  /** The real `verify` handler (registered by api/queries.ts, unmocked in
   * this file) dispatches a real `apiFetch('/me')` when these tests drive
   * the real sessionLifecycle module. Stub it rather than let a real
   * network call happen. */
  function stubMeFetch(identity: MeResponse) {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(identity), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    })))
  }

  /** Held open until `resolve` is called — an immediately-resolving mock
   * lets the real verify round trip complete during `mountShell`'s own
   * awaits, before the "in flight" assertions below ever run. */
  function stubMeFetchDeferred(identity: MeResponse): { resolve: () => void } {
    let resolveFetch!: () => void
    const gate = new Promise<void>((r) => { resolveFetch = r })
    vi.stubGlobal('fetch', vi.fn(async () => {
      await gate
      return new Response(JSON.stringify(identity), { status: 200, headers: { 'Content-Type': 'application/json' } })
    }))
    return { resolve: resolveFetch }
  }

  it('renders the neutral loading copy during in-flight verification, never the blocked-attempt copy (F3)', async () => {
    const identity = orgSession()
    const { resolve } = stubMeFetchDeferred(identity)

    // A same-tab login (or a cold load of a persisted settled marker)
    // starts verification of the CURRENT generation with no outstanding
    // attempt — the ordinary, non-alarming path, never F1's recovery path.
    const epoch = beginSessionTransition()
    settleSessionTransition(epoch, identity)
    expect(useSessionVerificationInFlight().value).toBe(true)
    expect(useSessionBlockedByOutstandingAttempt().value).toBe(false)

    const { wrapper } = await mountShell('/today', identity)
    expect(wrapper.text()).toContain('Loading your session…')
    expect(wrapper.text()).not.toContain('has not finished')
    expect(wrapper.find('[data-testid="session-reset"]').exists()).toBe(false)

    resolve()
    await vi.waitFor(() => expect(isSessionVerified()).toBe(true))
    await flushPromises()
    expect(wrapper.text()).not.toContain('Loading your session…')
    wrapper.unmount()
  })

  it('renders the F1 recovery copy and control only when blocked by an outstanding attempt, and recovers after Reset and continue', async () => {
    const identity = orgSession()
    stubMeFetch(identity)

    // Simulate another tab's attempt that never settled — a durable
    // pending record with no timer ever "trusting" that it finished.
    beginSessionTransition()
    expect(useSessionBlockedByOutstandingAttempt().value).toBe(true)

    const { wrapper } = await mountShell('/today', identity)
    expect(wrapper.text()).toContain('has not finished')
    expect(wrapper.text()).not.toContain('Loading your session…')
    const resetButton = wrapper.get('[data-testid="session-reset"]')
    expect(resetButton.text()).toBe('Reset and continue')

    await resetButton.trigger('click')
    await flushPromises()

    await vi.waitFor(() => expect(isSessionVerified()).toBe(true))
    await flushPromises()
    expect(wrapper.text()).not.toContain('has not finished')
    expect(wrapper.find('[data-testid="session-reset"]').exists()).toBe(false)
    wrapper.unmount()
  })
})


it('opens the person route before launching the Operator without sending a turn', async () => {
  const personId = '55555555-5555-5555-5555-555555555555'
  const Launcher = defineComponent({
    setup() { return { launch: inject(OPERATOR_LAUNCHER)!, personId } },
    template: '<button data-testid="preview-ask" @click="launch(personId)">Ask about person</button>',
  })
  const { wrapper, router } = await mountShell('/people', orgSession(), Launcher)
  await wrapper.get('[data-testid="preview-ask"]').trigger('click')
  await flushPromises()
  expect(router.currentRoute.value.path).toBe(`/people/${personId}`)
  expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(true)
  expect(wrapper.get('[data-testid="operator-input"]').element).toBe(document.activeElement)
  expect(wrapper.get('[data-testid="operator-input"]').text()).toBe('')
  wrapper.unmount()
})


it('allows Escape to close Operator over the non-modal person inspector', async () => {
  const { wrapper } = await mountShell('/people', orgSession(), '<section role="dialog" data-testid="person-preview">Person</section>')
  await wrapper.get('[data-testid="ask-toggle"]').trigger('click')
  keydown({ key: 'Escape' })
  await flushPromises()
  expect(wrapper.get('[data-testid="operator-panel"]').isVisible()).toBe(false)
  expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(true)
  wrapper.unmount()
})

// SLICE_011e §5: Tags is a member route (rule 1/D-051 lets a creator
// manage their own unused tag) — visible in Manage for every Organization
// member, unlike the three admin-only entries beside it.
describe('AppShell Manage nav (SLICE_011e §5)', () => {
  it('shows the Tags entry for a member, with the admin-only entries hidden', async () => {
    const { wrapper } = await mountShell('/today', orgSession())
    const tagsLink = wrapper.find('a[href="/manage/tags"]')
    expect(tagsLink.exists()).toBe(true)
    expect(tagsLink.text()).toContain('Tags')
    expect(wrapper.find('a[href="/manage/members"]').exists()).toBe(false)
    expect(wrapper.find('a[href="/manage/intake"]').exists()).toBe(false)
    expect(wrapper.find('a[href="/manage/today-feeds"]').exists()).toBe(false)
    expect(wrapper.find('a[href="/manage/migration"]').exists()).toBe(false)
    wrapper.unmount()
  })
})


describe('workspace review shell', () => {
  it('keeps the migration child mounted but hides its content during same-identity workspace verification', async () => {
    let setups = 0
    const Child = defineComponent({ setup() { setups++; return () => h('p', { 'data-testid': 'retained-confirmation' }, 'Private confirmation state') } })
    const identity = orgSession(); identity.organization!.role = 'admin'
    const { wrapper } = await mountShell('/manage/migration', identity, Child)
    observeWorkspace(identity)
    let resolve!: (value: MeResponse) => void
    configureWorkspaceLifecycle({ discard: vi.fn(), verify: () => new Promise(done => { resolve = done }), install: value => { meRef.value = value } })
    const refresh = refreshWorkspace(); await flushPromises()
    expect(wrapper.get('[data-testid="workspace-verification"]').text()).toContain('Updating workspace access')
    expect(wrapper.get('[data-testid="retained-confirmation"]').isVisible()).toBe(false)
    expect(wrapper.find('[data-testid="operator-panel"]').exists()).toBe(false)
    expect(setups).toBe(1)
    resolve({ ...identity, organization: { ...identity.organization!, workspace_mode: 'migration_review', workspace_revision: '2' } })
    await refresh; await flushPromises()
    expect(wrapper.get('[data-testid="retained-confirmation"]').isVisible()).toBe(true)
    expect(wrapper.get('[data-testid="workspace-review-banner"]').text()).toContain('Ordinary work')
    expect(setups).toBe(1)
    wrapper.unmount()
  })
  it('shows only the member waiting navigation and logout in review mode', async () => {
    const identity = orgSession(); identity.organization!.workspace_mode = 'migration_review'; identity.organization!.workspace_revision = '2'
    const { wrapper } = await mountShell('/workspace-review', identity)
    expect(wrapper.find('a[href="/workspace-review"]').exists()).toBe(true)
    for (const path of ['/people', '/today', '/intake/new', '/manage/migration']) expect(wrapper.find(`a[href="${path}"]`).exists()).toBe(false)
    expect(wrapper.find('[aria-label="Log out"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="operator-panel"]').exists()).toBe(false)
    wrapper.unmount()
  })
})
