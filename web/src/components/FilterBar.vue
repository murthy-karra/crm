<script setup lang="ts">
// PeopleView's FilterBar (docs/specs/SLICE_011a.md §6): an "Add filter"
// control opening one small editor per axis; active clauses render as
// removable chips on Badge.vue — neutral tint (UI_STYLE §3: warm stays
// reserved for source/origin accents). Purely presentational + emits — all
// URL sync and server-degrade handling lives in PeopleView.vue.
import { computed, ref, watch } from 'vue'
import Select from 'primevue/select'
import { Plus, X } from 'lucide-vue-next'
import Badge from './Badge.vue'
import type { AgeOp, Assignee, FilterClause, FilterClauseKind, Member, Stage } from '../api/types'
import { buttonClasses, INPUT_CLASSES, selectPt } from '../lib/controls'
import {
  CLAUSE_KIND_LABEL,
  FILTER_CLAUSE_KINDS,
  defaultClauseFor,
  describeClause,
  type FilterNames,
} from '../lib/filter'

type StageClause = Extract<FilterClause, { kind: 'stage' }>
type AssignedToClause = Extract<FilterClause, { kind: 'assigned_to' }>
type SourceClause = Extract<FilterClause, { kind: 'source' }>
type AgeClauseKind = 'created' | 'last_inquiry' | 'last_contact' | 'last_inbound'
type AgeClause = Extract<FilterClause, { kind: AgeClauseKind }>
type BoolClauseKind = 'has_replied' | 'has_phone' | 'has_email'
type BoolClause = Extract<FilterClause, { kind: BoolClauseKind }>

function isAgeKind(kind: FilterClauseKind): kind is AgeClauseKind {
  return kind === 'created' || kind === 'last_inquiry' || kind === 'last_contact' || kind === 'last_inbound'
}
function isBoolKind(kind: FilterClauseKind): kind is BoolClauseKind {
  return kind === 'has_replied' || kind === 'has_phone' || kind === 'has_email'
}

const props = defineProps<{
  clauses: FilterClause[]
  stages: Stage[]
  members: Member[]
  sources: string[]
}>()

const emit = defineEmits<{ 'update:clauses': [FilterClause[]] }>()

const names = computed<FilterNames>(() => ({
  stageNames: Object.fromEntries(props.stages.map((s) => [s.id, s.name])),
  memberNames: Object.fromEntries(props.members.map((m) => [m.user_id, m.display_name])),
}))

const usedKinds = computed(() => new Set(props.clauses.map((c) => c.kind)))
const addableKinds = computed(() => FILTER_CLAUSE_KINDS.filter((k) => !usedKinds.value.has(k)))
const addKindOptions = computed(() => addableKinds.value.map((k) => ({ value: k, label: CLAUSE_KIND_LABEL[k] })))

const editingKind = ref<FilterClauseKind | null>(null)
const editingAgeKind = computed(() => (editingKind.value && isAgeKind(editingKind.value) ? editingKind.value : null))
const editingBoolKind = computed(() => (editingKind.value && isBoolKind(editingKind.value) ? editingKind.value : null))

function updateClauses(next: FilterClause[]) {
  emit('update:clauses', next)
}

function addClause(kind: FilterClauseKind | null) {
  if (!kind) return
  updateClauses([...props.clauses, defaultClauseFor(kind)])
  editingKind.value = kind
}

function removeClause(kind: FilterClauseKind) {
  updateClauses(props.clauses.filter((c) => c.kind !== kind))
  if (editingKind.value === kind) editingKind.value = null
}

function toggleEditor(kind: FilterClauseKind) {
  editingKind.value = editingKind.value === kind ? null : kind
}

/**
 * Review R1 fix: APPEND-IF-MISSING. `props.clauses` can legitimately no
 * longer contain `kind` while its editor is still open (e.g. the parent
 * cleared the filter out from under an open editor — a URL-origin
 * degrade, or any future reason) — the old `.map()`-only version silently
 * no-op'd in that case (no matching element to replace), so ticking a
 * checkbox produced no `update:clauses` change at all and the editor was
 * permanently wedged. Appending when `kind` isn't found makes that class
 * of bug structurally impossible: a commit always lands somewhere.
 */
function replaceClause(kind: FilterClauseKind, clause: FilterClause | null) {
  if (clause === null) {
    removeClause(kind)
    return
  }
  const exists = props.clauses.some((c) => c.kind === kind)
  updateClauses(
    exists ? props.clauses.map((c) => (c.kind === kind ? clause : c)) : [...props.clauses, clause],
  )
}

// ---- Multi-value axes (stage / assigned_to / source) ---------------------

const stageClause = computed<StageClause>(
  () => (props.clauses.find((c): c is StageClause => c.kind === 'stage') ?? { kind: 'stage', stage_ids: [] }),
)
const assignedToClause = computed<AssignedToClause>(
  () =>
    props.clauses.find((c): c is AssignedToClause => c.kind === 'assigned_to') ?? {
      kind: 'assigned_to',
      assignees: [],
    },
)
const sourceClause = computed<SourceClause>(
  () => props.clauses.find((c): c is SourceClause => c.kind === 'source') ?? { kind: 'source', sources: [] },
)

