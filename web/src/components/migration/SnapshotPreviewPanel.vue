<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import Select from 'primevue/select'
import Card from '../Card.vue'
import { ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import {
  fetchSnapshotPreview, fetchSnapshotRecords, fetchSnapshotGroups, fetchSnapshotGroupMembers,
  retrySnapshotPreview, snapshotFamilies, type SnapshotFamily, type SnapshotPreviewRecord,
} from '../../api/snapshots'
import { buttonClasses, selectPt } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel, snapshotTime } from './format'

const props = defineProps<{ snapshotId: string; previewId: string; orgId: string; actorId: string; sessionLifetime: number }>()
const emit = defineEmits<{ refresh: []; accessDenied: [] }>()
const client = useQueryClient()
const prefix = computed(() => [...queryKeys.snapshots(props.orgId, props.actorId, props.sessionLifetime), 'preview-panel', props.snapshotId, props.previewId] as const)
const identity = computed(() => JSON.stringify(prefix.value))
const denied = ref(false)
const enabled = computed(() => !denied.value && !!props.orgId && !!props.actorId && !!props.snapshotId && !!props.previewId)
const family = ref<SnapshotFamily>('people')
const disposition = ref('')
const recordCursors = ref<Array<string | undefined>>([undefined])
const groupCursors = ref<Array<string | undefined>>([undefined])
const memberCursors = ref<Array<string | undefined>>([undefined])
const selectedRecordId = ref<string | null>(null)
const selectedGroupId = ref<string | null>(null)
const resumePending = ref(false)
const resumeError = ref<string | null>(null)
const uncertainResume = ref(false)
const pollFailures = ref(0)
let resumeRequestId: string | null = null
let generation = 0
let disposed = false

const familyOptions = snapshotFamilies.map(value => ({ value, label: snapshotLabel(value) }))
const dispositions = ['reviewable', 'needs_decision', 'unsupported_value', 'unresolved_reference']
const dispositionOptions = [{ value: '', label: 'All dispositions' }, ...dispositions.map(value => ({ value, label: snapshotLabel(value) }))]
const recordCursor = computed(() => recordCursors.value.at(-1))
const groupCursor = computed(() => groupCursors.value.at(-1))
const memberCursor = computed(() => memberCursors.value.at(-1))
const detailKey = computed(() => [...prefix.value, 'detail'])
const recordsKey = computed(() => [...prefix.value, 'records', family.value, disposition.value, recordCursor.value ?? ''])
const groupsKey = computed(() => [...prefix.value, 'groups', selectedRecordId.value, groupCursor.value ?? ''])
const membersKey = computed(() => [...prefix.value, 'members', selectedRecordId.value, selectedGroupId.value, memberCursor.value ?? ''])

function removeBranch(key: readonly unknown[]) {
  void client.cancelQueries({ queryKey: key })
  client.removeQueries({ queryKey: key })
}
function loseAccess() {
  denied.value = true
  generation += 1
  resumeError.value = null
  resumePending.value = false
  resetGroups()
  removeBranch(prefix.value)
  emit('accessDenied')
}
function accessError(error: unknown): boolean { return error instanceof ApiError && [401, 403].includes(error.status) }
async function scopedRead<T>(key: readonly unknown[], current: () => readonly unknown[], request: () => Promise<T>) {
  const tag = JSON.stringify(key)
  const expected = generation
  const currentResponse = () => !disposed && !denied.value && expected === generation && tag === JSON.stringify(current())
  try {
    const value = await request()
    if (!currentResponse()) throw new Error('Discarded stale preview response')
    return { tag, value }
  } catch (error) {
    if (!currentResponse()) throw new Error('Discarded stale preview response', { cause: error })
    if (accessError(error)) loseAccess()
    throw error
  }
}

