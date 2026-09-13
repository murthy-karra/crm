<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { importAccessError } from '../../api/imports'
import { fetchAdmittedPeopleRefreshField, refreshLabel, useAdmittedPeopleRefreshAccess, type RefreshPlan } from '../../api/admittedPeopleRefreshes'
const props = defineProps<{ refreshId: string; itemId: string; plan: RefreshPlan; side: 'baseline' | 'current' | 'proposed'; field: 'first_name' | 'last_name' }>()
const emit = defineEmits<{ accessDenied: []; close: [] }>()
const access = useAdmittedPeopleRefreshAccess(); const pages = ref([''])
watch([() => props.refreshId, () => props.itemId, () => props.plan.id, () => props.plan.revision, () => props.side, () => props.field, access.scope], () => { pages.value = [''] }, { flush: 'sync' })
const key = computed(() => [...access.prefix.value, 'field', props.refreshId, props.itemId, props.plan.id, props.plan.revision, props.side, props.field, pages.value.at(-1)])
const result = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(key.value, () => key.value, async () => {
  const value = await fetchAdmittedPeopleRefreshField(props.refreshId, props.itemId, props.side, props.field, pages.value.at(-1) || undefined, signal)
  if (value.plan_id !== props.plan.id || value.plan_revision !== props.plan.revision || value.side !== props.side || value.field !== props.field) throw new Error('The field fragment did not match the selected plan.')
  return value
}) })
watch(result.error, error => { if (importAccessError(error)) emit('accessDenied') })
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="space-y-2 rounded border border-border p-3"
    aria-label="Complete retained name pages"
  >
    <div class="flex flex-wrap items-start justify-between gap-2">
      <h5 class="font-medium">
        {{ refreshLabel(field) }} · {{ side === 'baseline' ? 'Last settled baseline' : side === 'current' ? 'CRM at preview' : 'Proposed after refresh' }}
      </h5><button
        type="button"
        :class="buttonClasses('ghost')"
        @click="emit('close')"
      >
        Close name pages
      </button>
    </div>
    <template v-if="result.data.value">
      <p>Page {{ pages.length }} · {{ result.data.value.total_bytes }} retained bytes in this name.</p><p class="max-h-80 overflow-auto whitespace-pre-wrap break-all">
        {{ result.data.value.fragment }}
      </p>
    </template>
    <p
      v-if="result.isFetching.value"
      role="status"
    >
      Loading the retained name…
    </p><p
      v-if="result.error.value"
      role="alert"
      class="text-danger"
    >
      {{ describeApiError(result.error.value, 'The name fragment could not be verified. Close it and reload the preview.') }}
    </p>
    <div class="flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="pages.length < 2 || result.isFetching.value"
        @click="pages.pop()"
      >
        Previous name page
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="!result.data.value?.next_cursor || result.isFetching.value"
        @click="result.data.value?.next_cursor && pages.push(result.data.value.next_cursor)"
      >
        More name pages
      </button>
    </div>
  </section>
</template>
