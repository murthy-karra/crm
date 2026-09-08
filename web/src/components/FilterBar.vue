<script setup lang="ts">
// Presentational filter controls. PeopleView owns URL synchronization and
// fetching; opening an editor never changes its committed clauses.
import { computed, nextTick, ref, useId, watch } from 'vue'
import Popover, { type PopoverPassThroughOptions } from 'primevue/popover'
import { ChevronDown, ChevronRight, Lock, SlidersHorizontal, X } from 'lucide-vue-next'
import StageLabel from './StageLabel.vue'
import type { AgeOp, Assignee, FilterClause, FilterClauseKind, Member, Stage, StageRef, TagRef } from '../api/types'
import { BUTTON_BASE, buttonClasses, INPUT_CLASSES } from '../lib/controls'
import {
  CLAUSE_KIND_LABEL,
  FILTER_CLAUSE_KINDS,
  committedClauses,
  describeClause,
  type FilterNames,
} from '../lib/filter'

type AgeKind = 'created' | 'last_inquiry' | 'last_contact' | 'last_inbound'
type AgeClause = Extract<FilterClause, { kind: AgeKind }>
type BoolKind = 'has_replied' | 'has_phone' | 'has_email' | 'awaiting_response' | 'client_replied_unanswered' | 'awaiting_call_outcome'
type BoolClause = Extract<FilterClause, { kind: BoolKind }>
type OptionKind = 'stage' | 'assigned_to' | 'source' | 'tags' | 'not_tags'
type OptionValue = string | { user_id: string }
type Option = { key: string; label: string; value: OptionValue; stage?: StageRef; inactive?: boolean }

const props = defineProps<{
  clauses: FilterClause[]
  stages: Stage[]
  members: Member[]
  sources: string[]
  // Slice 011e e2 (docs/specs/SLICE_011e.md §5): shared by both `tags` and
  // `not_tags` editors, like `stages` is shared by nothing else needing it.
  tags: TagRef[]
  stagesPending?: boolean
  stagesError?: boolean
  membersPending?: boolean
  membersError?: boolean
  sourcesPending?: boolean
  sourcesError?: boolean
  sourcesTruncated?: boolean
  tagsPending?: boolean
  tagsError?: boolean
  // SLICE_011d §6: the Today rules editor's locked-clause mode — the anchor
  // clause is shown but cannot be removed or negated (spec §1 rule 4), and
  // (for the two person-state feeds) `assigned_to` must keep `me` (rule 3).
  // Every other consumer (People, list editing) leaves both unset and gets
  // today's fully-editable behavior unchanged.
  lockedAnchorKind?: FilterClauseKind
  requireAssigneeMe?: boolean
}>()
const emit = defineEmits<{
  'update:clauses': [FilterClause[]]
  'retry-options': [OptionKind]
}>()

const id = useId()
const popover = ref<InstanceType<typeof Popover> | null>(null)
const panel = ref<HTMLElement | null>(null)
const toolbar = ref<HTMLElement | null>(null)
const open = ref(false)
const editingKind = ref<FilterClauseKind | null>(null)
const search = ref('')
let trigger: HTMLElement | null = null
let restoreFocus = false
const applied = computed(() => committedClauses(props.clauses))
const names = computed<FilterNames>(() => ({
  stageNames: Object.fromEntries(props.stages.map((stage) => [stage.id, stage.name])),
  memberNames: Object.fromEntries(props.members.map((member) => [member.user_id, member.display_name])),
  tagNames: Object.fromEntries(props.tags.map((tag) => [tag.id, tag.name])),
}))
const menuKinds = computed(() => FILTER_CLAUSE_KINDS.filter((kind) =>
  kind !== 'stage' && kind !== 'assigned_to' && matchesSearch(CLAUSE_KIND_LABEL[kind]),
))
const title = computed(() => editingKind.value ? CLAUSE_KIND_LABEL[editingKind.value] : 'Filters')
const activeClause = computed(() => applied.value.find((clause) => clause.kind === editingKind.value))
const ageKind = computed(() => {
  const kind = editingKind.value
  return kind === 'created' || kind === 'last_inquiry' || kind === 'last_contact' || kind === 'last_inbound' ? kind : null
})
const BOOL_KINDS = new Set<FilterClauseKind>([
  'has_replied', 'has_phone', 'has_email',
  'awaiting_response', 'client_replied_unanswered', 'awaiting_call_outcome',
])
const boolKind = computed(() => {
  const kind = editingKind.value
  return kind !== null && BOOL_KINDS.has(kind) ? (kind as BoolKind) : null
})
const optionKind = computed<OptionKind | null>(() => {
  const kind = editingKind.value
  return kind === 'stage' || kind === 'assigned_to' || kind === 'source' || kind === 'tags' || kind === 'not_tags' ? kind : null
})
const activeAge = computed(() => ageKind.value ? activeClause.value as AgeClause | undefined : undefined)
const activeBool = computed(() => boolKind.value ? activeClause.value as BoolClause | undefined : undefined)
const optionPending = computed(() => {
  const kind = optionKind.value
  if (kind === 'stage') return props.stagesPending
  if (kind === 'assigned_to') return props.membersPending
  if (kind === 'tags' || kind === 'not_tags') return props.tagsPending
  return props.sourcesPending
})
const optionError = computed(() => {
  const kind = optionKind.value
  if (kind === 'stage') return props.stagesError
  if (kind === 'assigned_to') return props.membersError
  if (kind === 'tags' || kind === 'not_tags') return props.tagsError
  return props.sourcesError
})
const optionNoun = computed(() => {
  const kind = optionKind.value
  if (kind === 'stage') return 'stages'
  if (kind === 'assigned_to') return 'assignees'
  if (kind === 'tags' || kind === 'not_tags') return 'tags'
  return 'sources'
})

