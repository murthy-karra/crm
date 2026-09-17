<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { fetchRefreshFields, fetchRefreshFragment, useFamilyRefreshAccess, type RefreshField } from '../../api/familyRefreshes'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ bundleId: string; itemId: string; revision: string }>()
const access = useFamilyRefreshAccess()
const pages = ref([''])
const fragments = ref([''])
const selected = ref<RefreshField | null>(null)
watch([access.scope, access.enabled, () => props.bundleId, () => props.itemId, () => props.revision], () => { pages.value = ['']; fragments.value = ['']; selected.value = null }, { flush: 'sync' })
watch(selected, () => { fragments.value = [''] }, { flush: 'sync' })
const key = computed(() => [...access.prefix.value, 'fields', props.bundleId, props.itemId, props.revision, pages.value.at(-1)])
const fields = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchRefreshFields(props.bundleId, props.itemId, pages.value.at(-1) || undefined, signal)) })
const fieldKey = computed(() => [...key.value, 'fragment', selected.value?.id, fragments.value.at(-1)])
const fragment = useQuery({ queryKey: fieldKey, enabled: computed(() => access.enabled.value && !!selected.value), retry: false, gcTime: 0, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(fieldKey.value, () => fieldKey.value, async () => {
  const value = await fetchRefreshFragment(props.bundleId, props.itemId, selected.value!.id, fragments.value.at(-1) || undefined, signal)
  if (value.item_id !== props.itemId || value.field_id !== selected.value?.id) throw new Error('The response did not match the selected field.')
  return value
}) })
function move(forward: boolean) {
  selected.value = null
  if (forward && fields.data.value?.next_cursor) pages.value.push(fields.data.value.next_cursor)
  else if (!forward && pages.value.length > 1) pages.value.pop()
}
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="min-w-0 space-y-3 rounded border border-border p-3"
    aria-label="Refresh field review"
  >
    <h4 class="font-medium">
      Retained values and proposed changes
    </h4>
    <p class="text-small text-text-muted">
      Values are shown exactly as retained or prepared. Long values have additional pages.
    </p>
    <p
      v-if="fields.isFetching.value"
      role="status"
    >
      Loading fields…
    </p>
    <p
      v-if="fields.error.value"
      role="alert"
    >
      {{ describeApiError(fields.error.value, 'Reload the review to read these fields.') }}
    </p>
    <ul
      v-if="fields.data.value"
      class="space-y-2"
    >
      <li
        v-for="field in fields.data.value.items"
        :key="field.id"
        class="flex min-w-0 flex-wrap items-center justify-between gap-2"
      >
        <span class="min-w-0 break-all">{{ field.section }} · {{ field.label }} <span class="text-small text-text-muted">({{ field.total_bytes }} bytes)</span></span>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :aria-pressed="selected?.id === field.id"
          @click="selected = field"
        >
          Inspect {{ field.label }}
        </button>
      </li>
    </ul>
    <div class="flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="pages.length < 2 || fields.isFetching.value"
        @click="move(false)"
      >
        Previous fields
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="!fields.data.value?.next_cursor || fields.isFetching.value"
        @click="move(true)"
      >
        More fields
      </button>
    </div>
    <section
      v-if="selected"
      class="min-w-0 space-y-2 border-t border-border pt-3"
      aria-label="Selected field value"
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h5 class="break-all font-medium">
          {{ selected.section }} · {{ selected.label }}
        </h5>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          @click="selected = null"
        >
          Close field
        </button>
      </div>
      <p
        v-if="fragment.isFetching.value"
        role="status"
      >
        Loading value…
      </p>
      <p
        v-if="fragment.error.value"
        role="alert"
      >
        {{ describeApiError(fragment.error.value, 'The field could not be verified. Reload the review.') }}
      </p>
      <template v-if="fragment.data.value">
        <p class="text-small text-text-muted">
          Page {{ fragments.length }} · {{ fragment.data.value.total_bytes }} bytes in total
        </p>
        <pre class="max-h-80 max-w-full overflow-auto whitespace-pre-wrap break-all font-mono text-small">{{ fragment.data.value.text }}</pre>
      </template>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="fragments.length < 2 || fragment.isFetching.value"
          @click="fragments.pop()"
        >
          Previous value page
        </button>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!fragment.data.value?.next_cursor || fragment.isFetching.value"
          @click="fragment.data.value?.next_cursor && fragments.push(fragment.data.value.next_cursor)"
        >
          More value pages
        </button>
      </div>
    </section>
  </section>
</template>