const detailQuery = useQuery({
  queryKey: detailKey,
  queryFn: async ({ signal }) => {
    const expected = identity.value
    try {
      const result = await scopedRead(detailKey.value, () => detailKey.value, () => fetchSnapshotPreview(props.snapshotId, props.previewId, signal))
      if (expected === identity.value) pollFailures.value = 0
      return result
    } catch (error) {
      if (expected === identity.value && !denied.value && !signal.aborted) pollFailures.value += 1
      throw error
    }
  },
  enabled, retry: false, gcTime: 0, refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
  refetchInterval: (query) => {
    const state = query.state.data?.value.preview.state
    if (denied.value || (state && !['queued', 'running'].includes(state))) return false
    return Math.min(30_000, 2_000 * 2 ** Math.min(pollFailures.value, 4))
  },
})
const report = computed(() => !denied.value && detailQuery.data.value?.tag === JSON.stringify(detailKey.value) ? detailQuery.data.value.value : undefined)
const completed = computed(() => enabled.value && report.value?.preview.state === 'completed')
const recordsQuery = useQuery({
  queryKey: recordsKey,
  queryFn: ({ signal }) => scopedRead(recordsKey.value, () => recordsKey.value, () => fetchSnapshotRecords(props.snapshotId, props.previewId, family.value, disposition.value || undefined, recordCursor.value, signal)),
  enabled: completed, retry: false, gcTime: 0,
})
const recordPage = computed(() => !denied.value && recordsQuery.data.value?.tag === JSON.stringify(recordsKey.value) ? recordsQuery.data.value.value : undefined)
const groupsQuery = useQuery({
  queryKey: groupsKey,
  queryFn: ({ signal }) => scopedRead(groupsKey.value, () => groupsKey.value, () => fetchSnapshotGroups(props.snapshotId, props.previewId, selectedRecordId.value ?? undefined, groupCursor.value, signal)),
  enabled: computed(() => completed.value && !!selectedRecordId.value), retry: false, gcTime: 0,
})
const groupPage = computed(() => !denied.value && groupsQuery.data.value?.tag === JSON.stringify(groupsKey.value) ? groupsQuery.data.value.value : undefined)
const membersQuery = useQuery({
  queryKey: membersKey,
  queryFn: ({ signal }) => scopedRead(membersKey.value, () => membersKey.value, () => fetchSnapshotGroupMembers(props.snapshotId, props.previewId, selectedGroupId.value ?? '', memberCursor.value, signal)),
  enabled: computed(() => completed.value && !!selectedRecordId.value && !!selectedGroupId.value), retry: false, gcTime: 0,
})
const memberPage = computed(() => !denied.value && membersQuery.data.value?.tag === JSON.stringify(membersKey.value) ? membersQuery.data.value.value : undefined)

function resetMembers() { selectedGroupId.value = null; memberCursors.value = [undefined] }
function resetGroups() { selectedRecordId.value = null; groupCursors.value = [undefined]; resetMembers() }
watch([family, disposition], () => { recordCursors.value = [undefined]; resetGroups() }, { flush: 'sync' })
watch(recordCursor, resetGroups, { flush: 'sync' })
watch(groupCursor, resetMembers, { flush: 'sync' })
watch(identity, (_next, previous) => {
  generation += 1
  denied.value = false
  resumePending.value = false
  resumeError.value = null
  uncertainResume.value = false
  resumeRequestId = null
  pollFailures.value = 0
  family.value = 'people'
  disposition.value = ''
  recordCursors.value = [undefined]
  resetGroups()
  removeBranch(JSON.parse(previous) as unknown[])
}, { flush: 'sync' })
onBeforeUnmount(() => { disposed = true; generation += 1; resumeRequestId = null; removeBranch(prefix.value) })

