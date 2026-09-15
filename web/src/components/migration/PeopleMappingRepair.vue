<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import ConfirmDialog from '../ConfirmDialog.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchImportMembers, fetchImportStages, importAccessError, uncertainImportError, useImportAccess } from '../../api/imports'
import { fetchRepairMappings, postRepair, repairPath, type RepairChoice, type RepairMapping, type RepairOwner } from '../../api/peopleMappingRepairs'
import { refreshInteger, type PeopleRefresh } from '../../api/peopleRefreshes'

const props = defineProps<{ owner: RepairOwner; current: PeopleRefresh; disabled: boolean }>()
const emit = defineEmits<{ selected: [id: string]; reload: []; denied: []; busy: [value: boolean] }>()
const access = useImportAccess()
const cursors = ref(['']); const drafts = ref<Record<string, RepairChoice>>({})
const error = ref(''); const confirming = ref(false); const pending = ref(false)
const intent = ref<{ path: string; body: unknown; identity: string; root: string } | null>(null)
const repair = computed(() => props.current.mode === 'mapping_repair')
const editing = computed(() => access.enabled.value && repair.value && props.current.repair_candidates_complete && (props.current.state === 'ready' || props.current.state === 'paused' && props.current.pause_reason === 'awaiting_mapping_choices'))
const branch = computed(() => [...access.prefix.value, 'mapping-repair', props.owner, props.current.id, props.current.repair_draft_revision])
const key = computed(() => [...branch.value, cursors.value.at(-1)])
const stageKey = computed(() => [...branch.value, 'stages']); const memberKey = computed(() => [...branch.value, 'members'])
const enabled = computed(() => access.enabled.value && repair.value && !!props.current.repair_candidates_complete)
const mappingQuery = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchRepairMappings(props.owner, props.current.id, cursors.value.at(-1) || undefined, signal)) })
const stageQuery = useQuery({ queryKey: stageKey, enabled: editing, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(stageKey.value, () => stageKey.value, () => fetchImportStages(signal)) })
const memberQuery = useQuery({ queryKey: memberKey, enabled: editing, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(memberKey.value, () => memberKey.value, () => fetchImportMembers(signal)) })
const page = computed(() => enabled.value ? mappingQuery.data.value : undefined)
const stages = computed(() => editing.value ? stageQuery.data.value?.stages ?? [] : [])
const members = computed(() => editing.value ? memberQuery.data.value?.members.filter(v => v.status === 'active') ?? [] : [])
const busy = computed(() => props.disabled || pending.value || !!intent.value)
watch(() => pending.value || !!intent.value, value => emit('busy', value), { flush: 'sync' })
const canStart = computed(() => access.enabled.value && !!props.current.plan && ['ready', 'completed', 'cancelled'].includes(props.current.state) && (props.current.mapping_repair_available ?? props.current.plan.counts.held !== '0'))
const changed = computed(() => Object.keys(drafts.value).length)
let disposed = false
watch(() => JSON.stringify([access.scope.value, props.current.id, props.current.repair_draft_revision]), () => { cursors.value = ['']; drafts.value = {}; confirming.value = false; error.value = '' }, { flush: 'sync' })
watch(access.identity, () => { intent.value = null; pending.value = false }, { flush: 'sync' })
watch(() => [mappingQuery.error.value, stageQuery.error.value, memberQuery.error.value], errors => { if (errors.some(importAccessError)) { access.denied.value = true; emit('denied') } })
onBeforeUnmount(() => { emit('busy', false); disposed = true; access.remove(branch.value) })
const label = (row: RepairMapping) => row.source.kind === 'missing_stage' ? 'Source stage explicitly empty' : row.source.key ?? 'Unknown source key'
const selected = (row: RepairMapping) => { const value = drafts.value[row.id] ?? row.choice; return value ? `${value.disposition}${value.target_id ? `:${value.target_id}` : ''}` : '' }
function change(row: RepairMapping, event: Event) {
  if (!editing.value || busy.value) return
  const value = (event.target as HTMLSelectElement).value
  if (!value) { delete drafts.value[row.id]; return }
  const [disposition, target] = value.split(':')
  drafts.value[row.id] = { key_id: row.id, disposition: disposition as RepairChoice['disposition'], target_id: target ?? null }
}
async function send(value: NonNullable<typeof intent.value>) {
  if (!access.enabled.value || pending.value || value.identity !== access.identity.value || value.root !== props.current.id) return
  const scope = access.scope.value
  intent.value = value; pending.value = true; confirming.value = false; error.value = ''
  try {
    const result = await postRepair(value.path, value.body)
    if (disposed || value.identity !== access.identity.value) return
    if (!access.enabled.value || scope !== access.scope.value || value.root !== props.current.id) { error.value = 'Access or selection changed. Retry the same request after verifying access to recover the saved outcome.'; return }
    intent.value = null; drafts.value = {}; cursors.value = ['']; emit('selected', result.refresh_id); emit('reload')
    await access.client.invalidateQueries({ queryKey: branch.value })
  } catch (e) {
    if (disposed || value.identity !== access.identity.value) return
    if (importAccessError(e)) { intent.value = null; access.denied.value = true; emit('denied'); return }
    if (!uncertainImportError(e)) intent.value = null
    error.value = uncertainImportError(e) ? 'The outcome is uncertain. Retry this same request to recover its receipt.' : describeApiError(e, 'Mapping repair changed. Reload and review before continuing.')
  } finally { if (!disposed && value.identity === access.identity.value) pending.value = false }
}
function command(path: string, body: unknown) { void send({ path, body, identity: access.identity.value, root: props.current.id }) }
function start() {
  if (!canStart.value || busy.value || !confirming.value || !props.current.plan) return
  const value = props.current
  try {
    command(`${repairPath(props.owner, value.id)}/mapping-repairs`, { request_id: crypto.randomUUID(), report_id: value.report_id, expected_lifecycle_revision: refreshInteger(value.lifecycle_revision), anchor: value.state === 'ready' || value.state === 'cancelled' && !value.confirmed_refresh_plan_id ? { kind: 'preview', plan_id: value.plan!.id, plan_revision: refreshInteger(value.plan!.revision) } : { kind: 'results', plan_id: value.plan!.id } })
  } catch { error.value = 'The revision cannot be submitted safely. Reload the refresh.' }
}
function save() {
  if (!editing.value || busy.value || !changed.value || changed.value > 50) return
  try { command(`${repairPath(props.owner, props.current.id)}/repair-mappings`, { request_id: crypto.randomUUID(), expected_draft_revision: refreshInteger(props.current.repair_draft_revision!), choices: Object.values(drafts.value) }) }
  catch { error.value = 'The revision cannot be submitted safely.' }
}
function preview() {
  if (!editing.value || busy.value || changed.value) return
  try { command(`${repairPath(props.owner, props.current.id)}/plans`, { request_id: crypto.randomUUID(), expected_plan_revision: refreshInteger(props.current.lifecycle_revision) }) }
  catch { error.value = 'The revision cannot be submitted safely.' }
}
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="my-4 space-y-3 rounded border border-border p-4"
  >
    <h3 class="text-body font-semibold">
      {{ repair ? 'Repair stage and agent mappings' : 'Held because of a stage or agent mapping?' }}
    </h3>
    <p class="text-small text-text-muted">
      Repairs apply only to held People who were already imported. Local edits stay protected. Names and contact changes appear in the full preview before you confirm.
    </p>
    <template v-if="repair">
      <button
        v-if="current.repair_source_refresh_id"
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="busy"
        @click="emit('selected', current.repair_source_refresh_id)"
      >
        View original held evidence
      </button>
      <p
        v-if="!current.repair_candidates_complete"
        class="text-small"
      >
        Finding eligible held People in saved evidence…
      </p>
      <p
        v-else
        class="text-small"
      >
        {{ current.repair_candidate_count }} People in this repair. Unresolved mappings remain held. Approval takes effect only after that Person succeeds.
      </p>
      <p
        v-if="current.plan?.mapping_repair"
        class="text-small"
      >
        This preview includes {{ current.plan.mapping_repair.approval_only_count }} mapping approvals that need no CRM field changes.
      </p>
      <div
        v-for="row in page?.items ?? []"
        :key="row.id"
        class="space-y-1 border-b border-border pb-3"
      >
        <label
          :for="`repair-${row.id}`"
          class="block break-words text-small font-medium"
        >{{ row.kind === 'stage' ? 'Stage' : 'Agent / pond' }}: {{ label(row) }} · {{ row.affected_count }} People</label>
        <select
          :id="`repair-${row.id}`"
          :class="INPUT_CLASSES"
          :value="selected(row)"
          :disabled="!editing || busy"
          @change="change(row, $event)"
        >
          <option value="">
            Keep current approval if valid
          </option>
          <option value="unresolved">
            Leave unresolved
          </option>
          <option
            v-if="row.kind === 'assignee'"
            value="unassigned"
          >
            Explicitly unassigned
          </option>
          <option
            v-if="row.choice?.target_id"
            :value="`${row.choice.disposition}:${row.choice.target_id}`"
          >
            Saved choice: {{ row.choice.target?.name ?? row.choice.target?.email ?? 'target no longer available' }}
          </option>
          <template v-if="row.kind === 'stage'">
            <option
              v-for="stage in stages"
              :key="stage.id"
              :value="`existing:${stage.id}`"
            >
              {{ stage.name }}
            </option>
          </template>
          <template v-else>
            <option
              v-for="member in members"
              :key="member.user_id"
              :value="`member:${member.user_id}`"
            >
              {{ member.display_name }} · {{ member.email }}
            </option>
          </template>
        </select>
      </div>
      <div
        v-if="page"
        class="flex flex-wrap gap-2"
      >
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="busy || changed > 0 || cursors.length < 2"
          @click="cursors.pop()"
        >
          Previous mappings
        </button>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="busy || changed > 0 || !page.next_cursor"
          @click="page.next_cursor && cursors.push(page.next_cursor)"
        >
          Next mappings
        </button>
      </div>
      <div
        v-if="editing"
        class="flex flex-wrap gap-2"
      >
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="busy || changed === 0"
          @click="save"
        >
          Save mapping choices
        </button>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="busy || changed > 0 || current.repair_candidate_count === '0'"
          @click="preview"
        >
          Prepare full People preview
        </button>
      </div>
    </template>
    <button
      v-if="canStart"
      type="button"
      :class="buttonClasses('secondary')"
      :disabled="busy"
      @click="confirming = true"
    >
      Start mapping repair from held evidence
    </button>
    <p
      v-for="e in [mappingQuery.error.value, stageQuery.error.value, memberQuery.error.value].filter(Boolean)"
      :key="String(e)"
      role="alert"
      class="text-small text-danger"
    >
      {{ describeApiError(e, 'Could not load mappings. Reload to try again.') }}
    </p>
    <p
      v-if="error"
      role="alert"
      class="text-small text-danger"
    >
      {{ error }}
    </p>
    <button
      v-if="intent"
      type="button"
      :class="buttonClasses('secondary')"
      :disabled="pending || intent.root !== current.id"
      @click="send(intent)"
    >
      Retry same mapping request
    </button>
    <ConfirmDialog
      :is-pending="pending"
      :visible="confirming"
      title="Start this mapping repair?"
      :message="current.state === 'ready' ? 'Retire this unconfirmed preview and preserve its held evidence. Prepare a separate repair using the same saved source report. No CRM fields change until you review and confirm the repair.' : 'Prepare a repair from held evidence using the same saved source report. Successfully settled People are excluded. You will review the mappings and full People changes before confirming.'"
      confirm-label="Prepare mapping repair"
      @update:visible="confirming = $event"
      @confirm="start"
    />
  </section>
</template>