function toggleStage(stageId: string) {
  const current = stageClause.value.stage_ids
  const has = current.includes(stageId)
  const next = has ? current.filter((id) => id !== stageId) : [...current, stageId]
  replaceClause('stage', next.length > 0 ? { kind: 'stage', stage_ids: next } : null)
}

function assigneeEquals(a: Assignee, b: Assignee): boolean {
  if (typeof a === 'string' || typeof b === 'string') return a === b
  return a.user_id === b.user_id
}
function isAssigneeSelected(value: Assignee): boolean {
  return assignedToClause.value.assignees.some((a) => assigneeEquals(a, value))
}
function toggleAssignee(value: Assignee) {
  const current = assignedToClause.value.assignees
  const has = isAssigneeSelected(value)
  const next = has ? current.filter((a) => !assigneeEquals(a, value)) : [...current, value]
  replaceClause('assigned_to', next.length > 0 ? { kind: 'assigned_to', assignees: next } : null)
}

function toggleSource(source: string) {
  const current = sourceClause.value.sources
  const has = current.includes(source)
  const next = has ? current.filter((s) => s !== source) : [...current, source]
  replaceClause('source', next.length > 0 ? { kind: 'source', sources: next } : null)
}

// ---- Age axes --------------------------------------------------------------

const AGE_OPS: { value: AgeOp['op']; label: string }[] = [
  { value: 'within_days', label: 'within' },
  { value: 'not_within_days', label: 'not within' },
  { value: 'never', label: 'never' },
]

const activeAgeClause = computed<AgeClause | null>(() => {
  const kind = editingAgeKind.value
  if (!kind) return null
  return props.clauses.find((c): c is AgeClause => c.kind === kind) ?? null
})

/** Local draft so the days input commits on blur/Enter, not per keystroke
 * (§6). Keyed by clause kind since at most one clause per kind exists. */
const daysDraft = ref<Record<string, string>>({})

watch(activeAgeClause, (clause) => {
  if (clause && clause.age.op !== 'never' && daysDraft.value[clause.kind] === undefined) {
    daysDraft.value[clause.kind] = String(clause.age.days)
  }
})

function onDaysInput(event: Event) {
  const kind = editingAgeKind.value
  if (!kind) return
  daysDraft.value[kind] = (event.target as HTMLInputElement).value
}

function setAgeOp(op: AgeOp['op']) {
  const kind = editingAgeKind.value
  if (!kind) return
  // M1: Math.trunc — a fractional draft (e.g. "7.5" mid-edit) must never
  // serialize as a float; the server 400s a non-integer `days` (§4b),
  // which would otherwise wipe the filter via the degrade path.
  const age: AgeOp =
    op === 'never' ? { op: 'never' } : { op, days: Math.trunc(Number(daysDraft.value[kind] ?? 30)) || 30 }
  replaceClause(kind, { kind, age } as AgeClause)
}

function commitDays() {
  const kind = editingAgeKind.value
  const clause = activeAgeClause.value
  if (!kind || !clause || clause.age.op === 'never') return
  // M1: Math.trunc before clamping — integers only ever reach the model.
  const days = Math.max(1, Math.min(3650, Math.trunc(Number(daysDraft.value[kind])) || 1))
  daysDraft.value[kind] = String(days)
  replaceClause(kind, { kind, age: { op: clause.age.op, days } } as AgeClause)
}

// ---- Boolean axes -----------------------------------------------------------

const activeBoolClause = computed<BoolClause | null>(() => {
  const kind = editingBoolKind.value
  if (!kind) return null
  return props.clauses.find((c): c is BoolClause => c.kind === kind) ?? null
})

function setBoolValue(value: boolean) {
  const kind = editingBoolKind.value
  if (!kind) return
  replaceClause(kind, { kind, value } as BoolClause)
}
</script>