async function resume() {
  if (!report.value?.preview.actions.includes('retry') || resumePending.value || denied.value) return
  const expected = identity.value
  const epoch = generation
  resumePending.value = true
  resumeError.value = null
  resumeRequestId ??= crypto.randomUUID()
  const requestId = resumeRequestId
  const current = () => !disposed && !denied.value && epoch === generation && expected === identity.value
  try {
    await retrySnapshotPreview(props.snapshotId, props.previewId, requestId)
    if (!current()) return
    resumeRequestId = null
    uncertainResume.value = false
    await detailQuery.refetch()
    if (current()) emit('refresh')
  } catch (error) {
    if (!current()) return
    if (accessError(error)) { loseAccess(); return }
    uncertainResume.value = !(error instanceof ApiError) || error.status === 0 || error.status >= 500
    if (!uncertainResume.value) resumeRequestId = null
    resumeError.value = uncertainResume.value
      ? 'The resume result could not be confirmed. Try Resume preview again to recover the same request.'
      : describeApiError(error, 'Could not resume this preview.')
  } finally {
    if (current()) resumePending.value = false
  }
}
function openGroups(record: SnapshotPreviewRecord) {
  if (record.family !== 'people') return
  resetGroups()
  selectedRecordId.value = record.id
}
function openMembers(groupId: string) { memberCursors.value = [undefined]; selectedGroupId.value = groupId }
function count(value: string | null | undefined): string { return value != null && /^\d+$/.test(value) ? value : 'Unknown' }
function countFor(name: string, value: string) { return count(report.value?.counts.records.find(r => r.family === name && r.disposition === value)?.count ?? '0') }
function invalidFor(name: string) { return count(report.value?.counts.invalid_ids.find(r => r.family === name)?.count ?? '0') }
function gapsFor(name: string) {
  const rows = report.value?.counts.streams.filter(r => r.family === name) ?? []
  if (!rows.length || rows.some(r => !/^\d+$/.test(r.content_gaps))) return 'Unknown'
  return rows.reduce((n, r) => n + BigInt(r.content_gaps), 0n).toString()
}
function displayValue(value: unknown): string {
  if (value === null || value === undefined) return 'Not provided'
  if (typeof value === 'string') return value
  return JSON.stringify(value, null, 2) ?? 'Not provided'
}
function projectionObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null
}
function displayedVariants(projection: unknown): unknown[] | null {
  const value = projectionObject(projection)
  return value?.display_limited === true && value.review_requires_decision === true && Array.isArray(value.variants) ? value.variants : null
}
function projectionNotice(projection: unknown): string {
  const variants = displayedVariants(projection)
  const values = variants ?? [projection]
  const notices: string[] = variants ? ['Only a limited set of source variants is shown.'] : []
  let rawPreserved = values.length > 0
  values.forEach((value, index) => {
    const metadata = projectionObject(projectionObject(value)?._snapshot)
    rawPreserved &&= metadata?.raw_capture_preserved_separately === true
    const prefix = variants ? `Displayed variant ${index + 1}: ` : ''
    const omitted = metadata?.omitted_fields
    if (typeof omitted === 'number' && Number.isSafeInteger(omitted) && omitted > 0) {
      notices.push(`${prefix}${omitted} source ${omitted === 1 ? 'field is' : 'fields are'} omitted from this preview.`)
    }
    if (Array.isArray(metadata?.flags) && metadata.flags.includes('projection_truncated')) {
      notices.push(`${prefix}Some field values are clipped or truncated for display.`)
    }
  })
  if (notices.length && rawPreserved) notices.push('Complete raw source bytes are preserved separately.')
  return notices.join(' ')
}
function fields(projection: unknown): Array<[string, unknown]> {
  const value = projectionObject(projection)
  if (value) {
    const variants = displayedVariants(projection)
    return Object.entries(value).filter(([name]) => name !== '_snapshot').map(([name, field]) => [name, name === 'variants' && variants ? variants.map(variant => Object.fromEntries(fields(variant))) : field])
  }
  return [['Captured value', projection]]
}
function fieldLabel(key: string) { return snapshotLabel(key.replace(/([a-z])([A-Z])/g, '$1 $2')) }
function title(record: SnapshotPreviewRecord): string {
  const projection = record.projection
  if (projection && typeof projection === 'object' && !Array.isArray(projection)) {
    if ('name' in projection && typeof projection.name === 'string' && projection.name.trim()) return projection.name
    if (record.family === 'people') {
      const names = ['firstName', 'lastName'].map(key => Reflect.get(projection, key)).filter((value): value is string => typeof value === 'string' && !!value.trim())
      if (names.length) return names.join(' ')
    }
  }
  return `${snapshotLabel(record.family)} source ${record.source_id}`
}
function candidateRows(record: SnapshotPreviewRecord) {
  return Object.entries(record.candidates).filter(([, id]) => typeof id === 'string')
}
function moreGroups(record: SnapshotPreviewRecord) { return /^\d+$/.test(record.overlap_group_count) && BigInt(record.overlap_group_count) > 0n }
</script>

