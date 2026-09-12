<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { fetchMetadataField, useMetadataAccess, type MetadataFieldRequest } from '../../api/metadataImports'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ request: MetadataFieldRequest; title: string }>()
const access = useMetadataAccess()
const cursors = ref<string[]>([''])
const branch = computed(() => [...access.prefix.value, 'field', props.request])
const key = computed(() => [...branch.value, cursors.value.at(-1) ?? ''])
const reading = useQuery({ queryKey: key, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchMetadataField(props.request, cursors.value.at(-1) || undefined, signal)), enabled: access.enabled, retry: false, gcTime: 0 })
const segment = computed(() => access.enabled.value ? reading.data.value : undefined)
const end = computed(() => segment.value ? (BigInt(segment.value.offset_bytes) + BigInt(new TextEncoder().encode(segment.value.text).length)).toString() : '')
watch(() => JSON.stringify([access.scope.value, props.request]), () => { cursors.value = [''] }, { flush: 'sync' })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <section
    class="mt-3 min-w-0 rounded-lg border border-border p-3"
    :aria-label="title"
  >
    <h4 class="break-words text-body font-medium text-text">
      {{ title }}
    </h4>
    <p
      v-if="!access.enabled.value"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      Source inspection requires a current administrator session.
    </p>
    <p
      v-else-if="reading.error.value"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load this field segment.') }}
    </p>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-2 text-small text-text-muted"
    >
      Loading field segment…
    </p>
    <template v-if="segment">
      <p class="mt-2 text-small text-text-muted">
        Bytes {{ segment.offset_bytes }}–{{ end }} of {{ segment.full_utf8_bytes }}. One segment is shown at a time.
      </p>
      <pre
        tabindex="0"
        aria-label="Metadata source field text"
        class="mt-2 max-h-96 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg bg-surface-1 p-3 text-small text-text"
      >{{ segment.text }}</pre>
    </template>
    <div
      v-if="access.enabled.value"
      class="mt-3 flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous segment
      </button>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!segment?.next_cursor || reading.isFetching.value"
        @click="segment?.next_cursor && cursors.push(segment.next_cursor)"
      >
        Next segment
      </button>
      <button
        v-if="reading.error.value"
        type="button"
        :class="buttonClasses('secondary')"
        @click="reading.refetch()"
      >
        Retry field read
      </button>
    </div>
  </section>
</template>
