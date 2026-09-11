<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { CustomField } from '../api/types'
import { buttonClasses, INPUT_CLASSES } from '../lib/controls'
import { customClauseError, type CustomFilterClause } from '../lib/filter'

const props = defineProps<{ field: CustomField; clause?: CustomFilterClause }>()
const emit = defineEmits<{ apply: [CustomFilterClause]; cancel: [] }>()

type PresenceOp = 'is_set' | 'is_not_set'
type TextOp = 'contains' | 'not_contains'
type RangeOp = 'range' | 'not_range'
type ChoiceOp = 'any_of' | 'none_of'
type EditorOp = PresenceOp | TextOp | RangeOp | ChoiceOp

const op = ref<EditorOp>('is_set')
const text = ref('')
const min = ref('')
const max = ref('')
const selected = ref<string[]>([])
const choiceSearch = ref('')
const isPresence = computed(() => op.value === 'is_set' || op.value === 'is_not_set')
const isRange = computed(() => props.field.field_type === 'number' || props.field.field_type === 'date')
const candidate = computed<CustomFilterClause>(() => {
  const base = { field_id: props.field.id }
  if (op.value === 'is_set' || op.value === 'is_not_set') {
    return { kind: `custom_${props.field.field_type}` as CustomFilterClause['kind'], ...base, test: { op: op.value } } as CustomFilterClause
  }
  if (props.field.field_type === 'text') return { kind: 'custom_text', ...base, test: { op: op.value as TextOp, text: text.value } }
  if (props.field.field_type === 'choice') return { kind: 'custom_choice', ...base, test: { op: op.value as ChoiceOp, option_ids: selected.value } }
  const bounds = { ...(min.value !== '' ? { min: min.value } : {}), ...(max.value !== '' ? { max: max.value } : {}) }
  return {
    kind: props.field.field_type === 'number' ? 'custom_number' : 'custom_date',
    ...base,
    test: { op: op.value as RangeOp, ...bounds },
  } as CustomFilterClause
})
const error = computed(() => customClauseError(candidate.value))
const visibleOptions = computed(() => {
  const needle = choiceSearch.value.trim().toLocaleLowerCase()
  return needle ? props.field.options.filter((option) => option.label.toLocaleLowerCase().includes(needle)) : props.field.options
})

function resetDraft(clause?: CustomFilterClause) {
  op.value = 'is_set'
  text.value = ''
  min.value = ''
  max.value = ''
  selected.value = []
  choiceSearch.value = ''
  if (!clause || clause.field_id !== props.field.id) return
  op.value = clause.test.op as EditorOp
  if ('text' in clause.test) text.value = clause.test.text
  if ('min' in clause.test || 'max' in clause.test) { min.value = clause.test.min ?? ''; max.value = clause.test.max ?? '' }
  if ('option_ids' in clause.test) selected.value = [...clause.test.option_ids]
}
watch(() => [props.field.id, props.clause] as const, ([, clause]) => resetDraft(clause), { immediate: true })
function apply() { if (error.value === null) emit('apply', candidate.value) }
</script>

