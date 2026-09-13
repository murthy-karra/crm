<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import { coreChangeDispositions, coreChangeFamilies, coreChangeLabel, coreChangeMeaning, fetchCoreChangeRow, fetchCoreChangeRows, useCoreChangeAccess, type CoreChangeDisposition, type CoreChangeFamily } from '../../api/coreChangeReports'
import { importAccessError } from '../../api/imports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'

const props = defineProps<{ reportId: string; outputRevision: string; sourceScope: string }>()
const emit = defineEmits<{ reviewSnapshot: [snapshotId: string]; accessDenied: [] }>()
const access = useCoreChangeAccess()
const family = ref<CoreChangeFamily | ''>('')
const disposition = ref<CoreChangeDisposition | ''>('')
const cursors = ref<string[]>([''])
const selectedId = ref('')
const outputInvalid = ref(false)
const branch = computed(() => [...access.prefix.value, 'output', props.reportId, props.outputRevision])
const key = computed(() => [...branch.value, 'rows', family.value, disposition.value, cursors.value.at(-1) ?? ''])
const enabled = computed(() => access.enabled.value && !!props.outputRevision && !outputInvalid.value)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false,
  queryFn: ({ signal }) => access.read(key.value, () => key.value, async () => {
    const tag = JSON.stringify(key.value); const scope = access.scope.value
    const result = await fetchCoreChangeRows(props.reportId, { family: family.value || undefined, disposition: disposition.value || undefined }, cursors.value.at(-1) || undefined, signal)
    if (result.output_revision !== props.outputRevision) {
      if (!signal.aborted && tag === JSON.stringify(key.value) && scope === access.scope.value) outputInvalid.value = true
      throw new Error('The published report revision changed')
    }
    return result
  }),
})
const detailKey = computed(() => [...branch.value, 'detail', selectedId.value])
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => enabled.value && !!selectedId.value), retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false,
  queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, async () => {
    const tag = JSON.stringify(detailKey.value); const scope = access.scope.value
    const result = await fetchCoreChangeRow(props.reportId, selectedId.value, signal)
    if (result.output_revision !== props.outputRevision) {
      if (!signal.aborted && tag === JSON.stringify(detailKey.value) && scope === access.scope.value) outputInvalid.value = true
      throw new Error('The published report revision changed')
    }
    return result
  }),
})
const page = computed(() => enabled.value ? reading.data.value : undefined)
const row = computed(() => enabled.value ? detail.data.value?.row : undefined)
watch([family, disposition], () => { cursors.value = ['']; selectedId.value = '' }, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selectedId.value = '' }, { flush: 'sync' })
watch(() => JSON.stringify([access.scope.value, props.reportId, props.outputRevision]), () => { cursors.value = ['']; selectedId.value = ''; outputInvalid.value = false }, { flush: 'sync' })
watch([reading.error, detail.error], errors => { if (errors.some(importAccessError)) emit('accessDenied') })
onBeforeUnmount(() => access.remove(branch.value))
</script>