const popoverPt: PopoverPassThroughOptions = {
  root: 'glass-panel z-50 mt-2 w-80 max-w-[calc(100vw-2rem)] text-text',
  content: 'p-4',
}
const rowClasses = 'flex min-h-10 w-full items-center gap-3 rounded-lg px-2 py-2 text-left text-body text-text hover:bg-surface-1'
const checkboxClasses = 'h-4 w-4 shrink-0 accent-accent focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 disabled:opacity-40'

function matchesSearch(value: string) {
  return value.toLocaleLowerCase().includes(search.value.trim().toLocaleLowerCase())
}
function stageFor(id: string): StageRef {
  return props.stages.find((stage) => stage.id === id) ?? { id, name: 'Unknown stage' }
}
function selectedValues(kind: OptionKind): OptionValue[] {
  const clause = applied.value.find((item) => item.kind === kind)
  if (clause?.kind === 'stage') return clause.stage_ids
  if (clause?.kind === 'source') return clause.sources
  if (clause?.kind === 'assigned_to') return clause.assignees
  if (clause?.kind === 'tags' || clause?.kind === 'not_tags') return clause.tag_ids
  return []
}
function optionEquals(a: OptionValue, b: OptionValue) {
  return typeof a === 'string' || typeof b === 'string' ? a === b : a.user_id === b.user_id
}
function isSelected(option: Option) {
  return optionKind.value !== null && selectedValues(optionKind.value).some((value) => optionEquals(value, option.value))
}
const selectionCount = computed(() => optionKind.value ? selectedValues(optionKind.value).length : 0)
const options = computed<Option[]>(() => {
  if (optionKind.value === 'stage') {
    const ids = [...new Set([...props.stages.map((stage) => stage.id), ...selectedValues('stage') as string[]])]
    return ids.map((id) => ({ key: id, label: stageFor(id).name, value: id, stage: stageFor(id) }))
  }
  if (optionKind.value === 'assigned_to') {
    const memberIds = new Set(props.members.map((member) => member.user_id))
    const missingMembers = selectedValues('assigned_to').filter(
      (value): value is { user_id: string } => typeof value !== 'string' && !memberIds.has(value.user_id),
    )
    return [
      { key: 'me', label: 'Me', value: 'me' },
      { key: 'unassigned', label: 'Unassigned', value: 'unassigned' },
      ...props.members.map((member) => ({ key: `user-${member.user_id}`, label: member.display_name, value: { user_id: member.user_id }, inactive: member.status === 'inactive' })),
      ...missingMembers.map((value) => ({ key: `user-${value.user_id}`, label: 'Unknown member', value })),
    ]
  }
  if (optionKind.value === 'source') {
    return [...new Set([...props.sources, ...selectedValues('source') as string[]])].map((source) => ({ key: source, label: source, value: source }))
  }
  if (optionKind.value === 'tags' || optionKind.value === 'not_tags') {
    const tagIds = new Set(props.tags.map((tag) => tag.id))
    const missingTagIds = (selectedValues(optionKind.value) as string[]).filter((id) => !tagIds.has(id))
    return [
      ...props.tags.map((tag) => ({ key: tag.id, label: tag.name, value: tag.id })),
      // Slice 011e e2 (docs/specs/SLICE_011e.md §5): an unresolvable id
      // never renders the raw uuid.
      ...missingTagIds.map((id) => ({ key: id, label: 'an unknown tag', value: id })),
    ]
  }
  return []
})
const visibleOptions = computed(() => options.value.filter((option) => matchesSearch(`${option.label}${option.inactive ? ' inactive' : ''}`)))

