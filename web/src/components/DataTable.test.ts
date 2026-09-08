import { mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ColumnDef } from '@tanstack/vue-table'
import DataTable, { type TableSort } from './DataTable.vue'

interface Row {
  id: string
  name: string
  count: number
}

const rows: Row[] = [
  { id: '1', name: 'Ada', count: 3 },
  { id: '2', name: 'Grace', count: 1 },
]

// SLICE_011b_SORT.md §9: a column is sortable only when `meta.sortKey` is
// set. `name` is sortable (natural direction ascending, like People's Name);
// `count` is a plain column, matching every non-People DataTable consumer
// (Members, Today, Unresolved, Platform organizations) whose columns carry
// no `sortKey` at all.
const columns: ColumnDef<Row>[] = [
  { id: 'name', header: 'Name', meta: { sortKey: 'name', sortNaturalDirection: 'asc' }, cell: (info) => info.row.original.name },
  { id: 'count', header: 'Count', meta: { align: 'right' }, cell: (info) => String(info.row.original.count) },
]

const cleanups: Array<() => void> = []
afterEach(() => { cleanups.splice(0).forEach((cleanup) => cleanup()) })

interface MountOverrides {
  columns?: ColumnDef<Row>[]
  sort?: TableSort
  truncated?: boolean
  truncatedSortLabel?: string
  onRowIntent?: (row: Row) => void
}

async function mountTable(overrides: MountOverrides = {}) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
  })
  await router.push('/')
  const wrapper = mount(DataTable<Row>, {
    props: {
      data: rows,
      columns: overrides.columns ?? columns,
      rowKey: (row: Row) => row.id,
      countNoun: 'rows',
      emptyMessage: 'No rows',
      sort: overrides.sort,
      truncated: overrides.truncated,
      truncatedSortLabel: overrides.truncatedSortLabel,
      onRowIntent: overrides.onRowIntent,
    },
    global: { plugins: [router] },
  })
  cleanups.push(() => wrapper.unmount())
  return wrapper
}

