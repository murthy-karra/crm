<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { fetchRefreshMappings, fetchRefreshTargets, useFamilyRefreshAccess, type RefreshFamily, type RefreshMapping, type RefreshPatch, type RefreshSelection, type RefreshTarget } from '../../api/familyRefreshes'
import type { TaskKind } from '../../api/types'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ bundleId: string; planId: string; revision: string; family: RefreshFamily; disabled: boolean; progressVersion?: string }>()
const emit = defineEmits<{ change: [patches: RefreshPatch[]] }>()
const access = useFamilyRefreshAccess()
const pages = ref(['']); const targetPages = ref(['']); const drafts = ref<Record<string, RefreshSelection>>({}); const picking = ref<RefreshMapping | null>(null)
const message = ref(''); const labels = ref<Record<string, string>>({})
const key = computed(() => [...access.prefix.value, 'mappings', props.bundleId, props.planId, props.revision, props.family, props.progressVersion, pages.value.at(-1)])
const reading = useQuery({ queryKey: key, enabled: access.enabled, retry: false, gcTime: 0, refetchOnWindowFocus: false, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchRefreshMappings(props.bundleId, props.family, props.planId, pages.value.at(-1) || undefined, signal)) })
const targetKey = computed(() => [...key.value, 'targets', picking.value?.id, targetPages.value.at(-1)])
const targets = useQuery({ queryKey: targetKey, enabled: computed(() => access.enabled.value && !!picking.value && !props.disabled), retry: false, gcTime: 0, refetchOnWindowFocus: false, queryFn: ({ signal }) => access.read(targetKey.value, () => targetKey.value, () => fetchRefreshTargets(props.bundleId, picking.value!.id, targetPages.value.at(-1) || undefined, signal)) })
function reset() { pages.value = ['']; targetPages.value = ['']; drafts.value = {}; labels.value = {}; picking.value = null; message.value = ''; emit('change', []) }
watch([access.scope, access.enabled, () => props.bundleId, () => props.planId, () => props.revision], reset, { flush: 'sync' })
watch(picking, () => { targetPages.value = [''] }, { flush: 'sync' })
watch(() => props.disabled, value => { if (value) picking.value = null })
onBeforeUnmount(() => emit('change', []))
function choice(row: RefreshMapping) { return drafts.value[row.id] ?? row.choice }
function encoded(row: RefreshMapping) { const value = choice(row); return value.action === 'kind' ? `kind:${value.kind}` : value.action }
function change(row: RefreshMapping, value: RefreshSelection, label?: string) {
  if (props.disabled || !access.enabled.value) return
  if (!(row.id in drafts.value) && Object.keys(drafts.value).length >= 50) { message.value = 'Apply these 50 choices before changing more mappings.'; return }
  drafts.value[row.id] = value
  if (label) labels.value[row.id] = label
  message.value = ''; emit('change', Object.entries(drafts.value).map(([mapping_id, choice]) => ({ mapping_id, choice })))
}
function select(row: RefreshMapping, event: Event) {
  const value = (event.target as HTMLSelectElement).value
  if (value.startsWith('kind:')) change(row, { action: 'kind', kind: value.slice(5) as TaskKind })
  else if (value === 'hold' || value === 'unassigned' || value === 'create_matching') change(row, { action: value })
}
function choose(target: RefreshTarget) { if (picking.value) change(picking.value, { action: 'existing', target_id: target.id }, target.label); picking.value = null }
function move(forward: boolean) { picking.value = null; if (forward && reading.data.value?.next_cursor) pages.value.push(reading.data.value.next_cursor); else if (!forward && pages.value.length > 1) pages.value.pop() }
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="min-w-0 space-y-3"
    aria-label="Refresh mappings"
  >
    <h3 class="font-medium">
      Mappings
    </h3>
    <p class="text-small text-text-muted">
      Choose destinations explicitly. Unchosen values remain held. New choice fields need a choice for every option.
    </p>
    <p
      v-if="reading.isFetching.value"
      role="status"
    >
      Loading mappings…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
    >
      {{ describeApiError(reading.error.value, 'Reload the mappings.') }}
    </p>
    <p
      v-if="message"
      role="alert"
    >
      {{ message }}
    </p>
    <p
      v-if="reading.data.value && !reading.data.value.inventory_complete"
      role="status"
    >
      The mapping inventory is still being prepared. Refresh the review to see progress.
    </p>
    <ul class="space-y-3">
      <li
        v-for="row in reading.data.value?.items ?? []"
        :key="row.id"
        class="min-w-0 space-y-2 rounded border border-border p-3"
      >
        <p class="break-all font-medium">
          {{ row.label ?? 'Source value needs review' }} <span class="text-small text-text-muted">· {{ row.kind.replaceAll('_', ' ') }}</span>
        </p>
        <p
          v-if="labels[row.id]"
          class="break-all text-small"
        >
          Selected: {{ labels[row.id] }}
        </p>
        <p
          v-if="row.kind === 'timezone'"
          class="text-small"
        >
          {{ row.choice.action === 'timezone' ? row.choice.zone : 'No source timezone selected' }}. Set the source timezone below.
        </p>
        <div
          v-else
          class="flex flex-wrap items-center gap-2"
        >
          <select
            :class="INPUT_CLASSES"
            :aria-label="`Mapping choice for ${row.label ?? row.kind}`"
            :value="encoded(row)"
            :disabled="disabled"
            @change="select(row, $event)"
          >
            <option value="hold">
              Keep held
            </option>
            <option
              v-if="row.creation_allowed"
              value="create_matching"
            >
              Create matching destination
            </option>
            <option
              v-if="['note_author', 'task_creator', 'task_assignee'].includes(row.kind)"
              value="unassigned"
            >
              Leave unassigned
            </option>
            <template v-if="row.kind === 'task_kind'">
              <option
                v-for="kind in (['call', 'email', 'text', 'follow_up', 'other'] as const)"
                :key="kind"
                :value="`kind:${kind}`"
              >
                {{ kind.replace('_', ' ') }}
              </option>
            </template>
            <option
              v-if="choice(row).action === 'existing'"
              value="existing"
            >
              Existing destination selected
            </option>
          </select>
          <button
            v-if="row.kind !== 'task_kind'"
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="disabled"
            @click="picking = row"
          >
            Choose existing destination
          </button>
        </div>
      </li>
    </ul>
    <div class="flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="pages.length < 2 || reading.isFetching.value"
        @click="move(false)"
      >
        Previous mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="!reading.data.value?.next_cursor || reading.isFetching.value"
        @click="move(true)"
      >
        More mappings
      </button>
    </div>
    <section
      v-if="picking && !disabled"
      class="min-w-0 space-y-2 rounded border border-border p-3"
      aria-label="Choose existing refresh destination"
    >
      <h4 class="break-all font-medium">
        Destination for {{ picking.label ?? picking.kind }}
      </h4>
      <p
        v-if="targets.error.value"
        role="alert"
      >
        {{ describeApiError(targets.error.value, 'Destinations could not be loaded.') }}
      </p>
      <p
        v-if="targets.isFetching.value"
        role="status"
      >
        Loading destinations…
      </p>
      <p v-if="targets.data.value?.items.length === 0">
        No matching destinations on this page.
      </p>
      <ul class="space-y-2">
        <li
          v-for="target in targets.data.value?.items ?? []"
          :key="target.id"
          class="flex min-w-0 flex-wrap items-center justify-between gap-2"
        >
          <span class="break-all">{{ target.label }} {{ target.field_type ? `· ${target.field_type}` : '' }} {{ target.status ? `· ${target.status}` : '' }}</span><button
            type="button"
            :class="buttonClasses('secondary')"
            @click="choose(target)"
          >
            Choose {{ target.label }}
          </button>
        </li>
      </ul>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="targetPages.length < 2 || targets.isFetching.value"
          @click="targetPages.pop()"
        >
          Previous destinations
        </button>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!targets.data.value?.next_cursor || targets.isFetching.value"
          @click="targets.data.value?.next_cursor && targetPages.push(targets.data.value.next_cursor)"
        >
          More destinations
        </button>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          @click="picking = null"
        >
          Close destinations
        </button>
      </div>
    </section>
  </section>
</template>