<template>
  <Card
    class="min-w-0"
    data-testid="snapshot-preview-panel"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-section font-medium text-text">
          Migration preview
        </h2><p class="mt-1 text-small text-text-muted">
          Review captured source records before a future import.
        </p>
      </div>
      <button
        v-if="!denied"
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="detailQuery.isFetching.value"
        @click="detailQuery.refetch()"
      >
        Refresh preview
      </button>
    </div>
    <p
      v-if="denied"
      role="alert"
      class="mt-4 text-body text-danger"
    >
      Preview access is no longer available.
    </p>
    <template v-else>
      <p
        v-if="detailQuery.error.value"
        role="alert"
        class="mt-3 text-body text-danger"
      >
        {{ describeApiError(detailQuery.error.value, 'Could not load this preview.') }}
      </p>
      <p
        v-if="!report"
        role="status"
        class="mt-4 text-body text-text-muted"
      >
        {{ detailQuery.isFetching.value ? 'Loading preview…' : 'Refresh to try loading this preview again.' }}
      </p>
      <template v-else>
        <div class="mt-4 border-t border-border pt-4">
          <p
            role="status"
            aria-live="polite"
            class="text-body font-medium text-text"
          >
            {{ snapshotLabel(report.preview.state) }}<span v-if="report.preview.pause_reason"> · {{ snapshotLabel(report.preview.pause_reason) }}</span>
          </p>
          <p class="mt-2 text-body text-text-muted">
            The first import requires a new, empty Organization. This preview does not create or change CRM records.
          </p>
          <p
            v-if="report.destination_has_people"
            class="mt-2 text-body text-text-muted"
          >
            This Organization already contains People.
          </p>
          <p
            v-if="report.destination_stale"
            role="status"
            class="mt-2 text-body text-danger"
          >
            Destination configuration changed after this preview. Generate a new preview to compare the current configuration.
          </p>
          <dl class="mt-3 grid grid-cols-1 gap-x-6 gap-y-2 text-small sm:grid-cols-2">
            <div>
              <dt class="text-text-muted">
                Destination observed
              </dt><dd class="text-text">
                {{ snapshotTime(report.preview.input_observed_at) }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Captured through observation
              </dt><dd class="text-text">
                {{ count(report.preview.capture_sequence) }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Report created
              </dt><dd class="text-text">
                {{ snapshotTime(report.preview.created_at) }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Completed
              </dt><dd class="text-text">
                {{ snapshotTime(report.preview.completed_at) }}
              </dd>
            </div>
          </dl>
          <div
            v-if="report.preview.actions.includes('retry')"
            class="mt-4 flex flex-wrap items-center gap-3"
          >
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="resumePending"
              data-testid="resume-preview"
              @click="resume"
            >
              {{ resumePending ? 'Resuming preview…' : 'Resume preview' }}
            </button>
            <p class="text-small text-text-muted">
              Continues this saved report. No Follow Up Boss requests are made.
            </p>
          </div>
          <p
            v-if="resumeError"
            role="alert"
            class="mt-2 text-body text-danger"
          >
            {{ resumeError }}
          </p>
        </div>

        <section
          class="mt-5 border-t border-border pt-4"
          aria-label="Preview counts"
        >
          <h3 class="text-body font-medium text-text">
            Review by record family
          </h3>
          <p
            v-if="!completed"
            class="mt-1 text-small text-text-muted"
          >
            These counts cover preview pages completed so far. The report is still incomplete.
          </p>
          <div class="mt-3 overflow-x-auto">
            <table class="w-full min-w-[640px] text-left text-small">
              <thead class="text-text-muted">
                <tr>
                  <th class="py-2 pr-3 font-medium">
                    Family
                  </th><th
                    v-for="item in dispositions"
                    :key="item"
                    class="px-2 py-2 font-medium"
                  >
                    {{ snapshotLabel(item) }}
                  </th><th class="px-2 py-2 font-medium">
                    Invalid source IDs
                  </th><th class="px-2 py-2 font-medium">
                    Content retrieval gaps
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="name in snapshotFamilies"
                  :key="name"
                  class="border-t border-border text-text"
                >
                  <th class="py-3 pr-3 font-medium">
                    {{ snapshotLabel(name) }}
                  </th><td
                    v-for="item in dispositions"
                    :key="item"
                    class="px-2 py-3 tabular-nums"
                  >
                    {{ countFor(name, item) }}
                  </td><td class="px-2 py-3 tabular-nums">
                    {{ invalidFor(name) }}
                  </td><td class="px-2 py-3 tabular-nums">
                    {{ gapsFor(name) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <details class="mt-3">
            <summary class="min-h-10 cursor-pointer py-2 text-body text-text">
              Issue counts
            </summary><p class="mb-2 text-small text-text-muted">
              One record may have several issues, so these counts overlap.
            </p><ul
              v-if="report.counts.issues.length"
              class="divide-y divide-border text-small"
            >
              <li
                v-for="issue in report.counts.issues"
                :key="`${issue.family}:${issue.issue}`"
                class="flex flex-wrap justify-between gap-2 py-2"
              >
                <span>{{ snapshotLabel(issue.family) }} · {{ snapshotLabel(issue.issue) }}</span><span class="tabular-nums">{{ count(issue.count) }}</span>
              </li>
            </ul><p
              v-else
              class="text-small text-text-muted"
            >
              No issues recorded{{ completed ? '.' : ' on completed preview pages yet.' }}
            </p>
          </details>
          <details class="mt-2">
            <summary class="min-h-10 cursor-pointer py-2 text-body text-text">
              Source enumeration evidence
            </summary><div class="overflow-x-auto">
              <table class="w-full min-w-[580px] text-left text-small">
                <thead class="text-text-muted">
                  <tr>
                    <th class="py-2 font-medium">
                      Stream
                    </th><th class="px-2 font-medium">
                      Reported total
                    </th><th class="px-2 font-medium">
                      Returned items
                    </th><th class="px-2 font-medium">
                      Distinct family IDs
                    </th><th class="px-2 font-medium">
                      Captures
                    </th>
                  </tr>
                </thead><tbody>
                  <tr
                    v-for="stream in report.counts.streams"
                    :key="stream.stream"
                    class="border-t border-border"
                  >
                    <th class="py-2 text-left font-medium">
                      {{ snapshotLabel(stream.stream) }}
                    </th><td class="px-2">
                      {{ count(stream.reported_total) }}
                    </td><td class="px-2">
                      {{ count(stream.returned_items) }}
                    </td><td class="px-2">
                      {{ count(stream.distinct_ids) }}
                    </td><td class="px-2">
                      {{ count(stream.accepted_captures) }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div><p class="mt-2 text-small text-text-muted">
              Distinct IDs are counted per family; task partitions and note representations are not added together.
            </p>
          </details>
        </section>

        <section
          v-if="completed"
          class="mt-5 border-t border-border pt-4"
          aria-label="Captured record review"
        >
          <div class="flex flex-wrap gap-3">
            <div class="w-full sm:w-48">
              <label
                :for="`preview-family-${previewId}`"
                class="mb-1 block text-small text-text-muted"
              >Record family</label><Select
                v-model="family"
                :input-id="`preview-family-${previewId}`"
                :options="familyOptions"
                option-label="label"
                option-value="value"
                :pt="selectPt()"
                data-testid="preview-family"
              />
            </div><div class="w-full sm:w-64">
              <label
                :for="`preview-disposition-${previewId}`"
                class="mb-1 block text-small text-text-muted"
              >Disposition</label><Select
                v-model="disposition"
                :input-id="`preview-disposition-${previewId}`"
                :options="dispositionOptions"
                option-label="label"
                option-value="value"
                placeholder="All dispositions"
                :pt="selectPt()"
                data-testid="preview-disposition"
              />
            </div>
          </div>
          <p
            v-if="recordsQuery.error.value"
            role="alert"
            class="mt-3 text-body text-danger"
          >
            {{ describeApiError(recordsQuery.error.value, 'Could not load captured records.') }} <button
              type="button"
              :class="buttonClasses('ghost')"
              @click="recordsQuery.refetch()"
            >
              Retry records
            </button>
          </p>
          <p
            v-if="!recordPage && recordsQuery.isFetching.value"
            role="status"
            class="mt-3 text-body text-text-muted"
          >
            Loading records…
          </p>
          <p
            v-else-if="recordPage?.records.length === 0"
            class="mt-3 text-body text-text-muted"
          >
            No records match this selection.
          </p>
          <div
            class="mt-3 divide-y divide-border"
            data-testid="preview-records"
          >
            <details
              v-for="record in recordPage?.records ?? []"
              :key="record.id"
              class="py-3"
            >
              <summary class="min-h-10 cursor-pointer break-words py-2 text-body text-text">
                <span class="font-medium">{{ title(record) }}</span><span class="ml-2 text-small text-text-muted">Source {{ record.source_id }} · {{ snapshotLabel(record.disposition) }}</span>
              </summary>
              <p
                v-if="projectionNotice(record.projection)"
                class="mb-3 text-small text-text-muted"
                data-testid="projection-review-notice"
              >
                {{ projectionNotice(record.projection) }}
              </p>
              <ul
                v-if="record.issues.length"
                class="mb-3 list-inside list-disc text-small text-text-muted"
              >
                <li
                  v-for="issue in record.issues"
                  :key="issue"
                >
                  {{ snapshotLabel(issue) }}
                </li>
              </ul>
              <p
                v-if="candidateRows(record).length"
                class="mb-3 text-small text-text-muted"
              >
                Possible matches require review; they do not authorize an import.
              </p>
              <dl
                v-if="candidateRows(record).length"
                class="mb-3 text-small"
              >
                <div
                  v-for="[kind, value] in candidateRows(record)"
                  :key="kind"
                  class="flex flex-wrap gap-x-2"
                >
                  <dt class="text-text-muted">
                    {{ fieldLabel(kind) }}
                  </dt><dd class="break-all text-text">
                    {{ value }}
                  </dd>
                </div>
              </dl>
              <dl class="divide-y divide-border text-small">
                <div
                  v-for="[name, value] in fields(record.projection)"
                  :key="name"
                  class="grid grid-cols-1 gap-1 py-2 sm:grid-cols-[150px_minmax(0,1fr)] sm:gap-4"
                >
                  <dt class="text-text-muted">
                    {{ fieldLabel(name) }}
                  </dt><dd class="min-w-0">
                    <pre class="max-h-64 overflow-auto whitespace-pre-wrap break-all font-sans text-small text-text">{{ displayValue(value) }}</pre>
                  </dd>
                </div>
              </dl>
              <button
                v-if="record.family === 'people' && moreGroups(record)"
                type="button"
                :class="buttonClasses('ghost')"
                class="mt-2"
                :data-record-groups="record.id"
                @click="openGroups(record)"
              >
                Review {{ count(record.overlap_group_count) }} overlap groups
              </button>
              <section
                v-if="selectedRecordId === record.id"
                class="mt-3 border-t border-border pt-3"
                aria-label="Contact overlap groups"
              >
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <h4 class="text-body font-medium">
                    Shared contact keys
                  </h4><button
                    type="button"
                    :class="buttonClasses('ghost')"
                    @click="resetGroups"
                  >
                    Close groups
                  </button>
                </div>
                <p class="text-small text-text-muted">
                  A shared household or office contact does not establish a duplicate identity.
                </p>
                <p
                  v-if="groupsQuery.error.value"
                  role="alert"
                  class="mt-2 text-small text-danger"
                >
                  {{ describeApiError(groupsQuery.error.value, 'Could not load groups.') }} <button
                    type="button"
                    :class="buttonClasses('ghost')"
                    @click="groupsQuery.refetch()"
                  >
                    Retry groups
                  </button>
                </p>
                <p
                  v-if="!groupPage && groupsQuery.isFetching.value"
                  role="status"
                  class="mt-2 text-small text-text-muted"
                >
                  Loading groups…
                </p>
                <div
                  v-for="(group, index) in groupPage?.groups ?? []"
                  :key="group.id"
                  class="mt-2 border-t border-border py-2"
                >
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <p class="text-small">
                      {{ snapshotLabel(group.kind) }} · {{ count(group.member_count) }} distinct People
                    </p><button
                      type="button"
                      :class="buttonClasses('ghost')"
                      :aria-label="`Show members for ${group.kind} group ${index + 1}`"
                      :data-group-members="group.id"
                      @click="openMembers(group.id)"
                    >
                      Show members
                    </button>
                  </div>
                  <div
                    v-if="selectedGroupId === group.id"
                    class="mt-2"
                  >
                    <p
                      v-if="membersQuery.error.value"
                      role="alert"
                      class="text-small text-danger"
                    >
                      {{ describeApiError(membersQuery.error.value, 'Could not load group members.') }} <button
                        type="button"
                        :class="buttonClasses('ghost')"
                        @click="membersQuery.refetch()"
                      >
                        Retry members
                      </button>
                    </p>
                    <p
                      v-if="!memberPage && membersQuery.isFetching.value"
                      role="status"
                      class="text-small text-text-muted"
                    >
                      Loading members…
                    </p>
                    <ul class="grid grid-cols-1 gap-x-4 text-small sm:grid-cols-2">
                      <li
                        v-for="member in memberPage?.members ?? []"
                        :key="member.record_id"
                        class="break-all py-1"
                      >
                        Source Person {{ member.source_id }}
                      </li>
                    </ul>
                    <nav
                      class="mt-2 flex flex-wrap items-center gap-2"
                      aria-label="Group member pages"
                    >
                      <button
                        type="button"
                        :class="buttonClasses('ghost')"
                        :disabled="memberCursors.length === 1 || membersQuery.isFetching.value"
                        @click="memberCursors.pop()"
                      >
                        Previous members
                      </button><span class="text-small text-text-muted">Page {{ memberCursors.length }}</span><button
                        type="button"
                        :class="buttonClasses('ghost')"
                        :disabled="!memberPage?.next_cursor || membersQuery.isFetching.value"
                        @click="memberPage?.next_cursor && memberCursors.push(memberPage.next_cursor)"
                      >
                        Next members
                      </button>
                    </nav>
                  </div>
                </div>
                <nav
                  class="mt-2 flex flex-wrap items-center gap-2"
                  aria-label="Overlap group pages"
                >
                  <button
                    type="button"
                    :class="buttonClasses('ghost')"
                    :disabled="groupCursors.length === 1 || groupsQuery.isFetching.value"
                    @click="groupCursors.pop()"
                  >
                    Previous groups
                  </button><span class="text-small text-text-muted">Page {{ groupCursors.length }}</span><button
                    type="button"
                    :class="buttonClasses('ghost')"
                    :disabled="!groupPage?.next_cursor || groupsQuery.isFetching.value"
                    @click="groupPage?.next_cursor && groupCursors.push(groupPage.next_cursor)"
                  >
                    Next groups
                  </button>
                </nav>
              </section>
            </details>
          </div>
          <nav
            class="mt-3 flex flex-wrap items-center gap-2"
            aria-label="Captured record pages"
          >
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="recordCursors.length === 1 || recordsQuery.isFetching.value"
              @click="recordCursors.pop()"
            >
              Previous records
            </button><span class="text-small text-text-muted">Page {{ recordCursors.length }}</span><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!recordPage?.next_cursor || recordsQuery.isFetching.value"
              @click="recordPage?.next_cursor && recordCursors.push(recordPage.next_cursor)"
            >
              Next records
            </button>
          </nav>
        </section>

        <details class="mt-5 border-t border-border pt-3">
          <summary class="min-h-10 cursor-pointer py-2 text-body font-medium text-text">
            Coverage and remaining migration work
          </summary><p class="mt-1 text-small text-text-muted">
            Core capture is not a complete account export or a cutover decision.
          </p><dl class="mt-2 divide-y divide-border">
            <div
              v-for="item in report.coverage"
              :key="item.family"
              class="py-3"
            >
              <dt class="text-body font-medium text-text">
                {{ snapshotLabel(item.family) }} · {{ snapshotLabel(item.state) }}
              </dt><dd class="mt-1 text-small text-text-muted">
                {{ item.reason }} {{ item.next_action }}
              </dd>
            </div>
          </dl>
        </details>
      </template>
    </template>
  </Card>
</template>
