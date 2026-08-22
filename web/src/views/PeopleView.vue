<script setup lang="ts">
// UI_STYLE.md §10: page header with "New lead" as the primary action, one
// card containing the table (name over primary email, stage badge,
// assignee, inquiry count, last inquiry, row click -> detail).
import { computed, h } from 'vue'
import { RouterLink } from 'vue-router'
import { Plus } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import { useMe, usePeople } from '../api/queries'
import type { PersonSummary } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization.id ?? '')

const { data: peopleData, isPending, isError, error } = usePeople(orgId)
const people = computed(() => peopleData.value?.people ?? [])

const columns: ColumnDef<PersonSummary>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const person = info.row.original
      return h('div', [
        h('p', { class: 'text-body font-medium text-text' }, person.display_name),
        person.primary_email
          ? h('p', { class: 'text-small text-text-muted' }, person.primary_email)
          : null,
      ])
    },
  },
  {
    id: 'stage',
    header: 'Stage',
    cell: (info) =>
      h(Badge, { tint: 'neutral' }, () => h(StageLabel, { stage: info.row.original.stage })),
  },
  {
    id: 'assignee',
    header: 'Assignee',
    cell: (info) => {
      const assignee = info.row.original.assigned_user
      return assignee
        ? h('span', { class: 'text-text' }, assignee.display_name)
        : h('span', { class: 'text-text-muted' }, 'Unassigned')
    },
  },
  {
    id: 'inquiry_count',
    header: 'Inquiries',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.inquiry_count),
  },
  {
    id: 'last_inquiry_at',
    header: 'Last inquiry',
    cell: (info) => {
      const value = info.row.original.last_inquiry_at
      if (!value) return h('span', { class: 'text-text-muted' }, '—')
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
]
</script>

<template>
  <div>
    <PageHeader title="People">
      <template #action>
        <RouterLink
          to="/intake/new"
          :class="buttonClasses('primary')"
        >
          <Plus
            class="h-4 w-4"
            stroke-width="1.5"
          />
          New lead
        </RouterLink>
      </template>
    </PageHeader>

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load people.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="people"
      :columns="columns"
      :row-key="(person) => person.id"
      :row-to="(person) => `/people/${person.id}`"
      count-noun="people"
      count-noun-singular="person"
      :truncated="peopleData?.truncated ?? false"
      empty-message="No people yet."
      empty-action-label="Add a lead"
      empty-action-to="/intake/new"
    />
  </div>
</template>