/** Whole-clause removal is blocked for the locked anchor (rule 4: "cannot be
 * negated or removed") and for `assigned_to` under `requireAssigneeMe`
 * (rule 3: an admin cannot strip viewer-relativity by removing the clause
 * that carries `me`). Every other kind removes normally. */
function isLockedClauseKind(kind: FilterClauseKind): boolean {
  return kind === props.lockedAnchorKind || (kind === 'assigned_to' && !!props.requireAssigneeMe)
}
function isLockedOptionValue(kind: OptionKind, value: OptionValue): boolean {
  return kind === 'assigned_to' && !!props.requireAssigneeMe && value === 'me'
}

// ---- Toolbar trigger selected state (SLICE_014 §5) -------------------------
// The chip row stays the source of truth for what's applied; this only
// changes how the two persistent toolbar triggers (Assignee, Stage) look
// when their own clause is already applied, so the toolbar itself hints at
// what's active without needing to read the chip row.
type ToggleTriggerKind = 'assigned_to' | 'stage'
const SELECTED_TRIGGER_CLASSES = `${BUTTON_BASE} bg-surface-2 text-text hover:bg-surface-1`
function triggerValueCount(kind: ToggleTriggerKind): number {
  const clause = applied.value.find((item) => item.kind === kind)
  if (!clause) return 0
  if (clause.kind === 'stage') return clause.stage_ids.length
  if (clause.kind === 'assigned_to') return clause.assignees.length
  return 0
}
function triggerLabel(kind: ToggleTriggerKind): string {
  const count = triggerValueCount(kind)
  return count > 0 ? `${CLAUSE_KIND_LABEL[kind]} · ${count}` : CLAUSE_KIND_LABEL[kind]
}
function triggerClasses(kind: ToggleTriggerKind): string {
  return triggerValueCount(kind) > 0 ? SELECTED_TRIGGER_CLASSES : buttonClasses()
}
// Clear all does nothing in locked-only mode (every applied clause is
// locked) — hide it there rather than show a control with no effect.
const hasClearableClause = computed(() => applied.value.some((clause) => !isLockedClauseKind(clause.kind)))

function updateClauses(next: FilterClause[]) {
  if (JSON.stringify(next) !== JSON.stringify(props.clauses)) emit('update:clauses', next)
}
function replaceClause(kind: FilterClauseKind, clause: FilterClause | null) {
  const next = applied.value.filter((item) => item.kind !== kind)
  if (clause) {
    const index = applied.value.findIndex((item) => item.kind === kind)
    next.splice(index < 0 ? next.length : index, 0, clause)
  }
  updateClauses(next)
}
function removeClause(kind: FilterClauseKind) {
  if (isLockedClauseKind(kind)) return
  if (kind === editingKind.value) resetAgeDraft()
  replaceClause(kind, null)
}
function clearAll() {
  resetAgeDraft()
  // A locked anchor/`me` clause survives Clear all — it is not removable by
  // any control (rule 3/4), including this one.
  updateClauses(applied.value.filter((clause) => isLockedClauseKind(clause.kind)))
}
function toggleOption(option: Option) {
  const kind = optionKind.value
  if (!kind) return
  if (isLockedOptionValue(kind, option.value)) return
  const current = selectedValues(kind)
  const selected = isSelected(option)
  if (!selected && current.length >= 50) return
  const next = selected ? current.filter((value) => !optionEquals(value, option.value)) : [...current, option.value]
  if (!next.length) replaceClause(kind, null)
  else if (kind === 'stage') replaceClause(kind, { kind, stage_ids: next as string[] })
  else if (kind === 'source') replaceClause(kind, { kind, sources: next as string[] })
  else if (kind === 'tags' || kind === 'not_tags') replaceClause(kind, { kind, tag_ids: next as string[] })
  else replaceClause(kind, { kind, assignees: next as Assignee[] })
}

