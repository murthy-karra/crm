// SLICE_007e §10 criterion 17: admin rows are clickable and open the
// workbench dialog (content fetched on open, never before); the member
// rendering is byte-identical to the plain table — no clickable rows, no
// dialog, no detail fetch.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import type {
  MeResponse,
  MembershipRole,
  UnresolvedDetailResponse,
  UnresolvedResponse,
} from '../api/types'
import UnresolvedView from './UnresolvedView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ROW_ID = '44444444-4444-4444-4444-444444444444'

function me(role: MembershipRole): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role },
    platform_admin: false,
  }
}

function unresolved(): UnresolvedResponse {
  return {
    items: [
      {
        id: ROW_ID,
        source: 'email',
        received_at: '2026-08-25T12:00:00.000Z',
        resolution: 'unresolved',
        reason: 'email_unrecognized_format',
        byte_len: 426,
      },
    ],
    truncated: false,
  }
}

function detail(): UnresolvedDetailResponse {
  return {
    id: ROW_ID,
    source: 'email',
    payload_format: 'rfc822_v1',
    received_at: '2026-08-25T12:00:00.000Z',
    resolution: 'unresolved',
    reason: 'email_unrecognized_format',
    byte_len: 426,
    content: {
      kind: 'email',
      subject: 'Interested in the downtown listing',
      from_display: 'Jordan Rivera',
      from_addr: 'jordan.rivera@example.com',
      date: '2026-08-24T21:32:00.000Z',
      text: 'Hi, I saw the listing at 123 Main St.',
      truncated: false,
    },
  }
}

function stub(role: MembershipRole) {
  apiFetchMock.mockImplementation((path: string) => {
    if (path === '/me') return Promise.resolve(me(role))
    if (path === '/intake/unresolved') return Promise.resolve(unresolved())
    if (path === `/intake/unresolved/${ROW_ID}`) return Promise.resolve(detail())
    return Promise.reject(new Error(`unexpected ${path}`))
  })
}

async function mountView() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(UnresolvedView, {
    global: {
      plugins: [[VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]],
      stubs: { RouterLink: { template: '<a><slot /></a>' } },
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('UnresolvedView (SLICE_007e)', () => {
  it('member rows are not clickable and no detail is ever fetched', async () => {
    stub('member')
    const wrapper = await mountView()

    const row = wrapper.find('tbody tr')
    expect(row.exists()).toBe(true)
    expect(row.classes()).not.toContain('cursor-pointer')

    await row.trigger('click')
    await flushPromises()
    const detailCalls = apiFetchMock.mock.calls.filter(([path]) =>
      path.startsWith(`/intake/unresolved/${ROW_ID}`),
    )
    expect(detailCalls).toHaveLength(0)
    expect(document.body.textContent).not.toContain('Interested in the downtown listing')
    wrapper.unmount()
  })

  it('admin rows open the dialog, fetching the content only on open', async () => {
    stub('admin')
    const wrapper = await mountView()

    // Nothing fetched before the click (content never prefetched, §7).
    expect(
      apiFetchMock.mock.calls.filter(([path]) => path === `/intake/unresolved/${ROW_ID}`),
    ).toHaveLength(0)

    const row = wrapper.find('tbody tr')
    expect(row.classes()).toContain('cursor-pointer')
    await row.trigger('click')
    await flushPromises()

    expect(
      apiFetchMock.mock.calls.filter(([path]) => path === `/intake/unresolved/${ROW_ID}`),
    ).toHaveLength(1)
    // Dialogs teleport to body — assert there.
    expect(document.body.textContent).toContain('Interested in the downtown listing')
    expect(document.body.textContent).toContain('jordan.rivera@example.com')
    expect(document.body.textContent).toContain('Try again')
    expect(document.body.textContent).toContain('Discard')
    wrapper.unmount()
  })
})
