<script setup lang="ts">
// People filtering stays in the existing v1 URL/API representation.
// The page owns committed filters; FilterBar owns only its open editor.
import { computed, h, nextTick, ref, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { Plus, Users } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import PersonPreview from '../components/PersonPreview.vue'
import StageLabel from '../components/StageLabel.vue'
import FilterBar from '../components/FilterBar.vue'
import { useInquirySources, useMe, useMembers, usePeople, useStages } from '../api/queries'
import { ApiError } from '../api/client'
import type { FilterClause, PersonSummary } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime, initials } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { committedClauses, parseFilter, serializeFilter } from '../lib/filter'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

const stagesQuery = useStages(orgId)
const stages = computed(() => stagesQuery.data.value?.stages ?? [])
const membersQuery = useMembers(orgId)
const members = computed(() => membersQuery.data.value?.members ?? [])
const sourcesQuery = useInquirySources(orgId)
const sources = computed(() => sourcesQuery.data.value?.sources ?? [])

function retryOptions(kind: 'stage' | 'assigned_to' | 'source') {
  const query = kind === 'stage' ? stagesQuery : kind === 'assigned_to' ? membersQuery : sourcesQuery
  void query.refetch()
}

const route = useRoute()
const router = useRouter()
const clauses = ref<FilterClause[]>([])
const filterOrigin = ref<'url' | null>(null)
const editorRevision = ref(0)
const serializedFilter = computed(() => clauses.value.length > 0 ? serializeFilter(clauses.value) : undefined)
const hasFilter = computed(() => serializedFilter.value !== undefined)
let pendingWrite: { value: string | undefined } | null = null

function writeFilterParam(value: string | undefined) {
  // Compare the raw value: empty, bare, and repeated params still need
  // removal even though none represents an applied filter.
  // Even if the URL already matches, a newer clear must supersede an
  // earlier navigation that has not finished yet.
  if (value === route.query.filter && !pendingWrite) return
  const query = { ...route.query }
  if (value === undefined) delete query.filter
  else query.filter = value
  const write = { value }
  pendingWrite = write
  void router.replace({ query, hash: route.hash }).finally(() => {
    if (pendingWrite === write) pendingWrite = null
  })
}

function onClausesUpdate(next: FilterClause[]) {
  clauses.value = committedClauses(next)
  filterOrigin.value = null
  writeFilterParam(serializedFilter.value)
}

// Run before usePeople so a cached session and a shared URL issue only
// the filtered request. A router echo of our own edit keeps its origin;
// actual navigation (including to /people without a filter) restores it.
watch(
  () => route.query.filter,
  (raw) => {
    if (route.path !== '/people' || raw === serializedFilter.value) return
    const parsed = typeof raw === 'string' && raw !== '' ? parseFilter(raw) : null
    clauses.value = parsed ?? []
    filterOrigin.value = hasFilter.value ? 'url' : null
    editorRevision.value++
    if (!hasFilter.value) writeFilterParam(undefined)
  },
  { immediate: true },
)

const {
  data: peopleData, isPending, isFetching, isPlaceholderData, isError, error, refetch,
} = usePeople(orgId, serializedFilter)
const people = computed(() => peopleData.value?.people ?? [])

// Preserve the existing §6 policy: only a URL-origin 400/422 drops the
// filter. User-created filters and all 5xx failures remain recoverable.
const filterWillDegrade = computed(
  () => isError.value && filterOrigin.value === 'url' && hasFilter.value &&
    error.value instanceof ApiError && [400, 422].includes(error.value.status),
)
watch(filterWillDegrade, (degrade) => {
  if (degrade) {
    editorRevision.value++
    onClausesUpdate([])
  }
})

const resultLabel = computed(() => {
  if (isPlaceholderData.value) return people.value.length > 0
    ? 'Updating results… Previous results are shown below.'
    : 'Updating results…'
  if (isPending.value || filterWillDegrade.value) return 'Loading people…'
  if (!peopleData.value) return ''
  const count = people.value.length
  const noun = hasFilter.value ? (count === 1 ? 'match' : 'matches') : (count === 1 ? 'person' : 'people')
  return `${count}${peopleData.value.truncated ? '+' : ''} ${noun}`
})

const selectedId = ref('')
const preview = ref<InstanceType<typeof PersonPreview> | null>(null)
let previewTrigger: HTMLElement | null = null
const selectedPerson = computed(() => people.value.find((person) => person.id === selectedId.value))

function selectPerson(person: PersonSummary) {
  previewTrigger = document.querySelector<HTMLElement>(`a[href="/people/${CSS.escape(person.id)}"]`) ?? null
  selectedId.value = person.id
  void nextTick(() => preview.value?.focus())
}
function closePreview() {
  selectedId.value = ''
  void nextTick(() => {
    if (previewTrigger?.isConnected) previewTrigger.focus()
  })
}
watch([orgId, serializedFilter], () => { selectedId.value = '' }, { flush: 'sync' })
watch(selectedPerson, (person) => { if (!person) selectedId.value = '' })

