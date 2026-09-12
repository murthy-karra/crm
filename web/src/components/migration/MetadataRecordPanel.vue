<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import MetadataEvidence from './MetadataEvidence.vue'
import { fetchMetadataRecords, fetchMetadataResults, metadataName, useMetadataAccess, type MetadataPage, type MetadataRecord, type MetadataResult, type MetadataResultKind } from '../../api/metadataImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const props = defineProps<{ importId: string; planId: string; mode: 'plan' | 'results'; progressVersion: string }>()
const access = useMetadataAccess()
const cursors = ref<string[]>([''])
const disposition = ref('')
const kind = ref<MetadataResultKind | ''>('')
const selected = ref<MetadataRecord | MetadataResult | null>(null)
const branch = computed(() => [...access.prefix.value, 'records', props.importId, props.planId, props.mode])
const key = computed(() => [...branch.value, disposition.value, kind.value, cursors.value.at(-1) ?? '', props.progressVersion])
const enabled = computed(() => access.enabled.value && !!props.importId && !!props.planId)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read<MetadataPage<MetadataRecord | MetadataResult>>(key.value, () => key.value, () => props.mode === 'plan'
  ? fetchMetadataRecords(props.importId, props.planId, disposition.value || undefined, cursors.value.at(-1) || undefined, signal)
  : fetchMetadataResults(props.importId, kind.value || undefined, disposition.value || undefined, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => enabled.value ? reading.data.value : undefined)
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.mode, disposition.value, kind.value]), () => { cursors.value = ['']; selected.value = null }, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selected.value = null })
watch(() => props.mode, () => { disposition.value = ''; kind.value = '' }, { flush: 'sync' })
watch(page, value => { if (selected.value && value) selected.value = value.items.find(row => row.id === selected.value?.id) ?? null })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="metadata-records"
  >
    <h2 class="text-section font-semibold">
      {{ mode === 'plan' ? 'Planned Person metadata' : 'Import reconciliation' }}
    </h2>
    <p class="mt-1 text-small text-text-muted">
      {{ mode === 'plan' ? 'Review the exact source and proposed operations before confirming.' : 'Inspect each settled catalog or Person unit, including held and already-present data.' }}
    </p>
    <div
      v-if="enabled"
      class="mt-3 flex flex-wrap gap-3"
    >
      <FormField
        v-slot="{ id }"
        label="Metadata disposition"
        bare
      >
        <select
          :id="id"
          v-model="disposition"
          :class="INPUT_CLASSES"
        >
          <option value="">
            All dispositions
          </option><option
            v-for="value in (mode === 'plan' ? ['eligible', 'held'] : ['created', 'applied', 'already_present', 'held', 'not_supplied', 'source_null'])"
            :key="value"
            :value="value"
          >
            {{ snapshotLabel(value) }}
          </option>
        </select>
      </FormField>
      <FormField
        v-if="mode === 'results'"
        v-slot="{ id }"
        label="Result kind"
        bare
      >
        <select
          :id="id"
          v-model="kind"
          :class="INPUT_CLASSES"
        >
          <option value="">
            All kinds
          </option><option
            v-for="value in (['tag', 'field', 'option', 'people'] as const)"
            :key="value"
            :value="value"
          >
            {{ snapshotLabel(value) }}
          </option>
        </select>
      </FormField>
    </div>
    <p
      v-if="reading.isFetching.value && enabled"
      role="status"
      class="mt-3 text-small text-text-muted"
    >
      Loading metadata records…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load metadata records.') }}
    </p>
    <div
      v-if="page"
      class="mt-3 min-w-0 overflow-x-auto"
    >
      <table class="w-full min-w-[600px] text-left text-small">
        <thead class="text-text-muted">
          <tr>
            <th class="p-2 font-medium">
              Record
            </th><th class="p-2 font-medium">
              Result
            </th><th class="p-2 font-medium">
              Details
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in page.items"
            :key="row.id"
            class="border-t border-border align-top"
          >
            <td class="max-w-64 p-2">
              <p class="whitespace-pre-wrap break-all font-medium">
                {{ metadataName(row.source, 'Source record') }}
              </p>
              <p>{{ 'kind' in row ? snapshotLabel(row.kind) : 'Person' }}<span v-if="row.source_id"> · FUB {{ row.source_id }}</span></p><RouterLink
                v-if="row.person_id"
                :to="`/people/${encodeURIComponent(row.person_id)}`"
                class="mt-1 inline-block underline"
              >
                Open Person
              </RouterLink><p
                v-if="'committed_at' in row && row.committed_at"
                class="mt-1 text-text-muted"
              >
                {{ snapshotTime(row.committed_at) }}
              </p>
            </td>
            <td class="max-w-80 p-2">
              <p class="font-medium">
                {{ snapshotLabel(row.disposition) }}
              </p><ul class="mt-1 space-y-1">
                <li
                  v-for="reason in row.reasons"
                  :key="reason"
                >
                  {{ snapshotLabel(reason) }}
                </li>
              </ul><p
                v-if="'added_byte_bound' in row"
                class="mt-1 text-text-muted"
              >
                Added-data bound {{ formatBytes(row.added_byte_bound) }}
              </p>
            </td>
            <td class="p-2">
              <button
                type="button"
                :class="buttonClasses('secondary')"
                @click="selected = row"
              >
                Inspect {{ row.source_id ? `FUB ${row.source_id}` : snapshotLabel('kind' in row ? row.kind : 'person') }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p
        v-if="!page.items.length"
        class="py-3 text-small text-text-muted"
      >
        No metadata records match these filters.
      </p>
    </div>
    <div
      v-if="enabled"
      class="mt-3 flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous metadata records
      </button><button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next metadata records
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch()"
      >
        Refresh metadata records
      </button>
    </div>
    <MetadataEvidence
      v-if="selected && enabled"
      :source="selected.source"
      :operations="selected.operations"
      :request="mode === 'results' ? { kind: 'result', importId, resultId: selected.id, fieldKey: 'all' } : { kind: 'record', importId, planId, recordId: selected.id, fieldKey: 'all' }"
      title="Metadata source and operations"
    />
  </Card>
</template>
