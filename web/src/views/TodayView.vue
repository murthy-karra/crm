<script setup lang="ts">
// SLICE_003 §1, §10: the new landing route. One table card: Name (over
// primary contact), Reasons (neutral badges, wire order — §3's fixed
// order, never re-sorted), Priority (text weight, not color), Waiting
// (relative; absolute in tooltip), Recommended (Call + phone / Email +
// address), per-row secondary "Log contact". SLICE_006c §5a (D-033): a
// `low` item — the viewer's own call with no chosen outcome — reads
// "Outcome needed" / "Low" and its action is "Set outcome", which opens the
// Person page with `?outcome=<call_id>` (the Set-outcome dialog). Server
// order is the only order (§3: `rank()` preserves SQL order; `low` arrives
// last) — this view never sorts `items` itself.
import { computed, h, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Mail, Phone, PhoneOutgoing, Sun } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import LogContactDialog from '../components/LogContactDialog.vue'
import { useMe, useToday } from '../api/queries'
import type { TodayItem, TodayReason } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const router = useRouter()
const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

const { data: todayData, dataUpdatedAt, isPending, isError, error } = useToday(orgId)
const items = computed(() => todayData.value?.items ?? [])

const subtitle = computed(() => {
  if (isPending.value || isError.value) return undefined
  const count = items.value.length
  const need = count === 1 ? 'needs' : 'need'
  return `Updated ${formatRelativeTime(dataUpdatedAt.value)} · ${count} ${need} a response`
})

function reasonLabel(reason: TodayReason): string {
  switch (reason.code) {
    case 'new_inquiry':
      return 'New inquiry'
    case 'no_contact_attempt':
      return 'No contact attempt'
    case 'repeat_inquiry':
      return `Inquired again (${reason.inquiry_count})`
    case 'call_outcome_needed':
      return 'Outcome needed'
  }
}

const PRIORITY_LABEL: Record<TodayItem['priority'], string> = { high: 'High', normal: 'Normal', low: 'Low' }

/** The `call_outcome_needed` reason on a `set_outcome` item (§5a: present
 * whenever the action is `set_outcome`; null otherwise). */
function outcomeNeeded(item: TodayItem): { call_id: string; ended_at: string } | null {
  if (item.recommended_action !== 'set_outcome') return null
  const reason = item.reasons.find((r) => r.code === 'call_outcome_needed')
  return reason && reason.code === 'call_outcome_needed' ? reason : null
}

function setOutcomeTo(item: TodayItem, callId: string): string {
  return `/people/${item.person.id}?outcome=${encodeURIComponent(callId)}`
}

const logContactTarget = ref<{ personId: string; personName: string } | null>(null)

function openLogContact(item: TodayItem) {
  logContactTarget.value = { personId: item.person.id, personName: item.person.display_name }
}

function onDialogVisibleChange(value: boolean) {
  if (!value) logContactTarget.value = null
}

const columns: ColumnDef<TodayItem>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const person = info.row.original.person
      const contact = person.primary_email ?? person.primary_phone
      return h('div', [
        h('p', { class: 'text-body font-medium text-text' }, person.display_name),
        contact ? h('p', { class: 'text-small text-text-muted' }, contact) : null,
      ])
    },
  },
  {
    id: 'reasons',
    header: 'Reasons',
    cell: (info) =>
      h(
        'div',
        { class: 'flex flex-wrap gap-1.5' },
        info.row.original.reasons.map((reason) =>
          h(Badge, { key: reason.code, tint: 'neutral' }, () => reasonLabel(reason)),
        ),
      ),
  },
  {
    id: 'priority',
    header: 'Priority',
    cell: (info) => {
      const priority = info.row.original.priority
      return h('span', { class: priority === 'high' ? 'font-semibold text-text' : 'text-text-muted' }, PRIORITY_LABEL[priority])
    },
  },
  {
    id: 'waiting_since',
    header: 'Waiting',
    cell: (info) => {
      const value = info.row.original.waiting_since
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
  {
    id: 'recommended_action',
    header: 'Recommended',
    cell: (info) => {
      const item = info.row.original
      const needed = outcomeNeeded(item)
      if (needed) {
        return h('div', { class: 'flex items-center gap-1.5', 'data-testid': 'today-set-outcome' }, [
          h(PhoneOutgoing, { class: 'h-4 w-4 shrink-0 text-text-muted', 'stroke-width': 1.5 }),
          h('span', { class: 'text-text' }, 'Set outcome'),
          h('span', { class: 'text-text-muted', title: formatAbsoluteTime(needed.ended_at) }, `Call ${formatRelativeTime(needed.ended_at)} has no outcome yet`),
        ])
      }
      const isCall = item.recommended_action === 'call'
      const detail = isCall ? item.person.primary_phone : item.person.primary_email
      return h('div', { class: 'flex items-center gap-1.5' }, [
        h(isCall ? Phone : Mail, { class: 'h-4 w-4 shrink-0 text-text-muted', 'stroke-width': 1.5 }),
        h('span', { class: 'text-text' }, isCall ? 'Call' : 'Email'),
        detail ? h('span', { class: 'text-text-muted' }, detail) : null,
      ])
    },
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const item = info.row.original
      const needed = outcomeNeeded(item)
      if (needed) {
        return h(
          'button',
          {
            type: 'button',
            class: buttonClasses('secondary'),
            'data-testid': 'today-set-outcome-action',
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              void router.push(setOutcomeTo(item, needed.call_id))
            },
          },
          'Set outcome',
        )
      }
      return h(
        'button',
        {
          type: 'button',
          class: buttonClasses('secondary'),
          onClick: (event: MouseEvent) => {
            // Rows are links (DataTable.vue) — stop the click from also
            // bubbling into the row's own navigate() (SLICE_003 §10).
            event.stopPropagation()
            openLogContact(item)
          },
        },
        'Log contact',
      )
    },
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Today"
      :subtitle="subtitle"
    />

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load Today.') }}
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
      :row-key="(item) => item.person.id"
      :row-to="(item) => `/people/${item.person.id}`"
      count-noun="items"
      count-noun-singular="item"
      :truncated="todayData?.truncated ?? false"
      empty-title="You're all caught up"
      empty-message="Nothing assigned to you is waiting for a response."
      :empty-icon="Sun"
    />

    <LogContactDialog
      v-if="logContactTarget"
      :visible="logContactTarget !== null"
      :org-id="orgId"
      :person-id="logContactTarget.personId"
      :person-name="logContactTarget.personName"
      @update:visible="onDialogVisibleChange"
    />
  </div>
</template>
