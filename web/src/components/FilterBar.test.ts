import { DOMWrapper, flushPromises, mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import { afterEach, describe, expect, it } from 'vitest'
import FilterBar from './FilterBar.vue'
import type { FilterClause, Member, Stage } from '../api/types'

const STAGES: Stage[] = [
  { id: 'stage-1', name: 'Lead', position: 1 },
  { id: 'stage-2', name: 'Hot Prospect', position: 2 },
  { id: 'stage-3', name: 'Nurture', position: 3 },
]
function member(id: string, name: string, status: Member['status'] = 'active'): Member {
  return { user_id: id, display_name: name, status, role: 'member', email: `${id}@example.invalid`, joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 }
}
const MEMBERS = [member('user-1', 'Morgan Vale'), member('user-2', 'Riley North', 'inactive')]
type Props = Partial<InstanceType<typeof FilterBar>['$props']>
const wrappers: ReturnType<typeof mountBar>[] = []
function mountBar(clauses: FilterClause[] = [], extra: Props = {}) {
  const wrapper = mount(FilterBar, {
    props: {
      clauses, stages: STAGES, members: MEMBERS, sources: ['website', 'zillow'], ...extra,
      'onUpdate:clauses': (next: FilterClause[]) => { void wrapper.setProps({ clauses: next }) },
    },
    global: { plugins: [[PrimeVue, { unstyled: true }]], stubs: { transition: false } },
    attachTo: document.body,
  })
  return wrapper
}
function setup(clauses: FilterClause[] = [], extra: Props = {}) {
  const wrapper = mountBar(clauses, extra)
  wrappers.push(wrapper)
  return wrapper
}
const body = () => new DOMWrapper(document.body)
const get = (id: string) => body().get(`[data-testid="${id}"]`)
async function click(id: string) {
  await get(id).trigger('click')
  await flushPromises()
}
async function open(kind: string) {
  if (kind === 'stage' || kind === 'assigned_to') await click(`filter-trigger-${kind}`)
  else { await click('filter-add'); await click(`filter-add-${kind}`) }
}
async function check(id: string, checked: boolean) {
  await get(id).setValue(checked)
  await flushPromises()
}
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})

