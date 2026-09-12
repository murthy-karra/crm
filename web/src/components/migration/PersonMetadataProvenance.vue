<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { ApiError } from '../../api/client'
import { fetchMetadataProvenance, useMetadataAccess, type MetadataResult } from '../../api/metadataImports'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel, snapshotTime } from './format'
import MetadataEvidence from './MetadataEvidence.vue'
const props = defineProps<{ personId: string }>()
const access = useMetadataAccess()
const cursors = ref<string[]>([''])
const selected = ref<MetadataResult | null>(null)
const branch = computed(() => [...access.prefix.value, 'person-provenance', props.personId])
const key = computed(() => [...branch.value, cursors.value.at(-1) ?? ''])
const reading = useQuery({ queryKey: key, enabled: computed(() => access.enabled.value && !!props.personId), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchMetadataProvenance(props.personId, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => access.enabled.value ? reading.data.value : undefined)
const absent = computed(() => reading.error.value instanceof ApiError && reading.error.value.status === 404 || !!page.value && !page.value.items.length && cursors.value.length === 1)
watch(() => JSON.stringify([access.scope.value, props.personId]), () => { cursors.value = ['']; selected.value = null }, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selected.value = null })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <section
    v-if="access.enabled.value && !absent"
    class="min-w-0"
    aria-label="Imported tags and custom fields"
  >
    <h2 class="text-section font-semibold">
      Imported tags and custom fields
    </h2>
    <p class="mt-1 text-small text-text-muted">
      Original source evidence and committed metadata results. This workspace remains for administrator review.
    </p>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-2 text-small text-text-muted"
    >
      Loading metadata source information…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load metadata source information.') }}
    </p>
    <ul class="mt-3 space-y-2 text-small">
      <li
        v-for="row in page?.items ?? []"
        :key="row.id"
      >
        <p class="break-all">
          FUB {{ row.source_id }} · {{ snapshotLabel(row.disposition) }}<span v-if="row.committed_at"> · {{ snapshotTime(row.committed_at) }}</span>
        </p><p
          v-for="reason in row.reasons"
          :key="reason"
        >
          {{ snapshotLabel(reason) }}
        </p><button
          type="button"
          :class="buttonClasses('secondary')"
          @click="selected = row"
        >
          Inspect imported metadata
        </button>
      </li>
    </ul>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        v-if="cursors.length > 1"
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous metadata source
      </button><button
        v-if="page?.next_cursor"
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next metadata source
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch()"
      >
        Refresh metadata source
      </button>
    </div>
    <MetadataEvidence
      v-if="selected"
      :source="selected.source"
      :operations="selected.operations"
      :request="{ kind: 'provenance', personId, resultId: selected.id, fieldKey: 'all' }"
      title="Imported metadata evidence"
    />
  </section>
</template>
