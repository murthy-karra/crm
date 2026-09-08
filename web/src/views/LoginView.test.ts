// SLICE_014 §4, §8.6: mounting the login screen starts importing the Today
// route chunk immediately, so it is already in Vite's module cache by the
// time a successful login navigates there — this only asserts the mount
// hook fires the shared `preloadTodayView` export (spied); the router
// guard's own call to it (the OTHER caller, from the Today route record) is
// covered in router.test.ts.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import * as preloadModule from '../preload'
import LoginView from './LoginView.vue'

// Round-1 review fix, item 9: LoginView mounts no query today, but this
// mock is defensive against a future change adding one — under happy-dom
// the default test origin happens to be localhost:3000 (this project's own
// dev API port), so an unmocked apiFetch call here could silently reach a
// real local server instead of failing loudly. See router.test.ts's own
// note on the same hazard (SLICE_014 part C's fixed test-isolation bug).
vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn(() => Promise.reject(new Error('unexpected apiFetch call in LoginView.test.ts'))) }
})

const apiFetchMock = vi.mocked(apiFetch)

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/login', component: LoginView },
      { path: '/today', component: { template: '<div />' } },
    ],
  })
  await router.push('/login')
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = mount(LoginView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }]] },
  })
  await flushPromises()
  return wrapper
}

describe('LoginView preload (SLICE_014 §4)', () => {
  let preloadSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    preloadSpy = vi.spyOn(preloadModule, 'preloadTodayView').mockImplementation(() => Promise.resolve({} as never))
    apiFetchMock.mockClear()
  })
  afterEach(() => {
    preloadSpy.mockRestore()
  })

  it('calls preloadTodayView once on mount, without any network call', async () => {
    const wrapper = await mountView()
    expect(preloadSpy).toHaveBeenCalledTimes(1)
    expect(apiFetchMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