describe('FilterBar committed filters', () => {
  it('opens every field without emitting an arbitrary default', async () => {
    const wrapper = setup()
    for (const kind of ['stage', 'assigned_to', 'source', 'created', 'last_inquiry', 'last_contact', 'last_inbound', 'has_replied', 'has_phone', 'has_email']) {
      await open(kind)
      expect(get(`filter-editor-${kind}`).element).toBeDefined()
      expect(wrapper.emitted('update:clauses')).toBeUndefined()
      await click('filter-editor-done')
    }
  })

  it('keeps the editor open after the last value is removed, and appends the next selection', async () => {
    const wrapper = setup()
    await open('stage')
    await check('filter-option-stage-1', true)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'stage', stage_ids: ['stage-1'] }])
    await check('filter-option-stage-1', false)
    expect(wrapper.props('clauses')).toEqual([])
    expect(get('filter-editor-stage').element).toBeDefined()
    expect(body().find('[data-testid="filter-chip-stage"]').exists()).toBe(false)
    await check('filter-option-stage-2', true)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'stage', stage_ids: ['stage-2'] }])
  })

  it('recovers after the parent clears clauses under an open editor without duplicating the kind', async () => {
    const wrapper = setup([{ kind: 'stage', stage_ids: ['stage-1'] }])
    await open('stage')
    await wrapper.setProps({ clauses: [] })
    await check('filter-option-stage-2', true)
    await check('filter-option-stage-3', true)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'stage', stage_ids: ['stage-2', 'stage-3'] }])
  })

  it('preserves other clauses, symbolic Me and inactive members; supports chip removal and Clear all', async () => {
    const wrapper = setup([{ kind: 'stage', stage_ids: ['stage-1'] }])
    await open('assigned_to')
    const labels = get('filter-editor-assigned_to').findAll('label').map((label) => label.text())
    expect(labels.slice(1, 3)).toEqual(['Me', 'Unassigned'])
    expect(labels).toContain('Riley North (inactive)')
    await check('filter-assignee-me', true)
    await check('filter-assignee-user-user-2', true)
    expect(wrapper.props('clauses')).toEqual([
      { kind: 'stage', stage_ids: ['stage-1'] },
      { kind: 'assigned_to', assignees: ['me', { user_id: 'user-2' }] },
    ])
    await click('filter-editor-done')
    await click('filter-chip-remove-stage')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'assigned_to', assignees: ['me', { user_id: 'user-2' }] }])
    await click('filter-clear-all')
    expect(wrapper.props('clauses')).toEqual([])
  })

  it('searches field and option lists without changing selected filters, and renders StageLabel', async () => {
    const wrapper = setup([{ kind: 'stage', stage_ids: ['stage-1'] }])
    await click('filter-add')
    await get('filter-search').setValue('contact')
    expect(body().findAll('[data-testid^="filter-add-"]')).toHaveLength(1)
    expect(get('filter-add-last_contact').text()).toBe('Last contact attempt')
    await click('filter-add-last_contact')
    await click('filter-editor-done')
    await open('stage')
    await get('filter-option-search').setValue('HOT')
    expect(get('filter-editor-stage').findAll('input[type="checkbox"]')).toHaveLength(1)
    expect(get('filter-editor-stage').find('svg.lucide-flame').exists()).toBe(true)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'stage', stage_ids: ['stage-1'] }])
    await check('filter-option-stage-2', true)
    await get('filter-option-search').setValue('nothing')
    expect(get('filter-editor-stage').text()).toContain('No matching options.')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'stage', stage_ids: ['stage-1', 'stage-2'] }])
  })

  it('keeps missing selected members removable without exposing identifiers or dropping other assignees', async () => {
    const wrapper = setup([{ kind: 'assigned_to', assignees: ['me', { user_id: 'missing-private-id' }, { user_id: 'user-1' }] }])
    await open('assigned_to')
    expect(get('filter-assignee-user-missing-private-id').element).toHaveProperty('checked', true)
    expect(get('filter-editor-assigned_to').text()).toContain('Unknown member')
    expect(body().text()).not.toContain('missing-private-id')
    await check('filter-assignee-user-missing-private-id', false)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'assigned_to', assignees: ['me', { user_id: 'user-1' }] }])
    expect(body().find('[data-testid="filter-assignee-user-missing-private-id"]').exists()).toBe(false)
  })

  it('shows compact chips with full accessible labels and the Hot Prospect marker', () => {
    setup([{ kind: 'stage', stage_ids: STAGES.map((stage) => stage.id) }])
    const chip = get('filter-chip-stage')
    expect(chip.text()).toContain('+1')
    expect(chip.text()).not.toContain('Nurture')
    expect(chip.get('button').attributes('aria-label')).toBe('Edit Stage: Lead or Hot Prospect or Nurture')
    expect(chip.find('svg.lucide-flame').exists()).toBe(true)
  })

  it.each(['stage', 'assigned_to', 'source'] as const)('enforces 50 values for %s while allowing deselection', async (kind) => {
    const stages = Array.from({ length: 51 }, (_, i) => ({ id: `s${i}`, name: `Stage ${i}`, position: i }))
    const members = Array.from({ length: 51 }, (_, i) => member(`u${i}`, `Member ${i}`))
    const sources = Array.from({ length: 51 }, (_, i) => `source${i}`)
    const clause: FilterClause = kind === 'stage' ? { kind, stage_ids: stages.slice(0, 50).map((stage) => stage.id) }
      : kind === 'assigned_to' ? { kind, assignees: members.slice(0, 50).map((item) => ({ user_id: item.user_id })) }
        : { kind, sources: sources.slice(0, 50) }
    const wrapper = setup([clause], { stages, members, sources })
    await open(kind)
    const selectedId = kind === 'stage' ? 'filter-option-s0' : kind === 'assigned_to' ? 'filter-assignee-user-u0' : 'filter-option-source0'
    const extraId = kind === 'stage' ? 'filter-option-s50' : kind === 'assigned_to' ? 'filter-assignee-user-u50' : 'filter-option-source50'
    expect(get(extraId).attributes('disabled')).toBeDefined()
    expect(get(`filter-editor-${kind}`).text()).toContain('50 values selected')
    await check(selectedId, false)
    expect(get(extraId).attributes('disabled')).toBeUndefined()
    await check(extraId, true)
    expect(wrapper.props('clauses')).toHaveLength(1)
    expect(get(extraId).element).toHaveProperty('checked', true)
  })
})

