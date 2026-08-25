<script setup lang="ts">
// UI_STYLE.md §10: "Unresolved — one table card (source, received,
// resolution badge, reason, size) with an empty state." SLICE_007e adds
// the admin-only workbench: rows become clickable for org admins and
// open the detail dialog (decrypt on demand, Try again, Discard —
// D-037). The member rendering is byte-identical to the 002-era table.
import { computed, h, ref } from 'vue'
import { Inbox } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import UnresolvedDetailDialog from '../components/UnresolvedDetailDialog.vue'
import { useMe, useUnresolved } from '../api/queries'
import type { UnresolvedItem, UnresolvedResolution } from '../api/types'
import { formatAbsoluteTime, formatBytes, formatRelativeTime } from '../lib/format'
import { describeApiError } from '../lib/errors'
import { UNRESOLVED_REASON_LABEL } from '../lib/labels'
import type { BadgeTint } from '../lib/controls'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

const isAdmin = computed(() => me.value?.organization?.role === 'admin')

const { data: unresolvedData, isPending, isError, error } = useUnresolved(orgId)
const items = computed(() => unresolvedData.value?.items ?? [])

const detailId = ref<string | null>(null)
const dialogOpen = computed({
  get: () => detailId.value !== null,
  set: (open: boolean) => {
    if (!open) detailId.value = null
  },
})

function openDetail(item: UnresolvedItem) {
  detailId.value = item.id
}

const RESOLUTION_TINT: Record<UnresolvedResolution, BadgeTint> = {
  unresolved: 'red',
  pending: 'neutral',
}

const RESOLUTION_LABEL: Record<UnresolvedResolution, string> = {
  unresolved: 'Unresolved',
  pending: 'Pending',
}

const columns: ColumnDef<UnresolvedItem>[] = [
  {
    id: 'source',
    header: 'Source',
    cell: (info) => h('span', { class: 'font-medium text-text' }, info.row.original.source),
  },
  {
    id: 'received_at',
    header: 'Received',
    cell: (info) => {
      const value = info.row.original.received_at
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
  {
    id: 'resolution',
    header: 'Resolution',
    cell: (info) => {
      const resolution = info.row.original.resolution
      return h(Badge, { tint: RESOLUTION_TINT[resolution] }, () => RESOLUTION_LABEL[resolution])
    },
  },
  {
    id: 'reason',
    header: 'Reason',
    cell: (info) => {
      const reason = info.row.original.reason
      return reason
        ? UNRESOLVED_REASON_LABEL[reason]
        : h('span', { class: 'text-text-muted' }, '—')
    },
  },
  {
    id: 'byte_len',
    header: 'Size',
    meta: { align: 'right' },
    cell: (info) => formatBytes(info.row.original.byte_len),
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Unresolved"
      subtitle="Leads that could not be matched to a contact method."
    />

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load the unresolved queue.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="items"
      :columns="columns"
      :row-key="(item) => item.id"
      :on-row-click="isAdmin ? openDetail : undefined"
      count-noun="unresolved leads"
      count-noun-singular="unresolved lead"
      :truncated="unresolvedData?.truncated ?? false"
      empty-title="No unresolved leads"
      empty-message="New leads that can't be matched to a contact method will show up here."
      :empty-icon="Inbox"
    />

    <UnresolvedDetailDialog
      v-if="detailId"
      :visible="dialogOpen"
      :org-id="orgId"
      :raw-payload-id="detailId"
      @update:visible="dialogOpen = $event"
    />
  </div>
</template>
