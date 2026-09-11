import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { CustomField } from '../api/types'
import type { CustomFilterClause } from '../lib/filter'
import CustomFilterEditor from './CustomFilterEditor.vue'

const fieldId = '11111111-1111-1111-1111-111111111111'
const otherId = '22222222-2222-2222-2222-222222222222'
const warmId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'
const coldId = 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'
function field(field_type: CustomField['field_type']): CustomField {
  return { id: fieldId, label: 'Budget', field_type, position: 1, person_count: 2, archived_at: null, options: [] }
}

describe('custom filter draft editor', () => {
  it('opens without committing and cancels without changing the original clause', async () => {
    const clause: CustomFilterClause = { kind: 'custom_text', field_id: fieldId, test: { op: 'contains', text: 'Alice' } }
    const wrapper = mount(CustomFilterEditor, { props: { field: field('text'), clause } })
    expect(wrapper.emitted('apply')).toBeUndefined()
    await wrapper.get('[data-testid="custom-filter-text"]').setValue('Bob')
    await wrapper.get('[data-testid="custom-filter-cancel"]').trigger('click')
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('apply')).toBeUndefined()
    expect(clause.test).toEqual({ op: 'contains', text: 'Alice' })
  })

  it('reopens a maximum-only range and preserves its exact decimal spelling', async () => {
    const clause: CustomFilterClause = { kind: 'custom_number', field_id: fieldId, test: { op: 'range', max: '012.50' } }
    const wrapper = mount(CustomFilterEditor, { props: { field: field('number'), clause } })
    expect(wrapper.get<HTMLInputElement>('[data-testid="custom-filter-min"]').element.value).toBe('')
    expect(wrapper.get<HTMLInputElement>('[data-testid="custom-filter-max"]').element.value).toBe('012.50')
    await wrapper.get('[data-testid="custom-filter-apply"]').trigger('click')
    expect(wrapper.emitted('apply')).toEqual([[clause]])
  })

  it('resynchronizes a replaced clause and resets operands when switching fields', async () => {
    const wrapper = mount(CustomFilterEditor, { props: { field: field('text'), clause: { kind: 'custom_text', field_id: fieldId, test: { op: 'contains', text: 'Alice' } } as CustomFilterClause } })
    await wrapper.get('[data-testid="custom-filter-text"]').setValue('Uncommitted')
    await wrapper.setProps({ clause: { kind: 'custom_text', field_id: fieldId, test: { op: 'not_contains', text: 'Bob' } } })
    expect(wrapper.get<HTMLInputElement>('[data-testid="custom-filter-text"]').element.value).toBe('Bob')
    await wrapper.setProps({ field: { ...field('text'), id: otherId }, clause: undefined })
    expect(wrapper.find('[data-testid="custom-filter-text"]').exists()).toBe(false)
    await wrapper.get('[data-testid="custom-filter-operator"]').setValue('contains')
    expect(wrapper.get<HTMLInputElement>('[data-testid="custom-filter-text"]').element.value).toBe('')
    expect(wrapper.emitted('apply')).toBeUndefined()
  })

  it('blocks empty and reversed numeric drafts, then applies an exact valid bound with Enter', async () => {
    const wrapper = mount(CustomFilterEditor, { props: { field: field('number') } })
    await wrapper.get('[data-testid="custom-filter-operator"]').setValue('range')
    expect(wrapper.get<HTMLButtonElement>('[data-testid="custom-filter-apply"]').element.disabled).toBe(true)
    await wrapper.get('[data-testid="custom-filter-min"]').setValue('999999999999999.9999')
    await wrapper.get('[data-testid="custom-filter-max"]').setValue('999999999999999.9998')
    await wrapper.get('[data-testid="custom-filter-max"]').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toBeUndefined()
    expect(wrapper.get('[role="alert"]').text()).toContain('From')
    await wrapper.get('[data-testid="custom-filter-max"]').setValue('')
    expect(wrapper.get('[data-testid="custom-filter-min"]').attributes('inputmode')).toBe('decimal')
    await wrapper.get('[data-testid="custom-filter-min"]').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toEqual([[{ kind: 'custom_number', field_id: fieldId, test: { op: 'range', min: '999999999999999.9999' } }]])
  })

  it('uses calendar controls and rejects reversed dates without changing the committed clause', async () => {
    const clause: CustomFilterClause = { kind: 'custom_date', field_id: fieldId, test: { op: 'not_range', min: '2000-02-29', max: '2000-02-29' } }
    const wrapper = mount(CustomFilterEditor, { props: { field: field('date'), clause } })
    expect(wrapper.get('[data-testid="custom-filter-min"]').attributes('type')).toBe('date')
    await wrapper.get('[data-testid="custom-filter-min"]').setValue('2000-03-01')
    expect(wrapper.get<HTMLButtonElement>('[data-testid="custom-filter-apply"]').element.disabled).toBe(true)
    await wrapper.get('[data-testid="custom-filter-min"]').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toBeUndefined()
    await wrapper.get('[data-testid="custom-filter-min"]').setValue('2000-02-29')
    await wrapper.get('[data-testid="custom-filter-max"]').trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toEqual([[clause]])
  })

  it('accepts 500 Unicode characters and rejects padded text without trimming it', async () => {
    const wrapper = mount(CustomFilterEditor, { props: { field: field('text') } })
    await wrapper.get('[data-testid="custom-filter-operator"]').setValue('not_contains')
    const input = wrapper.get<HTMLInputElement>('[data-testid="custom-filter-text"]')
    // Native maxlength counts UTF-16 code units, unlike the 500-code-point contract.
    expect(input.attributes('maxlength')).toBeUndefined()
    await input.setValue(' padded ')
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toBeUndefined()
    await input.setValue('😀'.repeat(500))
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('apply')).toEqual([[{ kind: 'custom_text', field_id: fieldId, test: { op: 'not_contains', text: '😀'.repeat(500) } }]])
  })

  it.each(['text', 'number', 'date', 'choice'] as const)('offers explicit empty/not-empty selection for %s without operands', async (type) => {
    const wrapper = mount(CustomFilterEditor, { props: { field: field(type) } })
    expect(wrapper.text()).toContain('Is not empty')
    expect(wrapper.text()).toContain('Is empty')
    await wrapper.get('[data-testid="custom-filter-operator"]').setValue('is_not_set')
    await wrapper.get('[data-testid="custom-filter-apply"]').trigger('click')
    expect(wrapper.emitted('apply')).toEqual([[{ kind: `custom_${type}`, field_id: fieldId, test: { op: 'is_not_set' } }]])
  })

  it('searches live and archived choices while preserving selected option IDs', async () => {
    const choiceField = { ...field('choice'), options: [
      { id: coldId, label: 'Cold', position: 1, archived_at: null },
      { id: warmId, label: 'Warm', position: 2, archived_at: '2026-09-10T00:00:00Z' },
    ] }
    const wrapper = mount(CustomFilterEditor, { props: { field: choiceField } })
    await wrapper.get('[data-testid="custom-filter-operator"]').setValue('any_of')
    expect(wrapper.get<HTMLButtonElement>('[data-testid="custom-filter-apply"]').element.disabled).toBe(true)
    expect(wrapper.text()).toContain('Archived')
    await wrapper.get(`input[value="${coldId}"]`).setValue(true)
    await wrapper.get('[data-testid="custom-filter-choice-search"]').setValue('warm')
    expect(wrapper.find(`input[value="${coldId}"]`).exists()).toBe(false)
    await wrapper.get(`input[value="${warmId}"]`).setValue(true)
    await wrapper.get('[data-testid="custom-filter-apply"]').trigger('click')
    expect(wrapper.emitted('apply')).toEqual([[{ kind: 'custom_choice', field_id: fieldId, test: { op: 'any_of', option_ids: [coldId, warmId] } }]])
  })
})