<template>
  <div class="mb-4 flex flex-col gap-3">
    <div class="flex flex-wrap items-center gap-2">
      <Badge
        v-for="clause in clauses"
        :key="clause.kind"
        tint="neutral"
      >
        <span :data-testid="`filter-chip-${clause.kind}`">
          <button
            type="button"
            class="mr-1"
            @click="toggleEditor(clause.kind)"
          >
            {{ describeClause(clause, names) }}
          </button>
          <button
            type="button"
            :data-testid="`filter-chip-remove-${clause.kind}`"
            class="inline-flex items-center"
            :aria-label="`Remove ${CLAUSE_KIND_LABEL[clause.kind]} filter`"
            @click="removeClause(clause.kind)"
          >
            <X
              class="h-3 w-3"
              stroke-width="2"
            />
          </button>
        </span>
      </Badge>

      <Select
        v-if="addableKinds.length > 0"
        data-testid="filter-add"
        :model-value="null"
        :options="addKindOptions"
        option-label="label"
        option-value="value"
        placeholder="Add filter"
        :pt="selectPt()"
        class="w-40"
        @update:model-value="addClause"
      >
        <template #dropdownicon>
          <Plus
            class="h-4 w-4"
            stroke-width="1.5"
          />
        </template>
      </Select>
    </div>

    <div
      v-if="editingKind"
      :data-testid="`filter-editor-${editingKind}`"
      class="rounded-xl border border-border bg-surface-0 p-4"
    >
      <template v-if="editingKind === 'stage'">
        <p class="mb-2 text-small font-medium text-text-muted">
          Stage
        </p>
        <div class="flex flex-wrap gap-3">
          <label
            v-for="stage in stages"
            :key="stage.id"
            class="flex items-center gap-1.5 text-body text-text"
          >
            <input
              type="checkbox"
              :checked="stageClause.stage_ids.includes(stage.id)"
              @change="toggleStage(stage.id)"
            >
            {{ stage.name }}
          </label>
        </div>
      </template>

      <template v-else-if="editingKind === 'assigned_to'">
        <p class="mb-2 text-small font-medium text-text-muted">
          Assigned to
        </p>
        <div class="flex flex-wrap gap-3">
          <label class="flex items-center gap-1.5 text-body text-text">
            <input
              type="checkbox"
              data-testid="filter-assignee-me"
              :checked="isAssigneeSelected('me')"
              @change="toggleAssignee('me')"
            >
            Me
          </label>
          <label class="flex items-center gap-1.5 text-body text-text">
            <input
              type="checkbox"
              data-testid="filter-assignee-unassigned"
              :checked="isAssigneeSelected('unassigned')"
              @change="toggleAssignee('unassigned')"
            >
            Unassigned
          </label>
          <label
            v-for="member in members"
            :key="member.user_id"
            class="flex items-center gap-1.5 text-body text-text"
          >
            <input
              type="checkbox"
              :data-testid="`filter-assignee-user-${member.user_id}`"
              :checked="isAssigneeSelected({ user_id: member.user_id })"
              @change="toggleAssignee({ user_id: member.user_id })"
            >
            {{ member.display_name }}<span
              v-if="member.status === 'inactive'"
              class="text-text-muted"
            > (inactive)</span>
          </label>
        </div>
      </template>

      <template v-else-if="editingKind === 'source'">
        <p class="mb-2 text-small font-medium text-text-muted">
          Source
        </p>
        <div
          v-if="sources.length === 0"
          class="text-body text-text-muted"
        >
          No inquiry sources yet.
        </div>
        <div
          v-else
          class="flex flex-wrap gap-3"
        >
          <label
            v-for="source in sources"
            :key="source"
            class="flex items-center gap-1.5 text-body text-text"
          >
            <input
              type="checkbox"
              :checked="sourceClause.sources.includes(source)"
              @change="toggleSource(source)"
            >
            {{ source }}
          </label>
        </div>
      </template>

      <template v-else-if="editingAgeKind && activeAgeClause">
        <p class="mb-2 text-small font-medium text-text-muted">
          {{ CLAUSE_KIND_LABEL[editingAgeKind] }}
        </p>
        <div class="flex items-center gap-2">
          <Select
            :data-testid="`filter-age-op-${editingAgeKind}`"
            :model-value="activeAgeClause.age.op"
            :options="AGE_OPS"
            option-label="label"
            option-value="value"
            :pt="selectPt()"
            class="w-36"
            @update:model-value="setAgeOp"
          />
          <template v-if="activeAgeClause.age.op !== 'never'">
            <input
              :data-testid="`filter-days-${editingAgeKind}`"
              type="number"
              min="1"
              max="3650"
              :class="INPUT_CLASSES"
              class="w-20"
              :value="daysDraft[editingAgeKind]"
              @input="onDaysInput"
              @blur="commitDays"
              @keydown.enter="commitDays"
            >
            <span class="text-body text-text-muted">days</span>
          </template>
        </div>
      </template>

      <template v-else-if="editingBoolKind && activeBoolClause">
        <p class="mb-2 text-small font-medium text-text-muted">
          {{ CLAUSE_KIND_LABEL[editingBoolKind] }}
        </p>
        <div class="flex gap-2">
          <button
            type="button"
            :data-testid="`filter-bool-yes-${editingBoolKind}`"
            :class="buttonClasses(activeBoolClause.value === true ? 'primary' : 'secondary')"
            @click="setBoolValue(true)"
          >
            Yes
          </button>
          <button
            type="button"
            :data-testid="`filter-bool-no-${editingBoolKind}`"
            :class="buttonClasses(activeBoolClause.value === false ? 'primary' : 'secondary')"
            @click="setBoolValue(false)"
          >
            No
          </button>
        </div>
      </template>

      <button
        type="button"
        data-testid="filter-editor-done"
        class="mt-3 text-small font-medium text-accent"
        @click="editingKind = null"
      >
        Done
      </button>
    </div>
  </div>
</template>
