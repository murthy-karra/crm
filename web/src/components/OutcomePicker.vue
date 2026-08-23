<script setup lang="ts">
// SLICE_006c §1/§10: the five-choice outcome picker shared by CallPanel's
// "How did it go?" prompt and the History "Change outcome" dialog. A
// vertical radio group of 40 px rows (UI_STYLE §5/§8: minimum target,
// `surface-2` selection fill, weight — not colour — carries the selected
// state). Options derive from the label map; there is no free text. D-033:
// `null` means nothing is selected — the prompt never pre-selects the
// system's observation, so the owner gates Save on a pick.
import { Check } from 'lucide-vue-next'
import type { CallOutcomeCorrection } from '../api/types'
import { CALL_OUTCOME_CORRECTION_LABEL } from '../lib/labels'

defineProps<{
  modelValue: CallOutcomeCorrection | null
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: CallOutcomeCorrection] }>()

const OPTIONS = (Object.keys(CALL_OUTCOME_CORRECTION_LABEL) as CallOutcomeCorrection[]).map((id) => ({
  id,
  label: CALL_OUTCOME_CORRECTION_LABEL[id],
}))
</script>

<template>
  <div
    role="radiogroup"
    aria-label="Outcome"
    class="flex flex-col gap-0.5"
    data-testid="outcome-picker"
  >
    <button
      v-for="option in OPTIONS"
      :key="option.id"
      type="button"
      role="radio"
      :aria-checked="option.id === modelValue"
      :disabled="disabled"
      :data-outcome="option.id"
      class="flex h-10 w-full items-center gap-2 rounded-lg px-3 text-left text-body text-text transition-colors duration-150 ease-out hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-surface-0 disabled:opacity-50 disabled:pointer-events-none"
      :class="option.id === modelValue ? 'bg-surface-2 font-medium' : ''"
      @click="emit('update:modelValue', option.id)"
    >
      <Check
        class="h-4 w-4 shrink-0"
        :class="option.id === modelValue ? 'text-text' : 'invisible'"
        stroke-width="1.5"
      />
      {{ option.label }}
    </button>
  </div>
</template>
