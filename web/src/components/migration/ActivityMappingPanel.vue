<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ActivityFieldViewer from './ActivityFieldViewer.vue'
import ActivityTargetPicker from './ActivityTargetPicker.vue'
import { fetchActivityMappings, useActivityAccess, type ActivityChoice, type ActivityRole, type ActivityMapping, type ActivityPatch, type ActivityTarget } from '../../api/activityImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel } from './format'
const props = defineProps<{ importId: string; planId: string; revision: string; disabled: boolean; progressVersion: string; resetVersion: number }>()
const emit = defineEmits<{ change: [patches: ActivityPatch[]] }>()
const access = useActivityAccess()
const role = ref<ActivityRole>('note_author')
const cursors = ref<string[]>([''])
const drafts = ref<Record<string, ActivityChoice>>({})
const labels = ref<Record<string, string>>({})
const selected = ref<ActivityMapping | null>(null)
const picking = ref<ActivityMapping | null>(null)
const message = ref('')
const branch = computed(() => [...access.prefix.value, 'mappings', props.importId, props.planId, props.revision])
const key = computed(() => [...branch.value, role.value, cursors.value.at(-1) ?? '', props.progressVersion])
const enabled = computed(() => access.enabled.value && !!props.importId && !!props.planId)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchActivityMappings(props.importId, props.planId, role.value, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => enabled.value ? reading.data.value : undefined)
const encode = (value: ActivityChoice) => value.kind === 'map_existing' ? `map_existing:${value.target_id}` : value.kind === 'map_kind' ? `map_kind:${value.native_kind}` : value.kind
const choice = (row: ActivityMapping) => drafts.value[row.id] ?? row.choice
function report() { emit('change', Object.entries(drafts.value).sort(([a], [b]) => a.localeCompare(b)).map(([mapping_id, choice]) => ({ mapping_id, choice: { ...choice } }))) }
function reset() { drafts.value = {}; labels.value = {}; message.value = ''; report() }
function change(row: ActivityMapping, value: ActivityChoice, target?: ActivityTarget) {
  if (!enabled.value || props.disabled) return
  if (!(row.id in drafts.value) && Object.keys(drafts.value).length >= 50 && encode(value) !== encode(row.choice)) { message.value = 'Apply these 50 choices before changing more mappings.'; return }
  if (encode(value) === encode(row.choice)) delete drafts.value[row.id]
  else drafts.value[row.id] = value
  if (target) labels.value[row.id] = target.display_name
  message.value = ''; report()
}
function select(row: ActivityMapping, event: Event) {
  const el = event.target as HTMLSelectElement
  const value = el.value
  if (value.startsWith('map_kind:')) change(row, { kind: 'map_kind', native_kind: value.slice(9) as 'call' | 'email' | 'text' | 'follow_up' | 'other' })
  else if (value === 'hold' || value === 'leave_unmapped') change(row, { kind: value })
  el.value = encode(choice(row))
}
function picked(target: ActivityTarget) { if (picking.value) change(picking.value, { kind: 'map_existing', target_id: target.id }, target); picking.value = null }
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.revision, props.resetVersion]), () => { reset(); cursors.value = ['']; selected.value = null; picking.value = null }, { flush: 'sync' })
watch(role, () => { cursors.value = ['']; selected.value = null; picking.value = null })
watch(() => cursors.value.at(-1), () => { selected.value = null; picking.value = null })
watch(() => props.disabled, value => { if (value) picking.value = null })
onBeforeUnmount(() => { emit('change', []); access.remove(branch.value) })
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="activity-mappings"
  >
    <h2 class="text-section font-medium">
      Authors, assignees and task kinds
    </h2>
    <p class="mt-1 text-small text-text-muted">
      Choose each role explicitly. Suggestions never authorize writes. Unmapped authors have no linked CRM user or author edit rights; ordinary actions remain unavailable during review. Task assignees must be active. Mapping an Appointment creates a task, not a calendar event.
    </p>
    <div
      class="mt-3 flex flex-wrap gap-2"
      aria-label="Activity mapping role"
    >
      <button
        v-for="value in (['note_author', 'task_creator', 'task_assignee', 'task_kind'] as const)"
        :key="value"
        type="button"
        :class="buttonClasses(role === value ? 'primary' : 'secondary')"
        :aria-pressed="role === value"
        @click="role = value"
      >
        {{ snapshotLabel(value) }}
      </button>
    </div>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-3 text-small"
    >
      Loading mappings…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load mappings.') }}
    </p>
    <ul
      v-if="page"
      class="mt-3 divide-y divide-border"
    >
      <li
        v-for="row in page.items"
        :key="row.id"
        class="min-w-0 space-y-2 py-3"
      >
        <p class="whitespace-pre-wrap break-all text-body font-medium">
          {{ row.source_value }}<span
            v-if="row.source_value_abbreviated"
            class="text-small font-normal"
          > (abbreviated; {{ row.source_value_full_utf8_bytes }} bytes)</span>
        </p>
        <p class="text-small text-text-muted">
          {{ row.dependent_count }} dependent records · {{ snapshotLabel(row.role) }}
        </p>
        <div class="flex flex-wrap items-center gap-2">
          <select
            :value="encode(choice(row))"
            :aria-label="`${snapshotLabel(row.role)} choice for ${row.source_value}`"
            :class="[INPUT_CLASSES, 'max-w-full sm:max-w-80']"
            :disabled="disabled || !enabled"
            @change="select(row, $event)"
          >
            <option value="hold">
              Hold for later
            </option>
            <option
              v-if="row.role !== 'task_kind'"
              value="leave_unmapped"
            >
              Leave CRM user unmapped
            </option>
            <template v-if="row.role === 'task_kind'">
              <option
                v-for="kind in (['call', 'email', 'text', 'follow_up', 'other'] as const)"
                :key="kind"
                :value="`map_kind:${kind}`"
              >
                {{ snapshotLabel(kind) }}
              </option>
            </template>
            <option
              v-if="choice(row).kind === 'map_existing'"
              :value="encode(choice(row))"
            >
              Use {{ labels[row.id] ?? (choice(row) as { target_id: string }).target_id }}
            </option>
          </select>
          <button
            v-if="row.role !== 'task_kind'"
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="disabled || !enabled"
            @click="picking = row"
          >
            Choose CRM user
          </button>
          <button
            type="button"
            :class="buttonClasses('ghost')"
            @click="selected = selected?.id === row.id ? null : row"
          >
            Inspect source mapping
          </button>
        </div>
        <p
          v-if="row.suggested_kind"
          class="text-small text-text-muted"
        >
          Suggested kind: {{ snapshotLabel(row.suggested_kind) }}. Select it to approve this choice.
        </p>
        <p
          v-if="row.suggestions.length"
          class="break-all text-small text-text-muted"
        >
          Suggested user IDs: {{ row.suggestions.join(', ') }}. Choose explicitly.
        </p>
        <ActivityFieldViewer
          v-if="selected?.id === row.id && enabled"
          :request="{ kind: 'mappings', importId, planId, rowId: row.id, fieldKey: 'all' }"
          title="Exact source mapping"
        />
      </li>
    </ul>
    <p
      v-if="page && !page.items.length"
      class="mt-3 text-small text-text-muted"
    >
      No mappings for this role.
    </p>
    <p
      v-if="message"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ message }}
    </p>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous mappings
      </button>
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch()"
      >
        Refresh mappings
      </button>
    </div>
    <ActivityTargetPicker
      v-if="picking && enabled"
      :import-id="importId"
      :plan-id="planId"
      :mapping="picking"
      :disabled="disabled"
      @close="picking = null"
      @select="picked"
    />
  </Card>
</template>
