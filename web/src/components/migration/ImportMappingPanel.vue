<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ImportFieldViewer from './ImportFieldViewer.vue'
import { fetchImportMappings, fetchImportMembers, fetchImportStages, sourceName, useImportAccess, type AssigneeChoice, type ImportFieldRequest, type ImportMapping, type ImportPatches, type StageChoice } from '../../api/imports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel } from './format'
const props = defineProps<{ importId: string; planId: string; revision: string; canReplan: boolean }>()
const emit = defineEmits<{ apply: [patches: ImportPatches]; dirty: [value: boolean] }>()
const access = useImportAccess()
const kind = ref<'stage' | 'assignee'>('stage')
const cursors = ref<string[]>([''])
const stageDrafts = ref<Record<string, StageChoice>>({})
const assigneeDrafts = ref<Record<string, AssigneeChoice>>({})
const message = ref('')
const selected = ref<{ request: ImportFieldRequest; title: string } | null>(null)
const branch = computed(() => [...access.prefix.value, 'mappings', props.importId, props.planId, props.revision])
const key = computed(() => [...branch.value, kind.value, cursors.value.at(-1) ?? ''])
const stageKey = computed(() => [...branch.value, 'destination-stages'])
const memberKey = computed(() => [...branch.value, 'destination-members'])
const enabled = computed(() => access.enabled.value && !!props.importId && !!props.planId)
const reading = useQuery({ queryKey: key, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchImportMappings(props.importId, props.planId, kind.value, cursors.value.at(-1) || undefined, signal)), enabled, retry: false, gcTime: 0 })
const stages = useQuery({ queryKey: stageKey, queryFn: ({ signal }) => access.read(stageKey.value, () => stageKey.value, () => fetchImportStages(signal)), enabled: computed(() => enabled.value && props.canReplan), retry: false, gcTime: 0 })
const members = useQuery({ queryKey: memberKey, queryFn: ({ signal }) => access.read(memberKey.value, () => memberKey.value, () => fetchImportMembers(signal)), enabled: computed(() => enabled.value && props.canReplan), retry: false, gcTime: 0 })
const page = computed(() => enabled.value ? reading.data.value : undefined)
const activeMembers = computed(() => enabled.value ? members.data.value?.members.filter(m => m.status === 'active') ?? [] : [])
const changes = computed(() => Object.keys(stageDrafts.value).length + Object.keys(assigneeDrafts.value).length)
watch(() => changes.value > 0, value => emit('dirty', value), { immediate: true, flush: 'sync' })
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.revision]), () => { stageDrafts.value = {}; assigneeDrafts.value = {}; cursors.value = ['']; selected.value = null; message.value = '' }, { flush: 'sync' })
watch(kind, () => { cursors.value = ['']; selected.value = null })
watch(() => cursors.value.at(-1), () => { selected.value = null })
onBeforeUnmount(() => { emit('dirty', false); access.remove(branch.value) })
function encode(choice: StageChoice | AssigneeChoice) { return choice.kind === 'existing' ? `existing:${choice.stage_id}` : choice.kind === 'member' ? `member:${choice.user_id}` : choice.kind }
function choiceFor(row: ImportMapping) { return kind.value === 'stage' ? stageDrafts.value[row.source_key] ?? row.choice : assigneeDrafts.value[row.source_key] ?? row.choice }
function change(row: ImportMapping, event: Event) {
  if (!enabled.value || !props.canReplan) return
  const input = event.target as HTMLSelectElement
  const value = input.value
  const choice = value.startsWith('existing:') ? { kind: 'existing' as const, stage_id: value.slice(9) }
    : value.startsWith('member:') ? { kind: 'member' as const, user_id: value.slice(7) }
      : value === 'create' ? { kind: 'create' as const } : value === 'unassigned' ? { kind: 'unassigned' as const } : { kind: 'hold' as const }
  const drafts = kind.value === 'stage' ? stageDrafts.value : assigneeDrafts.value
  if (!drafts[row.source_key] && changes.value >= 50 && encode(choice) !== encode(row.choice)) {
    message.value = 'Apply these 50 changes before choosing more mappings.'; input.value = encode(choiceFor(row)); return
  }
  if (encode(choice) === encode(row.choice)) delete drafts[row.source_key]
  else if (kind.value === 'stage') stageDrafts.value[row.source_key] = choice as StageChoice
  else assigneeDrafts.value[row.source_key] = choice as AssigneeChoice
  message.value = ''
}
function apply() {
  if (!enabled.value || !props.canReplan || !changes.value || changes.value > 50) return
  emit('apply', { stage_mappings: Object.entries(stageDrafts.value).sort(([a], [b]) => a.localeCompare(b)).map(([source_key, choice]) => ({ source_key, choice: { ...choice } })), assignee_mappings: Object.entries(assigneeDrafts.value).sort(([a], [b]) => a.localeCompare(b)).map(([source_key, choice]) => ({ source_key, choice: { ...choice } })) })
}
function discard() { if (enabled.value && props.canReplan) { stageDrafts.value = {}; assigneeDrafts.value = {}; message.value = '' } }
function inspect(row: ImportMapping) { selected.value = { request: { kind: 'mapping', importId: props.importId, planId: props.planId, mappingId: row.id, fieldKey: 'provenance' }, title: `Source fields for ${row.source_key}` } }
</script>
<template>
  <Card class="min-w-0">
    <h2 class="text-section font-semibold text-text">
      Stage and assignment mappings
    </h2>
    <p class="mt-1 text-small text-text-muted">
      Choices apply only to this import. Unchanged choices carry forward; matching suggestions need your approval.
    </p>
    <div
      class="mt-3 flex flex-wrap gap-2"
      aria-label="Mapping kind"
    >
      <button
        type="button"
        :class="buttonClasses(kind === 'stage' ? 'primary' : 'secondary')"
        :aria-pressed="kind === 'stage'"
        @click="kind = 'stage'"
      >
        Stage mappings
      </button>
      <button
        type="button"
        :class="buttonClasses(kind === 'assignee' ? 'primary' : 'secondary')"
        :aria-pressed="kind === 'assignee'"
        @click="kind = 'assignee'"
      >
        Assignment mappings
      </button>
    </div>
    <p
      v-if="!enabled"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      Mappings require a current administrator session.
    </p>
    <p
      v-if="reading.isPending.value && enabled"
      role="status"
      class="mt-3 text-small text-text-muted"
    >
      Loading mappings…
    </p>
    <p
      v-if="reading.error.value || stages.error.value || members.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value ?? stages.error.value ?? members.error.value, 'Could not load mapping choices. Refresh to recover.') }}
    </p>
    <div
      v-if="page"
      class="mt-3 min-w-0 overflow-x-auto"
    >
      <table class="w-full min-w-[600px] text-left text-small">
        <thead class="text-text-muted">
          <tr>
            <th class="p-2 font-medium">
              Source
            </th><th class="p-2 font-medium">
              Choice
            </th><th class="p-2 font-medium">
              Review
            </th>
          </tr>
        </thead><tbody>
          <tr
            v-for="row in page.mappings"
            :key="row.id"
            class="border-t border-border align-top"
          >
            <td class="max-w-64 p-2">
              <p class="whitespace-pre-wrap break-all font-medium">
                {{ sourceName(row.source) }}
              </p><p class="break-all text-text-muted">
                Source {{ row.source_key }} · {{ row.dependent_count }} People
              </p><p
                v-if="row.source?.label?.abbreviated || row.source?.name?.abbreviated || row.source?.email?.abbreviated"
                class="text-text-muted"
              >
                Source text abbreviated; inspect the complete fields.
              </p><button
                v-if="row.source"
                type="button"
                :class="buttonClasses('ghost')"
                @click="inspect(row)"
              >
                Inspect source {{ row.source_key }}
              </button>
            </td>
            <td class="min-w-64 max-w-80 p-2">
              <select
                v-if="canReplan"
                :value="encode(choiceFor(row))"
                :aria-label="`${kind === 'stage' ? 'Stage' : 'Assignment'} choice for source ${row.source_key}`"
                :class="INPUT_CLASSES"
                :disabled="!enabled || stages.isPending.value || members.isPending.value || !!stages.error.value || !!members.error.value"
                @change="change(row, $event)"
              >
                <option value="hold">
                  Hold for later
                </option>
                <template v-if="kind === 'stage'">
                  <option
                    v-if="row.qualified && row.source?.can_create"
                    value="create"
                  >
                    Create captured stage: {{ row.source.label?.value }}
                  </option><option
                    v-for="stage in stages.data.value?.stages ?? []"
                    :key="stage.id"
                    :value="`existing:${stage.id}`"
                  >
                    Existing stage: {{ stage.name }}
                  </option>
                </template>
                <template v-else>
                  <option value="unassigned">
                    Leave unassigned
                  </option><option
                    v-for="member in activeMembers"
                    :key="member.user_id"
                    :value="`member:${member.user_id}`"
                  >
                    Member: {{ member.display_name }} ({{ member.email }})
                  </option>
                </template>
              </select>
              <p
                v-else
                class="break-all"
              >
                {{ snapshotLabel(row.disposition) }}{{ row.target ? ` · ${row.target.name}` : '' }}
              </p>
              <p
                v-for="suggestion in row.suggestions"
                :key="suggestion.id"
                class="mt-1 break-all text-text-muted"
              >
                Suggested: {{ suggestion.name }}{{ suggestion.email ? ` (${suggestion.email})` : '' }}
              </p>
            </td>
            <td class="max-w-64 p-2">
              <p
                v-if="!row.qualified"
                class="text-danger"
              >
                Source evidence is not qualified. A choice cannot resolve this hold.
              </p><ul class="space-y-1">
                <li
                  v-for="reason in row.reasons"
                  :key="reason"
                >
                  {{ snapshotLabel(reason) }}
                </li>
              </ul>
            </td>
          </tr>
        </tbody>
      </table>
      <p
        v-if="!page.mappings.length"
        class="py-3 text-small text-text-muted"
      >
        No mappings in this page.
      </p>
    </div>
    <div
      v-if="enabled"
      class="mt-3 flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch(); stages.refetch(); members.refetch()"
      >
        Refresh mappings
      </button>
      <button
        v-if="canReplan"
        type="button"
        :class="buttonClasses('primary')"
        :disabled="!changes || changes > 50"
        @click="apply"
      >
        Apply {{ changes }} mapping changes
      </button>
      <button
        v-if="canReplan && changes"
        type="button"
        :class="buttonClasses('secondary')"
        @click="discard"
      >
        Discard mapping changes
      </button>
    </div>
    <p
      v-if="message"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ message }}
    </p>
    <ImportFieldViewer
      v-if="selected && enabled"
      :request="selected.request"
      :title="selected.title"
    />
  </Card>
</template>
