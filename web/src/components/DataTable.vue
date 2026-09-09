<script setup lang="ts" generic="TData extends object">
// Shared TanStack table. People uses D-045's inspector on plain activation;
// real hrefs preserve new-tab/context-menu behavior. Other tables keep their
// existing navigation or action callbacks. Counts come from returned rows.
//
// SLICE_011b_SORT.md §9: an optional server-driven sort. A column becomes a
// clickable header only when its `ColumnDef.meta.sortKey` is set (only
// People's Added/Name/Stage/Assignee columns do); every other consumer's
// columns are untouched and render exactly as before. No TanStack sorting
// row model is introduced — the server orders the rows; this component only
// renders the control and reports clicks via `update:sort`.
import { computed, type Component } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { FlexRender, getCoreRowModel, useVueTable, type Column, type ColumnDef } from '@tanstack/vue-table'
import { ArrowDown, ArrowUp } from 'lucide-vue-next'
import Card from './Card.vue'
import { buttonClasses } from '../lib/controls'

/** Generic so this component stays domain-agnostic; People's `PersonSort`
 * (`lib/sort.ts`) is structurally assignable — same `{key, direction}` shape. */
export interface TableSort {
  key: string
  direction: 'asc' | 'desc'
}

const props = defineProps<{
  data: TData[]
  columns: ColumnDef<TData>[]
  rowKey: (row: TData) => string
  rowTo?: (row: TData) => string
  /** Row click as an action (e.g. open a dialog) instead of a route.
   *  SLICE_007e: the Unresolved table's admin-only detail dialog. */
  onRowClick?: (row: TData) => void
  /** SLICE_014 §4: fired on `pointerenter`/`focusin` of a row — additive
   *  hover/focus-intent signal for a caller-owned prefetch dwell timer
   *  (PeopleView.vue). Optional and unused by every other DataTable
   *  consumer, which render exactly as before. */
  onRowIntent?: (row: TData) => void
  selectedRowKey?: string
  /** Noun for the footer count, e.g. "people", "unresolved leads". */
  countNoun: string
  /** Singular form used when the count is exactly 1, e.g. "person", "unresolved lead". Defaults to `countNoun`. */
  countNounSingular?: string
  truncated?: boolean
  /** The active server-applied sort. Omit on tables with no sortable columns. */
  sort?: TableSort
  /** Appended to the truncated-count copy, e.g. "by Name (A–Z)" (SLICE_011b_SORT.md
   *  §9). Consumers without a sort keep today's plain "Showing the first N — more exist." */
  truncatedSortLabel?: string
  emptyMessage: string
  /** Short headline above `emptyMessage`, e.g. "No people yet". */
  emptyTitle?: string
  emptyIcon?: Component
  emptyActionLabel?: string
  emptyActionTo?: string
}>()

const emit = defineEmits<{
  'update:sort': [value: TableSort]
}>()

const router = useRouter()

function sortKeyOf(column: Column<TData, unknown>): string | undefined {
  return column.columnDef.meta?.sortKey
}

/** The column's plain text label, reused as the accessible-name subject.
 * Every sortable People column declares a literal string `header` (Added,
 * Name, Stage, Assignee), so this never needs to render the header cell. */
function columnLabel(column: Column<TData, unknown>): string {
  const header = column.columnDef.header
  return typeof header === 'string' ? header : ''
}

function isActiveSort(column: Column<TData, unknown>): boolean {
  return sortKeyOf(column) !== undefined && sortKeyOf(column) === props.sort?.key
}

/** The direction a click on this header applies next: the column's declared
 * natural (first-click) direction, or the toggle of the current direction
 * when this column is already active (§9). */
function nextSortDirection(column: Column<TData, unknown>): 'asc' | 'desc' {
  if (isActiveSort(column)) return props.sort?.direction === 'asc' ? 'desc' : 'asc'
  return column.columnDef.meta?.sortNaturalDirection ?? 'asc'
}

function ariaSort(column: Column<TData, unknown>): 'ascending' | 'descending' | 'none' | undefined {
  if (sortKeyOf(column) === undefined) return undefined
  if (!isActiveSort(column)) return 'none'
  return props.sort?.direction === 'asc' ? 'ascending' : 'descending'
}

/** "Sort by <Column>, ascending|descending" — describes the direction a
 * click WILL apply next, not the column's current state (§9). */
function sortAccessibleName(column: Column<TData, unknown>): string {
  const direction = nextSortDirection(column) === 'asc' ? 'ascending' : 'descending'
  return `Sort by ${columnLabel(column)}, ${direction}`
}

function toggleSort(column: Column<TData, unknown>) {
  const key = sortKeyOf(column)
  if (key === undefined) return
  emit('update:sort', { key, direction: nextSortDirection(column) })
}

const table = useVueTable({
  get data() {
    return props.data
  },
  columns: props.columns,
  getCoreRowModel: getCoreRowModel(),
})

const rows = computed(() => table.getRowModel().rows)

