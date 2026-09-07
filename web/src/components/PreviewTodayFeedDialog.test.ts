// SLICE_011d.md §4, §6: preview's session-identity fence — a late response
// must never paint after the actor or Organization changed, matching the
// 011b/011c pattern (`api/queries.ts`'s saved-list/Today-source mutation
// identity fence). Coordinator-requested follow-up to the Slice 011d Lane W
// implementation.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type { MeResponse, Member, PreviewTodayFeedResponse } from '../api/types'
import PreviewTodayFeedDialog from './PreviewTodayFeedDialog.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ADMIN_ID = 'u-admin'
const BOB_ID = '22222222-2222-2222-2222-222222222222'
const OTHER_ORG_ID = '33333333-3333-3333-3333-333333333333'

function me(overrides: Partial<MeResponse> = {}): MeResponse {
  return {
    user: { id: ADMIN_ID, email: 'admin@example.test', display_name: 'Admin Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', role: 'admin' },
    platform_admin: false,
    ...overrides,
  }
}

function member(id: string, name: string): Member {
  return { user_id: id, display_name: name, email: `${id}@example.test`, role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => { resolve = yes })
  return { promise, resolve }
}

const cleanups: Array<() => void> = []

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

function mountDialog(queryClient: QueryClient) {
  const wrapper = mount(PreviewTodayFeedDialog, {
    props: {
      visible: true,
      orgId: ORG_ID,
      feedKey: 'unanswered_inquiry',
      filter: { version: 1, clauses: [{ kind: 'awaiting_response', value: true }] },
      freshWithinHours: 24,
      members: [member(ADMIN_ID, 'Admin Alice'), member(BOB_ID, 'Bob')],
      defaultSubjectId: ADMIN_ID,
    },
    global: { plugins: [[VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanups.push(() => wrapper.unmount())
  return wrapper
}

describe('PreviewTodayFeedDialog session-identity fence', () => {
  it('discards a late preview response after the actor changes and paints nothing', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
    queryClient.setQueryData(queryKeys.me, me())
    const response = deferred<PreviewTodayFeedResponse>()
    apiFetchMock.mockReturnValueOnce(response.promise)
    mountDialog(queryClient)
    await flushPromises()

    // The request is in flight; the actor changes before it resolves (a
    // different object identity, as a real session replacement produces).
    queryClient.setQueryData(queryKeys.me, me({ user: { id: BOB_ID, email: 'bob@example.test', display_name: 'Bob' } }))
    await flushPromises()

    response.resolve({
      subject: { id: ADMIN_ID, display_name: 'Admin Alice' },
      items: [{
        person: {
          id: 'p-1', first_name: 'Marcus', last_name: 'Chen', display_name: 'Marcus Chen',
          stage: { id: 's-1', name: 'Lead' }, assigned_user: null, primary_email: null, primary_phone: null,
          inquiry_count: 1, last_inquiry_at: '2026-09-01T00:00:00Z', created_at: '2026-09-01T00:00:00Z',
        },
        priority: 'high', recommended_action: 'call',
        reasons: [{ code: 'new_inquiry', source: 'web', received_at: '2026-09-01T00:00:00Z' }],
        waiting_since: '2026-09-01T00:00:00Z', latest_inquiry: null, last_contact_attempt: null,
      }],
      truncated: false,
      description: ['Awaiting a response'],
    })
    await flushPromises()

    // The Dialog teleports its content to document.body, outside the
    // wrapper's own (now-empty) mount root. "Admin Alice" alone is not a
    // useful assertion — it is also the picker's own still-selected label,
    // present regardless of the discard — so this checks specifically for
    // the response's own rendered content: a data row, the empty-state
    // sentence that interpolates `subject.display_name`, and the table
    // chrome that only appears once `items` is set at all.
    expect(document.body.textContent).not.toContain('Marcus Chen')
    expect(document.body.textContent).not.toContain('No people currently match this rule')
    expect(document.body.textContent).not.toContain('No matches')
    expect(document.body.textContent).toContain('Loading preview')
  })

  it('discards a late preview response after the Organization changes', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
    queryClient.setQueryData(queryKeys.me, me())
    const response = deferred<PreviewTodayFeedResponse>()
    apiFetchMock.mockReturnValueOnce(response.promise)
    mountDialog(queryClient)
    await flushPromises()

    queryClient.setQueryData(queryKeys.me, me({ organization: { id: OTHER_ORG_ID, name: 'Other Realty', role: 'admin' } }))
    await flushPromises()

    response.resolve({ subject: { id: ADMIN_ID, display_name: 'Admin Alice' }, items: [], truncated: false, description: [] })
    await flushPromises()

    expect(document.body.textContent).not.toContain('No matches')
    expect(document.body.textContent).not.toContain('No people currently match this rule')
    expect(document.body.textContent).toContain('Loading preview')
  })

  it('does not discard a response when the session object is unchanged', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
    queryClient.setQueryData(queryKeys.me, me())
    apiFetchMock.mockResolvedValueOnce({ subject: { id: ADMIN_ID, display_name: 'Admin Alice' }, items: [], truncated: false, description: [] })
    mountDialog(queryClient)
    await flushPromises()

    expect(document.body.textContent).toContain('No matches')
    expect(document.body.textContent).toContain('Admin Alice')
  })

  it('guards onSubjectChange so a non-member id never reaches the API', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
    queryClient.setQueryData(queryKeys.me, me())
    apiFetchMock.mockResolvedValue({ subject: { id: ADMIN_ID, display_name: 'Admin Alice' }, items: [], truncated: false, description: [] })
    const wrapper = mountDialog(queryClient)
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledTimes(1)

    const select = wrapper.getComponent(Select)
    await select.vm.$emit('update:model-value', 'inactive-id')
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledTimes(1)
  })
})
