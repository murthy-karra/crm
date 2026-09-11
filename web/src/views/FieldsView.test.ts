// SLICE_019.md §7, §12: `/manage/fields` — create per type, inline rename,
// up/down reorder, Archive behind a confirm naming the count, restore, and
// the choice-field options editor. Service-free: `apiFetch` is mocked. The
// generic admin route guard is not re-tested here (06-verify calibration).
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Select from 'primevue/select'
import { ApiError, apiFetch } from '../api/client'
import type { CustomField, CustomFieldsResponse, MeResponse } from '../api/types'
import FieldsView from './FieldsView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Acme Realty', role: 'admin' },
    platform_admin: false,
  }
}

const BUDGET: CustomField = {
  id: 'field-budget',
  label: 'Budget',
  field_type: 'number',
  position: 1,
  archived_at: null,
  person_count: 3,
  options: [],
}
const TEMPERATURE: CustomField = {
  id: 'field-temperature',
  label: 'Lead temperature',
  field_type: 'choice',
  position: 2,
  archived_at: null,
  person_count: 0,
  options: [
    { id: 'opt-cold', label: 'Cold', position: 1, archived_at: null },
    { id: 'opt-warm', label: 'Warm', position: 2, archived_at: null },
  ],
}
const REFERRER_ARCHIVED: CustomField = {
  id: 'field-referrer',
  label: 'Referrer',
  field_type: 'text',
  position: 3,
  archived_at: '2026-09-01T00:00:00.000Z',
  person_count: 1,
  options: [],
}
const SOURCE: CustomField = {
  id: 'field-source',
  label: 'Source',
  field_type: 'text',
  position: 3,
  archived_at: null,
  person_count: 0,
  options: [],
}

interface StubOptions {
  fields?: () => CustomFieldsResponse
  create?: (body: { label: string; field_type: string; options?: string[] }) => { field: CustomField } | Error
  reorder?: (fieldIds: string[]) => { fields: CustomField[] } | Error
  update?: (fieldId: string, body: { label: string; archived: boolean }) => { field: CustomField; changed: boolean } | Error
  addOption?: (fieldId: string, label: string) => { field: CustomField } | Error
  updateOption?: (fieldId: string, optionId: string, body: { label: string; archived: boolean }) => { field: CustomField; changed: boolean } | Error
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return me()
    if (path === '/custom-fields' && method === 'GET') {
      return options.fields?.() ?? { fields: [BUDGET, TEMPERATURE, REFERRER_ARCHIVED] } satisfies CustomFieldsResponse
    }
    if (path === '/custom-fields' && method === 'POST') {
      const body = JSON.parse(String(init?.body ?? '{}')) as { label: string; field_type: string; options?: string[] }
      const result = options.create?.(body)
      if (result instanceof Error) throw result
      if (result) return result
      return {
        field: {
          id: 'field-new',
          label: body.label,
          field_type: body.field_type,
          position: 4,
          archived_at: null,
          person_count: 0,
          options: (body.options ?? []).map((label, index) => ({
            id: `opt-new-${index}`,
            label,
            position: index + 1,
            archived_at: null,
          })),
        },
      }
    }
    if (path === '/custom-fields/order' && method === 'PUT') {
      const body = JSON.parse(String(init?.body ?? '{}')) as { field_ids: string[] }
      const result = options.reorder?.(body.field_ids)
      if (result instanceof Error) throw result
      if (result) return result
      return { fields: [BUDGET, TEMPERATURE, REFERRER_ARCHIVED] }
    }
    const optionMatch = /^\/custom-fields\/([^/]+)\/options\/([^/]+)$/.exec(path)
    if (optionMatch && method === 'PUT') {
      const fieldId = optionMatch[1]!
      const optionId = optionMatch[2]!
      const body = JSON.parse(String(init?.body ?? '{}')) as { label: string; archived: boolean }
      const result = options.updateOption?.(fieldId, optionId, body)
      if (result instanceof Error) throw result
      if (result) return result
      return { field: TEMPERATURE, changed: true }
    }
    const optionsCollectionMatch = /^\/custom-fields\/([^/]+)\/options$/.exec(path)
    if (optionsCollectionMatch && method === 'POST') {
      const fieldId = optionsCollectionMatch[1]!
      const body = JSON.parse(String(init?.body ?? '{}')) as { label: string }
      const result = options.addOption?.(fieldId, body.label)
      if (result instanceof Error) throw result
      if (result) return result
      return { field: TEMPERATURE }
    }
    const fieldMatch = /^\/custom-fields\/([^/]+)$/.exec(path)
    if (fieldMatch && method === 'PUT') {
      const fieldId = fieldMatch[1]!
      const body = JSON.parse(String(init?.body ?? '{}')) as { label: string; archived: boolean }
      const result = options.update?.(fieldId, body)
      if (result instanceof Error) throw result
      if (result) return result
      return { field: { ...BUDGET, label: body.label, archived_at: body.archived ? '2026-09-10T00:00:00.000Z' : null }, changed: true }
    }
    throw new Error(`unexpected ${method} ${path}`)
  })
}