// One local age draft follows the currently open field and resynchronizes
// on external clause changes; unfinished or invalid values never emit.
const daysDraft = ref('30')
const ageOp = ref<AgeOp['op'] | null>(null)
const daysDirty = ref(false)
const daysError = ref(false)
const ageOps = computed(() => [
  { value: 'within_days' as const, label: 'In the last' },
  { value: 'not_within_days' as const, label: 'Not in the last' },
  ...(ageKind.value === 'created' ? [] : [{ value: 'never' as const, label: 'Never' }]),
])
function resetAgeDraft() {
  daysDraft.value = '30'
  ageOp.value = null
  daysDirty.value = false
  daysError.value = false
}
function syncAgeDraft() {
  resetAgeDraft()
  const age = activeAge.value?.age
  if (!age) return
  ageOp.value = age.op
  if (age.op !== 'never') daysDraft.value = String(age.days)
}
watch([editingKind, activeAge], syncAgeDraft)
function validDays() {
  const days = Number(daysDraft.value)
  const valid = daysDraft.value.trim() !== '' && Number.isInteger(days) && days >= 1 && days <= 3650
  daysError.value = !valid
  return valid ? days : null
}
function commitAge(op: AgeOp['op']) {
  const kind = ageKind.value
  if (!kind || (kind === 'created' && op === 'never')) return false
  ageOp.value = op
  if (op === 'never') {
    daysDirty.value = false
    daysError.value = false
    replaceClause(kind, { kind, age: { op } })
    return true
  }
  const days = validDays()
  if (days === null) return false
  daysDirty.value = false
  replaceClause(kind, { kind, age: { op, days } })
  return true
}
function commitDays() {
  if (!daysDirty.value || !ageKind.value || ageOp.value === 'never') return true
  return commitAge(ageOp.value ?? 'within_days')
}
function setPreset(days: number) {
  daysDraft.value = String(days)
  commitAge(ageOp.value === 'not_within_days' ? 'not_within_days' : 'within_days')
}
function setBool(value: boolean) {
  if (!boolKind.value) return
  // Rule 4: the locked anchor's value is pinned `true` — negating it here
  // would silently disable the feed's own axis. Guarded again in the
  // template (the "No" button is disabled), this is the defense-in-depth
  // backstop against any other path reaching this function.
  if (boolKind.value === props.lockedAnchorKind && !value) return
  replaceClause(boolKind.value, { kind: boolKind.value, value })
}
const ageHint = computed(() => {
  if (ageOp.value !== 'not_within_days' || !ageKind.value || ageKind.value === 'created') return ''
  if (ageKind.value === 'last_contact') return 'Includes people with no recorded contact attempt.'
  if (ageKind.value === 'last_inbound') return 'Includes people with no received email recorded.'
  return 'Includes people with no inquiry.'
})

async function focusEditor() {
  await nextTick()
  panel.value?.querySelector<HTMLElement>('[data-editor-focus]')?.focus()
}
function selectKind(kind: FilterClauseKind | null) {
  editingKind.value = kind
  search.value = ''
  syncAgeDraft()
  void focusEditor()
}
async function openEditor(kind: FilterClauseKind | null, event: Event) {
  if (open.value && editingKind.value === kind && trigger === event.currentTarget) {
    closeEditor()
    return
  }
  if (!commitDays()) return
  trigger = event.currentTarget as HTMLElement
  restoreFocus = false
  selectKind(kind)
  open.value = true
  popover.value?.show(event)
  await nextTick()
  popover.value?.alignOverlay()
}
function closeEditor() {
  if (!commitDays()) return
  restoreFocus = true
  popover.value?.hide()
}
function dismissEditor() {
  // Escape/X must remain an exit even for an unfinished invalid value.
  // Flush a valid edit; otherwise retain only the last committed filter.
  commitDays()
  daysDirty.value = false
  restoreFocus = true
  popover.value?.hide()
}
function onHide() {
  // Outside-click/scroll dismissal can precede blur. Flush a valid draft,
  // while retaining the last committed filter if the draft is invalid.
  commitDays()
  open.value = false
  editingKind.value = null
  if (restoreFocus || document.activeElement === document.body || panel.value?.contains(document.activeElement)) {
    const target = trigger?.isConnected ? trigger : toolbar.value?.querySelector<HTMLElement>('button')
    target?.focus()
  }
  restoreFocus = false
}
</script>