function navigate(row: TData) {
  if (props.onRowClick) {
    props.onRowClick(row)
    return
  }
  if (!props.rowTo) return
  router.push(props.rowTo(row)).catch(() => {
    // Redundant navigation to the row's own already-open target (the
    // first cell's real <RouterLink> may have already triggered it) —
    // nothing to report.
  })
}

function clickRow(event: MouseEvent, row: TData) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  if (event.target instanceof Element && event.target.closest('a,button,input,select,textarea')) return
  navigate(row)
}

function clickLink(event: MouseEvent, row: TData) {
  event.stopPropagation()
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  event.preventDefault()
  navigate(row)
}
</script>

<template>
  <Card
    :padded="false"
    class="overflow-hidden"
  >
    <div
      v-if="data.length === 0"
      class="flex flex-col items-center px-5 py-16 text-center"
    >
      <div
        v-if="emptyIcon"
        class="mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-surface-2 text-text-muted"
      >
        <component
          :is="emptyIcon"
          class="h-5 w-5"
          stroke-width="1.5"
        />
      </div>
      <p
        v-if="emptyTitle"
        class="text-body font-medium text-text"
      >
        {{ emptyTitle }}
      </p>
      <p
        class="max-w-md text-body text-text-muted"
        :class="emptyTitle ? 'mt-1 text-small' : ''"
      >
        {{ emptyMessage }}
      </p>
      <RouterLink
        v-if="emptyActionLabel && emptyActionTo"
        :to="emptyActionTo"
        :class="buttonClasses('primary')"
        class="mt-4 inline-flex"
      >
        {{ emptyActionLabel }}
      </RouterLink>
    </div>

    <template v-else>
      <div class="overflow-x-auto">
        <table class="w-full border-collapse">
          <thead>
            <tr
              v-for="headerGroup in table.getHeaderGroups()"
              :key="headerGroup.id"
              class="h-12 bg-surface-1"
            >
              <th
                v-for="header in headerGroup.headers"
                :key="header.id"
                class="px-5 text-left align-middle text-small font-medium text-text-muted"
                :class="header.column.columnDef.meta?.align === 'right' ? 'text-right' : ''"
                :aria-sort="ariaSort(header.column)"
              >
                <button
                  v-if="!header.isPlaceholder && sortKeyOf(header.column) !== undefined"
                  type="button"
                  class="-mx-1 flex min-h-10 items-center gap-1 rounded-md px-1 text-small font-medium text-text-muted hover:text-text"
                  :aria-label="sortAccessibleName(header.column)"
                  @click="toggleSort(header.column)"
                >
                  <FlexRender
                    :render="header.column.columnDef.header"
                    :props="header.getContext()"
                  />
                  <component
                    :is="props.sort?.direction === 'asc' ? ArrowUp : ArrowDown"
                    v-if="isActiveSort(header.column)"
                    class="h-4 w-4 shrink-0"
                    stroke-width="1.5"
                    aria-hidden="true"
                  />
                </button>
                <FlexRender
                  v-else-if="!header.isPlaceholder"
                  :render="header.column.columnDef.header"
                  :props="header.getContext()"
                />
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="row in rows"
              :key="rowKey(row.original)"
              class="h-14 border-t border-border"
              :class="rowTo || onRowClick ? 'cursor-pointer hover:bg-surface-1' : ''"
              :data-selected="selectedRowKey === rowKey(row.original)"
              @click="clickRow($event, row.original)"
              @pointerenter="onRowIntent?.(row.original)"
              @focusin="onRowIntent?.(row.original)"
            >
              <td
                v-for="(cell, index) in row.getVisibleCells()"
                :key="cell.id"
                class="px-5 align-middle text-body text-text"
                :class="cell.column.columnDef.meta?.align === 'right' ? 'text-right tabular-nums' : ''"
              >
                <a
                  v-if="rowTo && index === 0"
                  :href="router.resolve(rowTo(row.original)).href"
                  class="flex min-h-10 items-center rounded-md"
                  :aria-haspopup="onRowClick ? 'dialog' : undefined"
                  @click="clickLink($event, row.original)"
                >
                  <FlexRender
                    :render="cell.column.columnDef.cell"
                    :props="cell.getContext()"
                  />
                </a>
                <button
                  v-else-if="onRowClick && index === 0"
                  type="button"
                  class="flex min-h-10 items-center text-left"
                  @click.stop="navigate(row.original)"
                >
                  <FlexRender
                    :render="cell.column.columnDef.cell"
                    :props="cell.getContext()"
                  />
                </button>
                <FlexRender
                  v-else
                  :render="cell.column.columnDef.cell"
                  :props="cell.getContext()"
                />
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="flex items-center justify-between border-t border-border px-5 py-3">
        <p class="text-small text-text-muted">
          {{ data.length }} {{ data.length === 1 ? (countNounSingular ?? countNoun) : countNoun }}
        </p>
        <p
          v-if="truncated"
          class="text-small text-text-muted"
        >
          Showing the first {{ data.length }}{{ truncatedSortLabel ? ` by ${truncatedSortLabel}` : '' }} — more exist.
        </p>
      </div>
    </template>
  </Card>
</template>