<template>
  <div data-testid="custom-filter-editor">
    <p class="mb-3 text-small text-text-muted">
      {{ field.label }}<span v-if="field.archived_at"> (archived)</span>
    </p>
    <label
      class="mb-1.5 block text-small font-medium"
      for="custom-filter-operator"
    >Match</label>
    <select
      id="custom-filter-operator"
      v-model="op"
      data-testid="custom-filter-operator"
      data-editor-focus
      :class="INPUT_CLASSES"
    >
      <option value="is_set">
        Is not empty
      </option>
      <option value="is_not_set">
        Is empty
      </option>
      <template v-if="field.field_type === 'text'">
        <option value="contains">
          Contains
        </option><option value="not_contains">
          Does not contain (or empty)
        </option>
      </template>
      <template v-else-if="isRange">
        <option value="range">
          In range
        </option><option value="not_range">
          Not in range (or empty)
        </option>
      </template>
      <template v-else>
        <option value="any_of">
          Is one of
        </option><option value="none_of">
          Is not one of (or empty)
        </option>
      </template>
    </select>
    <template v-if="field.field_type === 'text' && !isPresence">
      <label
        class="mb-1.5 mt-3 block text-small font-medium"
        for="custom-filter-text"
      >Text</label>
      <input
        id="custom-filter-text"
        v-model="text"
        data-testid="custom-filter-text"
        :class="INPUT_CLASSES"
        autocomplete="off"
        @keydown.enter.prevent="apply"
      >
    </template>
    <template v-else-if="isRange && !isPresence">
      <div class="mt-3 grid grid-cols-1 gap-3 min-[390px]:grid-cols-2">
        <div>
          <label
            class="mb-1.5 block text-small font-medium"
            for="custom-filter-min"
          >From</label><input
            id="custom-filter-min"
            v-model="min"
            data-testid="custom-filter-min"
            :type="field.field_type === 'date' ? 'date' : 'text'"
            :class="INPUT_CLASSES"
            :inputmode="field.field_type === 'number' ? 'decimal' : 'numeric'"
            :placeholder="field.field_type === 'number' ? 'e.g. 250000.00' : 'YYYY-MM-DD'"
            autocomplete="off"
            @keydown.enter.prevent="apply"
          >
        </div>
        <div>
          <label
            class="mb-1.5 block text-small font-medium"
            for="custom-filter-max"
          >To</label><input
            id="custom-filter-max"
            v-model="max"
            data-testid="custom-filter-max"
            :type="field.field_type === 'date' ? 'date' : 'text'"
            :class="INPUT_CLASSES"
            :inputmode="field.field_type === 'number' ? 'decimal' : 'numeric'"
            :placeholder="field.field_type === 'number' ? 'e.g. 500000.00' : 'YYYY-MM-DD'"
            autocomplete="off"
            @keydown.enter.prevent="apply"
          >
        </div>
      </div>
    </template>
    <fieldset
      v-else-if="field.field_type === 'choice' && !isPresence"
      class="mt-3"
    >
      <legend class="mb-1.5 text-small font-medium">
        Options
      </legend>
      <label
        class="sr-only"
        for="custom-filter-choice-search"
      >Search options</label>
      <input
        id="custom-filter-choice-search"
        v-model="choiceSearch"
        data-testid="custom-filter-choice-search"
        type="search"
        :class="INPUT_CLASSES"
        placeholder="Search options"
        autocomplete="off"
      >
      <div class="mt-2 max-h-48 overflow-y-auto">
        <label
          v-for="option in visibleOptions"
          :key="option.id"
          class="flex min-h-10 items-center gap-3 rounded-lg px-2 py-2 text-body hover:bg-surface-1"
        ><input
          v-model="selected"
          type="checkbox"
          :value="option.id"
          class="h-4 w-4 accent-accent"
        ><span>{{ option.label }}<span
          v-if="option.archived_at"
          class="text-text-muted"
        > (Archived)</span></span></label><p
          v-if="!visibleOptions.length"
          class="px-2 py-3 text-small text-text-muted"
        >
          No matching options.
        </p>
      </div>
    </fieldset>
    <p
      v-if="error"
      role="alert"
      data-testid="custom-filter-error"
      class="mt-3 text-small text-danger"
    >
      {{ error }}
    </p>
    <div class="mt-4 flex items-center justify-end gap-2 border-t border-border pt-3">
      <button
        type="button"
        data-testid="custom-filter-cancel"
        :class="buttonClasses('ghost')"
        @click="emit('cancel')"
      >
        Cancel
      </button><button
        type="button"
        data-testid="custom-filter-apply"
        :class="buttonClasses()"
        :disabled="error !== null"
        @click="apply"
      >
        Apply
      </button>
    </div>
  </div>
</template>