<template>
  <div class="mb-3 flex flex-col gap-3">
    <div
      ref="toolbar"
      class="flex flex-wrap items-center gap-2"
      aria-label="People filters"
    >
      <button
        v-for="kind in (['assigned_to', 'stage'] as const)"
        :key="kind"
        type="button"
        :data-testid="`filter-trigger-${kind}`"
        :class="triggerClasses(kind)"
        :aria-expanded="open && editingKind === kind"
        :aria-controls="`${id}-popover`"
        aria-haspopup="dialog"
        @click="openEditor(kind, $event)"
      >
        {{ triggerLabel(kind) }}
        <ChevronDown
          class="h-4 w-4"
          stroke-width="1.5"
          aria-hidden="true"
        />
      </button>
      <button
        type="button"
        data-testid="filter-add"
        :class="buttonClasses()"
        :aria-expanded="open && editingKind !== 'stage' && editingKind !== 'assigned_to'"
        :aria-controls="`${id}-popover`"
        aria-haspopup="dialog"
        @click="openEditor(null, $event)"
      >
        <SlidersHorizontal
          class="h-4 w-4"
          stroke-width="1.5"
          aria-hidden="true"
        />
        Filters
      </button>
    </div>

    <div
      v-if="applied.length"
      class="flex flex-wrap items-center gap-2"
      aria-label="Applied filters"
    >
      <span
        v-for="clause in applied"
        :key="clause.kind"
        :data-testid="`filter-chip-${clause.kind}`"
        class="inline-flex max-w-full items-stretch rounded-lg border border-border bg-surface-0 text-small text-text"
      >
        <button
          type="button"
          class="flex min-h-10 min-w-0 flex-wrap items-center gap-1 rounded-l-lg px-3 py-2 text-left hover:bg-surface-1 focus-visible:ring-2 focus-visible:ring-focus"
          :aria-label="`Edit ${describeClause(clause, names)}`"
          :aria-expanded="open && editingKind === clause.kind"
          :aria-controls="`${id}-popover`"
          aria-haspopup="dialog"
          @click="openEditor(clause.kind, $event)"
        >
          <template v-if="clause.kind === 'stage'">
            <span>Stage:</span>
            <template
              v-for="(stageId, index) in clause.stage_ids.slice(0, 2)"
              :key="stageId"
            >
              <span v-if="index">or</span>
              <StageLabel :stage="stageFor(stageId)" />
            </template>
            <span v-if="clause.stage_ids.length > 2">+{{ clause.stage_ids.length - 2 }}</span>
          </template>
          <span
            v-else
            class="min-w-0 break-words"
          >{{ describeClause(clause, names, 2) }}</span>
          <ChevronDown
            class="h-4 w-4 shrink-0"
            stroke-width="1.5"
            aria-hidden="true"
          />
        </button>
        <span
          v-if="isLockedClauseKind(clause.kind)"
          :data-testid="`filter-chip-locked-${clause.kind}`"
          class="inline-flex min-h-10 shrink-0 items-center pr-3 text-small text-text-muted"
          title="Required by this rule"
        >
          <Lock
            class="h-4 w-4"
            stroke-width="1.5"
            aria-hidden="true"
          />
          <span class="sr-only">Required, cannot be removed</span>
        </span>
        <button
          v-else
          type="button"
          :data-testid="`filter-chip-remove-${clause.kind}`"
          class="inline-flex min-h-10 w-10 shrink-0 items-center justify-center rounded-r-lg hover:bg-surface-1 focus-visible:ring-2 focus-visible:ring-focus"
          :aria-label="`Remove ${CLAUSE_KIND_LABEL[clause.kind]} filter`"
          @click="removeClause(clause.kind)"
        >
          <X
            class="h-4 w-4"
            stroke-width="1.5"
            aria-hidden="true"
          />
        </button>
      </span>
      <button
        v-if="hasClearableClause"
        type="button"
        data-testid="filter-clear-all"
        :class="buttonClasses('ghost')"
        @click="clearAll"
      >
        Clear all
      </button>
    </div>

    <Popover
      :id="`${id}-popover`"
      ref="popover"
      :pt="popoverPt"
      :aria-labelledby="`${id}-title`"
      @show="focusEditor"
      @hide="onHide"
    >
      <div
        ref="panel"
        @keydown.esc.stop.prevent="dismissEditor"
      >
        <div class="mb-3 flex items-center justify-between gap-2">
          <h2
            :id="`${id}-title`"
            class="text-body font-medium text-text"
          >
            {{ title }}
          </h2>
          <button
            type="button"
            :class="buttonClasses('ghost')"
            aria-label="Close filter editor"
            @click="dismissEditor"
          >
            <X
              class="h-4 w-4"
              stroke-width="1.5"
              aria-hidden="true"
            />
          </button>
        </div>

        <template v-if="!editingKind">
          <label
            :for="`${id}-search`"
            class="sr-only"
          >Search filters</label>
          <input
            :id="`${id}-search`"
            v-model="search"
            type="search"
            data-testid="filter-search"
            data-editor-focus
            :class="INPUT_CLASSES"
            placeholder="Search filters"
            autocomplete="off"
          >
          <div class="mt-2 max-h-64 overflow-y-auto">
            <button
              v-for="kind in menuKinds"
              :key="kind"
              type="button"
              :data-testid="`filter-add-${kind}`"
              :class="rowClasses"
              @click="selectKind(kind)"
            >
              <span class="min-w-0 flex-1">{{ CLAUSE_KIND_LABEL[kind] }}</span>
              <span
                v-if="applied.some((clause) => clause.kind === kind)"
                class="text-small text-text-muted"
              >Active</span>
              <ChevronRight
                class="h-4 w-4 shrink-0"
                stroke-width="1.5"
                aria-hidden="true"
              />
            </button>
            <p
              v-if="!menuKinds.length"
              class="px-2 py-3 text-small text-text-muted"
            >
              No matching filters.
            </p>
          </div>
        </template>

        <div
          v-else
          :data-testid="`filter-editor-${editingKind}`"
        >
          <template v-if="optionKind">
            <p
              v-if="optionKind === 'source'"
              class="mb-3 text-small text-text-muted"
            >
              Matches the latest inquiry's source.
            </p>
            <label
              :for="`${id}-search`"
              class="sr-only"
            >Search {{ optionNoun }}</label>
            <input
              :id="`${id}-search`"
              v-model="search"
              type="search"
              data-testid="filter-option-search"
              data-editor-focus
              :class="INPUT_CLASSES"
              :placeholder="`Search ${optionNoun}`"
              autocomplete="off"
            >
            <p
              v-if="optionPending"
              role="status"
              class="mt-3 text-small text-text-muted"
            >
              Loading {{ optionNoun }}…
            </p>
            <div
              v-else-if="optionError"
              role="alert"
              class="mt-3 text-small text-danger"
            >
              Couldn't load {{ optionNoun }}.
              <button
                type="button"
                :class="buttonClasses('ghost')"
                @click="emit('retry-options', optionKind)"
              >
                Retry
              </button>
            </div>
            <div class="mt-2 max-h-64 overflow-y-auto">
              <label
                v-for="option in visibleOptions"
                :key="option.key"
                :class="rowClasses"
              >
                <input
                  type="checkbox"
                  :data-testid="optionKind === 'assigned_to' ? `filter-assignee-${option.key}` : `filter-option-${option.key}`"
                  :class="checkboxClasses"
                  :checked="isSelected(option)"
                  :disabled="isLockedOptionValue(optionKind, option.value) || (selectionCount >= 50 && !isSelected(option))"
                  @change="toggleOption(option)"
                >
                <StageLabel
                  v-if="option.stage"
                  :stage="option.stage"
                />
                <span
                  v-else
                  class="min-w-0 break-words"
                >{{ option.label }}<span
                  v-if="option.inactive"
                  class="text-text-muted"
                > (inactive)</span><span
                  v-if="isLockedOptionValue(optionKind, option.value)"
                  class="text-text-muted"
                > (required)</span></span>
              </label>
              <p
                v-if="!visibleOptions.length && !optionPending && !optionError"
                class="px-2 py-3 text-small text-text-muted"
              >
                {{ search ? 'No matching options.' : `No ${optionNoun} available.` }}
              </p>
            </div>
            <p
              v-if="selectionCount >= 50"
              role="status"
              class="mt-2 text-small text-text-muted"
            >
              50 values selected. Remove one to select another.
            </p>
            <p
              v-else
              class="mt-2 text-small text-text-muted"
            >
              Any selected value · Changes apply immediately
            </p>
            <p
              v-if="optionKind === 'source' && sourcesTruncated"
              class="mt-2 text-small text-text-muted"
            >
              Showing the first 500 sources. Search covers these options.
            </p>
          </template>

          <template v-else-if="ageKind">
            <fieldset>
              <legend class="sr-only">
                {{ title }} time window
              </legend>
              <label
                v-for="op in ageOps"
                :key="op.value"
                :class="rowClasses"
              >
                <input
                  type="radio"
                  :name="`${id}-age-op`"
                  :data-testid="`filter-age-op-${ageKind}-${op.value}`"
                  :data-editor-focus="op.value === 'within_days' ? '' : undefined"
                  :class="checkboxClasses"
                  :checked="ageOp === op.value"
                  @click="commitAge(op.value)"
                >
                {{ op.label }}
              </label>
            </fieldset>
            <template v-if="ageOp !== 'never'">
              <div class="my-3 grid grid-cols-2 gap-2">
                <button
                  v-for="days in [7, 14, 30, 90]"
                  :key="days"
                  type="button"
                  :data-testid="`filter-days-preset-${days}`"
                  :class="buttonClasses('secondary')"
                  class="px-2"
                  :aria-pressed="Number(daysDraft) === days"
                  @click="setPreset(days)"
                >
                  {{ days }} days
                </button>
              </div>
              <label
                :for="`${id}-days`"
                class="mb-1.5 block text-small font-medium"
              >Custom window</label>
              <div class="flex items-center gap-2">
                <input
                  :id="`${id}-days`"
                  :data-testid="`filter-days-${ageKind}`"
                  type="number"
                  min="1"
                  max="3650"
                  step="1"
                  :class="INPUT_CLASSES"
                  class="max-w-24"
                  :value="daysDraft"
                  :aria-invalid="daysError"
                  :aria-describedby="daysError ? `${id}-days-error` : undefined"
                  @input="daysDraft = ($event.target as HTMLInputElement).value; daysDirty = true"
                  @blur="commitDays"
                  @keydown.enter.prevent="commitDays"
                >
                <span class="text-body text-text-muted">days</span>
              </div>
              <p
                v-if="daysError"
                :id="`${id}-days-error`"
                role="alert"
                class="mt-2 text-small text-danger"
              >
                Enter a whole number from 1 to 3,650.
              </p>
            </template>
            <p
              v-if="ageHint"
              class="mt-3 text-small text-text-muted"
            >
              {{ ageHint }}
            </p>
          </template>

          <template v-else-if="boolKind">
            <p
              v-if="boolKind === 'has_replied'"
              class="mb-3 text-small text-text-muted"
            >
              A received email was recorded, whether answered or not.
            </p>
            <p
              v-if="boolKind === editingKind && boolKind === lockedAnchorKind"
              data-testid="filter-anchor-locked-hint"
              class="mb-3 text-small text-text-muted"
            >
              This is the rule this feed is built on. It stays on and cannot be removed.
            </p>
            <div
              class="flex gap-2"
              role="group"
              :aria-label="title"
            >
              <button
                v-for="value in [true, false]"
                :key="String(value)"
                type="button"
                :data-testid="`filter-bool-${value ? 'yes' : 'no'}-${boolKind}`"
                :data-editor-focus="value ? '' : undefined"
                :aria-pressed="activeBool?.value === value"
                :disabled="!value && boolKind === lockedAnchorKind"
                :class="buttonClasses(activeBool?.value === value ? 'primary' : 'secondary')"
                @click="setBool(value)"
              >
                {{ value ? 'Yes' : 'No' }}
              </button>
            </div>
          </template>

          <div class="mt-4 flex items-center justify-between gap-2 border-t border-border pt-3">
            <button
              v-if="activeClause && editingKind && !isLockedClauseKind(editingKind)"
              type="button"
              data-testid="filter-clear-selection"
              :class="buttonClasses('ghost')"
              @click="removeClause(editingKind)"
            >
              Clear selection
            </button>
            <button
              type="button"
              data-testid="filter-editor-done"
              :class="buttonClasses()"
              class="ml-auto"
              @click="closeEditor"
            >
              Done
            </button>
          </div>
        </div>
      </div>
    </Popover>
  </div>
</template>