describe('DataTable sortable headers (SLICE_011b_SORT.md §9, §11.12 item 12)', () => {
  it('renders a sort button only for columns with meta.sortKey', async () => {
    const wrapper = await mountTable()
    const headers = wrapper.findAll('th')
    const nameHeader = headers[0]!
    const countHeader = headers[1]!
    expect(nameHeader.find('button').exists()).toBe(true)
    expect(countHeader.find('button').exists()).toBe(false)
    expect(countHeader.text()).toBe('Count')
  })

  it('other consumers (no sort prop, no sortKey on any column) render exactly as before', async () => {
    const plainColumns: ColumnDef<Row>[] = [
      { id: 'name', header: 'Name', cell: (info) => info.row.original.name },
      { id: 'count', header: 'Count', meta: { align: 'right' }, cell: (info) => String(info.row.original.count) },
    ]
    const wrapper = await mountTable({ columns: plainColumns })
    expect(wrapper.findAll('th button')).toHaveLength(0)
    for (const th of wrapper.findAll('th')) {
      expect(th.attributes('aria-sort')).toBeUndefined()
    }
  })

  it('gives an inactive sortable header aria-sort="none" and its natural-direction accessible name', async () => {
    const wrapper = await mountTable()
    const nameHeader = wrapper.findAll('th')[0]!
    expect(nameHeader.attributes('aria-sort')).toBe('none')
    const button = nameHeader.get('button')
    expect(button.attributes('aria-label')).toBe('Sort by Name, ascending')
    expect(button.find('svg').exists()).toBe(false)
  })

  it('marks the active column aria-sort and shows a single monochrome arrow', async () => {
    const wrapper = await mountTable({ sort: { key: 'name', direction: 'asc' } })
    const nameHeader = wrapper.findAll('th')[0]!
    expect(nameHeader.attributes('aria-sort')).toBe('ascending')
    const button = nameHeader.get('button')
    // Active column: the next click toggles, so the accessible name now
    // describes descending.
    expect(button.attributes('aria-label')).toBe('Sort by Name, descending')
    const arrow = button.get('svg')
    expect(arrow.classes()).toContain('h-4')
    expect(arrow.classes()).toContain('w-4')

    await wrapper.setProps({ sort: { key: 'name', direction: 'desc' } })
    expect(nameHeader.attributes('aria-sort')).toBe('descending')
    expect(button.attributes('aria-label')).toBe('Sort by Name, ascending')
  })

  it('the sortable header button meets the 40px minimum target', async () => {
    const wrapper = await mountTable()
    const button = wrapper.find('th button')
    expect(button.classes()).toContain('min-h-10')
  })

  it('emits update:sort with the natural direction on first click, then toggles', async () => {
    const wrapper = await mountTable()
    await wrapper.get('th button').trigger('click')
    expect(wrapper.emitted('update:sort')?.[0]).toEqual([{ key: 'name', direction: 'asc' }])

    await wrapper.setProps({ sort: { key: 'name', direction: 'asc' } })
    await wrapper.get('th button').trigger('click')
    expect(wrapper.emitted('update:sort')?.[1]).toEqual([{ key: 'name', direction: 'desc' }])
  })

  it('is keyboard-operable: a real <button> element receives native Enter/Space activation', async () => {
    // happy-dom (this suite's test environment) does not synthesize a click
    // from a keydown Enter/Space on a <button>, unlike every real browser's
    // native "activation behavior" for interactive elements — so this
    // asserts the two things that together guarantee keyboard operability:
    // the control is a genuine <button type="button"> (native Enter/Space
    // activation applies with zero extra JS), and its one click handler
    // (the same one a real keyboard activation invokes) does the right thing.
    const wrapper = await mountTable()
    const button = wrapper.get('th button')
    expect(button.element.tagName).toBe('BUTTON')
    expect(button.attributes('type')).toBe('button')
    await button.trigger('click')
    expect(wrapper.emitted('update:sort')?.[0]).toEqual([{ key: 'name', direction: 'asc' }])
  })

  it('appends the sort label to the truncated cap message when provided, and stays plain otherwise', async () => {
    const withLabel = await mountTable({ truncated: true, truncatedSortLabel: 'Name (A–Z)' })
    expect(withLabel.text()).toContain('Showing the first 2 by Name (A–Z) — more exist.')

    const withoutLabel = await mountTable({ truncated: true })
    expect(withoutLabel.text()).toContain('Showing the first 2 — more exist.')
    expect(withoutLabel.text()).not.toContain(' by ')
  })
})

// SLICE_014 §4: additive hover/focus-intent signal for a caller-owned
// prefetch dwell timer (PeopleView.vue). Every other consumer omits the
// prop and renders exactly as before — no assertion needed beyond the
// existing suite above already passing unchanged.
describe('DataTable row intent (SLICE_014 §4)', () => {
  it('fires onRowIntent with the row on pointerenter', async () => {
    const onRowIntent = vi.fn()
    const wrapper = await mountTable({ onRowIntent })
    await wrapper.findAll('tbody tr')[0]!.trigger('pointerenter')
    expect(onRowIntent).toHaveBeenCalledTimes(1)
    expect(onRowIntent).toHaveBeenCalledWith(rows[0])
  })

  it('fires onRowIntent with the row on focusin', async () => {
    const onRowIntent = vi.fn()
    const wrapper = await mountTable({ onRowIntent })
    await wrapper.findAll('tbody tr')[1]!.trigger('focusin')
    expect(onRowIntent).toHaveBeenCalledTimes(1)
    expect(onRowIntent).toHaveBeenCalledWith(rows[1])
  })

  it('does nothing when onRowIntent is omitted (every other DataTable consumer)', async () => {
    const wrapper = await mountTable()
    await expect(wrapper.findAll('tbody tr')[0]!.trigger('pointerenter')).resolves.not.toThrow()
  })
})
