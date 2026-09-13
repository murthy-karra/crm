<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import FormField from '../FormField.vue'
import PeopleRefreshItemDetail from './PeopleRefreshItemDetail.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { importAccessError } from '../../api/imports'
import { fetchPeopleRefreshItems, fetchPeopleRefreshResults, refreshDispositions, refreshLabel, usePeopleRefreshAccess, type PeopleRefresh, type RefreshDisposition, type RefreshPlan } from '../../api/peopleRefreshes'
import { snapshotTime } from './format'
const props = defineProps<{ refresh: PeopleRefresh; plan: RefreshPlan }>()
const emit = defineEmits<{ accessDenied: [] }>()
const access = usePeopleRefreshAccess()
const filter = ref<RefreshDisposition | ''>(''); const pages = ref(['']); const selected = ref('')
const resultPages = ref(['']); const resultEpoch = ref(0)
const firstResults = ref<Awaited<ReturnType<typeof fetchPeopleRefreshResults>> | null>(null)
const itemsKey = computed(() => [...access.prefix.value, 'items', props.refresh.id, props.plan.id, props.plan.revision, filter.value, pages.value.at(-1)])
const resultsKey = computed(() => [...access.prefix.value, 'results', props.refresh.id, resultEpoch.value, resultPages.value.at(-1)])
const items = useQuery({ queryKey: itemsKey, enabled: access.enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(itemsKey.value, () => itemsKey.value, async () => {
  const value = await fetchPeopleRefreshItems(props.refresh.id, filter.value || undefined, pages.value.at(-1) || undefined, signal)
  if (value.plan_id !== props.plan.id || value.plan_revision !== props.plan.revision) throw new Error('The preview revision did not match. Reload the People refresh.')
  return value
}) })
const results = useQuery({ queryKey: resultsKey, enabled: computed(() => access.enabled.value && ['running', 'paused', 'completed', 'cancelled'].includes(props.refresh.state)), retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(resultsKey.value, () => resultsKey.value, async () => {
  const first = resultPages.value.length === 1; const scope = access.scope.value; const key = JSON.stringify(resultsKey.value)
  if (first && firstResults.value) return firstResults.value
  const value = await fetchPeopleRefreshResults(props.refresh.id, resultPages.value.at(-1) || undefined, signal)
  if (first && !signal.aborted && scope === access.scope.value && key === JSON.stringify(resultsKey.value)) firstResults.value = value
  return value
}) })
watch([filter, () => props.plan.id, () => props.plan.revision, () => props.refresh.id], () => { selected.value = ''; pages.value = [''] }, { flush: 'sync' })
watch(pages, () => { selected.value = '' }, { deep: true, flush: 'sync' })
watch(access.scope, () => { selected.value = ''; firstResults.value = null; resultPages.value = ['']; resultEpoch.value++ }, { flush: 'sync' })
watch([items.error, results.error], errors => { if (errors.some(importAccessError)) emit('accessDenied') })
function reloadResults() { firstResults.value = null; resultPages.value = ['']; resultEpoch.value++ }
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="space-y-3"
    aria-label="People refresh preview and results"
  >
    <FormField
      v-slot="{ id }"
      label="People preview disposition"
      bare
    >
      <select
        :id="id"
        v-model="filter"
        :class="INPUT_CLASSES"
      >
        <option value="">
          All dispositions
        </option><option
          v-for="value in refreshDispositions"
          :key="value"
          :value="value"
        >
          {{ refreshLabel(value) }}
        </option>
      </select>
    </FormField>
    <p class="text-small text-text-muted">
      Inspect the baseline, current CRM values and proposed values for each Person. Clears and contact removals are listed separately. Held records receive no partial update.
    </p>
    <div
      v-if="items.data.value"
      class="space-y-2"
    >
      <article
        v-for="item in items.data.value.items"
        :key="item.id"
        class="min-w-0 space-y-2 rounded border border-border p-3 text-small"
      >
        <div class="flex flex-wrap items-start justify-between gap-2">
          <p class="break-all font-medium">
            Source Person {{ item.source_id ?? 'Unavailable source ID' }}
          </p><p>{{ refreshLabel(item.disposition) }}</p>
        </div>
        <p>Proposed clears: {{ item.clear_counts.names }} name fields, {{ item.clear_counts.assignments }} assignments, {{ item.clear_counts.contacts }} contact removals.</p>
        <p v-if="item.no_instruction.length">
          Preserved without a new source instruction: {{ item.no_instruction.map(refreshLabel).join(', ') }}.
        </p>
        <div class="flex flex-wrap gap-3">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="selected = selected === item.id ? '' : item.id"
          >
            {{ selected === item.id ? 'Close Person preview' : 'Inspect Person preview' }}
          </button><RouterLink
            v-if="item.person_id"
            :to="`/people/${encodeURIComponent(item.person_id)}`"
            class="inline-flex items-center text-accent underline"
          >
            Open CRM Person
          </RouterLink>
        </div>
        <PeopleRefreshItemDetail
          v-if="selected === item.id"
          :key="`${item.id}:${plan.id}:${plan.revision}`"
          :refresh-id="refresh.id"
          :item-id="item.id"
          :plan="plan"
          @access-denied="emit('accessDenied')"
        />
      </article>
      <p
        v-if="items.data.value.items.length === 0"
        class="text-small"
      >
        No People match this disposition on this page.
      </p>
    </div>
    <p
      v-if="items.isFetching.value"
      role="status"
      class="text-small"
    >
      Loading the frozen People preview…
    </p><p
      v-if="items.error.value"
      role="alert"
      class="text-small text-danger"
    >
      {{ describeApiError(items.error.value, 'The preview could not be verified. Reload the People refresh.') }}
    </p>
    <div class="flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="pages.length < 2 || items.isFetching.value"
        @click="pages.pop()"
      >
        Previous People previews
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="!items.data.value?.next_cursor || items.isFetching.value"
        @click="items.data.value?.next_cursor && pages.push(items.data.value.next_cursor)"
      >
        More People previews
      </button>
    </div>
    <section
      v-if="['running', 'paused', 'completed', 'cancelled'].includes(refresh.state)"
      class="space-y-3 border-t border-border pt-4"
      aria-label="Settled refresh results"
    >
      <div class="flex flex-wrap justify-between gap-2">
        <h4 class="text-body font-medium">
          Settled results
        </h4><button
          type="button"
          :class="buttonClasses()"
          @click="reloadResults"
        >
          Load latest settled results
        </button>
      </div>
      <p class="text-small text-text-muted">
        This traversal stays fixed while you page. Load the latest results to include newer settlements.
      </p>
      <article
        v-for="result in results.data.value?.results ?? []"
        :key="result.id"
        class="space-y-2 rounded border border-border p-3 text-small"
      >
        <p class="break-all">
          Source Person {{ result.source_id ?? 'Unavailable source ID' }} · {{ refreshLabel(result.disposition) }}
        </p><p>{{ snapshotTime(result.committed_at) }}</p><RouterLink
          v-if="result.person_id"
          :to="`/people/${encodeURIComponent(result.person_id)}`"
          class="text-accent underline"
        >
          Open settled Person
        </RouterLink>
      </article>
      <p
        v-if="results.data.value?.results.length === 0"
        class="text-small"
      >
        No settlements in this traversal.
      </p><p
        v-if="results.error.value"
        role="alert"
        class="text-small text-danger"
      >
        {{ describeApiError(results.error.value, 'Could not load settled results.') }}
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="resultPages.length < 2 || results.isFetching.value"
          @click="resultPages.pop()"
        >
          Previous settled results
        </button><button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!results.data.value?.next_cursor || results.isFetching.value"
          @click="results.data.value?.next_cursor && resultPages.push(results.data.value.next_cursor)"
        >
          More settled results
        </button>
      </div>
    </section>
  </section>
</template>