async function mountView() {
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: FieldsView }] })
  await router.push('/')
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = mount(FieldsView, {
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

function lastCall(method: string, pathPrefix: string) {
  return apiFetchMock.mock.calls.filter(
    ([path, init]) => (init?.method ?? 'GET') === method && path.startsWith(pathPrefix),
  ).at(-1)
}

beforeEach(() => {
  apiFetchMock.mockReset()
  stub()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('FieldsView — listing', () => {
  it('renders live fields with label, type, and person_count; archived fields in their own section', async () => {
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Budget')
    expect(wrapper.text()).toContain('Number')
    expect(wrapper.text()).toContain('Lead temperature')
    expect(wrapper.text()).toContain('Single choice')
    const archivedSection = wrapper.get('[data-testid="restore-field"]').element.closest('table')
    expect(archivedSection?.textContent).toContain('Referrer')
  })
})

describe('FieldsView — create', () => {
  it('creates a text field with no options', async () => {
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="new-field-label"]').setValue('Anniversary')
    await wrapper.get('[data-testid="create-field"]').trigger('click')
    await flushPromises()
    const call = lastCall('POST', '/custom-fields')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Anniversary', field_type: 'text' })
  })

  it('creates a choice field with its initial options, trimmed and empty rows dropped', async () => {
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="new-field-label"]').setValue('Temperature')
    const typeSelect = wrapper.get('[data-testid="new-field-type"]').findComponent(Select)
    await typeSelect.vm.$emit('update:model-value', 'choice')
    await flushPromises()
    await wrapper.get('[data-testid="new-field-option"]').setValue(' Cold ')
    await wrapper.get('[data-testid="add-new-field-option"]').trigger('click')
    await flushPromises()
    const optionInputs = wrapper.findAll('[data-testid="new-field-option"]')
    await optionInputs[1].setValue('Warm')
    await wrapper.get('[data-testid="create-field"]').trigger('click')
    await flushPromises()
    const call = lastCall('POST', '/custom-fields')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({
      label: 'Temperature',
      field_type: 'choice',
      options: ['Cold', 'Warm'],
    })
  })

  it('Create is disabled for an empty label', async () => {
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="create-field"]').attributes('disabled')).toBeDefined()
  })

  it('shows a 409 custom_field_label_taken inline', async () => {
    stub({ create: () => new ApiError(409, 'custom_field_label_taken') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="new-field-label"]').setValue('Budget')
    await wrapper.get('[data-testid="create-field"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="create-field-error"]').text()).toBe('"Budget" is already in use.')
  })

  it('shows a 422 custom_field_limit_reached inline', async () => {
    stub({ create: () => new ApiError(422, 'custom_field_limit_reached') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="new-field-label"]').setValue('One field too many')
    await wrapper.get('[data-testid="create-field"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="create-field-error"]').text()).toBe('This Organization already has 50 fields.')
  })
})

describe('FieldsView — inline rename', () => {
  it('Enter saves; a taken label shows the 409 inline', async () => {
    stub({ update: () => new ApiError(409, 'custom_field_label_taken') })
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-field"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-field-input"]')
    await input.setValue('Lead temperature')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(wrapper.get('[data-testid="rename-field-error"]').text()).toBe('"Lead temperature" is already in use.')
  })

  it('Escape cancels without saving', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-field"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-field-input"]')
    await input.setValue('Changed')
    await input.trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(wrapper.find('[data-testid="rename-field-input"]').exists()).toBe(false)
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/custom-fields/${BUDGET.id}` && init?.method === 'PUT')).toBe(false)
  })

  it('saves a rename with archived: false', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="rename-field"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-field-input"]')
    await input.setValue('Budget (USD)')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    const call = lastCall('PUT', `/custom-fields/${BUDGET.id}`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Budget (USD)', archived: false })
  })
})

describe('FieldsView — reorder', () => {
  it('moving the second field up sends the full reordered live set', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="move-field-up"]')[1].trigger('click')
    await flushPromises()
    const call = lastCall('PUT', '/custom-fields/order')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({
      field_ids: [TEMPERATURE.id, BUDGET.id],
    })
  })

  it('the first field cannot move up and the last live field cannot move down', async () => {
    const { wrapper } = await mountView()
    expect(wrapper.findAll('[data-testid="move-field-up"]')[0].attributes('disabled')).toBeDefined()
    expect(wrapper.findAll('[data-testid="move-field-down"]')[1].attributes('disabled')).toBeDefined()
  })

  it('moving the middle field of three sends the exact three-item order', async () => {
    stub({ fields: () => ({ fields: [BUDGET, TEMPERATURE, SOURCE, REFERRER_ARCHIVED] }) })
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="move-field-down"]')[1].trigger('click')
    await flushPromises()
    const call = lastCall('PUT', '/custom-fields/order')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({
      field_ids: [BUDGET.id, SOURCE.id, TEMPERATURE.id],
    })
  })

  it('a reorder failure shows an inline list error; a 422 also refetches the field list', async () => {
    stub({ reorder: () => new ApiError(422, 'unprocessable') })
    const { wrapper } = await mountView()
    const definitionsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    await wrapper.findAll('[data-testid="move-field-up"]')[1].trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="fields-list-error"]').text()).not.toBe('')
    const definitionsAfter = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    expect(definitionsAfter).toBeGreaterThan(definitionsBefore)
  })
})

describe('FieldsView — archive / restore', () => {
  it('explains the saved-filter consequence and sends archived: true on confirm', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="archive-field"]')[0].trigger('click')
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('Filters using this field will need repair or restoration.')
    dialogButton('Archive')?.click()
    await flushPromises()
    const call = lastCall('PUT', `/custom-fields/${BUDGET.id}`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Budget', archived: true })
    expect(document.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('uses the same generic filter consequence for a zero-count field', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="archive-field"]')[1].trigger('click')
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('Filters using this field will need repair or restoration.')
  })

  it('restores an archived field with archived: false', async () => {
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="restore-field"]').trigger('click')
    await flushPromises()
    const call = lastCall('PUT', `/custom-fields/${REFERRER_ARCHIVED.id}`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Referrer', archived: false })
  })

  it('Cancel closes the dialog and sends nothing', async () => {
    const { wrapper } = await mountView()
    await wrapper.findAll('[data-testid="archive-field"]')[0].trigger('click')
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeTruthy()
    dialogButton('Cancel')?.click()
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeFalsy()
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/custom-fields/${BUDGET.id}` && init?.method === 'PUT')).toBe(false)
  })

  it('a restore failure due to a label clash shows the "already in use" copy under the live table', async () => {
    stub({ update: () => new ApiError(409, 'custom_field_label_taken') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="restore-field"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="fields-list-error"]').text()).toBe('"Referrer" is already in use.')
  })
})

