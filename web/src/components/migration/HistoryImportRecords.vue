<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { RouterLink } from 'vue-router'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import ImportedHistoryMetadata from './ImportedHistoryMetadata.vue'
import { ApiError } from '../../api/client'
import { fetchHistoryImportRecords, fetchHistoryImportResults, useHistoryImportAccess, type HistoryManifestItem, type HistoryResultItem } from '../../api/historyImports'
import { historyFamilies, historyLabel, type HistoryFamily } from '../../api/historyCaptures'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel } from './format'
const props = defineProps<{ importId: string; planId: string; currentRevision: string; mode: 'records' | 'results' }>()
const access = useHistoryImportAccess()
const family = ref<HistoryFamily | ''>('')
const disposition = ref('')
const cursors = ref<string[]>([''])
const epoch = ref(0)
const selected = ref('')
const needsRefresh = ref(false)
type Page = { rows: Array<HistoryManifestItem | HistoryResultItem>; next_cursor: string | null; plan_id: string; revision: string }
const firstPage = ref<Page | null>(null)
const seriesRevision = ref('')
const branch = computed(() => [...access.prefix.value, 'observations', props.importId, props.planId, props.mode])
const key = computed(() => [...branch.value, family.value, disposition.value, epoch.value, cursors.value.at(-1) ?? ''])
const reading = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false,
  queryFn: ({ signal }) => access.read(key.value, () => key.value, async () => {
    const first = cursors.value.length === 1
    if (first && firstPage.value) return firstPage.value
    const expected = JSON.stringify(key.value)
    const filters = { family: family.value || undefined, disposition: disposition.value || undefined }
    const result = props.mode === 'records' ? await fetchHistoryImportRecords(props.importId, filters, cursors.value.at(-1) || undefined, signal) : await fetchHistoryImportResults(props.importId, filters, cursors.value.at(-1) || undefined, signal)
    const page: Page = { rows: 'records' in result ? result.records : result.results, next_cursor: result.next_cursor, plan_id: result.plan_id, revision: result.revision }
    if (page.plan_id !== props.planId || page.revision !== props.currentRevision || !first && page.revision !== seriesRevision.value) throw new ApiError(409, 'history_refresh_required')
    if (!signal.aborted && expected === JSON.stringify(key.value) && access.enabled.value && first) { firstPage.value = page; seriesRevision.value = page.revision }
    return page
  }),
})
const page = computed(() => access.enabled.value && !needsRefresh.value && reading.data.value?.revision === props.currentRevision ? reading.data.value : undefined)
function refresh() { access.remove(branch.value); firstPage.value = null; seriesRevision.value = ''; cursors.value = ['']; selected.value = ''; needsRefresh.value = false; epoch.value++ }
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.mode, family.value, disposition.value]), refresh, { flush: 'sync' })
watch(() => props.mode, () => { disposition.value = '' }, { flush: 'sync' })
watch(() => props.currentRevision, value => { if (seriesRevision.value && value !== seriesRevision.value) { needsRefresh.value = true; selected.value = ''; firstPage.value = null } }, { flush: 'sync' })
watch(reading.error, error => { if (error instanceof ApiError && error.status === 409) { needsRefresh.value = true; firstPage.value = null; selected.value = '' } })
watch(() => cursors.value.at(-1), () => { selected.value = '' })
onBeforeUnmount(() => { firstPage.value = null; access.remove(branch.value) })
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="history-import-records"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-section font-medium">
        {{ mode === 'records' ? 'Planned historical records' : 'Historical import results' }}
      </h3>
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="!access.enabled.value || reading.isFetching.value"
        @click="refresh"
      >
        Refresh records
      </button>
    </div>
    <p class="mt-2 text-small text-text-muted">
      Each retained occurrence has an outcome. Equal repeats share one fact; held records remain retained. Metadata only.
    </p>
    <template v-if="access.enabled.value">
      <div class="mt-3 grid gap-3 sm:grid-cols-2">
        <FormField
          v-slot="{ id }"
          label="Imported history family"
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
          label="Historical outcome"
          bare
        >
          <select
            :id="id"
            v-model="disposition"
            :class="INPUT_CLASSES"
          >
            <option value="">
              All outcomes
            </option><option :value="mode === 'records' ? 'eligible' : 'imported'">
              {{ mode === 'records' ? 'Eligible' : 'Imported' }}
            </option><option
              v-if="mode === 'results'"
              value="already_imported"
            >
              Already imported
            </option><option value="equal_repeat">
              Equal repeat
            </option><option value="held">
              Held
            </option>
          </select>
        </FormField>
      </div>
      <p
        v-if="needsRefresh"
        role="status"
        class="mt-3 text-small"
      >
        Import progress changed. Refresh records to start a current page series.
      </p>
      <p
        v-if="reading.isFetching.value"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        Loading historical records…
      </p>
      <p
        v-if="reading.error.value && !needsRefresh"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(reading.error.value, 'Could not load historical records. Refresh to retry.') }}
      </p>
      <p
        v-if="page"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        {{ page.rows.length }} occurrences on this page · revision {{ page.revision }}
      </p>
      <ul
        v-if="page"
        class="mt-2 divide-y divide-border"
      >
        <li
          v-for="row in page.rows"
          :key="row.id"
          class="min-w-0 space-y-2 py-4"
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h4 class="break-words text-body font-medium">
              {{ historyLabel(row.family) }} · {{ row.metadata?.source_id ?? 'Source ID unavailable' }}
            </h4><span class="text-small">{{ snapshotLabel(row.disposition) }}</span>
          </div>
          <p
            v-if="row.reason"
            class="text-small text-text-muted"
          >
            {{ snapshotLabel(row.reason) }}
          </p>
          <p class="break-all text-small text-text-muted">
            Occurrence {{ row.position }} · capture {{ row.capture_id }} · ordinal {{ row.ordinal }}
          </p>
          <div class="flex flex-wrap gap-2">
            <RouterLink
              v-if="row.person_id"
              :to="`/people/${encodeURIComponent(row.person_id)}`"
              :class="buttonClasses('ghost')"
            >
              Review Person
            </RouterLink><button
              v-if="row.metadata"
              type="button"
              :class="buttonClasses('ghost')"
              :aria-expanded="selected === row.id"
              :aria-label="`Inspect historical metadata ${row.id}`"
              @click="selected = selected === row.id ? '' : row.id"
            >
              {{ selected === row.id ? 'Close metadata' : 'Inspect metadata' }}
            </button>
          </div>
          <ImportedHistoryMetadata
            v-if="selected === row.id && row.metadata"
            :metadata="row.metadata"
          />
          <p
            v-if="!row.metadata"
            class="text-small text-text-muted"
          >
            Metadata is unavailable or suppressed.
          </p>
        </li>
      </ul>
      <p
        v-if="page && !page.rows.length"
        class="mt-3 text-small text-text-muted"
      >
        No historical records match this page.
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || reading.isFetching.value || cursors.length < 2"
          @click="cursors.pop()"
        >
          Previous records
        </button><button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || reading.isFetching.value || !page?.next_cursor"
          @click="page?.next_cursor && cursors.push(page.next_cursor)"
        >
          More records
        </button>
      </div>
    </template>
  </Card>
</template>