const myPeople = computed(() => clauses.value.length === 1 &&
  clauses.value[0]?.kind === 'assigned_to' && clauses.value[0].assignees.length === 1 &&
  clauses.value[0].assignees[0] === 'me')

const columns: ColumnDef<PersonSummary>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const person = info.row.original
      return h('div', { class: 'flex min-w-[150px] items-center gap-2.5', title: person.primary_email ?? undefined }, [
        h('span', { class: 'avatar-surface h-7 w-7', 'aria-hidden': 'true' }, initials(person.display_name)),
        h('span', { class: 'text-body text-text' }, person.display_name),
      ])
    },
  },
  {
    id: 'stage',
    header: 'Stage',
    cell: (info) =>
      h(StageLabel, { stage: info.row.original.stage, badge: true }),
  },
  {
    id: 'assignee',
    header: 'Assignee',
    cell: (info) => {
      const assignee = info.row.original.assigned_user
      return assignee
        ? h('span', { class: 'text-text-muted' }, assignee.display_name)
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
  <div class="people-layout">
    <div class="people-list">
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
        class="people-tabs"
        aria-label="People views"
      >
        <button
          type="button"
          class="people-tab"
          :aria-pressed="!hasFilter"
          @click="onClausesUpdate([])"
        >
          All people
        </button>
        <button
          type="button"
          class="people-tab"
          :aria-pressed="myPeople"
          @click="onClausesUpdate([{ kind: 'assigned_to', assignees: ['me'] }])"
        >
          My people
        </button>
        <span
          v-if="hasFilter && !myPeople"
          class="flex items-center text-small text-text-muted"
        >Filtered view</span>
      </div>

      <div class="people-tools">
        <FilterBar
          :key="editorRevision"
          :clauses="clauses"
          :stages="stages"
          :members="members"
          :sources="sources"
          :stages-pending="stagesQuery.isPending.value"
          :stages-error="stagesQuery.isError.value"
          :members-pending="membersQuery.isPending.value"
          :members-error="membersQuery.isError.value"
          :sources-pending="sourcesQuery.isPending.value"
          :sources-error="sourcesQuery.isError.value"
          :sources-truncated="sourcesQuery.data.value?.truncated ?? false"
          @update:clauses="onClausesUpdate"
          @retry-options="retryOptions"
        />

        <div class="flex min-h-10 flex-col items-end justify-center gap-1 text-small text-text-muted">
          <span v-if="clauses.length > 1">Match all filters</span>
          <p
            data-testid="people-result-count"
            role="status"
            aria-live="polite"
          >
            {{ resultLabel }}
            <span v-if="peopleData?.truncated && !isPlaceholderData"> · Showing the first {{ people.length }}</span>
            <span v-if="isFetching && !isPending && !isPlaceholderData"> · Refreshing…</span>
          </p>
        </div>
      </div>

      <div
        v-if="isError && !filterWillDegrade"
        role="alert"
        class="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      >
        <span>{{ describeApiError(error, 'Could not load people.') }}</span>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="refetch()"
        >
          Try again
        </button>
      </div>
      <div
        v-if="((isPending || filterWillDegrade) && !peopleData) || (isPlaceholderData && people.length === 0)"
        class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
      >
        {{ isPlaceholderData ? 'Updating results…' : 'Loading…' }}
      </div>
      <div
        v-else-if="peopleData"
        :aria-busy="isFetching"
        :inert="isPlaceholderData"
        :class="{ 'opacity-60': isPlaceholderData }"
      >
        <DataTable
          class="people-table"
          :on-row-click="selectPerson"
          :selected-row-key="selectedId"
          :data="people"
          :columns="columns"
          :row-key="(person) => person.id"
          :row-to="(person) => `/people/${person.id}`"
          count-noun="people"
          count-noun-singular="person"
          :truncated="peopleData.truncated"
          :empty-title="hasFilter ? 'No people match these filters' : 'No people yet'"
          :empty-message="hasFilter ? 'Change or clear your filters to see more people.' : 'Leads you add or receive will appear here.'"
          :empty-icon="Users"
          :empty-action-label="hasFilter ? undefined : 'Add a lead'"
          :empty-action-to="hasFilter ? undefined : '/intake/new'"
        />
      </div>
    </div>
    <PersonPreview
      v-if="selectedPerson"
      :key="`${orgId}:${selectedPerson.id}`"
      ref="preview"
      :org-id="orgId"
      :summary="selectedPerson"
      @close="closePreview"
    />
  </div>
</template>