describe('FilterBar time and boolean editors', () => {
  it('excludes Never for Created and commits a preset without changing other clauses', async () => {
    const wrapper = setup([{ kind: 'has_phone', value: false }])
    await open('created')
    expect(body().find('[data-testid="filter-age-op-created-never"]').exists()).toBe(false)
    expect(get('filter-days-created').element).toHaveProperty('value', '30')
    await click('filter-days-preset-14')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'has_phone', value: false }, { kind: 'created', age: { op: 'within_days', days: 14 } }])
    await click('filter-age-op-created-not_within_days')
    expect(get('filter-chip-created').text()).not.toContain('or never')
  })

  it('commits days only on blur/Enter and resynchronizes after an external replacement', async () => {
    const wrapper = setup([{ kind: 'last_contact', age: { op: 'not_within_days', days: 7 } }])
    await open('last_contact')
    const input = get('filter-days-last_contact')
    await input.setValue('45')
    expect(wrapper.emitted('update:clauses')).toBeUndefined()
    await input.trigger('blur')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_contact', age: { op: 'not_within_days', days: 45 } }])
    expect(get('filter-editor-last_contact').text()).toContain('Includes people with no recorded contact attempt.')
    await wrapper.setProps({ clauses: [{ kind: 'last_contact', age: { op: 'within_days', days: 90 } }] })
    expect(input.element).toHaveProperty('value', '90')
    await input.setValue('14')
    await input.trigger('keydown', { key: 'Enter' })
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_contact', age: { op: 'within_days', days: 14 } }])
  })

  it.each(['', '0', '-3', '7.5', '3651'])('explains invalid days %j without coercing or emitting', async (value) => {
    const clause: FilterClause = { kind: 'last_contact', age: { op: 'within_days', days: 30 } }
    const wrapper = setup([clause])
    await open('last_contact')
    await get('filter-days-last_contact').setValue(value)
    await get('filter-days-last_contact').trigger('blur')
    expect(wrapper.emitted('update:clauses')).toBeUndefined()
    expect(wrapper.props('clauses')).toEqual([clause])
    expect(get('filter-days-last_contact').attributes('aria-invalid')).toBe('true')
    expect(get('filter-editor-last_contact').get('[role="alert"]').text()).toContain('whole number from 1 to 3,650')
    await click('filter-age-op-last_contact-not_within_days')
    expect(wrapper.emitted('update:clauses')).toBeUndefined()
    await click('filter-days-preset-7')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_contact', age: { op: 'not_within_days', days: 7 } }])
    expect(get('filter-days-last_contact').attributes('aria-invalid')).toBe('false')
  })

  it('removes stale days on clearing and supports Never then a fresh window', async () => {
    const wrapper = setup([{ kind: 'last_inbound', age: { op: 'within_days', days: 90 } }])
    await open('last_inbound')
    await click('filter-clear-selection')
    expect(get('filter-editor-last_inbound').element).toBeDefined()
    expect(get('filter-days-last_inbound').element).toHaveProperty('value', '30')
    await click('filter-age-op-last_inbound-never')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_inbound', age: { op: 'never' } }])
    expect(body().find('[data-testid="filter-days-last_inbound"]').exists()).toBe(false)
    await click('filter-age-op-last_inbound-not_within_days')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_inbound', age: { op: 'not_within_days', days: 30 } }])
    expect(get('filter-chip-last_inbound').text()).toContain('(or never)')
  })

  it('flushes a valid typed duration on Done without a blur event', async () => {
    const wrapper = setup()
    await open('last_contact')
    await get('filter-days-last_contact').setValue('21')
    await click('filter-editor-done')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'last_contact', age: { op: 'within_days', days: 21 } }])
  })

  it('exposes Yes/No state and accurately describes received email', async () => {
    const wrapper = setup()
    await open('has_replied')
    expect(get('filter-bool-yes-has_replied').attributes('aria-pressed')).toBe('false')
    expect(get('filter-bool-no-has_replied').attributes('aria-pressed')).toBe('false')
    expect(get('filter-editor-has_replied').text()).toContain('whether answered or not')
    await click('filter-bool-no-has_replied')
    expect(wrapper.props('clauses')).toEqual([{ kind: 'has_replied', value: false }])
    expect(get('filter-bool-no-has_replied').attributes('aria-pressed')).toBe('true')
    expect(get('filter-chip-has_replied').text()).toContain('Received email: No')
  })
})

