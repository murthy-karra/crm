<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { fetchActivityObservations, useActivityAccess } from '../../api/activityImports'
import ActivityFieldViewer from './ActivityFieldViewer.vue'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ importId: string; planId: string; recordId: string }>()
const access = useActivityAccess()
const cursors = ref<string[]>([''])
const selected = ref<{ id: string; field: string } | null>(null)
const branch = computed(() => [...access.prefix.value, 'observations', props.importId, props.planId, props.recordId])
const key = computed(() => [...branch.value, cursors.value.at(-1) ?? ''])
const reading = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchActivityObservations(props.importId, props.planId, props.recordId, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => access.enabled.value ? reading.data.value : undefined)
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.recordId]), () => { cursors.value = ['']; selected.value = null }, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selected.value = null })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <section
    class="mt-3 min-w-0 rounded-lg border border-border p-3"
    aria-label="Retained source observations"
  >
    <h4 class="text-body font-medium">
      Retained source observations
    </h4>
    <p class="mt-1 text-small text-text-muted">
      List and detail observations may be complementary. Exact capture text remains untrusted source content; URLs are not opened.
    </p>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-2 text-small"
    >
      Loading observations…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load observations.') }}
    </p>
    <ul class="mt-2 divide-y divide-border">
      <li
        v-for="row in page?.items ?? []"
        :key="row.id"
        class="space-y-2 py-2 text-small"
      >
        <p class="break-all">
          {{ row.stream }} · {{ row.representation }} · {{ row.negative ? 'Negative observation' : 'Captured observation' }} · FUB {{ row.source_id ?? 'ID unavailable' }}
        </p>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses()"
            @click="selected = { id: row.id, field: 'all' }"
          >
            Inspect observation
          </button>
          <button
            type="button"
            :class="buttonClasses()"
            @click="selected = { id: row.id, field: 'raw' }"
          >
            Inspect exact capture text
          </button>
        </div>
      </li>
    </ul>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous observations
      </button>
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next observations
      </button>
      <button
        v-if="reading.error.value"
        type="button"
        :class="buttonClasses()"
        @click="reading.refetch()"
      >
        Retry observations
      </button>
    </div>
    <ActivityFieldViewer
      v-if="selected && access.enabled.value"
      :request="{ kind: 'observations', importId, planId, rowId: selected.id, fieldKey: selected.field }"
      :title="selected.field === 'raw' ? 'Exact captured source text' : 'Exact source observation'"
    />
  </section>
</template>
