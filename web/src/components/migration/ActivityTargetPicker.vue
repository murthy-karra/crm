<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Dialog from 'primevue/dialog'
import { fetchActivityTargets, useActivityAccess, type ActivityMapping, type ActivityTarget } from '../../api/activityImports'
import { buttonClasses, dialogPt } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ importId: string; planId: string; mapping: ActivityMapping; disabled: boolean }>()
const emit = defineEmits<{ close: []; select: [target: ActivityTarget] }>()
const access = useActivityAccess()
const titleId = useId()
const cursors = ref<string[]>([''])
const branch = computed(() => [...access.prefix.value, 'targets', props.importId, props.planId, props.mapping.id])
const key = computed(() => [...branch.value, cursors.value.at(-1) ?? ''])
const enabled = computed(() => access.enabled.value && !props.disabled)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchActivityTargets(props.importId, props.planId, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => enabled.value ? reading.data.value : undefined)
function compatible(target: ActivityTarget) { return props.mapping.role !== 'task_assignee' || target.status === 'active' }
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.mapping.id]), () => { cursors.value = [''] }, { flush: 'sync' })
watch(enabled, value => { if (!value) emit('close') })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <Dialog
    :visible="true"
    modal
    :closable="false"
    :aria-labelledby="titleId"
    :pt="dialogPt()"
    @update:visible="value => !value && emit('close')"
  >
    <template #header>
      <h2
        :id="titleId"
        class="text-section font-semibold"
      >
        Choose CRM user
      </h2>
    </template>
    <p class="break-all text-small text-text-muted">
      Map {{ mapping.source_value }}. This choice stays a draft until you apply the mapping changes.
    </p>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-3 text-small"
    >
      Loading existing choices…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load existing choices.') }}
    </p>
    <ul
      v-if="page"
      class="mt-3 max-h-80 space-y-2 overflow-y-auto"
    >
      <li
        v-for="target in page.items"
        :key="target.id"
        class="flex min-w-0 items-center justify-between gap-3 border-b border-border pb-2"
      >
        <span class="min-w-0 break-words text-small">{{ target.display_name }}<span
          v-if="target.status"
          class="block text-text-muted"
        >{{ target.status }}{{ !compatible(target) ? ' · Inactive users cannot be task assignees' : '' }}</span></span>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="!enabled || !compatible(target)"
          :aria-label="`Use ${target.display_name}`"
          @click="emit('select', target)"
        >
          Use
        </button>
      </li>
    </ul>
    <p
      v-if="page && !page.items.length"
      class="mt-3 text-small text-text-muted"
    >
      No existing choices in this page.
    </p>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous choices
      </button>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next choices
      </button>
      <button
        v-if="reading.error.value"
        type="button"
        :class="buttonClasses('secondary')"
        @click="reading.refetch()"
      >
        Retry choices
      </button>
    </div>
    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        @click="emit('close')"
      >
        Close choices
      </button>
    </template>
  </Dialog>
</template>