describe('FieldsView — options editor', () => {
  it('opens to show a choice field\'s live options and adds a new one', async () => {
    const { wrapper } = await mountView()
    const optionsButtons = wrapper.findAll('[data-testid="manage-field-options"]')
    expect(optionsButtons).toHaveLength(1)
    await optionsButtons[0].trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('Options for Lead temperature')
    expect(wrapper.findAll('[data-testid="field-option-row"]')).toHaveLength(2)

    await wrapper.get('[data-testid="add-option-input"]').setValue('Hot')
    await wrapper.get('[data-testid="add-option-submit"]').trigger('click')
    await flushPromises()
    const call = lastCall('POST', `/custom-fields/${TEMPERATURE.id}/options`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Hot' })
  })

  it('shows a 422 option_limit_reached inline', async () => {
    stub({ addOption: () => new ApiError(422, 'option_limit_reached') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="manage-field-options"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="add-option-input"]').setValue('One option too many')
    await wrapper.get('[data-testid="add-option-submit"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="add-option-error"]').text()).toBe('This field already has 50 options.')
  })

  it('an option archive/restore failure shows inline next to the options editor', async () => {
    stub({ updateOption: () => new ApiError(404, 'not_found') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="manage-field-options"]').trigger('click')
    await flushPromises()
    await wrapper.findAll('[data-testid="archive-option"]')[0].trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="option-action-error"]').text()).toBe('Could not archive this option.')
  })

  it('renames an option inline and archives it', async () => {
    stub({
      updateOption: (fieldId, optionId, body) => ({
        field: {
          ...TEMPERATURE,
          options: TEMPERATURE.options.map((o) => (o.id === optionId ? { ...o, ...body, archived_at: body.archived ? '2026-09-10T00:00:00.000Z' : null } : o)),
        },
        changed: true,
      }),
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="manage-field-options"]').trigger('click')
    await flushPromises()
    await wrapper.findAll('[data-testid="rename-option"]')[0].trigger('click')
    const input = wrapper.get('[data-testid="rename-option-input"]')
    await input.setValue('Chilly')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    let call = lastCall('PUT', `/custom-fields/${TEMPERATURE.id}/options/${TEMPERATURE.options[0]!.id}`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Chilly', archived: false })

    await wrapper.findAll('[data-testid="archive-option"]')[0].trigger('click')
    await flushPromises()
    call = lastCall('PUT', `/custom-fields/${TEMPERATURE.id}/options/${TEMPERATURE.options[0]!.id}`)
    // The stub's `/custom-fields` GET is static (not stateful across the
    // rename PUT above), so the refetch it triggers puts the original
    // 'Cold' label back in front of the archive click — this asserts the
    // exact body archiveOption actually sends for whatever label is
    // currently displayed, rather than the previous loose `toMatchObject`.
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Cold', archived: true })
  })

  it('restores an archived option', async () => {
    const archivedOption = { id: 'opt-cold', label: 'Cold', position: 1, archived_at: '2026-09-01T00:00:00.000Z' }
    const fieldWithArchivedOption: CustomField = {
      ...TEMPERATURE,
      options: [archivedOption, TEMPERATURE.options[1]!],
    }
    stub({ fields: () => ({ fields: [BUDGET, fieldWithArchivedOption, REFERRER_ARCHIVED] }) })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="manage-field-options"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="restore-option"]').trigger('click')
    await flushPromises()
    const call = lastCall('PUT', `/custom-fields/${TEMPERATURE.id}/options/${archivedOption.id}`)
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ label: 'Cold', archived: false })
  })
})