<template>
  <Card
    class="min-w-0"
    data-testid="core-change-rows"
  >
    <h3 class="text-section font-medium">
      Published source comparisons
    </h3>
    <p class="mt-1 text-small text-text-muted">
      These pages share completed output revision {{ outputRevision }}. Values and message bodies are not displayed here.
    </p>
    <p
      v-if="outputInvalid"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      The result revision did not match this report. Refresh report status before reviewing more rows.
    </p>
    <template v-if="enabled">
      <div class="mt-4 grid gap-3 sm:grid-cols-2">
        <FormField
          v-slot="{ id }"
          label="Comparison family"
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
              v-for="value in coreChangeFamilies"
              :key="value"
              :value="value"
            >
              {{ coreChangeLabel(value) }}
            </option>
          </select>
        </FormField>
        <FormField
          v-slot="{ id }"
          label="Comparison result"
          bare
        >
          <select
            :id="id"
            v-model="disposition"
            :class="INPUT_CLASSES"
          >
            <option value="">
              All results
            </option><option
              v-for="value in coreChangeDispositions"
              :key="value"
              :value="value"
            >
              {{ coreChangeLabel(value) }}
            </option>
          </select>
        </FormField>
      </div>
      <p
        v-if="disposition"
        class="mt-2 text-small text-text-muted"
      >
        {{ coreChangeMeaning(disposition) }}
      </p>
      <p class="mt-2 text-small text-text-muted">
        Not seen again never proves deletion. Newly observed never proves creation. {{ sourceScope === 'consistent_identity' ? 'Matching source identities do not prove unchanged permissions or complete account access.' : 'Source identity changed or is unknown; one-sided observations carry that uncertainty.' }}
      </p>
      <p
        v-if="reading.isFetching.value"
        role="status"
        class="mt-3 text-small"
      >
        Loading comparisons…
      </p>
      <p
        v-if="reading.error.value"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(reading.error.value, 'Could not read this completed report page.') }}
      </p>
      <button
        v-if="reading.error.value"
        type="button"
        :class="buttonClasses()"
        :disabled="reading.isFetching.value"
        @click="reading.refetch()"
      >
        Retry comparison page
      </button>
      <div
        v-if="page?.rows.length"
        class="mt-3 max-w-full overflow-x-auto"
      >
        <table class="w-full text-left text-small">
          <caption class="sr-only">
            Completed source comparisons
          </caption>
          <thead class="border-b border-border text-text-muted">
            <tr>
              <th class="p-2 font-medium">
                Source identity
              </th><th class="p-2 font-medium">
                Result and limits
              </th><th class="p-2 font-medium">
                Changed categories
              </th><th class="p-2 font-medium">
                Review
              </th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            <tr
              v-for="value in page.rows"
              :key="value.id"
            >
              <td class="max-w-56 break-all p-2">
                {{ coreChangeLabel(value.family) }} · {{ value.source_id ?? 'Invalid or missing ID' }}<p class="text-text-muted">
                  Observations: {{ value.baseline_observations }} original / {{ value.newer_observations }} newer
                </p>
              </td>
              <td class="max-w-72 p-2">
                <p class="font-medium">
                  {{ coreChangeLabel(value.disposition) }}
                </p><p class="text-text-muted">
                  {{ coreChangeMeaning(value.disposition) }}
                </p><p
                  v-for="(reason, index) in value.reasons"
                  :key="index"
                  class="text-text-muted"
                >
                  {{ coreChangeLabel(reason) }}
                </p>
              </td>
              <td class="max-w-56 p-2">
                {{ value.categories.map(coreChangeLabel).join(', ') || 'No qualified changed category' }}
              </td>
              <td class="p-2">
                <button
                  type="button"
                  :class="buttonClasses('ghost')"
                  :aria-label="`Inspect comparison ${value.source_id ?? value.id}`"
                  @click="selectedId = value.id"
                >
                  Inspect comparison
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p
        v-else-if="page && !reading.isFetching.value"
        class="mt-3 text-small"
      >
        No comparisons match these filters. This is not evidence of an empty source account.
      </p>
      <div class="mt-3 flex flex-wrap items-center gap-2">
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="cursors.length < 2 || reading.isFetching.value"
          @click="cursors.pop()"
        >
          Previous comparisons
        </button>
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="!page?.next_cursor || reading.isFetching.value"
          @click="page?.next_cursor && cursors.push(page.next_cursor)"
        >
          More comparisons
        </button>
        <span
          v-if="page"
          class="text-small text-text-muted"
        >{{ page.rows.length }} on this page</span>
      </div>
      <section
        v-if="selectedId"
        class="mt-4 min-w-0 border-t border-border pt-3"
        aria-label="Source comparison detail"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h4 class="text-body font-medium">
            Comparison detail
          </h4><button
            type="button"
            :class="buttonClasses('ghost')"
            @click="selectedId = ''"
          >
            Close comparison
          </button>
        </div>
        <p
          v-if="detail.isFetching.value"
          role="status"
          class="mt-2 text-small"
        >
          Loading comparison detail…
        </p>
        <p
          v-if="detail.error.value"
          role="alert"
          class="mt-2 text-small text-danger"
        >
          {{ describeApiError(detail.error.value, 'Could not read this comparison detail.') }}
        </p>
        <button
          v-if="detail.error.value"
          type="button"
          :class="buttonClasses()"
          :disabled="detail.isFetching.value"
          @click="detail.refetch()"
        >
          Retry comparison detail
        </button>
        <template v-if="row">
          <p class="mt-2 break-all text-small">
            {{ coreChangeLabel(row.family) }} · source ID {{ row.source_id ?? 'invalid or missing' }} · {{ coreChangeLabel(row.disposition) }}
          </p>
          <p class="mt-1 text-small text-text-muted">
            {{ coreChangeMeaning(row.disposition) }}
          </p>
          <p class="mt-2 text-small">
            Changed categories: {{ row.categories.map(coreChangeLabel).join(', ') || 'No qualified changed category' }}
          </p>
          <ul
            v-if="row.reasons.length"
            class="mt-2 list-inside list-disc text-small text-text-muted"
          >
            <li
              v-for="(reason, index) in row.reasons"
              :key="index"
            >
              {{ coreChangeLabel(reason) }}
            </li>
          </ul>
          <h5 class="mt-3 text-small font-medium">
            Separate source representations
          </h5>
          <p class="mt-1 text-small text-text-muted">
            Note list and detail observations are compared separately. Restricted detail is not filled from list content.
          </p>
          <ul class="mt-2 space-y-2 text-small">
            <li
              v-for="(component, index) in row.components"
              :key="index"
            >
              {{ coreChangeLabel(component.representation) }} · {{ coreChangeLabel(component.disposition) }} · {{ component.baseline_observations }} original / {{ component.newer_observations }} newer observations
            </li>
          </ul>
          <h5 class="mt-3 text-small font-medium">
            Representative retained evidence
          </h5>
          <p class="mt-1 text-small text-text-muted">
            {{ row.evidence.length }} references shown, up to 16 per comparison. {{ row.evidence_is_exhaustive ? 'The report marks these references exhaustive for this comparison.' : 'These references are not exhaustive.' }} Total observations: {{ row.baseline_observations }} original / {{ row.newer_observations }} newer.
          </p>
          <ul class="mt-2 space-y-3 text-small">
            <li
              v-for="(evidence, index) in row.evidence"
              :key="index"
              class="min-w-0 rounded border border-border p-3"
            >
              <p>{{ evidence.side === 'baseline' ? 'Original capture' : 'Newer capture' }} · {{ coreChangeLabel(evidence.stream) }} · observation {{ evidence.ordinal }}</p><p class="break-all text-text-muted">
                Capture {{ evidence.capture_id }}
              </p>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :aria-label="`Review ${evidence.side === 'baseline' ? 'original' : 'newer'} snapshot for evidence ${index + 1}`"
                @click="emit('reviewSnapshot', evidence.snapshot_id)"
              >
                Open authorized snapshot review
              </button>
            </li>
          </ul>
          <p class="mt-2 text-small text-text-muted">
            Snapshot review checks access again. Use the source ID above to locate the record; a reference does not grant access to raw content.
          </p>
        </template>
      </section>
    </template>
  </Card>
</template>
