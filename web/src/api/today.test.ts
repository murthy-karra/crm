// SLICE_011c §9 criterion 10: `useToday` must adopt the actor-aware
// `queryKeys.todayForActor` read key (§6 "Update all Today consumers to use
// the actor-aware read key") and isolate a same-Organization actor switch's
// cache the same way SLICE_011b's saved-list/source caches already do.
// Mirrors api/todaySources.test.ts's "drops a late same-Organization actor
// response and isolates the replacement actor key" pattern.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { queryKeys, useToday } from './queries'
import type { TodayResponse } from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_A = '22222222-2222-2222-2222-222222222222'
const ACTOR_B = '33333333-3333-3333-3333-333333333333'

/** `generated_at` doubles as a distinguishing sentinel here (no per-item
 * "name" field on `TodayResponse` the way a today-source has one) — only
 * this cache-identity test reads it, never a real timestamp comparison. */
function today(sentinel: string): TodayResponse {
  return {
    generated_at: sentinel,
    items: [],
    truncated: false,
    sources: { status: 'complete', issues: [] },
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

const cleanups: Array<() => void> = []

async function mountToday(
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } }),
) {
  const orgId = ref(ORG_ID)
  const actorId = ref(ACTOR_A)
  let query!: ReturnType<typeof useToday>
  const Harness = defineComponent({
    setup() {
      query = useToday(orgId, actorId)
      return () => null
    },
  })
  const wrapper = mount(Harness, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  })
  cleanups.push(() => {
    wrapper.unmount()
    queryClient.clear()
  })
  await flushPromises()
  return { actorId, orgId, query, queryClient }
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('Today cache identity and recovery', () => {
  it('drops a late same-Organization actor response and isolates the replacement actor key', async () => {
    const alice = deferred<TodayResponse>()
    const bob = deferred<TodayResponse>()
    apiFetchMock.mockReturnValueOnce(alice.promise).mockReturnValueOnce(bob.promise)
    const harness = await mountToday()

    expect(apiFetchMock.mock.calls[0]?.[0]).toBe('/today')
    harness.actorId.value = ACTOR_B
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledTimes(2)

    alice.resolve(today('alice-response'))
    await flushPromises()
    expect(harness.query.data.value?.generated_at).not.toBe('alice-response')

    bob.resolve(today('bob-response'))
    await flushPromises()
    expect(harness.query.data.value?.generated_at).toBe('bob-response')
    expect(queryKeys.todayForActor(ORG_ID, ACTOR_A)).not.toEqual(queryKeys.todayForActor(ORG_ID, ACTOR_B))
    expect(queryKeys.todayForActor(ORG_ID, ACTOR_A)).not.toEqual(queryKeys.todayForActor('other-org', ACTOR_A))
    expect(harness.queryClient.getQueryData(queryKeys.todayForActor(ORG_ID, ACTOR_A))).toBeUndefined()
    expect(harness.queryClient.getQueryData(queryKeys.todayForActor(ORG_ID, ACTOR_B))).toEqual(today('bob-response'))
  })
})
