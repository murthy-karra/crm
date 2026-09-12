<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import { fetchHistoryRecord, fetchHistoryRecords, historyDispositions, historyFamilies, historyLabel, useHistoryAccess, type HistoryDisposition, type HistoryFamily, type HistoryRecordPage } from '../../api/historyCaptures'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotTime } from './format'
const props = defineProps<{ captureId: string; currentSequence: string }>()
const access = useHistoryAccess()
const family = ref<HistoryFamily | ''>('')
const disposition = ref<HistoryDisposition | ''>('')
const recordId = ref('')
const selected = ref('')
const cursors = ref<string[]>([''])
const epoch = ref(0)
// Only the first page lacks a server boundary cursor. Keep that one bounded
// response locally so Previous cannot accidentally open a newer page series.
const firstPage = ref<HistoryRecordPage | null>(null)
const branch = computed(() => [...access.prefix.value, 'records', props.captureId])
const key = computed(() => [...branch.value, family.value, disposition.value, recordId.value, epoch.value, cursors.value.at(-1) ?? ''])
const reading = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false,
  queryFn: ({ signal }) => access.read(key.value, () => key.value, async () => {
    const initial = cursors.value.length === 1; const scope = access.scope.value; const tag = JSON.stringify(key.value)
    if (initial && firstPage.value) return firstPage.value
    const page = await fetchHistoryRecords(props.captureId, { family: family.value || undefined, disposition: disposition.value || undefined, record_id: recordId.value || undefined }, cursors.value.at(-1) || undefined, signal)
    if (initial && !signal.aborted && access.enabled.value && scope === access.scope.value && tag === JSON.stringify(key.value)) firstPage.value = page
    return page
  }),
})
const page = computed(() => access.enabled.value ? reading.data.value : undefined)
const detailKey = computed(() => [...access.prefix.value, 'record-detail', props.captureId, selected.value])
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selected.value), retry: false, gcTime: 0,
  queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, () => fetchHistoryRecord(props.captureId, selected.value, signal)),
})
const row = computed(() => access.enabled.value ? detail.data.value : undefined)
function refresh() { access.remove(branch.value); firstPage.value = null; cursors.value = ['']; selected.value = ''; epoch.value++ }
function variants(id: string) { family.value = ''; disposition.value = ''; recordId.value = id }
watch(() => JSON.stringify([access.scope.value, props.captureId, family.value, disposition.value, recordId.value]), refresh, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selected.value = '' })
onBeforeUnmount(() => { firstPage.value = null; access.remove(branch.value); access.remove([...access.prefix.value, 'record-detail', props.captureId]) })
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="history-capture-records"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h2 class="text-section font-medium">
        Captured observations
      </h2><button
        type="button"
        :class="buttonClasses()"
        :disabled="!access.enabled.value || reading.isFetching.value"
        @click="refresh"
      >
        Refresh captured observations
      </button>
    </div>
    <p class="mt-2 text-small text-text-muted">
      Source metadata only. Bodies, subjects, recordings and media are not displayed. Source flags or returned fields do not prove full content or complete access.
    </p>
    <template v-if="access.enabled.value">
      <div class="mt-3 grid gap-3 sm:grid-cols-2">
        <FormField
          v-slot="{ id }"
          label="Historical family"
          bare
        >
          <select
            :id="id"
            v-model="family"
            :class="INPUT_CLASSES"
          >
            <option value="">
              All families
            </option><option
              v-for="value in historyFamilies"
              :key="value"
              :value="value"
            >
              {{ historyLabel(value) }}
            </option>
          </select>
        </FormField>
        <FormField
          v-slot="{ id }"
          label="Historical Person link status"
          bare
        >
          <select
            :id="id"
            v-model="disposition"
            :class="INPUT_CLASSES"
          >
            <option value="">
              All link statuses
            </option><option
              v-for="value in historyDispositions"
              :key="value"
              :value="value"
            >
              {{ historyLabel(value) }}
            </option>
          </select>
        </FormField>
      </div>
      <div
        v-if="recordId"
        class="mt-2 flex flex-wrap items-center gap-2 text-small"
      >
        <span>Showing observations of one source identity. Invalid IDs select only that observation.</span><button
          type="button"
          :class="buttonClasses('ghost')"
          @click="recordId = ''"
        >
          Show all captured identities
        </button>
      </div>
      <p
        v-if="page"
        role="status"
        class="mt-3 text-small"
      >
        {{ page.records.length }} observations on this page · fixed capture boundary {{ page.capture_sequence }}. Current capture progress: {{ currentSequence }}.
      </p>
      <p
        v-if="page && page.capture_sequence !== currentSequence"
        class="mt-1 text-small text-text-muted"
      >
        Capture progress changed. Refresh observations to start a new page series.
      </p>
      <p class="mt-1 text-small text-text-muted">
        Coverage counters above describe the current run. This page and its variant annotations use the fixed capture boundary.
      </p>
      <p
        v-if="reading.isFetching.value"
        role="status"
        class="mt-2 text-small"
      >
        Loading captured observations…
      </p>
      <p
        v-if="reading.error.value"
        role="alert"
        class="mt-2 text-small text-danger"
      >
        {{ describeApiError(reading.error.value, 'Could not load captured observations. Refresh to start a new series.') }}
      </p>
      <div
        v-if="page?.records.length"
        class="mt-3 max-w-full overflow-x-auto"
      >
        <table class="w-full text-left text-small">
          <caption class="sr-only">
            Captured historical source observations
          </caption><thead class="border-b border-border text-text-muted">
            <tr>
              <th class="p-2 font-medium">
                Source record
              </th><th class="p-2 font-medium">
                Parent link
              </th><th class="p-2 font-medium">
                Observations / variants
              </th><th class="p-2 font-medium">
                Review
              </th>
            </tr>
          </thead><tbody class="divide-y divide-border">
            <tr
              v-for="value in page.records"
              :key="value.id"
            >
              <td class="max-w-64 break-all p-2">
                {{ historyLabel(value.family) }} · {{ value.source_id ?? 'Invalid or missing ID' }}<p class="text-text-muted">
                  {{ value.source_event_type ?? value.source_outcome ?? value.source_kind }}
                </p>
              </td><td class="max-w-56 p-2">
                {{ historyLabel(value.disposition) }}<p class="break-all text-text-muted">
                  Source Person {{ value.source_person_id ?? 'not returned or invalid' }}
                </p><p
                  v-if="value.disposition === 'linked' && !value.reference_available"
                  class="text-text-muted"
                >
                  Parent Person is no longer available.
                </p>
              </td><td class="p-2">
                {{ value.observation_count }} / {{ value.variant_count }}<p
                  v-if="value.relationship_uncertain"
                  class="text-text-muted"
                >
                  Relationship uncertain
                </p>
              </td><td class="p-2">
                <div class="flex flex-wrap gap-2">
                  <button
                    type="button"
                    :class="buttonClasses('ghost')"
                    :aria-label="`Inspect historical observation ${value.id}`"
                    @click="selected = value.id"
                  >
                    Inspect metadata
                  </button><button
                    type="button"
                    :class="buttonClasses('ghost')"
                    :aria-label="`Show variants for historical observation ${value.id}`"
                    @click="variants(value.id)"
                  >
                    Show variants
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p
        v-else-if="page && !reading.isFetching.value"
        class="mt-3 text-small"
      >
        No observations match this page series. This does not establish an empty source history.
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="cursors.length < 2 || reading.isFetching.value"
          @click="cursors.pop()"
        >
          Previous captured observations
        </button><button
          type="button"
          :class="buttonClasses()"
          :disabled="!page?.next_cursor || reading.isFetching.value"
          @click="page?.next_cursor && cursors.push(page.next_cursor)"
        >
          More captured observations
        </button>
      </div>
      <section
        v-if="selected"
        class="mt-4 min-w-0 border-t border-border pt-3"
        aria-label="Historical observation metadata"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h3 class="text-body font-medium">
            Observation metadata
          </h3><button
            type="button"
            :class="buttonClasses('ghost')"
            @click="selected = ''"
          >
            Close historical metadata
          </button>
        </div>
        <p
          v-if="detail.isFetching.value"
          role="status"
          class="text-small"
        >
          Loading metadata…
        </p><p
          v-if="detail.error.value"
          role="alert"
          class="text-small text-danger"
        >
          {{ describeApiError(detail.error.value, 'Could not load metadata.') }}
        </p>
        <template v-if="row">
          <p class="mt-2 text-small text-text-muted">
            Latest retained detail at capture boundary {{ row.series_capture_sequence }}; it can include observations committed after the table series.
          </p>
          <dl class="mt-3 grid min-w-0 gap-3 text-small sm:grid-cols-2">
            <div>
              <dt class="text-text-muted">
                Source identity
              </dt><dd class="break-all">
                {{ historyLabel(row.family) }} · {{ row.source_id ?? 'Invalid or missing ID' }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Source users
              </dt><dd class="break-all">
                {{ row.source_user_ids.join(', ') || 'Not returned or invalid' }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Source created / updated
              </dt><dd class="break-all">
                {{ row.source_created ?? 'Unknown' }} / {{ row.source_updated ?? 'Unknown' }}
              </dd><dd v-if="row.source_timestamp_uncertain">
                Source timestamp is uncertain.
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Source kind / label
              </dt><dd class="break-all">
                {{ row.source_kind }} · {{ row.source_event_type ?? row.source_outcome ?? 'Not returned' }}
              </dd><dd v-if="row.preview_truncated">
                Metadata label abbreviated.
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Source duration / direction
              </dt><dd>{{ row.source_duration ?? 'Unknown' }} / {{ row.source_is_incoming == null ? 'Unknown' : row.source_is_incoming ? 'Incoming source flag' : 'Outgoing source flag' }}</dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Content field presence
              </dt><dd>{{ row.content_availability === 'returned_in_raw' ? 'A content field was returned in raw evidence; completeness is unknown.' : 'No known content field was returned; availability is unknown.' }}</dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Parent linkage
              </dt><dd>{{ historyLabel(row.disposition) }} · {{ row.reference_available ? 'Parent reference available' : 'No available parent reference' }}</dd><dd v-if="row.relationship_uncertain">
                Relationship is uncertain; no participant fan-out or contact matching is applied.
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Captured
              </dt><dd>{{ snapshotTime(row.captured_at) }} · sequence {{ row.capture_sequence }} · ordinal {{ row.ordinal }}</dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Retained provenance
              </dt><dd class="break-all">
                Observation {{ row.id }} · capture {{ row.capture_id }}
              </dd><dd class="break-all">
                {{ row.representation }} · {{ row.profile_version }} · parser {{ row.parser_version }}
              </dd><dd class="break-all">
                {{ row.schema_version }}
              </dd>
            </div>
            <div>
              <dt class="text-text-muted">
                Integrity / variants
              </dt><dd>Encrypted projection verified · {{ row.observation_count }} observations / {{ row.variant_count }} variants</dd>
            </div>
          </dl>
        </template>
      </section>
    </template>
    <p
      v-else
      role="status"
      class="mt-2 text-small"
    >
      A verified administrator session is required.
    </p>
  </Card>
</template>
