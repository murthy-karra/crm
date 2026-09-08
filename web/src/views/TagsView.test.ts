// SLICE_011e.md §5, §9.9: `/manage/tags` — a flat table with per-row
// Rename/Delete gated on `can_manage`, the disabled tooltip otherwise, the
// delete confirm sentence only for a non-zero count, and the 409/403/404
// paths. Service-free: `apiFetch` is mocked.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type { DeleteTagResponse, MeResponse, RenameTagResponse, Tag, TagsResponse } from '../api/types'
import TagsView from './TagsView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

const UNUSED_MINE: Tag = { id: 'tag-1', name: 'Investr', person_count: 0, can_manage: true }
const IN_USE_ANOTHERS: Tag = { id: 'tag-2', name: 'Sphere', person_count: 3, can_manage: false }

interface StubOptions {
  tags?: () => TagsResponse
  rename?: (tagId: string, name: string) => { tag: Tag; changed: boolean } | Error
  del?: (tagId: string) => DeleteTagResponse | Error
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return me()
    if (path === '/tags' && method === 'GET') {
      return options.tags?.() ?? { tags: [UNUSED_MINE, IN_USE_ANOTHERS] }
    }
    const renameMatch = /^\/tags\/([^/]+)$/.exec(path)
    if (renameMatch && method === 'PUT') {
      const body = JSON.parse(String(init?.body ?? '{}')) as { name: string }
      const result = options.rename?.(renameMatch[1], body.name)
      if (result instanceof Error) throw result
      if (result) return result satisfies RenameTagResponse
      return { tag: { ...UNUSED_MINE, name: body.name }, changed: true } satisfies RenameTagResponse
    }
    if (renameMatch && method === 'DELETE') {
      const result = options.del?.(renameMatch[1])
      if (result instanceof Error) throw result
      if (result) return result satisfies DeleteTagResponse
      return { deleted: true, removed_from_people: 0 } satisfies DeleteTagResponse
    }
    throw new Error(`unexpected ${method} ${path}`)
  })
}

async function mountView() {
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: TagsView }] })
  await router.push('/')
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = mount(TagsView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper }
}

function dialogButton(text: string) {
  return Array.from(document.body.querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === text && b.closest('[role="dialog"]'),
  )
}

beforeEach(() => {
  apiFetchMock.mockReset()
  stub()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('TagsView — controls follow can_manage', () => {
  it('enables Rename/Delete only for the manageable row, disabled with the tooltip otherwise', async () => {
    const { wrapper } = await mountView()
    const renameButtons = wrapper.findAll('[data-testid="rename-tag"]')
    const deleteButtons = wrapper.findAll('[data-testid="delete-tag"]')
    expect(renameButtons).toHaveLength(2)
    expect(renameButtons[0].attributes('disabled')).toBeUndefined()
    expect(deleteButtons[0].attributes('disabled')).toBeUndefined()
    expect(renameButtons[1].attributes('disabled')).toBeDefined()
    expect(renameButtons[1].attributes('title')).toBe('Only an admin can change a tag that is in use')
    expect(deleteButtons[1].attributes('disabled')).toBeDefined()
    expect(deleteButtons[1].attributes('title')).toBe('Only an admin can change a tag that is in use')
  })

  it('does nothing when a disabled row is clicked', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-tag"]')[1].trigger('click')
    await wrapper.findAll('[data-testid="delete-tag"]')[1].trigger('click')
    expect(wrapper.find('[data-testid="rename-tag-input"]').exists()).toBe(false)
    expect(document.querySelector('[role="dialog"]')).toBeFalsy()
  })
})

describe('TagsView — inline rename', () => {
  it('Enter saves; a taken name shows the 409 inline', async () => {
    stub({ rename: () => new ApiError(409, 'tag_name_taken') })
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-tag"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-tag-input"]')
    await input.setValue('Sphere')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(wrapper.get('[data-testid="rename-tag-error"]').text()).toBe('"Sphere" is already in use.')
    expect(wrapper.find('[data-testid="rename-tag-input"]').exists()).toBe(true)
  })

  it('Escape cancels without saving', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-tag"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-tag-input"]')
    await input.setValue('Changed')
    await input.trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(wrapper.find('[data-testid="rename-tag-input"]').exists()).toBe(false)
    expect(apiFetchMock.mock.calls.some(([, init]) => (init?.method ?? 'GET') === 'PUT')).toBe(false)
  })

  it('a successful rename saves and re-reads the row', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-tag"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-tag-input"]')
    await input.setValue('Investor')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(wrapper.find('[data-testid="rename-tag-input"]').exists()).toBe(false)
    const putCall = apiFetchMock.mock.calls.find(([, init]) => (init?.method ?? 'GET') === 'PUT')
    expect(JSON.parse(String(putCall?.[1]?.body))).toEqual({ name: 'Investor' })
  })

  it('403 mid-edit closes the field and shows the refreshed notice', async () => {
    stub({ rename: () => new ApiError(403, 'forbidden') })
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-tag"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-tag-input"]')
    await input.setValue('Changed')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(wrapper.find('[data-testid="rename-tag-input"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="tags-notice"]').text()).toContain('only an admin can change it')
  })
})

describe('TagsView — delete', () => {
  it('states the count and the invalid-filter sentence only when non-zero', async () => {
    stub({ tags: () => ({ tags: [{ ...UNUSED_MINE, person_count: 0 }, { ...IN_USE_ANOTHERS, can_manage: true }] }) })
    const { wrapper } = await mountView()

    await wrapper.findAll('[data-testid="delete-tag"]')[0].trigger('click')
    await flushPromises()
    const zeroMessage = document.querySelector('[role="dialog"]')?.textContent ?? ''
    expect(zeroMessage).not.toContain('invalid-filter')
    dialogButton('Cancel')?.click()
    await flushPromises()

    await wrapper.findAll('[data-testid="delete-tag"]')[1].trigger('click')
    await flushPromises()
    const nonZeroMessage = document.querySelector('[role="dialog"]')?.textContent ?? ''
    expect(nonZeroMessage).toContain('3 people will lose this tag')
    expect(nonZeroMessage).toContain('Saved lists and Today rules that use this tag will show an invalid-filter notice until they are edited.')
  })

  it('confirms and issues the DELETE request', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="delete-tag"]')[0].trigger('click')
    await flushPromises()
    dialogButton('Delete')?.click()
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/tags/${UNUSED_MINE.id}` && init?.method === 'DELETE')).toBe(true)
    expect(document.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('404 closes the dialog and explains, having already refreshed the list', async () => {
    stub({ del: () => new ApiError(404, 'not_found') })
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="delete-tag"]')[0].trigger('click')
    await flushPromises()
    dialogButton('Delete')?.click()
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeFalsy()
    expect(wrapper.get('[data-testid="tags-notice"]').text()).toContain('no longer exists')
  })
})
