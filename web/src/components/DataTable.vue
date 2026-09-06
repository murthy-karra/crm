<script setup lang="ts" generic="TData extends object">
// Shared TanStack table. People uses D-045's inspector on plain activation;
// real hrefs preserve new-tab/context-menu behavior. Other tables keep their
// existing navigation or action callbacks. Counts come from returned rows.
import { computed, type Component } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { FlexRender, getCoreRowModel, useVueTable, type ColumnDef } from '@tanstack/vue-table'
import Card from './Card.vue'
import { buttonClasses } from '../lib/controls'

const props = defineProps<{
  data: TData[]
  columns: ColumnDef<TData>[]
  rowKey: (row: TData) => string
  rowTo?: (row: TData) => string
  /** Row click as an action (e.g. open a dialog) instead of a route.
   *  SLICE_007e: the Unresolved table's admin-only detail dialog. */
  onRowClick?: (row: TData) => void
  selectedRowKey?: string
  /** Noun for the footer count, e.g. "people", "unresolved leads". */
  countNoun: string
  /** Singular form used when the count is exactly 1, e.g. "person", "unresolved lead". Defaults to `countNoun`. */
  countNounSingular?: string
  truncated?: boolean
  emptyMessage: string
  /** Short headline above `emptyMessage`, e.g. "No people yet". */
  emptyTitle?: string
  emptyIcon?: Component
  emptyActionLabel?: string
  emptyActionTo?: string
}>()

const router = useRouter()

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
              >
                <FlexRender
                  v-if="!header.isPlaceholder"
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
          Showing the first {{ data.length }} — more exist.
        </p>
      </div>
    </template>
  </Card>
</template>