describe('FilterBar option feedback and keyboard', () => {
  it.each(['Escape', 'X'])('dismisses invalid days with %s and keeps the committed filter', async (action) => {
    const clause: FilterClause = { kind: 'last_contact', age: { op: 'within_days', days: 30 } }
    const wrapper = setup([clause])
    await open('last_contact')
    await get('filter-days-last_contact').setValue('3651')
    await get('filter-days-last_contact').trigger('blur')
    expect(get('filter-days-last_contact').attributes('aria-invalid')).toBe('true')
    if (action === 'Escape') {
      await get('filter-days-last_contact').trigger('keydown', { key: 'Escape', code: 'Escape' })
    } else {
      await body().get('button[aria-label="Close filter editor"]').trigger('click')
    }
    await flushPromises()
    expect(get('filter-add').attributes('aria-expanded')).toBe('false')
    expect(wrapper.props('clauses')).toEqual([clause])
    expect(wrapper.emitted('update:clauses')).toBeUndefined()
    await open('last_contact')
    expect(get('filter-days-last_contact').element).toHaveProperty('value', '30')
    expect(get('filter-days-last_contact').attributes('aria-invalid')).toBe('false')
  })

  it('distinguishes source loading, retry, empty, and truncated options', async () => {
    const wrapper = setup([], { sources: [], sourcesPending: true })
    await open('source')
    expect(get('filter-editor-source').text()).toContain('Loading sources')
    expect(get('filter-editor-source').text()).not.toContain('No sources available')
    await wrapper.setProps({ sourcesPending: false, sourcesError: true })
    const retry = get('filter-editor-source').findAll('button').find((button) => button.text() === 'Retry')!
    await retry.trigger('click')
    expect(wrapper.emitted('retry-options')).toEqual([['source']])
    await wrapper.setProps({ sourcesError: false })
    expect(get('filter-editor-source').text()).toContain('No sources available')
    await wrapper.setProps({ sources: ['zillow'], sourcesTruncated: true })
    expect(get('filter-editor-source').text()).toContain("latest inquiry's source")
    expect(get('filter-editor-source').text()).toContain('Search covers these options')
    await check('filter-option-zillow', true)
    expect(wrapper.props('clauses')).toEqual([{ kind: 'source', sources: ['zillow'] }])
  })

  it('names the dialog, focuses search, restores focus on Escape and closes outside', async () => {
    setup()
    await open('stage')
    const dialog = body().get('[role="dialog"]')
    expect(document.getElementById(dialog.attributes('aria-labelledby')!)?.textContent).toBe('Stage')
    expect(document.activeElement).toBe(get('filter-option-search').element)
    await get('filter-option-search').trigger('keydown', { key: 'Escape', code: 'Escape' })
    await flushPromises()
    expect(get('filter-trigger-stage').attributes('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(get('filter-trigger-stage').element)
    await open('assigned_to')
    await body().trigger('click')
    await flushPromises()
    expect(get('filter-trigger-assigned_to').attributes('aria-expanded')).toBe('false')
  })
})
