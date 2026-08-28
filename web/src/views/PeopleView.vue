<script setup lang="ts">
// UI_STYLE.md §10: page header with "New lead" as the primary action, one
// card containing the table (name over primary email, stage badge,
// assignee, inquiry count, last inquiry, row click -> detail).
//
// SLICE_011a §6 (amended by the adversarial-review fix round — R1/M2/M3):
// a FilterBar above the table. URL sync — mounting with `?filter=`
// present rehydrates the chips; back/forward navigation re-rehydrates too
// (M2); a chip edit `router.replace`s the query (PersonDetailView
// `?outcome=` precedent). DRAFT clauses (an empty value array — the state
// right after "Add filter" is clicked, before a value is picked) are
// NEVER serialized to the URL or the wire (§6 amended; `committedClauses`
// below) — only a committed filter can ever reach the server, so a fresh
// draft can never itself trigger a 400. A DECODABLE filter the server
// still rejects (400/422) degrades — drop, clear, refetch the plain list,
// no error toast (review F5) — but ONLY when that filter came from the
// URL (`filterOrigin`, amended §6): a user actively composing a filter
// through the FilterBar is never wiped out from under them, and a 5xx on
// any filtered fetch (URL-origin or not) NEVER degrades — chips and URL
// stay intact, error banner only (the F5 complement).
import { computed, h, onMounted, ref, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { Plus, Users } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import FilterBar from '../components/FilterBar.vue'
import { useInquirySources, useMe, useMembers, usePeople, useStages } from '../api/queries'
import { ApiError } from '../api/client'
import type { FilterClause, PersonSummary } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { committedClauses, parseFilter, serializeFilter } from '../lib/filter'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

const { data: stagesData } = useStages(orgId)
const stages = computed(() => stagesData.value?.stages ?? [])
const { data: membersData } = useMembers(orgId)
const members = computed(() => membersData.value?.members ?? [])
const { data: sourcesData } = useInquirySources(orgId)
const sources = computed(() => sourcesData.value?.sources ?? [])

const route = useRoute()
const router = useRouter()

const clauses = ref<FilterClause[]>([])
/** Set when `clauses` was last populated by reading the URL (mount or
 * back/forward); cleared the moment the user genuinely edits via
 * FilterBar (`onClausesUpdate`). Gates the 400/422 degrade path (amended
 * §6): only a URL-origin filter is ever auto-dropped. */
const filterOrigin = ref<'url' | null>(null)

/** What actually reaches the wire/URL — draft (empty-array) clauses are
 * filtered out (R1 fix, amended §6). */
const serializedFilter = computed(() => {
  const committed = committedClauses(clauses.value)
  return committed.length > 0 ? serializeFilter(committed) : undefined
})

function onClausesUpdate(next: FilterClause[]) {
  clauses.value = next
  filterOrigin.value = null
}

// --- URL <-> chips sync (M2/M3 rewrite) -------------------------------

/** The last value THIS component itself wrote to the URL — lets the
 * inbound watcher (M2) tell "the router just echoed our own
 * `router.replace`" apart from a genuine back/forward navigation,
 * without an infinite replace loop (the handled-flag pattern
 * PersonDetailView's `?outcome=` sync uses). */
const lastWrittenFilterParam = ref<string | undefined>(undefined)

function currentFilterParam(): string | undefined {
  const v = route.query.filter
  return typeof v === 'string' && v !== '' ? v : undefined
}

function writeFilterParam(value: string | undefined) {
  if (value === currentFilterParam()) return
  const query = { ...route.query }
  if (value === undefined) delete query.filter
  else query.filter = value
  lastWrittenFilterParam.value = value
  void router.replace({ query })
}

/**
 * Rehydrates `clauses` from one raw `route.query.filter` value — shared by
 * mount and the M2 back/forward watcher. Handles every edge (M3): truly
 * absent (`undefined`, the caller's job to skip calling this at all);
 * present-but-empty-or-repeated (not a string — vue-router represents a
 * bare `?filter` as `null` and a repeated `?filter=a&filter=b` as an
 * array, neither is `typeof … === 'string'`); undecodable JSON; and a
 * DECODABLE filter with zero clauses — all four canonicalize the URL to
 * "no `filter` param at all" (M3: previously only the undecodable case
 * cleared the param, so the other three left it lingering while the
 * legacy list quietly loaded underneath).
 */
function rehydrateFromUrlValue(raw: unknown) {
  if (typeof raw !== 'string' || raw === '') {
    clauses.value = []
    filterOrigin.value = null
    writeFilterParam(undefined)
    return
  }
  const parsed = parseFilter(raw)
  if (parsed === null || parsed.length === 0) {
    clauses.value = []
    filterOrigin.value = null
    writeFilterParam(undefined)
    return
  }
  clauses.value = parsed
  filterOrigin.value = 'url'
}

// Mount rehydrate (§6): an invalid/undecodable URL filter is dropped
// (chips empty, query param cleared, no error toast) — a shared broken
// link degrades to the plain People page.
onMounted(() => {
  const raw = route.query.filter
  if (raw === undefined) return // truly absent — nothing to rehydrate or clear
  rehydrateFromUrlValue(raw)
})

// M2: history navigation (back/forward) re-rehydrates chips from the URL
// too, not just mount — guarded against the outbound watcher's own
// `router.replace` echoing back through here (would otherwise loop).
watch(
  () => route.query.filter,
  (raw) => {
    const current = typeof raw === 'string' ? raw : undefined
    if (current === lastWrittenFilterParam.value) return
    if (raw === undefined) return // truly absent — nothing to rehydrate
    rehydrateFromUrlValue(raw)
  },
)

// Outbound sync: a genuine chip edit replaces the URL query (never a push
// — the PersonDetailView `?outcome=` precedent). No-op if nothing
// actually changed.
watch(serializedFilter, (value) => {
  writeFilterParam(value)
})

const { data: peopleData, isPending, isError, error } = usePeople(orgId, serializedFilter)
const people = computed(() => peopleData.value?.people ?? [])

/** True for exactly the one render frame where a URL-origin filtered
 * fetch has 400/422'd and the degrade-clear below is about to run —
 * suppressed from the error banner so the drop/clear/refetch never
 * flashes an error toast (§6, review F5: "no error toast"). Gated on
 * `filterOrigin === 'url'` (amended §6): a user-composed filter's 400/422
 * (which R1's draft-exclusion makes practically unreachable, but the gate
 * holds regardless) shows the ordinary error banner instead — its chips
 * are never auto-wiped. */
const filterWillDegrade = computed(
  () =>
    isError.value &&
    filterOrigin.value === 'url' &&
    committedClauses(clauses.value).length > 0 &&
    error.value instanceof ApiError &&
    (error.value.status === 400 || error.value.status === 422),
)

// A decodable-but-server-rejected URL-origin filter (400/422 on the
// rehydrated fetch — e.g. a link shared across Organizations carrying
// org-B stage ids) degrades IDENTICALLY to an invalid URL filter: drop,
// clear, refetch the plain list (§6, review F5). Never fires for a
// user-composed filter (`filterOrigin !== 'url'`) or for a 5xx (checked
// separately below, status 400/422 only) — both keep chips and URL intact.
watch([isError, error], ([failed, err]) => {
  if (!failed || filterOrigin.value !== 'url') return
  if (committedClauses(clauses.value).length === 0) return
  if (err instanceof ApiError && (err.status === 400 || err.status === 422)) {
    clauses.value = []
    filterOrigin.value = null
  }
})

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

    <FilterBar
      :clauses="clauses"
      :stages="stages"
      :members="members"
      :sources="sources"
      @update:clauses="onClausesUpdate"
    />

    <div
      v-if="isError && !filterWillDegrade"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load people.') }}
    </div>
    <div
      v-else-if="isPending || filterWillDegrade"
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
      empty-title="No people yet"
      empty-message="Leads you add or receive will appear here."
      :empty-icon="Users"
      empty-action-label="Add a lead"
      empty-action-to="/intake/new"
    />
  </div>
</template>
