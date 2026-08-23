<script setup lang="ts">
// SLICE_006c §1 step 7 / §10: the History "Change outcome" dialog — the
// same five-choice picker as the call panel's prompt, for the caller's own
// call-derived, non-superseded attempt rows. Presentational: the owner
// (PersonDetailView.vue) runs `useCorrectCallOutcome` and passes its state
// in, the same split as CallPanel.vue. No free text.
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import OutcomePicker from './OutcomePicker.vue'
import type { CallOutcomeCorrection } from '../api/types'
import { buttonClasses, dialogPt } from '../lib/controls'

const props = defineProps<{
  visible: boolean
  personName: string
  /** The row's current outcome — the picker opens on it. */
  currentOutcome: CallOutcomeCorrection
  saving: boolean
  error: string | null
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  save: [outcome: CallOutcomeCorrection]
}>()

const selected = ref<CallOutcomeCorrection>(props.currentOutcome)

watch(
  () => props.visible,
  (visible) => {
    if (visible) selected.value = props.currentOutcome
  },
)

function close() {
  if (props.saving) return
  emit('update:visible', false)
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="false"
    :close-on-escape="!saving"
    :dismissable-mask="!saving"
    :pt="dialogPt()"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        Change outcome
      </h2>
    </template>

    <p class="mb-4 text-body text-text-muted">
      {{ personName }}
    </p>

    <OutcomePicker
      :model-value="selected"
      :disabled="saving"
      @update:model-value="(value) => (selected = value)"
    />

    <p
      v-if="error"
      role="alert"
      class="mt-3 text-small text-danger"
      data-testid="change-outcome-error"
    >
      {{ error }}
    </p>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="saving"
        data-testid="change-outcome-cancel"
        @click="close"
      >
        Cancel
      </button>
      <button
        type="button"
        :class="buttonClasses('primary')"
        :disabled="saving"
        data-testid="change-outcome-save"
        @click="emit('save', selected)"
      >
        {{ saving ? 'Saving…' : 'Save outcome' }}
      </button>
    </template>
  </Dialog>
</template>
