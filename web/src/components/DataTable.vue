<script setup lang="ts" generic="TData extends object">
// Shared TanStack Table wrapper per docs/design/UI_STYLE.md §6 ("Sample 2
// is the reference"). No pagination this slice (spec §12 explicit
// exclusion) — the footer shows only a row count and a `truncated` notice.
//
// Row navigation (only when `rowTo` is given — the Unresolved queue has no
// detail view this slice, spec §12, so its rows are plain data): the row
// is a real link, not a button. The first cell's content is wrapped in an
// actual <RouterLink> (real tab order, Enter-to-activate, screen-reader
// "link" role, ctrl/cmd-click and right-click "open in new tab" all work
// natively). The rest of the row additionally forwards a plain click to
// the same destination as a mouse convenience — a harmless duplicate
// navigation to the row's own target.
import { computed } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { FlexRender, getCoreRowModel, useVueTable, type ColumnDef } from '@tanstack/vue-table'
import Card from './Card.vue'
import { buttonClasses } from '../lib/controls'

const props = defineProps<{
  data: TData[]
  columns: ColumnDef<TData>[]
  rowKey: (row: TData) => string
  rowTo?: (row: TData) => string
  /** Noun for the footer count, e.g. "people", "unresolved leads". */
  countNoun: string
  /** Singular form used when the count is exactly 1, e.g. "person", "unresolved lead". Defaults to `countNoun`. */
  countNounSingular?: string
  truncated?: boolean
  emptyMessage: string
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
  if (!props.rowTo) return
  router.push(props.rowTo(row)).catch(() => {
    // Redundant navigation to the row's own already-open target (the
    // first cell's real <RouterLink> may have already triggered it) —
    // nothing to report.
  })
}
</script>

<template>
  <Card
    :padded="false"
    class="overflow-hidden"
  >
    <div
      v-if="data.length === 0"
      class="px-5 py-16 text-center"
    >
      <p class="text-body text-text-muted">
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
              :class="rowTo ? 'cursor-pointer hover:bg-surface-1' : ''"
              @click="navigate(row.original)"
            >
              <td
                v-for="(cell, index) in row.getVisibleCells()"
                :key="cell.id"
                class="px-5 align-middle text-body text-text"
                :class="cell.column.columnDef.meta?.align === 'right' ? 'text-right tabular-nums' : ''"
              >
                <RouterLink
                  v-if="rowTo && index === 0"
                  :to="rowTo(row.original)"
                  tabindex="-1"
                  class="contents"
                >
                  <FlexRender
                    :render="cell.column.columnDef.cell"
                    :props="cell.getContext()"
                  />
                </RouterLink>
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
