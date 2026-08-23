// SLICE_005 §13 item 5: drawer toggle and ⌘K/Esc; Ask hidden off
// Organization routes; the drawer persists across navigation while open.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { ref } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MeResponse } from '../api/types'
import AppShell from './AppShell.vue'

const meRef = ref<MeResponse | undefined>(undefined)

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
    useLogoutMutation: () => ({ mutate: vi.fn(), isPending: ref(false) }),
    useOperatorTurn: () => ({ mutate: vi.fn(), isPending: ref(false), reset: vi.fn() }),
  }
})

vi.mock('../realtime/useRealtime', () => ({
  useRealtime: () => ({ status: ref('connected') }),
}))

function orgSession(): MeResponse {
  return {
    user: { id: 'u1', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: 'o1', name: 'Acme Realty', role: 'member' },
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

async function mountShell(path: string, me: MeResponse): Promise<{ wrapper: ReturnType<typeof mount>; router: Router }> {
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
    slots: { default: '<p>content</p>' },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, router }
}

function keydown(init: KeyboardEventInit) {
  window.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('AppShell Ask drawer', () => {
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
})
